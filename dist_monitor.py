"""
dist_monitor.py — read-only consumer of a coordinator's monitor stream.

A `MonitorClient` connects to a running `CoordinatorServer` (typically on
another machine) and replays its training events — logs, metrics, status,
progress, checkpoints — into caller-supplied callbacks. It never sends
transitions, never receives weights, and never influences training: it is a
pure dashboard feed. This lets the gradient steps run in a separate process
(e.g. `run_distributed.py` on a beefy host) while the web viewer just watches.

Wire model:
  • REQ → REP  : one MONITOR_HELLO at connect → MONITOR_OK snapshot.
  • SUB ← PUB  : subscribe TOPIC_MONITOR, stream events until stopped.

CLI (tail a coordinator to the console):
    FAB_DIST_TOKEN=secret python dist_monitor.py --coord tcp://10.0.0.5
"""

from __future__ import annotations

import argparse
import os
import signal
import sys
import threading
from typing import Callable, Dict, Optional

import zmq

import dist_protocol as proto
from card_embeddings import embeddings_hash
from dist_coordinator import DEFAULT_PUB_PORT, DEFAULT_REP_PORT


class MonitorClient:
    """Streams a remote coordinator's training events into callbacks."""

    def __init__(
        self,
        coord_url: str,
        token: str,
        *,
        pub_port: int = DEFAULT_PUB_PORT,
        rep_port: int = DEFAULT_REP_PORT,
        callbacks: Optional[Dict[str, Callable]] = None,
        log: Optional[Callable[[str], None]] = None,
    ) -> None:
        if not token:
            raise ValueError("MonitorClient requires FAB_DIST_TOKEN to be set.")
        self.coord_url = coord_url.rstrip("/")
        self.token = token
        self.pub_port = pub_port
        self.rep_port = rep_port

        cb = callbacks or {}
        self._on_log: Callable[[str], None] = cb.get("on_log", lambda m: print(m))
        self._on_metrics: Callable[[dict], None] = cb.get("on_metrics", lambda m: None)
        self._on_status: Callable[[str], None] = cb.get("on_status", lambda s: None)
        self._on_progress: Callable[[dict], None] = cb.get("on_progress", lambda p: None)
        self._on_checkpoint: Callable[[dict], None] = cb.get("on_checkpoint", lambda c: None)
        self._should_stop: Callable[[], bool] = cb.get("should_stop", lambda: False)
        self._log = log or self._on_log

        self._stop = threading.Event()
        self._ctx = zmq.Context.instance()
        self._sub: Optional[zmq.Socket] = None

    # ──────────────────────────────────────────────────────────

    def run(self) -> None:
        snapshot = self._connect()
        self._log(
            f"  👁  monitor connected to {self.coord_url} "
            f"(run={snapshot.get('run_name')!r} v={snapshot.get('weight_version')} "
            f"step={snapshot.get('grad_steps')})"
        )
        # Seed the UI with the snapshot so it shows live numbers immediately
        # rather than blank until the next event.
        if snapshot.get("status"):
            self._on_status(str(snapshot["status"]))
        self._on_progress({
            "grad_steps": snapshot.get("grad_steps", 0),
            "games": snapshot.get("games", 0),
        })

        try:
            while not self._stop.is_set() and not self._should_stop():
                try:
                    parts = self._sub.recv_multipart()
                except zmq.error.Again:
                    continue
                except zmq.ZMQError:
                    break
                if len(parts) < 2 or parts[0] != proto.TOPIC_MONITOR:
                    continue
                try:
                    event, data, _ver, _run = proto.unpack_monitor_event(parts[1])
                except Exception as exc:
                    self._log(f"  ⚠  bad monitor event: {exc!r}")
                    continue
                self._dispatch(event, data)
        finally:
            self._close()

    def stop(self) -> None:
        self._stop.set()

    # ──────────────────────────────────────────────────────────

    def _connect(self) -> Dict[str, object]:
        self._sub = self._ctx.socket(zmq.SUB)
        self._sub.connect(f"{self.coord_url}:{self.pub_port}")
        self._sub.setsockopt(zmq.SUBSCRIBE, proto.TOPIC_MONITOR)
        self._sub.RCVTIMEO = 250  # ms — let the loop notice the stop flag

        req = self._ctx.socket(zmq.REQ)
        req.connect(f"{self.coord_url}:{self.rep_port}")
        req.RCVTIMEO = 10000
        req.SNDTIMEO = 10000
        try:
            req.send(proto.make_envelope(
                {"embeddings_hash": embeddings_hash()},
                self.token,
                proto.KIND_MONITOR_HELLO,
            ))
            try:
                reply = req.recv()
            except zmq.error.Again:
                raise RuntimeError(
                    f"no response from coordinator at {self.coord_url}:{self.rep_port} "
                    f"(timed out). Is it running and is FAB_DIST_TOKEN correct?"
                )
            kind, _tok, payload = proto.decode_envelope(reply)
        finally:
            req.close(linger=0)

        if kind != proto.KIND_MONITOR_OK or not isinstance(payload, dict):
            reason = (payload or {}).get("reason") if isinstance(payload, dict) else None
            raise RuntimeError(
                f"monitor handshake failed: kind={kind} reason={reason}"
            )
        return payload

    def _close(self) -> None:
        if self._sub is not None:
            try:
                self._sub.close(linger=0)
            except Exception:
                pass

    def _dispatch(self, event: Optional[str], data) -> None:
        if event == "log":
            self._on_log(str(data))
        elif event == "metrics" and isinstance(data, dict):
            self._on_metrics(data)
        elif event == "status":
            self._on_status(str(data))
        elif event == "progress" and isinstance(data, dict):
            self._on_progress(data)
        elif event == "checkpoint" and isinstance(data, dict):
            self._on_checkpoint(data)


# ── CLI ──────────────────────────────────────────────────────────────────

def _build_cli() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Tail a distributed FaB coordinator's training stream."
    )
    p.add_argument("--coord", type=str, default="tcp://127.0.0.1",
                   help="Coordinator URL (e.g. tcp://10.0.0.5)")
    p.add_argument("--pub-port", type=int, default=DEFAULT_PUB_PORT)
    p.add_argument("--rep-port", type=int, default=DEFAULT_REP_PORT)
    return p


def main() -> None:
    args = _build_cli().parse_args()
    token = os.environ.get("FAB_DIST_TOKEN", "")
    if not token:
        print("FAB_DIST_TOKEN env var must be set.", file=sys.stderr)
        sys.exit(2)

    client = MonitorClient(
        coord_url=args.coord,
        token=token,
        pub_port=args.pub_port,
        rep_port=args.rep_port,
    )

    def _sigterm(_signum, _frame):
        client.stop()
    signal.signal(signal.SIGTERM, _sigterm)
    signal.signal(signal.SIGINT, _sigterm)

    client.run()


if __name__ == "__main__":
    main()

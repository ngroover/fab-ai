"""
dist_monitor.py — read-only consumer of a coordinator's monitor stream.

A `MonitorClient` connects to a running `CoordinatorServer` (typically on
another machine) and replays its training events — logs, metrics, status,
progress, checkpoints — into caller-supplied callbacks. It never sends
transitions, never receives weights, and never influences training: it is a
pure dashboard feed. This lets the gradient steps run in a separate process
(e.g. `run_distributed.py` on a beefy host) while the web viewer just watches.

Beyond the read-only stream, the client also mirrors the coordinator's saved
checkpoints to the local filesystem so the web viewer's Models / Play tabs can
load them like any locally-trained model. On connect it pulls the full
index + .pt blobs the coordinator has on disk; afterwards every `checkpoint`
event triggers a single fresh download in the background.

Wire model:
  • REQ → REP  : MONITOR_HELLO at connect → MONITOR_OK snapshot, plus on-
                 demand CHECKPOINT_LIST / CHECKPOINT_FETCH for sync.
  • SUB ← PUB  : subscribe TOPIC_MONITOR, stream events until stopped.

CLI (tail a coordinator to the console):
    FAB_DIST_TOKEN=secret python dist_monitor.py --coord tcp://10.0.0.5
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import signal
import sys
import threading
from typing import Any, Callable, Dict, Optional

import zmq

import dist_protocol as proto
from card_embeddings import embeddings_hash
from dist_coordinator import DEFAULT_PUB_PORT, DEFAULT_REP_PORT
from self_play_trainer import CHECKPOINT_DIR, INDEX_PATH


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
        checkpoint_dir: Optional[str] = None,
        index_path: Optional[str] = None,
    ) -> None:
        if not token:
            raise ValueError("MonitorClient requires FAB_DIST_TOKEN to be set.")
        self.coord_url = coord_url.rstrip("/")
        self.token = token
        self.pub_port = pub_port
        self.rep_port = rep_port
        self.checkpoint_dir = checkpoint_dir or CHECKPOINT_DIR
        self.index_path = index_path or INDEX_PATH

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
        # Long-lived REQ socket for checkpoint list/fetch. Owned by the
        # fetcher thread so we don't need a lock for strict REQ alternation.
        self._req: Optional[zmq.Socket] = None
        self._fetch_queue: "queue.Queue[Optional[Dict[str, Any]]]" = queue.Queue()
        self._fetch_thread: Optional[threading.Thread] = None
        # Guards index.json writes; readers (list_checkpoints) hit the file
        # without locking and tolerate concurrent atomic replaces.
        self._index_lock = threading.Lock()

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

        # Start background fetcher and request an initial full sync so the
        # local Models tab reflects whatever the coordinator already has.
        self._fetch_thread = threading.Thread(
            target=self._fetcher_loop, name="monitor-fetch", daemon=True,
        )
        self._fetch_thread.start()
        self._fetch_queue.put({"kind": "sync_all"})

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
            # Drain the fetcher before tearing down sockets so an in-flight
            # REQ doesn't race against socket close.
            self._fetch_queue.put(None)
            if self._fetch_thread is not None:
                self._fetch_thread.join(timeout=5.0)
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
        for sock in (self._sub, self._req):
            if sock is not None:
                try:
                    sock.close(linger=0)
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
            name = data.get("name")
            if isinstance(name, str) and name:
                self._fetch_queue.put({"kind": "fetch_one", "name": name,
                                       "meta": data})

    # ──────────────────────────────────────────────────────────
    # Checkpoint mirroring
    # ──────────────────────────────────────────────────────────

    def _fetcher_loop(self) -> None:
        # Long-lived REQ socket dedicated to the fetcher thread. Keeping it
        # separate from the handshake REQ means the SUB loop's lifetime is
        # decoupled from any in-flight checkpoint fetch.
        self._req = self._ctx.socket(zmq.REQ)
        self._req.connect(f"{self.coord_url}:{self.rep_port}")
        self._req.RCVTIMEO = 30000  # ms — .pt blobs can be a few MB
        self._req.SNDTIMEO = 10000
        try:
            while True:
                item = self._fetch_queue.get()
                if item is None:
                    return
                if self._stop.is_set():
                    continue
                kind = item.get("kind")
                if kind == "sync_all":
                    self._sync_all()
                elif kind == "fetch_one":
                    self._fetch_one(item["name"], item.get("meta"))
        finally:
            if self._req is not None:
                try:
                    self._req.close(linger=0)
                except Exception:
                    pass
                self._req = None

    def _request(self, kind: str, payload: dict):
        envelope = proto.make_envelope(payload, self.token, kind)
        self._req.send(envelope)
        reply = self._req.recv()
        rkind, _tok, rpayload = proto.decode_envelope(reply)
        return rkind, rpayload

    def _sync_all(self) -> None:
        try:
            kind, payload = self._request(proto.KIND_CHECKPOINT_LIST, {})
        except (zmq.ZMQError, zmq.error.Again) as exc:
            self._log(f"  ⚠  checkpoint index sync failed: {exc!r}")
            return
        if kind != proto.KIND_CHECKPOINT_LIST_OK or not isinstance(payload, dict):
            self._log(f"  ⚠  checkpoint index sync: unexpected kind={kind!r}")
            return
        ckpts = payload.get("checkpoints") or []
        downloaded = 0
        for meta in ckpts:
            if self._stop.is_set():
                return
            if not isinstance(meta, dict):
                continue
            name = meta.get("name")
            if not isinstance(name, str) or not name:
                continue
            self._merge_index_entry(meta)
            path = os.path.join(self.checkpoint_dir, f"{name}.pt")
            if not os.path.isfile(path):
                if self._fetch_blob(name):
                    downloaded += 1
        self._log(
            f"  📥  remote checkpoint sync: {len(ckpts)} known, "
            f"{downloaded} newly downloaded"
        )

    def _fetch_one(self, name: str, meta: Optional[dict] = None) -> None:
        if isinstance(meta, dict):
            self._merge_index_entry(meta)
        path = os.path.join(self.checkpoint_dir, f"{name}.pt")
        if os.path.isfile(path):
            return
        self._fetch_blob(name)

    def _fetch_blob(self, name: str) -> bool:
        if not _is_safe_name(name):
            self._log(f"  ⚠  refusing to fetch unsafe checkpoint name {name!r}")
            return False
        try:
            kind, payload = self._request(
                proto.KIND_CHECKPOINT_FETCH, {"name": name}
            )
        except (zmq.ZMQError, zmq.error.Again) as exc:
            self._log(f"  ⚠  checkpoint fetch {name!r} failed: {exc!r}")
            return False
        if kind != proto.KIND_CHECKPOINT_FETCH_OK or not isinstance(payload, dict):
            self._log(f"  ⚠  checkpoint {name!r} unavailable (kind={kind!r})")
            return False
        data = payload.get("data")
        if not isinstance(data, (bytes, bytearray)):
            self._log(f"  ⚠  checkpoint {name!r}: missing blob")
            return False
        os.makedirs(self.checkpoint_dir, exist_ok=True)
        path = os.path.join(self.checkpoint_dir, f"{name}.pt")
        tmp = f"{path}.{os.getpid()}.tmp"
        with open(tmp, "wb") as f:
            f.write(data)
        os.replace(tmp, path)
        meta = payload.get("meta")
        if isinstance(meta, dict):
            self._merge_index_entry(meta)
        self._log(
            f"  ⬇  downloaded checkpoint {name} ({len(data)/1024:.1f} KB)"
        )
        return True

    def _merge_index_entry(self, meta: dict) -> None:
        """Insert/replace one checkpoint entry in index.json atomically."""
        name = meta.get("name")
        if not isinstance(name, str) or not name:
            return
        os.makedirs(self.checkpoint_dir, exist_ok=True)
        with self._index_lock:
            data = {"checkpoints": []}
            if os.path.isfile(self.index_path):
                try:
                    with open(self.index_path) as f:
                        loaded = json.load(f)
                    if isinstance(loaded, dict) and isinstance(
                        loaded.get("checkpoints"), list
                    ):
                        data = loaded
                except (OSError, json.JSONDecodeError):
                    pass
            kept = [c for c in data["checkpoints"]
                    if isinstance(c, dict) and c.get("name") != name]
            kept.append(meta)
            data["checkpoints"] = kept
            tmp = f"{self.index_path}.{os.getpid()}.tmp"
            with open(tmp, "w") as f:
                json.dump(data, f, indent=2)
            os.replace(tmp, self.index_path)


def _is_safe_name(name: str) -> bool:
    if not name or not isinstance(name, str):
        return False
    if "/" in name or "\\" in name or os.sep in name or ".." in name:
        return False
    return True


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

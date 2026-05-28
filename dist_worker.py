"""
dist_worker.py — self-play game generator that streams transitions to a
remote coordinator.

The worker:
  1. Connects to the coordinator over ZMQ.
  2. Sends a HANDSHAKE_HELLO with the shared token + a worker ID.
  3. Receives the trainer config + current model weights + weight version.
  4. Spins in a loop: play one self-play game using the local network copy,
     PUSH the resulting batch of `Transition`s to the coordinator. Between
     decisions, polls the SUB socket for fresher weights.
  5. A dedicated background thread sends REQ heartbeats on a fixed cadence
     so the coordinator's liveness check stays satisfied even while a
     single self-play game is mid-MCTS-search (which can outlast the
     coordinator's worker_timeout_sec).

Process model: a worker is one Python process. Launch several to parallelize
on one machine; launch on additional machines to scale out.

CLI:
    FAB_DIST_TOKEN=secret python dist_worker.py \\
        --coord tcp://127.0.0.1 --worker-id 1
"""

from __future__ import annotations

import argparse
import json
import os
import random
import signal
import socket
import sys
import threading
import time
import uuid
from typing import Optional

import torch
import zmq

import dist_protocol as proto
from card_embeddings import embeddings_hash
from neural_agent import PolicyValueNetwork
from self_play_trainer import (
    CHECKPOINT_DIR,
    INDEX_PATH,
    TrainerConfig,
    play_one_self_play_game,
    sample_deck_pair,
    set_checkpoint_provider,
)

from dist_coordinator import (
    DEFAULT_PULL_PORT,
    DEFAULT_PUB_PORT,
    DEFAULT_REP_PORT,
)


class Worker:
    def __init__(
        self,
        coord_url: str,
        token: str,
        worker_id: str,
        pull_port: int = DEFAULT_PULL_PORT,
        pub_port: int = DEFAULT_PUB_PORT,
        rep_port: int = DEFAULT_REP_PORT,
        base_seed: Optional[int] = None,
        heartbeat_every_sec: float = 30.0,
        log: Optional[callable] = None,
    ) -> None:
        if not token:
            raise ValueError("Worker requires FAB_DIST_TOKEN to be set.")
        self.coord_url = coord_url.rstrip("/")
        self.token = token
        self.worker_id = worker_id
        self.pull_port = pull_port
        self.pub_port = pub_port
        self.rep_port = rep_port
        self.heartbeat_every_sec = float(heartbeat_every_sec)
        self._log = log or (lambda msg: print(f"[worker {worker_id}] {msg}", flush=True))
        self._stop = False

        # Per-worker deterministic-ish RNG.
        seed_material = (
            (base_seed or random.randrange(2**31))
            ^ (hash(str(worker_id)) & 0x7FFFFFFF)
            ^ int.from_bytes(os.urandom(4), "big")
        )
        self.rng = random.Random(seed_material)
        torch.manual_seed(seed_material & 0x7FFFFFFF)

        self._ctx = zmq.Context.instance()
        self._req: Optional[zmq.Socket] = None
        self._push: Optional[zmq.Socket] = None
        self._sub: Optional[zmq.Socket] = None

        self.cfg: Optional[TrainerConfig] = None
        self.net = PolicyValueNetwork()
        self.net.eval()
        self.weight_version = 0

        # Heartbeats run on a dedicated thread so they fire on schedule
        # even while the main thread is blocked inside a long MCTS search.
        self._hb_stop = threading.Event()
        self._hb_thread: Optional[threading.Thread] = None

        # Checkpoint sync: only needed when the opponent pool draws 'past'
        # models, which live as .pt files on the coordinator.
        self._wants_checkpoints = False
        self._last_ckpt_index_sync = 0.0
        self._ckpt_index_sync_every_sec = 60.0

    # ──────────────────────────────────────────────────────────
    # Public API
    # ──────────────────────────────────────────────────────────

    def run(self, max_games: int = 0) -> None:
        self._connect()
        self._log(f"connected to {self.coord_url}  (initial v={self.weight_version})")

        played = 0
        while not self._stop:
            try:
                self._maybe_consume_weight_updates()
                self._maybe_sync_checkpoint_index()
                self._play_and_send_one()
                played += 1
                if max_games > 0 and played >= max_games:
                    self._log(f"reached max_games={max_games}, exiting")
                    break
            except KeyboardInterrupt:
                self._log("interrupted")
                break
            except Exception as exc:
                self._log(f"error: {exc!r} — sleeping 1s then retrying")
                time.sleep(1.0)

        self._close()

    def stop(self) -> None:
        self._stop = True

    # ──────────────────────────────────────────────────────────
    # Connection
    # ──────────────────────────────────────────────────────────

    def _connect(self) -> None:
        # PUSH for transitions.
        self._push = self._ctx.socket(zmq.PUSH)
        self._push.connect(f"{self.coord_url}:{self.pull_port}")

        # SUB for weight broadcasts.
        self._sub = self._ctx.socket(zmq.SUB)
        self._sub.connect(f"{self.coord_url}:{self.pub_port}")
        self._sub.setsockopt(zmq.SUBSCRIBE, proto.TOPIC_WEIGHTS)

        # REQ for handshake + checkpoint fetches. Heartbeats use their own
        # socket on the background thread to avoid socket-state contention.
        self._req = self._ctx.socket(zmq.REQ)
        self._req.connect(f"{self.coord_url}:{self.rep_port}")
        self._req.RCVTIMEO = 10000  # 10s
        self._req.SNDTIMEO = 10000

        # Handshake.
        local_hash = embeddings_hash()
        self._log(f"local card embeddings hash: {local_hash}")
        envelope = proto.make_envelope(
            {
                "worker_id": self.worker_id,
                "host": socket.gethostname(),
                "embeddings_hash": local_hash,
            },
            self.token,
            proto.KIND_HANDSHAKE_HELLO,
        )
        self._req.send(envelope)
        reply = self._req.recv()
        kind, _tok, payload = proto.decode_envelope(reply)
        if kind != proto.KIND_HANDSHAKE_OK or not isinstance(payload, dict):
            reason = (payload or {}).get("reason") if isinstance(payload, dict) else None
            if reason == "bad_embeddings_hash":
                raise RuntimeError(
                    f"handshake rejected: card embeddings hash mismatch — "
                    f"coordinator expects {payload.get('expected')!r}, "
                    f"this worker has {payload.get('got')!r}. "
                    f"Sync `card_embeddings_out/` from the coordinator (or "
                    f"re-run `python card_embeddings.py` on both with the "
                    f"same git checkout)."
                )
            raise RuntimeError(f"handshake failed: kind={kind} payload={payload}")

        coord_hash = str(payload.get("embeddings_hash", ""))
        if coord_hash and coord_hash != local_hash:
            raise RuntimeError(
                f"handshake card embeddings hash mismatch: coordinator "
                f"{coord_hash!r} != worker {local_hash!r}"
            )

        self.cfg = proto.wire_to_config(payload["config"])
        self._apply_weights_blob(payload["weights"])

        # If this run uses 'past' opponents, register the on-demand fetcher and
        # pull the coordinator's checkpoint index so games can reference them.
        self._wants_checkpoints = "past" in (self.cfg.opponent_pool or ())
        if self._wants_checkpoints:
            set_checkpoint_provider(self._ensure_checkpoint_file)
            self._sync_checkpoint_index()
            self._last_ckpt_index_sync = time.time()

        self._start_heartbeat_thread()

    def _close(self) -> None:
        self._hb_stop.set()
        if self._hb_thread is not None:
            self._hb_thread.join(timeout=5.0)
            self._hb_thread = None
        if self._wants_checkpoints:
            set_checkpoint_provider(None)
        for s in (self._req, self._push, self._sub):
            if s is not None:
                try:
                    s.close(linger=0)
                except Exception:
                    pass

    # ──────────────────────────────────────────────────────────
    # Weight handling
    # ──────────────────────────────────────────────────────────

    def _apply_weights_blob(self, blob: bytes) -> None:
        state_dict, version, _run_name = proto.unpack_weights(blob)
        self.net.load_state_dict(state_dict)
        self.net.eval()
        self.weight_version = int(version)

    def _maybe_consume_weight_updates(self) -> None:
        """Drain pending SUB messages without blocking."""
        while True:
            try:
                parts = self._sub.recv_multipart(flags=zmq.NOBLOCK)
            except zmq.Again:
                return
            if len(parts) >= 2 and parts[0] == proto.TOPIC_WEIGHTS:
                try:
                    self._apply_weights_blob(parts[1])
                except Exception as exc:
                    self._log(f"failed to apply weights: {exc!r}")

    def _start_heartbeat_thread(self) -> None:
        if self._hb_thread is not None and self._hb_thread.is_alive():
            return
        self._hb_stop.clear()
        self._hb_thread = threading.Thread(
            target=self._heartbeat_loop,
            name=f"hb-{self.worker_id}",
            daemon=True,
        )
        self._hb_thread.start()

    def _heartbeat_loop(self) -> None:
        """Send liveness pings on a fixed schedule from a dedicated REQ socket.

        Lives on its own thread + socket so it keeps firing while the main
        thread is blocked in `play_one_self_play_game` — without this, a
        game whose MCTS runs longer than the coordinator's worker timeout
        causes the worker to be marked dead even though it is still working.
        """
        def _make_sock() -> zmq.Socket:
            s = self._ctx.socket(zmq.REQ)
            s.RCVTIMEO = 10000
            s.SNDTIMEO = 10000
            s.connect(f"{self.coord_url}:{self.rep_port}")
            return s

        sock = _make_sock()
        try:
            while not self._hb_stop.wait(self.heartbeat_every_sec):
                envelope = proto.make_envelope(
                    {"worker_id": self.worker_id},
                    self.token,
                    proto.KIND_HEARTBEAT,
                )
                try:
                    sock.send(envelope)
                    sock.recv()
                except zmq.ZMQError as exc:
                    self._log(f"heartbeat failed: {exc!r}")
                    # REQ is wedged after a send-or-recv timeout; recreate.
                    try:
                        sock.close(linger=0)
                    except Exception:
                        pass
                    sock = _make_sock()
        finally:
            try:
                sock.close(linger=0)
            except Exception:
                pass

    # ──────────────────────────────────────────────────────────
    # Checkpoint sync ('past' opponents)
    # ──────────────────────────────────────────────────────────

    def _request(self, kind: str, payload: dict):
        """Send one REQ envelope and return the decoded (kind, payload).

        The worker is single-threaded and the REQ socket is strictly
        alternating send→recv, so callers must not interleave requests.
        """
        envelope = proto.make_envelope(payload, self.token, kind)
        self._req.send(envelope)
        reply = self._req.recv()
        rkind, _tok, rpayload = proto.decode_envelope(reply)
        return rkind, rpayload

    def _maybe_sync_checkpoint_index(self) -> None:
        if not self._wants_checkpoints:
            return
        if time.time() - self._last_ckpt_index_sync < self._ckpt_index_sync_every_sec:
            return
        self._sync_checkpoint_index()
        self._last_ckpt_index_sync = time.time()

    def _sync_checkpoint_index(self) -> None:
        """Mirror the coordinator's checkpoint index locally.

        The worker produces no checkpoints of its own, so it simply overwrites
        its index with the coordinator's. The .pt payloads stay remote until a
        game actually references one (see `_ensure_checkpoint_file`).
        """
        try:
            kind, payload = self._request(
                proto.KIND_CHECKPOINT_LIST, {"worker_id": self.worker_id}
            )
        except zmq.ZMQError as exc:
            self._log(f"checkpoint index sync failed: {exc!r}")
            return
        if kind != proto.KIND_CHECKPOINT_LIST_OK or not isinstance(payload, dict):
            return
        ckpts = payload.get("checkpoints") or []
        os.makedirs(CHECKPOINT_DIR, exist_ok=True)
        with open(INDEX_PATH, "w") as f:
            json.dump({"checkpoints": ckpts}, f, indent=2)
        self._log(f"synced checkpoint index: {len(ckpts)} entries")

    def _ensure_checkpoint_file(self, name: str) -> bool:
        """Provider hook: download `<name>.pt` from the coordinator if missing."""
        path = os.path.join(CHECKPOINT_DIR, f"{name}.pt")
        if os.path.isfile(path):
            return True
        try:
            kind, payload = self._request(
                proto.KIND_CHECKPOINT_FETCH,
                {"worker_id": self.worker_id, "name": name},
            )
        except zmq.ZMQError as exc:
            self._log(f"checkpoint fetch {name!r} failed: {exc!r}")
            return False
        if kind != proto.KIND_CHECKPOINT_FETCH_OK or not isinstance(payload, dict):
            self._log(f"checkpoint {name!r} unavailable from coordinator")
            return False
        data = payload.get("data")
        if not isinstance(data, (bytes, bytearray)):
            return False
        os.makedirs(CHECKPOINT_DIR, exist_ok=True)
        tmp = f"{path}.{os.getpid()}.tmp"
        with open(tmp, "wb") as f:
            f.write(data)
        os.replace(tmp, path)  # atomic: a partial file is never observed as ready
        self._log(f"downloaded checkpoint {name} ({len(data)/1024:.1f} KB)")
        return True

    # ──────────────────────────────────────────────────────────
    # Game generation
    # ──────────────────────────────────────────────────────────

    def _play_and_send_one(self) -> None:
        opp_kind = self.rng.choice(self.cfg.opponent_pool or ("self",))
        deck0_name, deck1_name = sample_deck_pair(self.cfg, self.rng)

        decisions, outcome, length, _ = play_one_self_play_game(
            self.net, self.cfg, self.rng,
            opp_kind, deck0_name, deck1_name,
            weight_version=self.weight_version,
            should_stop=lambda: self._stop,
        )

        if not decisions:
            return

        payload = {
            "worker_id": self.worker_id,
            "games": 1,
            "transitions": proto.transitions_to_wire(decisions),
            "outcome": float(outcome),
            "length": int(length),
        }
        envelope = proto.make_envelope(payload, self.token, proto.KIND_TRANSITIONS)
        self._push.send(envelope)
        self._log(
            f"game(opp={opp_kind} decks={deck0_name}/{deck1_name}) "
            f"len={length} out={outcome:+.0f} transitions={len(decisions)} "
            f"v={self.weight_version}"
        )


# ── CLI ──────────────────────────────────────────────────────────────────

def _build_cli() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Self-play worker for the distributed FaB trainer."
    )
    p.add_argument("--coord", type=str, default="tcp://127.0.0.1",
                   help="Coordinator URL (e.g. tcp://10.0.0.5)")
    p.add_argument("--worker-id", type=str, default="",
                   help="Stable identifier (defaults to host+pid+uuid)")
    p.add_argument("--pull-port", type=int, default=DEFAULT_PULL_PORT)
    p.add_argument("--pub-port", type=int, default=DEFAULT_PUB_PORT)
    p.add_argument("--rep-port", type=int, default=DEFAULT_REP_PORT)
    p.add_argument("--seed", type=int, default=None)
    p.add_argument("--max-games", type=int, default=0,
                   help="Exit after N games (0 = run forever)")
    p.add_argument("--heartbeat-secs", type=float, default=30.0)
    return p


def main() -> None:
    args = _build_cli().parse_args()
    token = os.environ.get("FAB_DIST_TOKEN", "")
    if not token:
        print("FAB_DIST_TOKEN env var must be set.", file=sys.stderr)
        sys.exit(2)
    worker_id = args.worker_id or f"{socket.gethostname()}-{os.getpid()}-{uuid.uuid4().hex[:6]}"

    worker = Worker(
        coord_url=args.coord,
        token=token,
        worker_id=worker_id,
        pull_port=args.pull_port,
        pub_port=args.pub_port,
        rep_port=args.rep_port,
        base_seed=args.seed,
        heartbeat_every_sec=args.heartbeat_secs,
    )

    def _sigterm(_signum, _frame):
        worker.stop()
    signal.signal(signal.SIGTERM, _sigterm)
    signal.signal(signal.SIGINT, _sigterm)

    worker.run(max_games=args.max_games)


if __name__ == "__main__":
    main()

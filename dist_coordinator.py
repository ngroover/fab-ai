"""
dist_coordinator.py — coordinator server for distributed self-play.

The coordinator owns the network, optimizer, replay buffer, and runs all
gradient updates. Workers connect over ZMQ and stream `Transition`s back
that the coordinator pulls into the replay buffer; the coordinator
periodically broadcasts updated weights back to all workers.

Architecture:

  ┌─ PULL   :5556 ─────────► ReplayBuffer (lock-protected)
  │                                │
  │                                ▼
  ├─ PUB    :5557 ◄──── training thread: sample → train → bump weight_version
  │
  └─ ROUTER :5558  serves handshake (auth → config + initial weights),
                    heartbeat pings, and checkpoint list/fetch. Each request
                    is handed to a thread-pool worker so a slow request
                    (e.g. a large checkpoint blob) doesn't block heartbeats
                    or handshakes from other workers.

Token auth: every PUSH and REQ envelope must carry the shared token. Bad
envelopes are silently dropped (counter is logged).
"""

from __future__ import annotations

import concurrent.futures
import os
import threading
import time
from dataclasses import asdict
from datetime import datetime
from typing import Callable, Dict, List, Optional

import torch
import zmq

import self_play_trainer as spt
from self_play_trainer import (
    SelfPlayTrainer,
    TrainerConfig,
    Transition,
    _hash_state_dict,
)
import dist_protocol as proto
from card_embeddings import embeddings_hash


DEFAULT_PULL_PORT = 5556
DEFAULT_PUB_PORT = 5557
DEFAULT_REP_PORT = 5558


class CoordinatorServer:
    """Async distributed AlphaZero coordinator.

    Composes a `SelfPlayTrainer` to reuse its network, optimizer, replay
    buffer, training step, eval, and checkpointing — but replaces the
    sequential self-play phase with ZMQ-driven streaming from workers.
    """

    def __init__(
        self,
        config: TrainerConfig,
        token: str,
        callbacks: Optional[Dict[str, Callable]] = None,
        bind_host: str = "0.0.0.0",
        pull_port: int = DEFAULT_PULL_PORT,
        pub_port: int = DEFAULT_PUB_PORT,
        rep_port: int = DEFAULT_REP_PORT,
        broadcast_every_sec: float = 5.0,
        max_staleness: int = 10,
        min_buffer_to_train: int = 64,
        eval_every_grad_steps: int = 0,   # 0 = use config.eval_every*steps_per_iter
        checkpoint_every_grad_steps: int = 0,
        worker_timeout_sec: float = 90.0,
        rep_workers: int = 8,
    ) -> None:
        if not token:
            raise ValueError("Coordinator requires a non-empty auth token.")
        self.token = token
        self.bind_host = bind_host
        self.pull_port = pull_port
        self.pub_port = pub_port
        self.rep_port = rep_port
        self.broadcast_every_sec = float(broadcast_every_sec)
        self.max_staleness = int(max_staleness)
        self.min_buffer_to_train = int(min_buffer_to_train)
        self.worker_timeout_sec = float(worker_timeout_sec)
        self.rep_workers = max(1, int(rep_workers))
        self.eval_every_grad_steps = int(
            eval_every_grad_steps or max(1, config.eval_every) * max(1, config.steps_per_iter)
        )
        self.checkpoint_every_grad_steps = int(
            checkpoint_every_grad_steps or self.eval_every_grad_steps
        )

        # Last status string emitted by the trainer, kept so monitor clients
        # that connect mid-run get a meaningful snapshot.
        self._last_status = "idle"

        # Internal trainer carries all model + buffer state. Callbacks are
        # augmented so every event the local UI sees is ALSO broadcast to
        # remote monitor clients over the PUB socket (TOPIC_MONITOR).
        self._trainer = SelfPlayTrainer(
            config, callbacks=self._wrap_callbacks(callbacks)
        )

        # Fingerprint of the local card-embedding table. Workers must match
        # this exactly or their observations encode cards into a different
        # vector space than the network was trained against.
        self._embeddings_hash = embeddings_hash()

        # Lock guards trainer.buffer and the model state_dict during pub/copy.
        self._lock = threading.Lock()
        # ZMQ sockets are not thread-safe; the PUB socket is touched by the
        # broadcast thread and the training thread, so guard its sends. The
        # ROUTER socket is touched by every pool worker that ships a reply,
        # so it gets its own lock too.
        self._pub_lock = threading.Lock()
        self._router_lock = threading.Lock()
        self._weight_version = 0
        self._stop_flag = threading.Event()
        self._stats = {
            "transitions_received": 0,
            "transitions_dropped_token": 0,
            "transitions_dropped_stale": 0,
            "batches_received": 0,
            "workers_seen": set(),       # lifetime — every worker_id ever seen
            "last_broadcast_at": 0.0,
        }
        self._workers_last_seen: Dict[str, float] = {}
        # Workers we last reported as alive. Used to log transitions to dead.
        self._known_alive_workers: set = set()

        # ZMQ
        self._ctx = zmq.Context.instance()
        self._pull_sock: Optional[zmq.Socket] = None
        self._pub_sock: Optional[zmq.Socket] = None
        self._router_sock: Optional[zmq.Socket] = None

        # Pool of threads that build + send REQ replies. Lives only between
        # run() and its finally block.
        self._rep_executor: Optional[concurrent.futures.ThreadPoolExecutor] = None

    # ──────────────────────────────────────────────────────────
    # Monitor broadcasting
    # ──────────────────────────────────────────────────────────

    _MONITOR_EVENTS = {
        "on_log": "log",
        "on_metrics": "metrics",
        "on_status": "status",
        "on_progress": "progress",
        "on_checkpoint": "checkpoint",
    }

    def _wrap_callbacks(
        self, callbacks: Optional[Dict[str, Callable]]
    ) -> Dict[str, Callable]:
        """Return callbacks that fire the original AND publish to monitors.

        Control callbacks (`should_stop`, `wait_if_paused`) pass through
        untouched. When no local `on_log` is supplied (headless coordinator),
        the trainer's default print-to-console behaviour is preserved.
        """
        cb = dict(callbacks or {})
        for cb_name, event in self._MONITOR_EVENTS.items():
            default = (lambda msg: print(msg)) if cb_name == "on_log" else None
            cb[cb_name] = self._make_monitor_cb(cb_name, event, cb.get(cb_name, default))
        return cb

    def _make_monitor_cb(
        self, cb_name: str, event: str, orig: Optional[Callable]
    ) -> Callable:
        def wrapped(payload):
            if orig is not None:
                orig(payload)
            if cb_name == "on_status":
                self._last_status = str(payload)
            self._publish_monitor(event, payload)
        return wrapped

    def _publish_monitor(self, event: str, data) -> None:
        sock = self._pub_sock
        if sock is None:
            return
        try:
            blob = proto.pack_monitor_event(
                event, data, self._weight_version, self._trainer.config.run_name
            )
        except Exception:
            # Non-serializable payload — skip rather than crash the trainer.
            return
        with self._pub_lock:
            try:
                sock.send_multipart([proto.TOPIC_MONITOR, blob])
            except zmq.ZMQError:
                pass

    def _monitor_snapshot(self) -> Dict[str, object]:
        with self._lock:
            return {
                "run_name": self._trainer.config.run_name,
                "weight_version": self._weight_version,
                "grad_steps": self._trainer._grad_steps,
                "games": self._trainer._games_done,
                "buffer_size": len(self._trainer.buffer),
                "workers": len(self._active_worker_ids()),
                "status": self._last_status,
                "embeddings_hash": self._embeddings_hash,
            }

    # ──────────────────────────────────────────────────────────
    # Worker liveness
    # ──────────────────────────────────────────────────────────

    def _active_worker_ids(self) -> set:
        """Workers that pushed transitions or heartbeated within the timeout."""
        cutoff = time.time() - self.worker_timeout_sec
        return {wid for wid, ts in self._workers_last_seen.items() if ts >= cutoff}

    def _check_dead_workers(self) -> None:
        """Log workers that have gone silent past the timeout, once each."""
        active = self._active_worker_ids()
        dropped = self._known_alive_workers - active
        if dropped:
            now = time.time()
            for wid in sorted(dropped):
                silence = now - self._workers_last_seen.get(wid, now)
                self._trainer._log(
                    f"  ⚠  worker {wid} silent for {silence:.0f}s — marking dead"
                )
        self._known_alive_workers = active

    # ──────────────────────────────────────────────────────────
    # Public API
    # ──────────────────────────────────────────────────────────

    def run(self) -> None:
        """Block in the training loop until `stop()` is called or total_iters elapse."""
        self._bind_sockets()
        self._rep_executor = concurrent.futures.ThreadPoolExecutor(
            max_workers=self.rep_workers, thread_name_prefix="rep"
        )
        self._trainer._log(
            f"  ▶  Coordinator listening on PULL :{self.pull_port}  "
            f"PUB :{self.pub_port}  ROUTER :{self.rep_port}  "
            f"(rep_workers={self.rep_workers})"
        )
        self._trainer._log(f"  🔑  card embeddings hash: {self._embeddings_hash}")

        threads = [
            threading.Thread(target=self._pull_loop, name="pull", daemon=True),
            threading.Thread(target=self._rep_loop, name="rep", daemon=True),
            threading.Thread(target=self._broadcast_loop, name="broadcast", daemon=True),
        ]
        for t in threads:
            t.start()

        try:
            self._training_loop()
        finally:
            # Joining the helper threads BEFORE closing sockets is required:
            # ZMQ asserts (signaler.cpp POLLIN) if a socket is closed while
            # another thread is mid-recv/send on it. The pool drains next so
            # no worker is still mid-send when the ROUTER socket goes away.
            self._stop_flag.set()
            for t in threads:
                t.join(timeout=5.0)
            if self._rep_executor is not None:
                self._rep_executor.shutdown(wait=True)
                self._rep_executor = None
            self._trainer._on_status("idle")
            self._close_sockets()

    def stop(self) -> None:
        self._stop_flag.set()

    # ──────────────────────────────────────────────────────────
    # Sockets
    # ──────────────────────────────────────────────────────────

    def _bind_sockets(self) -> None:
        self._pull_sock = self._ctx.socket(zmq.PULL)
        self._pull_sock.bind(f"tcp://{self.bind_host}:{self.pull_port}")
        self._pull_sock.RCVTIMEO = 250  # ms — let pull loop notice stop flag

        self._pub_sock = self._ctx.socket(zmq.PUB)
        self._pub_sock.bind(f"tcp://{self.bind_host}:{self.pub_port}")

        # ROUTER instead of REP so the recv loop can dispatch each request
        # to a pool worker and stay responsive while the previous reply is
        # still being assembled or sent. REQ ↔ ROUTER is wire-compatible,
        # so clients (workers, monitor) need no changes.
        self._router_sock = self._ctx.socket(zmq.ROUTER)
        self._router_sock.bind(f"tcp://{self.bind_host}:{self.rep_port}")
        self._router_sock.RCVTIMEO = 250

    def _close_sockets(self) -> None:
        for s in (self._pull_sock, self._pub_sock, self._router_sock):
            if s is not None:
                try:
                    s.close(linger=0)
                except Exception:
                    pass

    # ──────────────────────────────────────────────────────────
    # PULL loop (workers PUSH transitions)
    # ──────────────────────────────────────────────────────────

    def _pull_loop(self) -> None:
        while not self._stop_flag.is_set():
            try:
                data = self._pull_sock.recv()
            except zmq.error.Again:
                continue
            except zmq.ZMQError:
                break
            kind, tok, payload = proto.decode_envelope(data)
            if not proto.check_token(tok, self.token):
                self._stats["transitions_dropped_token"] += 1
                continue
            if kind != proto.KIND_TRANSITIONS or not isinstance(payload, dict):
                continue

            items = payload.get("transitions") or []
            worker_id = str(payload.get("worker_id", "?"))
            self._workers_last_seen[worker_id] = time.time()
            self._stats["workers_seen"].add(worker_id)

            try:
                transitions = proto.wire_to_transitions(items)
            except Exception as exc:
                self._trainer._log(f"  ⚠  bad transition batch from {worker_id}: {exc}")
                continue

            # Staleness check & buffer push.
            games_done_snapshot: Optional[int] = None
            with self._lock:
                cv = self._weight_version
                kept: List[Transition] = []
                for t in transitions:
                    if (cv - t.weight_version) > self.max_staleness:
                        self._stats["transitions_dropped_stale"] += 1
                        continue
                    kept.append(t)
                if kept:
                    self._trainer.buffer.push_many(kept)
                    games_in_batch = int(payload.get("games", 0))
                    if games_in_batch > 0:
                        self._trainer._games_done += games_in_batch
                        games_done_snapshot = self._trainer._games_done
                    self._stats["transitions_received"] += len(kept)
                    self._stats["batches_received"] += 1

            # Notify the UI / monitor clients that the games counter moved.
            # Done outside the lock since on_progress publishes over ZMQ.
            if games_done_snapshot is not None:
                self._trainer._on_progress({"games": games_done_snapshot})

    # ──────────────────────────────────────────────────────────
    # ROUTER loop (handshake + heartbeat + checkpoint)
    # ──────────────────────────────────────────────────────────
    #
    # The receiver thread does only the recv + dispatch; each request is
    # handled on a pool thread so a slow handler (e.g. a checkpoint blob
    # read or a state_dict pack) doesn't block heartbeats from other
    # workers behind it. ZMQ sockets aren't thread-safe, so the sends back
    # through the ROUTER socket are serialized with `_router_lock`.

    def _rep_loop(self) -> None:
        while not self._stop_flag.is_set():
            try:
                frames = self._router_sock.recv_multipart()
            except zmq.error.Again:
                continue
            except zmq.ZMQError:
                break
            # REQ → ROUTER framing: [identity, empty_delimiter, payload].
            # Preserve everything before the payload as the routing prefix
            # so the reply lands back at the right client.
            if len(frames) < 2:
                continue
            prefix, envelope = frames[:-1], frames[-1]
            self._rep_executor.submit(self._handle_request, prefix, envelope)

    def _handle_request(self, prefix: List[bytes], envelope: bytes) -> None:
        try:
            reply = self._build_reply(envelope)
        except Exception as exc:
            self._trainer._log(f"  ⚠  REP handler error: {exc!r}")
            return
        if reply is None:
            return
        with self._router_lock:
            sock = self._router_sock
            if sock is None:
                return
            try:
                sock.send_multipart(prefix + [reply])
            except zmq.ZMQError:
                pass

    def _build_reply(self, data: bytes) -> Optional[bytes]:
        kind, tok, payload = proto.decode_envelope(data)
        if not proto.check_token(tok, self.token):
            return proto.make_envelope(
                {"reason": "bad_token"}, "", proto.KIND_HANDSHAKE_FAIL
            )

        if kind == proto.KIND_HANDSHAKE_HELLO:
            return self._reply_handshake(payload)
        if kind == proto.KIND_MONITOR_HELLO:
            return self._reply_monitor_hello()
        if kind == proto.KIND_HEARTBEAT:
            return self._reply_heartbeat(payload)
        if kind == proto.KIND_CHECKPOINT_LIST:
            return self._reply_checkpoint_list()
        if kind == proto.KIND_CHECKPOINT_FETCH:
            return self._reply_checkpoint_fetch(payload)
        return proto.make_envelope(
            {"reason": "unknown_kind"}, self.token, proto.KIND_HANDSHAKE_FAIL
        )

    def _reply_handshake(self, payload: Optional[dict]) -> bytes:
        worker_id = str((payload or {}).get("worker_id", "?"))
        worker_hash = str((payload or {}).get("embeddings_hash", ""))
        if worker_hash != self._embeddings_hash:
            self._trainer._log(
                f"  ⛔  worker {worker_id} rejected: embeddings hash "
                f"{worker_hash!r} != coordinator {self._embeddings_hash!r}"
            )
            return proto.make_envelope(
                {
                    "reason": "bad_embeddings_hash",
                    "expected": self._embeddings_hash,
                    "got": worker_hash,
                },
                self.token,
                proto.KIND_HANDSHAKE_FAIL,
            )

        self._workers_last_seen[worker_id] = time.time()
        self._stats["workers_seen"].add(worker_id)
        with self._lock:
            weights_blob = proto.pack_weights(
                self._trainer.net.state_dict(),
                self._weight_version,
                self._trainer.config.run_name,
            )
            cfg_wire = proto.config_to_wire(self._trainer.config)
            version = self._weight_version
        self._trainer._log(f"  🤝  worker {worker_id} handshake (v={version})")
        return proto.make_envelope(
            {
                "config": cfg_wire,
                "weights": weights_blob,
                "weight_version": version,
                "run_name": self._trainer.config.run_name,
                "embeddings_hash": self._embeddings_hash,
            },
            self.token,
            proto.KIND_HANDSHAKE_OK,
        )

    def _reply_monitor_hello(self) -> bytes:
        snapshot = self._monitor_snapshot()
        self._trainer._log(
            f"  👁  monitor attached "
            f"(v={snapshot['weight_version']} step={snapshot['grad_steps']})"
        )
        return proto.make_envelope(snapshot, self.token, proto.KIND_MONITOR_OK)

    def _reply_heartbeat(self, payload: Optional[dict]) -> bytes:
        worker_id = str((payload or {}).get("worker_id", "?"))
        self._workers_last_seen[worker_id] = time.time()
        with self._lock:
            version = self._weight_version
        return proto.make_envelope(
            {"weight_version": version}, self.token, proto.KIND_HEARTBEAT_OK
        )

    def _reply_checkpoint_list(self) -> bytes:
        with self._lock:
            ckpts = spt.list_checkpoints()
        return proto.make_envelope(
            {"checkpoints": ckpts}, self.token, proto.KIND_CHECKPOINT_LIST_OK
        )

    def _reply_checkpoint_fetch(self, payload: Optional[dict]) -> bytes:
        name = str((payload or {}).get("name", ""))
        blob, meta = self._read_checkpoint_file(name)
        if blob is None:
            return proto.make_envelope(
                {"reason": "not_found", "name": name},
                self.token, proto.KIND_CHECKPOINT_FAIL,
            )
        return proto.make_envelope(
            {"name": name, "data": blob, "meta": meta},
            self.token, proto.KIND_CHECKPOINT_FETCH_OK,
        )

    def _read_checkpoint_file(self, name: str):
        """Return (bytes, meta) for checkpoint `name`, or (None, None).

        `name` comes from an authenticated worker but is still treated as
        untrusted input: reject anything that could escape CHECKPOINT_DIR.
        """
        if (not name or "/" in name or "\\" in name
                or os.sep in name or ".." in name):
            return None, None
        path = os.path.join(spt.CHECKPOINT_DIR, f"{name}.pt")
        if not os.path.isfile(path):
            return None, None
        with open(path, "rb") as f:
            blob = f.read()
        meta = next(
            (c for c in spt.list_checkpoints() if c.get("name") == name), None
        )
        return blob, meta

    # ──────────────────────────────────────────────────────────
    # PUB broadcast loop (periodic weight gossip)
    # ──────────────────────────────────────────────────────────

    def _broadcast_loop(self) -> None:
        while not self._stop_flag.is_set():
            if self._stop_flag.wait(self.broadcast_every_sec):
                return
            self._broadcast_weights()

    def _broadcast_weights(self) -> None:
        with self._lock:
            blob = proto.pack_weights(
                self._trainer.net.state_dict(),
                self._weight_version,
                self._trainer.config.run_name,
            )
            version = self._weight_version
        with self._pub_lock:
            try:
                self._pub_sock.send_multipart([proto.TOPIC_WEIGHTS, blob])
                self._stats["last_broadcast_at"] = time.time()
            except zmq.ZMQError:
                pass

    # ──────────────────────────────────────────────────────────
    # Training thread (main)
    # ──────────────────────────────────────────────────────────

    def _training_loop(self) -> None:
        cfg = self._trainer.config
        t = self._trainer
        t._on_status("waiting")
        start = time.time()
        last_eval = 0
        last_ckpt = 0
        last_metrics = time.time()

        while not self._stop_flag.is_set():
            if t._should_stop():
                break
            t._wait_if_paused()

            # Wait until the buffer has enough samples to start training.
            if len(t.buffer) < self.min_buffer_to_train:
                time.sleep(0.5)
                if time.time() - last_metrics > 5.0:
                    self._check_dead_workers()
                    t._log(
                        f"  ⏳  buffer={len(t.buffer)}/{self.min_buffer_to_train}  "
                        f"workers={len(self._known_alive_workers)}"
                    )
                    last_metrics = time.time()
                continue

            t._on_status("training")
            with self._lock:
                batch = t.buffer.sample(cfg.batch_size, t._rng)
            if not batch:
                continue
            ploss, vloss = t._train_step(batch)
            t._grad_steps += 1

            # Bump weight version + broadcast on a schedule.
            with self._lock:
                self._weight_version += 1
            t._on_progress({"grad_steps": t._grad_steps,
                            "weight_version": self._weight_version})

            # Periodic logging.
            if time.time() - last_metrics > 5.0:
                self._check_dead_workers()
                t._log(
                    f"  ∇  step={t._grad_steps}  v={self._weight_version}  "
                    f"buf={len(t.buffer)}  "
                    f"workers={len(self._known_alive_workers)}  "
                    f"recv={self._stats['transitions_received']}  "
                    f"drop_stale={self._stats['transitions_dropped_stale']}  "
                    f"drop_auth={self._stats['transitions_dropped_token']}  "
                    f"p_loss={ploss:.4f}  v_loss={vloss:.4f}"
                )
                last_metrics = time.time()

            # Push fresh weights to workers proactively after every N steps.
            if t._grad_steps % max(1, int(self.broadcast_every_sec * 4)) == 0:
                self._broadcast_weights()

            # Eval + checkpoint cadences.
            eval_wr: Dict[str, float] = {}
            do_eval = (
                self.eval_every_grad_steps > 0
                and t._grad_steps - last_eval >= self.eval_every_grad_steps
            )
            if do_eval:
                last_eval = t._grad_steps
                t._on_status("evaluating")
                with self._lock:
                    eval_wr = t._run_eval_phase()
                t._on_status("training")

            do_ckpt = (
                self.checkpoint_every_grad_steps > 0
                and t._grad_steps - last_ckpt >= self.checkpoint_every_grad_steps
            )
            if do_ckpt:
                last_ckpt = t._grad_steps
                with self._lock:
                    meta = t._save_checkpoint(t._grad_steps, eval_wr)
                t._on_metrics({
                    "iter": t._grad_steps,
                    "games": t._games_done,
                    "grad_steps": t._grad_steps,
                    "weight_version": self._weight_version,
                    "policy_loss": ploss,
                    "value_loss": vloss,
                    "wr_rhinar": eval_wr.get("rhinar"),
                    "wr_dorinthea": eval_wr.get("dorinthea"),
                    "wr_random": eval_wr.get("random"),
                    "buffer_size": len(t.buffer),
                    "workers": len(self._active_worker_ids()),
                    "transitions_received": self._stats["transitions_received"],
                    "transitions_dropped_stale": self._stats["transitions_dropped_stale"],
                    "transitions_dropped_token": self._stats["transitions_dropped_token"],
                    "checkpoint": meta["name"] if meta else None,
                })

            # Optional finite cap (re-uses total_iters as "max grad steps").
            if cfg.total_iters > 0 and t._grad_steps >= cfg.total_iters:
                t._log(f"  ✓  Reached total_iters={cfg.total_iters} (grad steps).")
                break

        t._log(f"  ⏹  Training loop exiting (elapsed {time.time()-start:.1f}s)")

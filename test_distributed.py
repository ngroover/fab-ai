"""
test_distributed.py — end-to-end smoke test for distributed self-play.

Spins up an in-process `CoordinatorServer` plus two subprocess workers on
ephemeral loopback ports and verifies the basics: handshake succeeds,
transitions reach the buffer, the weight_version advances, bad-token
clients are rejected.
"""

from __future__ import annotations

import contextlib
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest

import zmq

import self_play_trainer as spt
import dist_protocol as proto
from dist_coordinator import CoordinatorServer
from self_play_trainer import TrainerConfig


def _free_port() -> int:
    with contextlib.closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class DistributedSmokeTest(unittest.TestCase):

    def setUp(self):
        self._orig_dir = spt.CHECKPOINT_DIR
        self._orig_index = spt.INDEX_PATH
        self._tmp = tempfile.mkdtemp(prefix="fab-dist-test-")
        spt.CHECKPOINT_DIR = self._tmp
        spt.INDEX_PATH = os.path.join(self._tmp, "index.json")

        self.pull_port = _free_port()
        self.pub_port = _free_port()
        self.rep_port = _free_port()
        self.token = "test-token-xyz"

        cfg = TrainerConfig(
            total_iters=0,            # run until stopped
            n_simulations=2,
            batch_size=4,
            buffer_size=200,
            eval_games=0,
            eval_every=1,
            seed=42,
            run_name="dist-smoke",
            opponent_pool=("self",),
            deck_pool=("rhinar", "dorinthea"),
        )
        self.coord = CoordinatorServer(
            config=cfg,
            token=self.token,
            bind_host="127.0.0.1",
            pull_port=self.pull_port,
            pub_port=self.pub_port,
            rep_port=self.rep_port,
            broadcast_every_sec=1.0,
            max_staleness=99,
            min_buffer_to_train=4,
            eval_every_grad_steps=10_000_000,         # disable eval during smoke
            checkpoint_every_grad_steps=10_000_000,   # disable ckpt during smoke
        )
        self._coord_thread = threading.Thread(target=self.coord.run, daemon=True)
        self._coord_thread.start()
        # Wait for sockets to bind.
        self._wait_for_port(self.rep_port, 5.0)

    def tearDown(self):
        self.coord.stop()
        self._coord_thread.join(timeout=5.0)
        spt.CHECKPOINT_DIR = self._orig_dir
        spt.INDEX_PATH = self._orig_index
        shutil.rmtree(self._tmp, ignore_errors=True)

    @staticmethod
    def _wait_for_port(port: int, timeout: float) -> None:
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                with contextlib.closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
                    s.settimeout(0.2)
                    s.connect(("127.0.0.1", port))
                return
            except OSError:
                time.sleep(0.05)
        raise RuntimeError(f"Coordinator never bound port {port}")

    # ──────────────────────────────────────────────────────────
    # Tests
    # ──────────────────────────────────────────────────────────

    def test_bad_token_is_rejected(self):
        ctx = zmq.Context.instance()
        sock = ctx.socket(zmq.REQ)
        sock.RCVTIMEO = 3000
        sock.SNDTIMEO = 3000
        sock.connect(f"tcp://127.0.0.1:{self.rep_port}")

        envelope = proto.make_envelope(
            {"worker_id": "bad"}, "WRONG-TOKEN", proto.KIND_HANDSHAKE_HELLO
        )
        sock.send(envelope)
        reply = sock.recv()
        kind, _tok, payload = proto.decode_envelope(reply)
        self.assertEqual(kind, proto.KIND_HANDSHAKE_FAIL)
        self.assertEqual(payload.get("reason"), "bad_token")
        sock.close(linger=0)

    def test_bad_embeddings_hash_is_rejected(self):
        ctx = zmq.Context.instance()
        sock = ctx.socket(zmq.REQ)
        sock.RCVTIMEO = 3000
        sock.SNDTIMEO = 3000
        sock.connect(f"tcp://127.0.0.1:{self.rep_port}")

        envelope = proto.make_envelope(
            {"worker_id": "drift", "embeddings_hash": "deadbeefdeadbeef"},
            self.token,
            proto.KIND_HANDSHAKE_HELLO,
        )
        sock.send(envelope)
        reply = sock.recv()
        kind, _tok, payload = proto.decode_envelope(reply)
        self.assertEqual(kind, proto.KIND_HANDSHAKE_FAIL)
        self.assertEqual(payload.get("reason"), "bad_embeddings_hash")
        self.assertEqual(payload.get("expected"), self.coord._embeddings_hash)
        self.assertEqual(payload.get("got"), "deadbeefdeadbeef")
        sock.close(linger=0)

    def test_workers_stream_transitions_and_weight_version_advances(self):
        env = os.environ.copy()
        env["FAB_DIST_TOKEN"] = self.token

        procs = []
        for i in range(2):
            p = subprocess.Popen(
                [
                    sys.executable, "-u", "dist_worker.py",
                    "--coord", "tcp://127.0.0.1",
                    "--pull-port", str(self.pull_port),
                    "--pub-port", str(self.pub_port),
                    "--rep-port", str(self.rep_port),
                    "--worker-id", f"w{i}",
                    "--max-games", "8",
                    "--heartbeat-secs", "100",
                ],
                cwd=os.path.dirname(os.path.abspath(__file__)) or ".",
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
            procs.append(p)

        try:
            # Wait until the coordinator has at least a few transitions
            # AND has bumped the weight version at least once.
            deadline = time.time() + 90.0
            ok = False
            while time.time() < deadline:
                with self.coord._lock:
                    buf_len = len(self.coord._trainer.buffer)
                    version = self.coord._weight_version
                if buf_len > 0 and version > 0:
                    ok = True
                    break
                # Bail if all workers have already exited unsuccessfully.
                if all(p.poll() is not None for p in procs):
                    break
                time.sleep(0.5)

            # Drain remaining stdout in case the test failed (useful debug).
            for p in procs:
                if p.poll() is None:
                    p.terminate()
                try:
                    out, _ = p.communicate(timeout=10)
                except subprocess.TimeoutExpired:
                    p.kill()
                    out, _ = p.communicate()
                if not ok:
                    sys.stderr.write(out.decode(errors="replace"))

            self.assertTrue(ok,
                f"buffer/version never advanced — buffer={buf_len} version={version}")

            self.assertGreater(self.coord._stats["transitions_received"], 0)
            self.assertGreaterEqual(len(self.coord._stats["workers_seen"]), 1)
        finally:
            for p in procs:
                if p.poll() is None:
                    p.terminate()


if __name__ == "__main__":
    unittest.main()

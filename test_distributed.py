"""
test_distributed.py — end-to-end smoke test for distributed self-play.

Spins up an in-process `CoordinatorServer` plus two subprocess workers on
ephemeral loopback ports and verifies the basics: handshake succeeds,
transitions reach the buffer, the weight_version advances, bad-token
clients are rejected.
"""

from __future__ import annotations

import contextlib
import json
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

    def test_monitor_handshake_returns_snapshot(self):
        ctx = zmq.Context.instance()
        sock = ctx.socket(zmq.REQ)
        sock.RCVTIMEO = 3000
        sock.SNDTIMEO = 3000
        sock.connect(f"tcp://127.0.0.1:{self.rep_port}")

        envelope = proto.make_envelope(
            {"embeddings_hash": self.coord._embeddings_hash},
            self.token,
            proto.KIND_MONITOR_HELLO,
        )
        sock.send(envelope)
        kind, _tok, payload = proto.decode_envelope(sock.recv())
        self.assertEqual(kind, proto.KIND_MONITOR_OK)
        self.assertEqual(payload.get("run_name"), "dist-smoke")
        self.assertIn("grad_steps", payload)
        self.assertIn("weight_version", payload)
        self.assertEqual(payload.get("embeddings_hash"), self.coord._embeddings_hash)
        sock.close(linger=0)

    def test_monitor_bad_token_is_rejected(self):
        ctx = zmq.Context.instance()
        sock = ctx.socket(zmq.REQ)
        sock.RCVTIMEO = 3000
        sock.SNDTIMEO = 3000
        sock.connect(f"tcp://127.0.0.1:{self.rep_port}")
        sock.send(proto.make_envelope({}, "WRONG-TOKEN", proto.KIND_MONITOR_HELLO))
        kind, _tok, payload = proto.decode_envelope(sock.recv())
        self.assertEqual(kind, proto.KIND_HANDSHAKE_FAIL)
        self.assertEqual(payload.get("reason"), "bad_token")
        sock.close(linger=0)

    def test_monitor_client_receives_published_events(self):
        from dist_monitor import MonitorClient

        received = {"log": [], "status": [], "metrics": []}
        stop = threading.Event()
        client = MonitorClient(
            coord_url="tcp://127.0.0.1",
            token=self.token,
            pub_port=self.pub_port,
            rep_port=self.rep_port,
            callbacks={
                "on_log": lambda m: received["log"].append(m),
                "on_status": lambda s: received["status"].append(s),
                "on_metrics": lambda m: received["metrics"].append(m),
                "should_stop": stop.is_set,
            },
        )
        t = threading.Thread(target=client.run, daemon=True)
        t.start()

        # PUB/SUB has a slow-joiner window; publish repeatedly until the
        # subscriber has caught at least one event or we time out.
        deadline = time.time() + 10.0
        got = False
        while time.time() < deadline:
            self.coord._publish_monitor("log", "hello-monitor")
            if any(m == "hello-monitor" for m in received["log"]):
                got = True
                break
            time.sleep(0.1)

        stop.set()
        client.stop()
        t.join(timeout=5.0)
        self.assertTrue(got, "monitor never received a published log event")

    def _req_socket(self):
        ctx = zmq.Context.instance()
        sock = ctx.socket(zmq.REQ)
        sock.RCVTIMEO = 5000
        sock.SNDTIMEO = 5000
        sock.connect(f"tcp://127.0.0.1:{self.rep_port}")
        return sock

    def test_checkpoint_list_and_fetch_over_rep(self):
        meta = self.coord._trainer._save_checkpoint(1, {})
        name = meta["name"]
        sock = self._req_socket()
        try:
            sock.send(proto.make_envelope(
                {"worker_id": "w"}, self.token, proto.KIND_CHECKPOINT_LIST))
            kind, _tok, payload = proto.decode_envelope(sock.recv())
            self.assertEqual(kind, proto.KIND_CHECKPOINT_LIST_OK)
            self.assertIn(name, [c["name"] for c in payload["checkpoints"]])

            sock.send(proto.make_envelope(
                {"name": name}, self.token, proto.KIND_CHECKPOINT_FETCH))
            kind, _tok, payload = proto.decode_envelope(sock.recv())
            self.assertEqual(kind, proto.KIND_CHECKPOINT_FETCH_OK)
            blob = payload["data"]
            self.assertIsInstance(blob, (bytes, bytearray))
            with open(os.path.join(spt.CHECKPOINT_DIR, f"{name}.pt"), "rb") as f:
                self.assertEqual(bytes(blob), f.read())

            sock.send(proto.make_envelope(
                {"name": "does-not-exist"}, self.token, proto.KIND_CHECKPOINT_FETCH))
            kind, _tok, payload = proto.decode_envelope(sock.recv())
            self.assertEqual(kind, proto.KIND_CHECKPOINT_FAIL)

            # Path traversal must be refused even with a valid token.
            sock.send(proto.make_envelope(
                {"name": "../secret"}, self.token, proto.KIND_CHECKPOINT_FETCH))
            kind, _tok, payload = proto.decode_envelope(sock.recv())
            self.assertEqual(kind, proto.KIND_CHECKPOINT_FAIL)
        finally:
            sock.close(linger=0)

    def test_worker_downloads_missing_checkpoint(self):
        import dist_worker

        meta = self.coord._trainer._save_checkpoint(2, {})
        name = meta["name"]

        worker_tmp = tempfile.mkdtemp(prefix="fab-dist-worker-")
        orig_dir, orig_index = dist_worker.CHECKPOINT_DIR, dist_worker.INDEX_PATH
        dist_worker.CHECKPOINT_DIR = worker_tmp
        dist_worker.INDEX_PATH = os.path.join(worker_tmp, "index.json")

        w = dist_worker.Worker(
            coord_url="tcp://127.0.0.1",
            token=self.token,
            worker_id="ckpt-w",
            pull_port=self.pull_port,
            pub_port=self.pub_port,
            rep_port=self.rep_port,
        )
        try:
            w._connect()

            w._sync_checkpoint_index()
            with open(dist_worker.INDEX_PATH) as f:
                local_names = [c["name"] for c in json.load(f)["checkpoints"]]
            self.assertIn(name, local_names)

            path = os.path.join(worker_tmp, f"{name}.pt")
            self.assertFalse(os.path.isfile(path))  # not pulled until referenced

            self.assertTrue(w._ensure_checkpoint_file(name))
            self.assertTrue(os.path.isfile(path))
            with open(path, "rb") as f1, \
                    open(os.path.join(spt.CHECKPOINT_DIR, f"{name}.pt"), "rb") as f2:
                self.assertEqual(f1.read(), f2.read())

            self.assertTrue(w._ensure_checkpoint_file(name))   # idempotent
            self.assertFalse(w._ensure_checkpoint_file("nope-not-real"))
        finally:
            w._close()
            dist_worker.CHECKPOINT_DIR = orig_dir
            dist_worker.INDEX_PATH = orig_index
            shutil.rmtree(worker_tmp, ignore_errors=True)

    def test_worker_heartbeat_thread_keeps_alive_during_long_work(self):
        """A worker that connects and then stays busy for longer than a few
        heartbeat intervals (no PUSHes meanwhile) must still be considered
        alive by the coordinator. Regression test for the trainer marking
        a still-working worker dead when a single self-play game outlasts
        worker_timeout_sec."""
        import dist_worker

        w = dist_worker.Worker(
            coord_url="tcp://127.0.0.1",
            token=self.token,
            worker_id="hb-w",
            pull_port=self.pull_port,
            pub_port=self.pub_port,
            rep_port=self.rep_port,
            heartbeat_every_sec=0.3,
        )
        try:
            w._connect()
            handshake_ts = self.coord._workers_last_seen.get("hb-w")
            self.assertIsNotNone(handshake_ts)

            # Simulate a long-running game: don't touch the worker for
            # multiple heartbeat intervals. The background thread should
            # still push last-seen forward on the coordinator.
            time.sleep(1.5)

            updated_ts = self.coord._workers_last_seen.get("hb-w")
            self.assertGreater(
                updated_ts, handshake_ts,
                "background heartbeat thread never updated last-seen on the coordinator",
            )
        finally:
            w._close()

    def test_monitor_recovers_from_wedged_req_socket(self):
        """A timed-out checkpoint fetch must not wedge the monitor's REQ socket.

        A REQ socket left mid-exchange (sent, never received) is stuck in its
        must-receive FSM state; the next send raises EFSM ("operation cannot be
        accomplished in current state") and every later fetch fails forever.
        Regression test for `MonitorClient._request` not rebuilding the socket
        on error the way the worker already does.
        """
        from dist_monitor import MonitorClient

        meta = self.coord._trainer._save_checkpoint(3, {})
        name = meta["name"]

        mon_tmp = tempfile.mkdtemp(prefix="fab-mon-")
        client = MonitorClient(
            coord_url="tcp://127.0.0.1",
            token=self.token,
            pub_port=self.pub_port,
            rep_port=self.rep_port,
            checkpoint_dir=mon_tmp,
            index_path=os.path.join(mon_tmp, "index.json"),
        )
        client._req = client._make_req_socket()
        try:
            # Wedge the socket: send a request but never read the reply, leaving
            # it in the must-receive state that makes the next send raise EFSM.
            client._req.send(proto.make_envelope(
                {"name": name}, self.token, proto.KIND_CHECKPOINT_FETCH))
            time.sleep(0.2)  # let the coordinator's reply land in the rx buffer

            # First fetch hits the wedge: EFSM is caught, the socket rebuilt.
            self.assertFalse(client._fetch_blob(name))
            self.assertFalse(
                os.path.isfile(os.path.join(mon_tmp, f"{name}.pt")))

            # The rebuilt socket must work — the wedge must not be permanent.
            self.assertTrue(client._fetch_blob(name))
            self.assertTrue(
                os.path.isfile(os.path.join(mon_tmp, f"{name}.pt")))
        finally:
            if client._req is not None:
                client._req.close(linger=0)
            shutil.rmtree(mon_tmp, ignore_errors=True)

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

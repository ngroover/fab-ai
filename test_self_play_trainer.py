"""
test_self_play_trainer.py — smoke test for the AlphaZero self-play backend.

Runs a tiny end-to-end training loop and verifies:
- the replay buffer fills with at least one transition
- a checkpoint file appears on disk and is recorded in the index
- network parameters change after the gradient steps run
"""

from __future__ import annotations

import copy
import os
import shutil
import tempfile
import unittest

import torch

import self_play_trainer as spt
from self_play_trainer import SelfPlayTrainer, TrainerConfig


class SelfPlayTrainerSmokeTest(unittest.TestCase):
    """Run a 1-iteration trainer with minimal sizes and check invariants."""

    def setUp(self):
        self._orig_dir = spt.CHECKPOINT_DIR
        self._orig_index = spt.INDEX_PATH
        self._tmp = tempfile.mkdtemp(prefix="fab-az-test-")
        spt.CHECKPOINT_DIR = self._tmp
        spt.INDEX_PATH = os.path.join(self._tmp, "index.json")

    def tearDown(self):
        spt.CHECKPOINT_DIR = self._orig_dir
        spt.INDEX_PATH = self._orig_index
        shutil.rmtree(self._tmp, ignore_errors=True)

    def test_one_iteration_end_to_end(self):
        cfg = TrainerConfig(
            games_per_iter=1,
            steps_per_iter=2,
            total_iters=1,
            n_simulations=2,
            batch_size=4,
            eval_games=1,
            eval_every=1,
            buffer_size=200,
            seed=42,
            run_name="smoke",
        )

        # Capture logs and metric callbacks.
        logs = []
        metrics = []
        ckpts = []
        trainer = SelfPlayTrainer(
            cfg,
            callbacks={
                "on_log": logs.append,
                "on_metrics": metrics.append,
                "on_checkpoint": ckpts.append,
            },
        )

        before = copy.deepcopy(trainer.net.state_dict())
        trainer.run()
        after = trainer.net.state_dict()

        # 1) Buffer filled with at least one transition.
        self.assertGreater(len(trainer.buffer), 0,
                           "Replay buffer should contain at least one transition")

        # 2) A checkpoint was emitted to disk and to the index.
        self.assertEqual(len(ckpts), 1, "Expected exactly one checkpoint")
        ckpt_name = ckpts[0]["name"]
        ckpt_path = os.path.join(self._tmp, f"{ckpt_name}.pt")
        self.assertTrue(os.path.isfile(ckpt_path),
                        f"Checkpoint file not written at {ckpt_path}")

        idx_path = spt.INDEX_PATH
        self.assertTrue(os.path.isfile(idx_path), "Checkpoint index missing")
        self.assertEqual(len(spt.list_checkpoints()), 1)

        # 3) At least one network parameter changed value after the grad steps.
        any_changed = False
        for k in before:
            if not torch.equal(before[k], after[k]):
                any_changed = True
                break
        self.assertTrue(
            any_changed,
            "Expected at least one network parameter to change after training",
        )

        # 4) Metrics callback was invoked with the expected keys.
        self.assertEqual(len(metrics), 1)
        m = metrics[0]
        for k in ("iter", "games", "grad_steps", "policy_loss", "value_loss"):
            self.assertIn(k, m)

    def test_delete_and_rename_checkpoint(self):
        cfg = TrainerConfig(
            games_per_iter=1,
            steps_per_iter=1,
            total_iters=1,
            n_simulations=2,
            batch_size=4,
            eval_games=0,
            eval_every=1,
            buffer_size=200,
            seed=7,
            run_name="rename-test",
        )
        SelfPlayTrainer(cfg, callbacks={"on_log": lambda _: None}).run()

        names = [c["name"] for c in spt.list_checkpoints()]
        self.assertEqual(len(names), 1)
        old_name = names[0]

        self.assertTrue(spt.rename_checkpoint(old_name, "renamed-ckpt"))
        self.assertIn("renamed-ckpt",
                      [c["name"] for c in spt.list_checkpoints()])
        self.assertTrue(os.path.isfile(
            os.path.join(self._tmp, "renamed-ckpt.pt")
        ))

        self.assertTrue(spt.delete_checkpoint("renamed-ckpt"))
        self.assertEqual(spt.list_checkpoints(), [])
        self.assertFalse(os.path.isfile(
            os.path.join(self._tmp, "renamed-ckpt.pt")
        ))


if __name__ == "__main__":
    unittest.main()

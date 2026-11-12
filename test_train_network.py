"""
test_train_network.py — tests for the trainer that fits the policy/value
network to the Rust environment's self-play records.

The records here are written by this file, in the format
environment/src/self_play.rs documents, so the tests need no Rust build.
"""

from __future__ import annotations

import os
import struct
import tempfile
import unittest

import numpy as np
import torch

import train_network as tn

OBS_SIZE = 40
ACTION_SIZE = 12


def write_records(path, records, obs_size=OBS_SIZE, action_size=ACTION_SIZE):
    """Write `records` — dicts with game, ply, player, outcome, root_value,
    obs [(index, value)], visits [(action, visits)] — as a self-play file."""
    with open(path, "wb") as f:
        f.write(b"FABSP001" + struct.pack("<II", obs_size, action_size))
        for r in records:
            obs, visits = r["obs"], r["visits"]
            f.write(struct.pack("<IHBBffH", r["game"], r["ply"], r["player"], len(visits),
                                r["outcome"], r["root_value"], len(obs)))
            f.write(struct.pack(f"<{len(obs)}H", *(i for i, _ in obs)))
            f.write(struct.pack(f"<{len(obs)}f", *(v for _, v in obs)))
            f.write(struct.pack(f"<{len(visits)}H", *(a for a, _ in visits)))
            f.write(struct.pack(f"<{len(visits)}I", *(n for _, n in visits)))


def learnable_records(games=40, plies=12, seed=0):
    """Records with something to learn: observation entry 0 says who is
    winning (the outcome), and entries 1..4 name the action the search liked."""
    rng = np.random.default_rng(seed)
    records = []
    for game in range(games):
        for ply in range(plies):
            outcome = float(rng.choice([-1.0, 1.0]))
            best = int(rng.integers(0, 4))
            legal = [0, 1, 2, 3, 4 + int(rng.integers(0, ACTION_SIZE - 4))]
            obs = [(0, 0.5 + 0.5 * outcome), (1 + best, 1.0)]
            # Some noise entries the network has to learn to ignore.
            obs += [(int(i), 1.0) for i in sorted(rng.choice(np.arange(8, OBS_SIZE), 3, replace=False))]
            visits = [(a, 90 if a == best else 3) for a in legal]
            records.append(dict(game=game, ply=ply, player=ply % 2, outcome=outcome,
                                root_value=0.5 * outcome, obs=obs, visits=visits))
    return records


class TrainNetworkTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory(prefix="fab-train-test-")
        self.dir = self._tmp.name
        torch.manual_seed(0)

    def tearDown(self):
        self._tmp.cleanup()

    def path(self, name):
        return os.path.join(self.dir, name)

    def test_records_load_as_written(self):
        written = [
            dict(game=0, ply=0, player=0, outcome=1.0, root_value=0.25,
                 obs=[(1, 0.5), (7, 2.0), (39, 1.0)], visits=[(0, 10), (5, 30)]),
            dict(game=0, ply=1, player=1, outcome=-1.0, root_value=-0.5,
                 obs=[(3, 1.0)], visits=[(2, 1), (3, 0), (11, 3)]),
        ]
        write_records(self.path("a.bin"), written)
        records = tn.load_records([self.path("a.bin")])

        self.assertEqual((len(records), records.obs_size, records.action_size), (2, OBS_SIZE, ACTION_SIZE))
        obs, mask, policy, outcome, root_value = records.batch(np.array([1, 0]))
        self.assertEqual(obs.shape, (2, OBS_SIZE))
        # Row 0 of the batch is record 1.
        expected = torch.zeros(OBS_SIZE)
        expected[3] = 1.0
        self.assertTrue(torch.equal(obs[0], expected))
        self.assertEqual(obs[1].nonzero().flatten().tolist(), [1, 7, 39])
        self.assertEqual(obs[1, 7].item(), 2.0)
        self.assertEqual(mask[0].nonzero().flatten().tolist(), [2, 3, 11])
        self.assertEqual(mask[1].nonzero().flatten().tolist(), [0, 5])
        # The policy target is the share of visits; a legal action with no
        # visits is legal all the same.
        self.assertEqual(policy[0].tolist()[2:4], [0.25, 0.0])
        self.assertAlmostEqual(policy[0, 11].item(), 0.75)
        self.assertEqual(policy[1].tolist()[0], 0.25)
        self.assertEqual(outcome.tolist(), [-1.0, 1.0])
        self.assertEqual(root_value.tolist(), [-0.5, 0.25])
        self.assertEqual(records.root_value.tolist(), [0.25, -0.5])
        self.assertEqual(records.player.tolist(), [0, 1])

    def test_games_stay_distinct_across_files(self):
        records = learnable_records(games=3, plies=2)
        write_records(self.path("a.bin"), records)
        write_records(self.path("b.bin"), records)
        loaded = tn.load_records([self.path("a.bin"), self.path("b.bin")])
        self.assertEqual(len(loaded), 12)
        self.assertEqual(loaded.game.tolist(), [0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5])

    def test_bad_files_are_refused(self):
        write_records(self.path("a.bin"), learnable_records(games=1, plies=3))
        with open(self.path("a.bin"), "rb") as f:
            data = f.read()
        with open(self.path("short.bin"), "wb") as f:
            f.write(data[:-5])
        with open(self.path("magic.bin"), "wb") as f:
            f.write(b"NOTAFILE" + data[8:])
        write_records(self.path("other.bin"), [], obs_size=OBS_SIZE + 1)
        for bad in (["short.bin"], ["magic.bin"], ["a.bin", "other.bin"]):
            with self.assertRaises(ValueError, msg=str(bad)):
                tn.load_records([self.path(name) for name in bad])

    def test_whole_games_are_held_out(self):
        write_records(self.path("a.bin"), learnable_records(games=20, plies=3))
        records = tn.load_records([self.path("a.bin")])
        train_rows, validation_rows = tn.split_by_game(records, 5)
        self.assertEqual(len(train_rows) + len(validation_rows), len(records))
        self.assertEqual(len(validation_rows), 4 * 3)
        self.assertFalse(set(records.game[train_rows]) & set(records.game[validation_rows]))
        everything, nothing = tn.split_by_game(records, 0)
        self.assertEqual((len(everything), len(nothing)), (len(records), 0))

    def test_policy_loss_ignores_illegal_actions(self):
        write_records(self.path("a.bin"), learnable_records(games=2, plies=4))
        records = tn.load_records([self.path("a.bin")])
        batch = records.batch(np.arange(len(records)))[:4]
        mask = batch[1]
        net = tn.PolicyValueNet(OBS_SIZE, ACTION_SIZE, [16])
        before = tn.losses(net, *batch)[0]
        self.assertTrue(torch.isfinite(before).all())

        # Push the illegal slots' logits around: nothing may change.
        with torch.no_grad():
            illegal_everywhere = ~mask.any(dim=0)
            self.assertTrue(illegal_everywhere.any())
            net.policy_head.bias[illegal_everywhere] += 50.0
        after = tn.losses(net, *batch)[0]
        self.assertTrue(torch.allclose(before, after, atol=1e-5))

    def test_training_learns_the_policy_and_the_value(self):
        write_records(self.path("a.bin"), learnable_records(games=60, plies=12))
        records = tn.load_records([self.path("a.bin")])
        train_rows, validation_rows = tn.split_by_game(records, 6)
        net = tn.PolicyValueNet(OBS_SIZE, ACTION_SIZE, [32, 32])
        untrained = tn.baselines(records, validation_rows)
        floor = tn.target_entropy(records, validation_rows)

        history = tn.train(net, records, train_rows, validation_rows, epochs=40,
                           batch_size=64, lr=3e-3, log=lambda _: None)
        _, held_out = history[-1]

        self.assertGreater(untrained.policy_loss, floor + 0.5)
        self.assertLess(held_out.policy_loss, floor + 0.1)
        self.assertLess(held_out.value_loss, 0.1 * untrained.value_loss)
        self.assertGreater(held_out.agreement, 0.95)
        self.assertLess(untrained.agreement, 0.3)

    def test_the_best_epoch_is_kept(self):
        """Training past the best validation score must not cost anything:
        the network ends up with that epoch's weights."""
        # Too few games to generalize from, so validation soon gets worse.
        write_records(self.path("a.bin"), learnable_records(games=8, plies=6))
        records = tn.load_records([self.path("a.bin")])
        train_rows, validation_rows = tn.split_by_game(records, 2)
        net = tn.PolicyValueNet(OBS_SIZE, ACTION_SIZE, [64])
        history = tn.train(net, records, train_rows, validation_rows, epochs=60,
                           batch_size=8, lr=1e-2, weight_decay=0.0, log=lambda _: None)
        scores = [held_out.policy_loss + held_out.value_loss for _, held_out in history]
        kept = tn.evaluate(net, records, validation_rows)
        self.assertAlmostEqual(kept.policy_loss + kept.value_loss, min(scores), places=5)
        self.assertLess(min(scores), scores[-1])

    def test_value_mix_trains_towards_the_search_estimate(self):
        """Fully mixed, the value learns root_value (half the outcome here)
        rather than the outcome itself."""
        write_records(self.path("a.bin"), learnable_records(games=60, plies=12))
        records = tn.load_records([self.path("a.bin")])
        rows, _ = tn.split_by_game(records, 0)
        net = tn.PolicyValueNet(OBS_SIZE, ACTION_SIZE, [32, 32])
        tn.train(net, records, rows, rows[:0], epochs=40, batch_size=64, lr=3e-3,
                 value_mix=1.0, log=lambda _: None)
        obs, _, _, outcome, root_value = records.batch(rows)
        with torch.no_grad():
            value = net(obs)[1]
        self.assertLess(((value - root_value) ** 2).mean().item(), 0.01)
        self.assertGreater(((value - outcome) ** 2).mean().item(), 0.15)

    def test_weights_survive_a_round_trip(self):
        net = tn.PolicyValueNet(OBS_SIZE, ACTION_SIZE, [24, 16])
        tn.save_network(net, self.path("net.bin"))
        loaded = tn.load_network(self.path("net.bin"))
        self.assertEqual(loaded.hidden, (24, 16))
        obs = torch.rand(5, OBS_SIZE)
        for a, b in zip(net(obs), loaded(obs)):
            self.assertTrue(torch.equal(a, b))

        # The layout the Rust side reads: header, then weight and bias of the
        # hidden layers, the policy head and the value head.
        with open(self.path("net.bin"), "rb") as f:
            data = f.read()
        self.assertEqual(data[:8], b"FABNN001")
        self.assertEqual(struct.unpack_from("<IIIII", data, 8), (OBS_SIZE, ACTION_SIZE, 2, 24, 16))
        first = np.frombuffer(data, "<f4", 24 * OBS_SIZE, 28).reshape(24, OBS_SIZE)
        self.assertTrue(np.array_equal(first, net.trunk[0].weight.detach().numpy()))
        value_bias = np.frombuffer(data, "<f4", 1, len(data) - 4)
        self.assertEqual(value_bias[0], net.value_head.bias.item())
        self.assertEqual(len(data), 28 + 4 * sum(p.numel() for p in net.parameters()))

        with open(self.path("cut.bin"), "wb") as f:
            f.write(data[:-4])
        with self.assertRaises(ValueError):
            tn.load_network(self.path("cut.bin"))

    def test_command_line_trains_and_continues(self):
        write_records(self.path("a.bin"), learnable_records(games=20, plies=6))
        common = [self.path("a.bin"), "--epochs", "2", "--hidden", "16", "--batch", "32"]
        tn.main([*common, "--out", self.path("first.bin")])
        first = tn.load_network(self.path("first.bin"))
        self.assertEqual((first.obs_size, first.action_size, first.hidden), (OBS_SIZE, ACTION_SIZE, (16,)))

        # Continuing takes its shape from the weights it starts from.
        tn.main([self.path("a.bin"), "--epochs", "1", "--hidden", "64", "--init", self.path("first.bin"),
                 "--out", self.path("second.bin")])
        second = tn.load_network(self.path("second.bin"))
        self.assertEqual(second.hidden, (16,))
        self.assertFalse(torch.equal(first.trunk[0].weight, second.trunk[0].weight))


if __name__ == "__main__":
    unittest.main()

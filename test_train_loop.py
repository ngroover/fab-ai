"""
test_train_loop.py — tests for the self-play / train / evaluate loop.

The Rust environment is replaced by a small stand-in script that writes a
valid self-play file and prints a match result, so the tests need no Rust
build and run in seconds. It logs every call, which is how the tests check
what the loop asked for.
"""

from __future__ import annotations

import json
import os
import stat
import sys
import tempfile
import unittest

import train_loop as tl
import train_network as tn
from test_train_network import ACTION_SIZE, OBS_SIZE

# Stands in for `environment` (and imports nothing heavy, since the loop
# starts it many times): `selfplay` writes records, `match` prints a
# RESULT line (A scoring 60%, or whatever FAKE_SCORE says). Every call's
# arguments are appended to calls.jsonl beside it.
FAKE_ENVIRONMENT = r'''#!{python}
import json, os, random, struct, sys

def write_records(path, games, seed):
    """Four decisions per game, in environment/src/self_play.rs's format."""
    rng = random.Random(seed)
    with open(path, "wb") as f:
        f.write(b"FABSP001" + struct.pack("<II", {obs_size}, {action_size}))
        for game in range(games):
            outcome = rng.choice([-1.0, 1.0])
            for ply in range(4):
                best = rng.randrange(4)
                f.write(struct.pack("<IHBBffH", game, ply, ply % 2, 4, outcome, 0.5 * outcome, 2))
                f.write(struct.pack("<2H", 0, 1 + best) + struct.pack("<2f", 0.5 + 0.5 * outcome, 1.0))
                f.write(struct.pack("<4H", 0, 1, 2, 3))
                f.write(struct.pack("<4I", *(90 if a == best else 3 for a in range(4))))

args = sys.argv[1:]
command, options = args[0], dict(zip(args[1::2], args[2::2]))
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "calls.jsonl"), "a") as f:
    f.write(json.dumps([command, options]) + "\n")

if command == "selfplay":
    if os.environ.get("FAKE_FAIL") == "selfplay":
        sys.exit("self-play blew up")
    network = options.get("--network")
    assert network is None or os.path.exists(network), network
    write_records(options["--out"], int(options["--games"]), int(options["--seed"]))
elif command == "match":
    for agent in (options["--a"], options["--b"]):
        assert not agent.startswith("net:") or os.path.exists(agent[4:]), agent
    games = int(options["--games"])
    wins = round(games * float(os.environ.get("FAKE_SCORE", "0.6")))
    half = games // 2
    print("Games played: ...")
    print(f"RESULT games={{games}} a_wins={{wins}} b_wins={{games - wins}} draws=0 "
          f"a_score={{wins / games:.4f}} a_rhinar_wins={{min(wins, half)}} a_rhinar_games={{half}} "
          f"a_dorinthea_wins={{wins - min(wins, half)}} a_dorinthea_games={{games - half}}")
else:
    sys.exit(f"unknown command {{command}}")
'''


class TrainLoopTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory(prefix="fab-loop-test-")
        self.dir = os.path.join(self._tmp.name, "run")
        self.binary = os.path.join(self._tmp.name, "environment")
        with open(self.binary, "w") as f:
            f.write(FAKE_ENVIRONMENT.format(python=sys.executable, obs_size=OBS_SIZE, action_size=ACTION_SIZE))
        os.chmod(self.binary, os.stat(self.binary).st_mode | stat.S_IXUSR)

    def tearDown(self):
        self._tmp.cleanup()
        os.environ.pop("FAKE_FAIL", None)
        os.environ.pop("FAKE_SCORE", None)

    def loop(self, rounds, *extra):
        tl.main(["--dir", self.dir, "--rounds", str(rounds), "--games", "20", "--epochs", "1",
                 "--hidden", "8", "--batch", "32", "--eval-games", "10", "--threads", "2",
                 "--window", "2", "--binary", self.binary, *extra])

    def net_bytes(self, round_):
        with open(tl.net_path(self.dir, round_), "rb") as f:
            return f.read()

    def calls(self):
        with open(os.path.join(self._tmp.name, "calls.jsonl")) as f:
            return [json.loads(line) for line in f]

    def test_parse_result_reads_the_last_result_line(self):
        output = "Games played: 4\nRESULT games=2 a_score=0.1\nnoise\nRESULT games=4 a_wins=3 a_score=0.7500\n"
        self.assertEqual(tl.parse_result(output), {"games": 4.0, "a_wins": 3.0, "a_score": 0.75})
        with self.assertRaises(RuntimeError):
            tl.parse_result("Games played: 4\n")

    def test_rounds_play_train_and_evaluate(self):
        self.loop(3)

        for round_ in range(3):
            self.assertTrue(os.path.exists(tl.records_path(self.dir, round_)))
            net = tn.load_network(tl.net_path(self.dir, round_))
            self.assertEqual((net.obs_size, net.action_size, net.hidden), (OBS_SIZE, ACTION_SIZE, (8,)))
        self.assertFalse([name for name in os.listdir(self.dir) if name.endswith(".partial")])

        # Self-play: random playouts first, then each round's predecessor,
        # every round on a seed of its own.
        plays = [options for command, options in self.calls() if command == "selfplay"]
        self.assertEqual([p.get("--network") for p in plays],
                         [None, tl.net_path(self.dir, 0), tl.net_path(self.dir, 1)])
        self.assertEqual(len({p["--seed"] for p in plays}), 3)

        # Evaluation: always the anchor; the previous network from round 1;
        # the first network too from round 2. The new network is always A.
        matches = [options for command, options in self.calls() if command == "match"]
        self.assertEqual(
            [(m["--a"], m["--b"]) for m in matches],
            [(f"net:{tl.net_path(self.dir, 0)}", "ismcts"),
             (f"net:{tl.net_path(self.dir, 1)}", "ismcts"),
             (f"net:{tl.net_path(self.dir, 1)}", f"net:{tl.net_path(self.dir, 0)}"),
             (f"net:{tl.net_path(self.dir, 2)}", "ismcts"),
             (f"net:{tl.net_path(self.dir, 2)}", f"net:{tl.net_path(self.dir, 1)}"),
             (f"net:{tl.net_path(self.dir, 2)}", f"net:{tl.net_path(self.dir, 0)}")])
        self.assertEqual(len({m["--seed"] for m in matches}), 6)
        self.assertFalse({m["--seed"] for m in matches} & {p["--seed"] for p in plays})

        results = tl.load_results(self.dir)
        self.assertEqual([r["round"] for r in results], [0, 1, 2])
        self.assertEqual([sorted(r["matches"]) for r in results],
                         [["ismcts"], ["ismcts", "previous"], ["first", "ismcts", "previous"]])
        self.assertEqual(results[2]["matches"]["ismcts"],
                         {"score": 0.6, "games": 10, "as_rhinar": "5/5", "as_dorinthea": "1/5"})
        # Trained on the last two rounds only (--window 2): 20 games of 4
        # decisions each.
        self.assertEqual([r["training"]["rounds_trained_on"] for r in results], [[0, 0], [0, 1], [1, 2]])
        self.assertEqual([r["training"]["records"] for r in results], [80, 160, 160])

    def test_each_round_carries_on_from_the_last_networks_weights(self):
        """With nothing to learn from (a learning rate of zero), every round's
        network is the first round's: later rounds start from their
        predecessor, not afresh."""
        self.loop(2, "--lr", "0")
        first = tn.load_network(tl.net_path(self.dir, 0))
        second = tn.load_network(tl.net_path(self.dir, 1))
        for a, b in zip(first.parameters(), second.parameters()):
            self.assertTrue((a == b).all())

    def test_a_finished_run_resumes_where_it_stopped(self):
        self.loop(2)
        before = len(self.calls())
        first_net = self.net_bytes(0)

        self.loop(2)  # nothing left to do
        self.assertEqual(len(self.calls()), before)

        self.loop(3)  # one more round
        new_calls = self.calls()[before:]
        self.assertEqual([command for command, _ in new_calls], ["selfplay", "match", "match", "match"])
        self.assertEqual(new_calls[0][1]["--network"], tl.net_path(self.dir, 1))
        self.assertEqual([r["round"] for r in tl.load_results(self.dir)], [0, 1, 2])
        self.assertEqual(self.net_bytes(0), first_net)

    def test_a_failed_round_is_reported_and_redone(self):
        self.loop(1)
        os.environ["FAKE_FAIL"] = "selfplay"
        with self.assertRaises(RuntimeError) as failure:
            self.loop(2)
        self.assertIn("self-play blew up", str(failure.exception))
        self.assertEqual(len(tl.load_results(self.dir)), 1)
        self.assertFalse(os.path.exists(tl.records_path(self.dir, 1)))

        del os.environ["FAKE_FAIL"]
        self.loop(2)
        self.assertEqual([r["round"] for r in tl.load_results(self.dir)], [0, 1])

    def test_a_missing_binary_is_explained(self):
        with self.assertRaises(SystemExit) as failure:
            tl.main(["--dir", self.dir, "--binary", os.path.join(self._tmp.name, "nowhere")])
        self.assertIn("cargo build --release", str(failure.exception))


if __name__ == "__main__":
    unittest.main()

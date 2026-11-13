"""
train_loop.py — the AlphaZero loop: self-play, train, evaluate, repeat.

Each round

1. **plays** self-play games in the Rust environment with the newest network
   (`environment selfplay`; the first round, with no network yet, uses random
   playouts),
2. **trains** the network on the records of the last few rounds
   (`train_network.py`), carrying on from the previous round's weights, and
3. **evaluates** the new network in matches where each side plays both heroes
   (`environment match`): against ISMCTS, a yardstick that never changes;
   against the previous round's network; and against the first round's, to
   catch a network that beats its predecessor while forgetting older lessons.

    python3 train_loop.py --dir runs/first --rounds 5

Everything lands in `--dir`: `records_NNN.bin` and `net_NNN.bin` per round,
and `results.jsonl`, one line per finished round with its training metrics
and match scores. Running the same command again carries on after the last
finished round, so a run can be stopped and resumed, or extended with a
larger `--rounds`.

The evaluation matches are only measurements: their games are not trained on,
and the newest network is used for self-play whatever it scored.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from typing import Dict, List, Optional, Sequence

import numpy as np
import torch

import train_network as tn

REPO = os.path.dirname(os.path.abspath(__file__))
DEFAULT_BINARY = os.path.join(REPO, "environment", "target", "release", "environment")


def records_path(directory: str, round_: int) -> str:
    return os.path.join(directory, f"records_{round_:03d}.bin")


def net_path(directory: str, round_: int) -> str:
    return os.path.join(directory, f"net_{round_:03d}.bin")


def results_path(directory: str) -> str:
    return os.path.join(directory, "results.jsonl")


def load_results(directory: str) -> List[dict]:
    """The finished rounds' results, in order."""
    path = results_path(directory)
    if not os.path.exists(path):
        return []
    with open(path) as f:
        return [json.loads(line) for line in f if line.strip()]


def run(command: Sequence[str]) -> str:
    """Run a command of the Rust environment and return what it printed."""
    done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"{' '.join(command)} failed:\n{done.stdout}{done.stderr}")
    return done.stdout


def parse_result(output: str) -> Dict[str, float]:
    """The `RESULT key=value ...` line a match ends with, as numbers."""
    for line in reversed(output.splitlines()):
        if line.startswith("RESULT "):
            return {key: float(value) for key, value in (pair.split("=") for pair in line.split()[1:])}
    raise RuntimeError(f"no RESULT line in the match output:\n{output}")


def self_play(binary: str, out: str, network: Optional[str], games: int, iterations: int,
              threads: int, seed: int) -> None:
    command = [binary, "selfplay", "--out", out, "--games", str(games), "--iterations", str(iterations),
               "--threads", str(threads), "--seed", str(seed)]
    if network:
        command += ["--network", network]
    run(command)


def play_match(binary: str, a: str, b: str, games: int, iterations: int, threads: int,
               seed: int) -> Dict[str, float]:
    """A match between agents `a` and `b`; `a_score` is A's share of the
    games, a draw counting half."""
    return parse_result(run([binary, "match", "--a", a, "--b", b, "--games", str(games),
                             "--iterations", str(iterations), "--threads", str(threads),
                             "--seed", str(seed)]))


def train_round(directory: str, round_: int, args: argparse.Namespace) -> dict:
    """Train round `round_`'s network on the last `--window` rounds' records
    and write it out. Returns the held-out metrics of the weights kept."""
    first = max(0, round_ - args.window + 1)
    records = tn.load_records([records_path(directory, r) for r in range(first, round_ + 1)])
    train_rows, validation_rows = tn.split_by_game(records, args.validation_every)
    if round_ > 0:
        net = tn.load_network(net_path(directory, round_ - 1))
    else:
        net = tn.PolicyValueNet(records.obs_size, records.action_size, args.hidden)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    tn.train(net, records, train_rows, validation_rows, epochs=args.epochs, batch_size=args.batch,
             lr=args.lr, weight_decay=args.weight_decay, value_mix=args.value_mix,
             seed=args.seed + round_, device=device, log=lambda _: None)
    kept = tn.evaluate(net, records, validation_rows if len(validation_rows) else train_rows, device=device)
    tn.save_network(net.cpu(), net_path(directory, round_))
    return {
        "records": len(records),
        "rounds_trained_on": [first, round_],
        "policy_loss": round(kept.policy_loss, 4),
        "value_loss": round(kept.value_loss, 4),
        "agreement": round(kept.agreement, 4),
    }


def evaluate_round(directory: str, round_: int, args: argparse.Namespace) -> Dict[str, dict]:
    """Round `round_`'s network against each yardstick."""
    new = f"net:{net_path(directory, round_)}"
    opponents = {args.anchor: args.anchor}
    if round_ >= 1:
        opponents["previous"] = f"net:{net_path(directory, round_ - 1)}"
    if round_ >= 2:
        opponents["first"] = f"net:{net_path(directory, 0)}"
    matches = {}
    for i, (name, opponent) in enumerate(opponents.items()):
        # A seed of its own per round and opponent, apart from self-play's.
        seed = args.seed + 500_000_000 + 1_000_003 * round_ + 7919 * i
        result = play_match(args.binary, new, opponent, args.eval_games, args.eval_iterations,
                            args.threads, seed)
        matches[name] = {
            "score": result["a_score"],
            "games": int(result["games"]),
            "as_rhinar": f"{int(result['a_rhinar_wins'])}/{int(result['a_rhinar_games'])}",
            "as_dorinthea": f"{int(result['a_dorinthea_wins'])}/{int(result['a_dorinthea_games'])}",
        }
    return matches


def run_round(directory: str, round_: int, args: argparse.Namespace, log=print) -> dict:
    """Play, train and evaluate round `round_`, and record its result."""
    started = time.time()
    previous = net_path(directory, round_ - 1) if round_ > 0 else None
    log(f"round {round_}: self-play, {args.games} games with "
        f"{'the round ' + str(round_ - 1) + ' network' if previous else 'random playouts'}")
    # Written under another name first, so a run stopped mid-way never
    # leaves a half-written file where a finished one is expected.
    partial = records_path(directory, round_) + ".partial"
    self_play(args.binary, partial, previous, args.games, args.iterations, args.threads,
              args.seed + 1_000_003 * round_)
    os.replace(partial, records_path(directory, round_))
    played = time.time()

    training = train_round(directory, round_, args)
    log(f"round {round_}: trained on {training['records']} records — held out: "
        f"policy {training['policy_loss']:.4f}  value {training['value_loss']:.4f}  "
        f"top-1 {100 * training['agreement']:.1f}%")
    trained = time.time()

    matches = evaluate_round(directory, round_, args)
    for name, match in matches.items():
        log(f"round {round_}: vs {name:8s} {100 * match['score']:5.1f}% of {match['games']} games  "
            f"(as Rhinar {match['as_rhinar']}, as Dorinthea {match['as_dorinthea']})")

    result = {
        "round": round_,
        "self_play_network": round_ - 1 if previous else None,
        "games": args.games,
        "iterations": args.iterations,
        "training": training,
        "matches": matches,
        "seconds": {
            "self_play": round(played - started, 1),
            "training": round(trained - played, 1),
            "evaluation": round(time.time() - trained, 1),
        },
    }
    with open(results_path(directory), "a") as f:
        f.write(json.dumps(result) + "\n")
    return result


def main(argv: Optional[Sequence[str]] = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--dir", required=True, help="where the run's files go")
    parser.add_argument("--rounds", type=int, default=5, help="rounds the run should have in all")
    parser.add_argument("--games", type=int, default=800, help="self-play games per round")
    parser.add_argument("--iterations", type=int, default=400, help="search iterations per self-play decision")
    parser.add_argument("--window", type=int, default=3, help="train on this many of the latest rounds")
    parser.add_argument("--epochs", type=int, default=4)
    parser.add_argument("--batch", type=int, default=256)
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--weight-decay", type=float, default=1e-4)
    parser.add_argument("--value-mix", type=float, default=0.5)
    parser.add_argument("--hidden", type=int, nargs="+", default=[256, 256])
    parser.add_argument("--validation-every", type=int, default=10)
    parser.add_argument("--eval-games", type=int, default=200, help="games per evaluation match")
    parser.add_argument("--eval-iterations", type=int, default=400)
    parser.add_argument("--anchor", default="ismcts",
                        help="the fixed yardstick every round is measured against: ismcts, puct or random")
    parser.add_argument("--threads", type=int, default=os.cpu_count() or 1)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--binary", default=DEFAULT_BINARY,
                        help="the Rust environment (build it with `cargo build --release`)")
    args = parser.parse_args(argv)

    if not os.path.exists(args.binary):
        raise SystemExit(f"{args.binary} not found: run `cargo build --release` in environment/ first")
    os.makedirs(args.dir, exist_ok=True)
    torch.manual_seed(args.seed)
    np.random.seed(args.seed)

    done = len(load_results(args.dir))
    if done:
        print(f"{done} rounds already finished in {args.dir}; carrying on")
    for round_ in range(done, args.rounds):
        run_round(args.dir, round_, args)

    print(f"\n{'round':>5} {'vs ' + args.anchor:>10} {'vs previous':>12} {'vs first':>9} {'value loss':>11}")
    for result in load_results(args.dir):
        def score(name: str) -> str:
            match = result["matches"].get(name)
            return f"{100 * match['score']:.1f}%" if match else "-"
        print(f"{result['round']:>5} {score(args.anchor):>10} {score('previous'):>12} {score('first'):>9} "
              f"{result['training']['value_loss']:>11.4f}")


if __name__ == "__main__":
    main(sys.argv[1:])

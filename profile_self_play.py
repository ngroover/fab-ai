"""
profile_self_play.py — Profile SelfPlayTrainer to find slow spots.

Runs a short training session under cProfile and also records wall-clock
time spent in each trainer phase (self-play / training / eval). Writes a
.prof file you can open in snakeviz:

    python profile_self_play.py --iters 1 --games 2 --steps 4 --sims 16
    snakeviz profile_out.prof

Use --top to control how many hot functions are printed, and --filter to
keep only functions whose path matches a substring (e.g. --filter fab-ai
to hide stdlib/torch frames).
"""

from __future__ import annotations

import argparse
import cProfile
import io
import pstats
import time
from collections import defaultdict
from typing import Dict, List

from self_play_trainer import SelfPlayTrainer, TrainerConfig


class PhaseTimer:
    """Tracks wall-clock time spent in each trainer status."""

    def __init__(self) -> None:
        self.totals: Dict[str, float] = defaultdict(float)
        self._current: str = "idle"
        self._t0: float = time.perf_counter()

    def on_status(self, status: str) -> None:
        now = time.perf_counter()
        self.totals[self._current] += now - self._t0
        self._current = status
        self._t0 = now

    def finish(self) -> None:
        now = time.perf_counter()
        self.totals[self._current] += now - self._t0
        self._current = "idle"
        self._t0 = now

    def report(self) -> str:
        total = sum(self.totals.values()) or 1.0
        lines = ["", "── Wall-clock time per phase ──"]
        for name, secs in sorted(self.totals.items(), key=lambda kv: -kv[1]):
            pct = 100.0 * secs / total
            lines.append(f"  {name:>12s}  {secs:8.2f}s  ({pct:5.1f}%)")
        lines.append(f"  {'TOTAL':>12s}  {total:8.2f}s")
        return "\n".join(lines)


def build_config(args: argparse.Namespace) -> TrainerConfig:
    return TrainerConfig(
        games_per_iter=args.games,
        steps_per_iter=args.steps,
        total_iters=args.iters,
        n_simulations=args.sims,
        batch_size=args.batch,
        lr=args.lr,
        eval_games=args.eval_games,
        eval_every=args.eval_every,
        determinize=not args.no_pimc,
        seed=args.seed,
        run_name=args.run_name or "profile",
    )


def print_pstats(prof: cProfile.Profile, top: int, path_filter: str | None) -> None:
    for sort_key, label in (("cumulative", "cumulative time"), ("tottime", "self time")):
        stream = io.StringIO()
        stats = pstats.Stats(prof, stream=stream).sort_stats(sort_key)
        if path_filter:
            stats.print_stats(path_filter, top)
        else:
            stats.print_stats(top)
        print(f"\n── Top {top} by {label} ──")
        print(stream.getvalue())


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[1])
    # Trainer workload knobs (kept small by default so profiling finishes fast).
    p.add_argument("--iters", type=int, default=1)
    p.add_argument("--games", type=int, default=2)
    p.add_argument("--steps", type=int, default=4)
    p.add_argument("--sims", type=int, default=16)
    p.add_argument("--batch", type=int, default=32)
    p.add_argument("--lr", type=float, default=3e-4)
    p.add_argument("--eval-games", type=int, default=0,
                   help="0 disables eval to focus the profile on self-play/training")
    p.add_argument("--eval-every", type=int, default=1)
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--run-name", type=str, default="")
    p.add_argument("--no-pimc", action="store_true")

    # Profiling output knobs.
    p.add_argument("--out", type=str, default="profile_out.prof",
                   help="Output .prof file (open with snakeviz)")
    p.add_argument("--top", type=int, default=30,
                   help="Rows printed per pstats table")
    p.add_argument("--filter", dest="path_filter", type=str, default=None,
                   help="Restrict pstats to filenames containing this substring "
                        "(e.g. 'fab-ai' to hide stdlib/torch frames)")
    p.add_argument("--quiet", action="store_true",
                   help="Silence the trainer's own log output")
    args = p.parse_args()

    cfg = build_config(args)
    timer = PhaseTimer()
    callbacks = {
        "on_status": timer.on_status,
        "on_log": (lambda msg: None) if args.quiet else (lambda msg: print(msg)),
    }
    trainer = SelfPlayTrainer(cfg, callbacks=callbacks)

    print(f"Profiling {cfg.total_iters} iters × {cfg.games_per_iter} games "
          f"× {cfg.n_simulations} sims (steps={cfg.steps_per_iter})")
    wall_t0 = time.perf_counter()
    prof = cProfile.Profile()
    prof.enable()
    try:
        trainer.run()
    finally:
        prof.disable()
        timer.finish()
    wall = time.perf_counter() - wall_t0

    prof.dump_stats(args.out)
    print(f"\nWrote {args.out}  →  view with:  snakeviz {args.out}")
    print(f"Total wall time: {wall:.2f}s")
    print(timer.report())
    print_pstats(prof, args.top, args.path_filter)


if __name__ == "__main__":
    main()

"""
run_distributed.py — launch the coordinator for distributed self-play.

Usage:
    FAB_DIST_TOKEN=secret python run_distributed.py \\
        --steps 10000 --eval-every 0 --batch 64

Then on each compute node:
    FAB_DIST_TOKEN=secret python dist_worker.py --coord tcp://COORD_HOST
"""

from __future__ import annotations

import argparse
import os
import sys
from typing import Tuple

from self_play_trainer import (
    DECK_BUILDERS,
    TrainerConfig,
)
from dist_coordinator import (
    CoordinatorServer,
    DEFAULT_PULL_PORT,
    DEFAULT_PUB_PORT,
    DEFAULT_REP_PORT,
)


def _build_cli() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Distributed AlphaZero coordinator for FaBEnv."
    )
    # Trainer config (mirrors self_play_trainer.py's CLI where it makes sense).
    p.add_argument("--steps", type=int, default=0,
                   help="Max grad steps (0 = run forever)")
    p.add_argument("--sims", type=int, default=32, help="MCTS simulations per decision")
    p.add_argument("--batch", type=int, default=64, help="Batch size for grad steps")
    p.add_argument("--lr", type=float, default=3e-4, help="Adam learning rate")
    p.add_argument("--eval-games", type=int, default=4)
    p.add_argument("--eval-every-grad-steps", type=int, default=0,
                   help="Run eval after every N grad steps (0 = derive from config)")
    p.add_argument("--ckpt-every-grad-steps", type=int, default=0,
                   help="Checkpoint every N grad steps (0 = match eval cadence)")
    p.add_argument("--seed", type=int, default=None)
    p.add_argument("--run-name", type=str, default="")
    p.add_argument("--base", type=str, default=None,
                   help="Base checkpoint name (under ./checkpoints/)")
    p.add_argument("--no-pimc", action="store_true",
                   help="Disable PIMC determinization on workers")
    p.add_argument(
        "--deck-pool", type=str, default="",
        help=(
            "Comma-separated deck pool for both sides "
            f"(choices: {','.join(sorted(DECK_BUILDERS))}). "
            "Each side's deck is sampled independently per game."
        ),
    )
    # Network / sync knobs.
    p.add_argument("--bind-host", type=str, default="0.0.0.0")
    p.add_argument("--pull-port", type=int, default=DEFAULT_PULL_PORT)
    p.add_argument("--pub-port", type=int, default=DEFAULT_PUB_PORT)
    p.add_argument("--rep-port", type=int, default=DEFAULT_REP_PORT)
    p.add_argument("--broadcast-secs", type=float, default=5.0,
                   help="Weight broadcast cadence")
    p.add_argument("--max-staleness", type=int, default=10,
                   help="Drop transitions whose weight_version is older than current by more than N")
    p.add_argument("--min-buffer", type=int, default=64,
                   help="Wait until replay buffer has at least this many transitions before training")
    return p


def _resolve_deck_pool(arg: str) -> Tuple[str, ...]:
    pool: Tuple[str, ...] = TrainerConfig.deck_pool
    if not arg:
        return pool
    names = [n.strip() for n in arg.split(",") if n.strip()]
    unknown = [n for n in names if n not in DECK_BUILDERS]
    if unknown:
        raise SystemExit(
            f"Unknown deck(s) in --deck-pool: {unknown}; "
            f"choices: {sorted(DECK_BUILDERS)}"
        )
    return tuple(names) if names else pool


def main() -> None:
    args = _build_cli().parse_args()
    token = os.environ.get("FAB_DIST_TOKEN", "")
    if not token:
        print("FAB_DIST_TOKEN env var must be set.", file=sys.stderr)
        sys.exit(2)

    cfg = TrainerConfig(
        total_iters=args.steps,
        n_simulations=args.sims,
        batch_size=args.batch,
        lr=args.lr,
        eval_games=args.eval_games,
        determinize=not args.no_pimc,
        seed=args.seed,
        run_name=args.run_name,
        base_checkpoint=args.base,
        deck_pool=_resolve_deck_pool(args.deck_pool),
    )
    server = CoordinatorServer(
        config=cfg,
        token=token,
        bind_host=args.bind_host,
        pull_port=args.pull_port,
        pub_port=args.pub_port,
        rep_port=args.rep_port,
        broadcast_every_sec=args.broadcast_secs,
        max_staleness=args.max_staleness,
        min_buffer_to_train=args.min_buffer,
        eval_every_grad_steps=args.eval_every_grad_steps,
        checkpoint_every_grad_steps=args.ckpt_every_grad_steps,
    )
    server.run()


if __name__ == "__main__":
    main()

"""
test_mcts_winrate.py — rough win-rate test for MCTSAgent vs RandomAgent.

Runs two configurations:
  1. MCTSAgent (Rhinar, player_idx=0) vs RandomAgent (Dorinthea)
  2. RandomAgent (Rhinar) vs MCTSAgent (Dorinthea, player_idx=1)
"""

from __future__ import annotations

import random
import sys
import time

from run_env import run_game
from mcts_agent import MCTSAgent
from agents import RandomAgent

N_GAMES = 20
N_SIMS = 50


def run_batch(label: str, mcts_player_idx: int, n_games: int = N_GAMES) -> float:
    mcts_wins = 0
    mcts_agent_id = f"agent_{mcts_player_idx}"

    for i in range(n_games):
        seed = 1000 + i
        mcts = MCTSAgent(player_idx=mcts_player_idx, n_simulations=N_SIMS)
        rng_agent = RandomAgent()

        if mcts_player_idx == 0:
            winner = run_game(verbose=False, seed=seed, agent0=mcts, agent1=rng_agent)
        else:
            winner = run_game(verbose=False, seed=seed, agent0=rng_agent, agent1=mcts)

        won = winner == mcts_agent_id
        if won:
            mcts_wins += 1
        print(f"  [{label}] game {i+1:2d}/{n_games}  winner={winner}  mcts_won={won}  cumulative={mcts_wins}/{i+1}", flush=True)

    win_rate = mcts_wins / n_games
    print(f"\n{label}: MCTS wins {mcts_wins}/{n_games}  ({win_rate:.1%})\n")
    return win_rate


if __name__ == "__main__":
    t0 = time.time()

    print(f"=== MCTS (n_sims={N_SIMS}) vs Random — {N_GAMES} games each ===\n")

    wr_rhinar = run_batch("MCTS as Rhinar  ", mcts_player_idx=0)
    wr_dorinthea = run_batch("MCTS as Dorinthea", mcts_player_idx=1)

    elapsed = time.time() - t0
    print("=" * 55)
    print(f"MCTS as Rhinar    win rate: {wr_rhinar:.1%}")
    print(f"MCTS as Dorinthea win rate: {wr_dorinthea:.1%}")
    print(f"Overall MCTS win rate:      {(wr_rhinar + wr_dorinthea) / 2:.1%}")
    print(f"Total time: {elapsed:.0f}s")

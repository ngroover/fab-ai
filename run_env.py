"""
run_env.py — run FaBEnv games using rule-based agents.

Usage:
  python run_env.py              # single verbose game
  python run_env.py --quiet      # single game, result only
  python run_env.py --sim 200    # 200-game simulation with win rates
  python run_env.py --seed 42    # fixed seed for reproducibility
"""

from __future__ import annotations

import argparse
import random
from typing import Optional

from fab_env import FaBEnv, Phase
from agents import RhinarAgent, DorintheiAgent
from actions import ActionType


def run_game(verbose: bool = True, seed: Optional[int] = None) -> Optional[str]:
    """
    Run one complete game. Returns the winning agent id, or None for draw.
    """
    env = FaBEnv(verbose=verbose)
    obs, infos = env.reset(seed=seed)

    rhinar_agent = RhinarAgent()
    dorinthea_agent = DorintheiAgent()

    # agent_0 = Rhinar (player 0), agent_1 = Dorinthea (player 1)
    def get_agent(agent_id: str):
        return rhinar_agent if agent_id == "agent_0" else dorinthea_agent

    while not env.done:
        agent_id = env.agent_selection
        agent = get_agent(agent_id)
        player_idx = int(agent_id[-1])
        player = env._game.players[player_idx]
        opponent = env._game.players[1 - player_idx]

        legal = env.legal_actions()
        if not legal:
            break

        # Dispatch to correct decision method based on current phase
        if env._phase == Phase.ATTACK:
            action = agent.select_action(obs[agent_id], legal, player, opponent)
        elif env._phase == Phase.DEFEND:
            attack_power = env._pending_attack_power
            action = agent.select_defend(obs[agent_id], legal, player, attack_power)
        elif env._phase == Phase.ARSENAL:
            action = agent.select_arsenal(obs[agent_id], legal, player)
        else:
            from actions import Action
            action = legal[0]

        obs, rewards, terminations, truncations, infos = env.step(action)

    winner_agent = None
    for agent_id in env.agents:
        if env._rewards[agent_id] > 0:
            winner_agent = agent_id
            break

    if verbose:
        env.render()

    return winner_agent


def run_simulation(n: int):
    rhinar_wins = 0
    dorinthea_wins = 0
    draws = 0
    total_turns = 0

    print(f"\nRunning {n} games (FaBEnv — Classic Battles)...\n")

    for i in range(n):
        seed = i  # deterministic seeding per game
        winner = run_game(verbose=False, seed=seed)

        env_dummy = FaBEnv(verbose=False)  # just to check turn count — we'll track inline
        # We don't have turn count easily here; use a separate counter approach
        if winner is None:
            draws += 1
        elif winner == "agent_0":
            rhinar_wins += 1
        else:
            dorinthea_wins += 1

        if (i + 1) % max(1, n // 10) == 0:
            print(f"  {i+1}/{n} ({(i+1)/n*100:.0f}%)")

    print(f"\n{'═'*50}")
    print(f"  SIMULATION RESULTS ({n} games)")
    print(f"{'═'*50}")
    print(f"  Rhinar (agent_0) wins:      {rhinar_wins:>5} ({rhinar_wins/n*100:.1f}%)")
    print(f"  Dorinthea (agent_1) wins:   {dorinthea_wins:>5} ({dorinthea_wins/n*100:.1f}%)")
    print(f"  Draws/timeouts:             {draws:>5} ({draws/n*100:.1f}%)")
    print(f"{'═'*50}\n")


def main():
    parser = argparse.ArgumentParser(description="FaBEnv — Classic Battles Runner")
    parser.add_argument("--sim", type=int, default=0, help="Run N games in simulation mode")
    parser.add_argument("--quiet", action="store_true", help="Suppress play-by-play output")
    parser.add_argument("--seed", type=int, default=None, help="Random seed for single game")
    args = parser.parse_args()

    if args.sim > 0:
        run_simulation(args.sim)
    else:
        winner = run_game(verbose=not args.quiet, seed=args.seed)
        if args.quiet:
            if winner:
                hero = "Rhinar" if winner == "agent_0" else "Dorinthea"
                print(f"\n🏆 Winner: {hero} ({winner})")
            else:
                print("\n⏱  Draw")


if __name__ == "__main__":
    main()

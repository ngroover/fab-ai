"""
run_env.py — run FaBEnv games using rule-based agents or a human player.

Usage:
  python run_env.py                        # single verbose game (both AI)
  python run_env.py --quiet                # single game, result only
  python run_env.py --sim 200              # 200-game simulation with win rates
  python run_env.py --seed 42              # fixed seed for reproducibility
  python run_env.py --human rhinar         # human plays Rhinar, AI plays Dorinthea
  python run_env.py --human dorinthea      # human plays Dorinthea, AI plays Rhinar
  python run_env.py --human both           # both players are human
  python run_env.py --human 0              # same as --human rhinar (agent_0)
  python run_env.py --human 1              # same as --human dorinthea (agent_1)
"""

from __future__ import annotations

import argparse
import os
import random
from datetime import datetime
from typing import Optional

from fab_env import FaBEnv, Phase
from agents import RhinarAgent, DorintheiAgent, HumanAgent, RandomAgent
from actions import ActionType

_AGENT_CHOICES = ("rhinar", "dorinthea", "random", "human")


def _make_agent(name: str):
    """Return an agent instance for the given name string."""
    name = name.lower().strip()
    if name == "rhinar":
        return RhinarAgent()
    if name == "dorinthea":
        return DorintheiAgent()
    if name == "random":
        return RandomAgent()
    if name == "human":
        return HumanAgent()
    raise argparse.ArgumentTypeError(
        f"Unknown agent '{name}'. Choose from: {', '.join(_AGENT_CHOICES)}"
    )


def _resolve_human_flags(human_arg: Optional[str]) -> tuple[bool, bool]:
    """
    Parse --human argument and return (rhinar_is_human, dorinthea_is_human).
    Accepts: rhinar/0/agent_0  dorinthea/1/agent_1  both
    """
    if human_arg is None:
        return False, False
    v = human_arg.lower().strip()
    if v in ("rhinar", "0", "agent_0"):
        return True, False
    if v in ("dorinthea", "1", "agent_1"):
        return False, True
    if v == "both":
        return True, True
    raise argparse.ArgumentTypeError(
        f"Unknown --human value '{human_arg}'. "
        "Use: rhinar, dorinthea, both, 0, 1, agent_0, or agent_1."
    )

LOGS_DIR = os.path.join(os.path.dirname(__file__), "logs")


def _new_log_path(seed: Optional[int] = None) -> str:
    os.makedirs(LOGS_DIR, exist_ok=True)
    ts = datetime.now().strftime("%Y%m%d_%H%M%S")
    seed_tag = f"_seed{seed}" if seed is not None else ""
    return os.path.join(LOGS_DIR, f"game_{ts}{seed_tag}.log")


def run_game(
    verbose: bool = True,
    seed: Optional[int] = None,
    save_log: bool = False,
    rhinar_is_human: bool = False,
    dorinthea_is_human: bool = False,
    agent0=None,
    agent1=None,
) -> Optional[str]:
    """
    Run one complete game. Returns the winning agent id, or None for draw.

    agent0 / agent1 accept any agent instance (RhinarAgent, DorintheiAgent,
    RandomAgent, HumanAgent, or custom).  When provided they take precedence
    over rhinar_is_human / dorinthea_is_human.
    """
    log_file = _new_log_path(seed) if save_log else None
    env = FaBEnv(verbose=verbose, log_file=log_file)
    obs, infos = env.reset(seed=seed)

    if agent0 is None:
        agent0 = HumanAgent() if rhinar_is_human else RhinarAgent()
    if agent1 is None:
        agent1 = HumanAgent() if dorinthea_is_human else DorintheiAgent()

    rhinar_agent = agent0
    dorinthea_agent = agent1

    # Give any MCTS-style agents a reference to the live env
    for _agent in (rhinar_agent, dorinthea_agent):
        if hasattr(_agent, "set_env"):
            _agent.set_env(env)

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
            already_defense = env._pending_defend_total
            action = agent.select_defend(obs[agent_id], legal, player, attack_power, already_defense)
        elif env._phase == Phase.REACTION:
            ap = env._pending_attack_power
            is_attacker = player_idx == env._reaction_attacker_idx
            action = agent.select_reaction(obs[agent_id], legal, player, ap, is_attacker)
        elif env._phase == Phase.INSTANT:
            ap = (env._pending_attack_power
                  if env._pending_attack is not None else 0)
            action = agent.select_instant(obs[agent_id], legal, player, ap)
        elif env._phase == Phase.ARSENAL:
            action = agent.select_arsenal(obs[agent_id], legal, player)
        elif env._phase == Phase.PITCH:
            action = agent.select_pitch(obs[agent_id], legal, player,
                                        env._pending_play_card)
        elif env._phase == Phase.PITCH_ORDER:
            action = agent.select_pitch_order(obs[agent_id], legal, player)
        else:
            action = legal[0]

        obs, rewards, terminations, truncations, infos = env.step(action)

    winner_agent = None
    for agent_id in env.agents:
        if env._rewards[agent_id] > 0:
            winner_agent = agent_id
            break

    if verbose:
        env.render()

    if log_file:
        print(f"  📄 Log saved: {log_file}")

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
    parser.add_argument("--log", action="store_true", help="Save game log to logs/ directory")
    parser.add_argument(
        "--agent0",
        metavar="AGENT",
        default=None,
        help=f"Agent for player 0 (Rhinar slot). Choices: {', '.join(_AGENT_CHOICES)}",
    )
    parser.add_argument(
        "--agent1",
        metavar="AGENT",
        default=None,
        help=f"Agent for player 1 (Dorinthea slot). Choices: {', '.join(_AGENT_CHOICES)}",
    )
    parser.add_argument(
        "--human",
        metavar="HERO",
        default=None,
        help=(
            "Make one or both heroes human-controlled (interactive stdin). "
            "Values: rhinar (or 0/agent_0), dorinthea (or 1/agent_1), both. "
            "Overridden by --agent0/--agent1 when both are specified."
        ),
    )
    args = parser.parse_args()

    rhinar_is_human, dorinthea_is_human = _resolve_human_flags(args.human)

    agent0 = _make_agent(args.agent0) if args.agent0 else None
    agent1 = _make_agent(args.agent1) if args.agent1 else None

    any_human = (
        (agent0 is not None and isinstance(agent0, HumanAgent))
        or (agent1 is not None and isinstance(agent1, HumanAgent))
        or rhinar_is_human
        or dorinthea_is_human
    )

    if args.sim > 0:
        if any_human:
            parser.error("--sim cannot be used together with a human agent")
        run_simulation(args.sim)
    else:
        winner = run_game(
            verbose=not args.quiet,
            seed=args.seed,
            save_log=args.log,
            rhinar_is_human=rhinar_is_human,
            dorinthea_is_human=dorinthea_is_human,
            agent0=agent0,
            agent1=agent1,
        )
        if args.quiet:
            if winner:
                hero = "Rhinar" if winner == "agent_0" else "Dorinthea"
                print(f"\n🏆 Winner: {hero} ({winner})")
            else:
                print("\n⏱  Draw")


if __name__ == "__main__":
    main()

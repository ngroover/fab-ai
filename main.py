#!/usr/bin/env python3
"""
Classic Battles: Rhinar vs Dorinthea Simulator
Two 40-card Blitz decks, young heroes, 20 life each.

Usage:
  python main.py              # Full play-by-play
  python main.py --quiet      # Just the result
  python main.py --sim 200    # Run 200 games, show win rates
  python main.py --dorinthea-first
"""

import sys
import argparse
from cards import (build_rhinar_deck, build_rhinar_equipment,
                   build_dorinthea_deck, build_dorinthea_equipment)
from game_state import Player, GameState
from engine import FaBEngine


def make_rhinar() -> Player:
    equip = build_rhinar_equipment()
    weapon = equip[0]       # Bone Basher
    armor = equip[1:]       # rest
    return Player(
        name="Rhinar",
        hero_name="Rhinar (Young Brute)",
        life=20,
        intellect=4,
        deck=build_rhinar_deck(),
        equipment_list=armor,
        weapon=weapon,
    )


def make_dorinthea() -> Player:
    equip = build_dorinthea_equipment()
    weapon = equip[0]       # Dawnblade, Resplendent
    armor = equip[1:]
    return Player(
        name="Dorinthea",
        hero_name="Dorinthea, Quicksilver Prodigy",
        life=20,
        intellect=4,
        deck=build_dorinthea_deck(),
        equipment_list=armor,
        weapon=weapon,
    )


def run_single_game(verbose=True, rhinar_first=True):
    p1 = make_rhinar() if rhinar_first else make_dorinthea()
    p2 = make_dorinthea() if rhinar_first else make_rhinar()
    game = GameState(p1, p2)
    engine = FaBEngine(game, verbose=verbose)
    return engine.run_game()


def run_simulation(n: int):
    rhinar_wins = 0
    dorinthea_wins = 0
    draws = 0
    total_turns = 0

    print(f"\nRunning {n} games (Classic Battles: Rhinar vs Dorinthea)...\n")

    for i in range(n):
        rhinar_first = (i % 2 == 0)
        p1 = make_rhinar() if rhinar_first else make_dorinthea()
        p2 = make_dorinthea() if rhinar_first else make_rhinar()
        game = GameState(p1, p2)
        engine = FaBEngine(game, verbose=False)
        winner = engine.run_game()
        total_turns += game.turn_number

        if winner is None:
            draws += 1
        elif "Rhinar" in winner.name:
            rhinar_wins += 1
        else:
            dorinthea_wins += 1

        if (i + 1) % max(1, n // 10) == 0:
            print(f"  {i+1}/{n} ({(i+1)/n*100:.0f}%)")

    print(f"\n{'═'*50}")
    print(f"  SIMULATION RESULTS ({n} games)")
    print(f"{'═'*50}")
    print(f"  Rhinar wins:      {rhinar_wins:>5} ({rhinar_wins/n*100:.1f}%)")
    print(f"  Dorinthea wins:   {dorinthea_wins:>5} ({dorinthea_wins/n*100:.1f}%)")
    print(f"  Draws/timeouts:   {draws:>5} ({draws/n*100:.1f}%)")
    print(f"  Avg game length:  {total_turns/n:.1f} turns")
    print(f"{'═'*50}\n")


def main():
    parser = argparse.ArgumentParser(description="Classic Battles: Rhinar vs Dorinthea")
    parser.add_argument("--sim", type=int, default=0)
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--dorinthea-first", action="store_true")
    args = parser.parse_args()

    if args.sim > 0:
        run_simulation(args.sim)
    else:
        winner = run_single_game(verbose=not args.quiet,
                                  rhinar_first=not args.dorinthea_first)
        if args.quiet:
            if winner:
                print(f"\n🏆 Winner: {winner.name} ({winner.hero_name})")
            else:
                print("\n⏱  Draw")


if __name__ == "__main__":
    main()

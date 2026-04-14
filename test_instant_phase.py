"""
Unit tests for the INSTANT phase & stack.

Covers:
  - An instant window opens after a defended attack and before arsenal.
  - Either player may play an instant during the window.
  - Instants go on a LIFO stack and only resolve when both players pass priority.
  - Sigil of Solace is playable as a test case and grants life on resolution.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CARD_CATALOG, CardType
from game_state import Player


def _empty_deck():
    return []


def _make_test_env(p0_hand=None, p1_hand=None, p0_life=20, p1_life=20,
                   active_idx=0):
    """Spin up a FaBEnv and force both players into a known state by
    overwriting ``_game`` after reset."""
    env = FaBEnv(verbose=False)
    env.reset(seed=0)
    g = env._game

    # Skip the CHOOSE_FIRST phase effects; just put us in ATTACK with clean hands
    g.active_player_idx = active_idx
    g.is_first_turn = False

    p0, p1 = g.players
    p0.hand = list(p0_hand) if p0_hand else []
    p1.hand = list(p1_hand) if p1_hand else []
    p0.life = p0_life
    p1.life = p1_life
    p0.resource_points = 0
    p1.resource_points = 0
    p0.action_points = 0
    p1.action_points = 0

    return env


class TestInstantPhaseEntry(unittest.TestCase):
    """Verify the instant phase opens at the right times."""

    def test_instant_phase_opens_before_arsenal(self):
        env = _make_test_env()
        env._phase = Phase.ATTACK
        env.agent_selection = "agent_0"
        env._game.players[0].action_points = 1

        # Active player passes their action phase — should open instant window.
        env.step(Action(ActionType.PASS))
        self.assertEqual(env._phase, Phase.INSTANT)
        # Active player has priority first.
        self.assertEqual(env.agent_selection, "agent_0")

    def test_instant_window_closes_after_two_passes_with_empty_stack(self):
        env = _make_test_env()
        env._phase = Phase.ATTACK
        env.agent_selection = "agent_0"
        env._game.players[0].action_points = 1
        env.step(Action(ActionType.PASS))
        self.assertEqual(env._phase, Phase.INSTANT)

        # Active passes priority.
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env._phase, Phase.INSTANT)
        self.assertEqual(env.agent_selection, "agent_1")

        # Opponent passes — window closes to ARSENAL with active holding the turn.
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env._phase, Phase.ARSENAL)
        self.assertEqual(env.agent_selection, "agent_0")


class TestSigilOfSolaceOnStack(unittest.TestCase):
    """Exercise playing Sigil of Solace as an instant during the window."""

    def test_sigil_of_solace_is_playable_in_instant_window(self):
        sigil = CARD_CATALOG["sigil_of_solace_blue"]
        self.assertEqual(sigil.card_type, CardType.INSTANT)

        # Give the opponent (agent_1) a Sigil of Solace; active is agent_0.
        env = _make_test_env(p1_hand=[sigil], p1_life=12, active_idx=0)
        env._phase = Phase.ATTACK
        env.agent_selection = "agent_0"
        env._game.players[0].action_points = 1

        # Active passes → instant window opens, active has priority.
        env.step(Action(ActionType.PASS))
        self.assertEqual(env._phase, Phase.INSTANT)

        # Active passes priority → agent_1 gets to act.
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env.agent_selection, "agent_1")

        # Sigil of Solace should be a legal action for agent_1 (cost 0).
        legal = env.legal_actions()
        play_sigil = next(
            (a for a in legal
             if a.action_type == ActionType.PLAY_CARD and a.card is sigil),
            None,
        )
        self.assertIsNotNone(play_sigil, f"Sigil not playable; legal={legal}")

        # Play it — the instant goes onto the stack, not resolved yet.
        env.step(play_sigil)
        self.assertEqual(env._phase, Phase.INSTANT)
        self.assertEqual(len(env._instant_stack), 1)
        owner_idx, stacked_card = env._instant_stack[0]
        self.assertEqual(owner_idx, 1)
        self.assertEqual(stacked_card.name, "Sigil of Solace")
        # Priority passes to the non-playing player (active).
        self.assertEqual(env.agent_selection, "agent_0")
        # Life hasn't changed yet — resolution is pending on the stack.
        self.assertEqual(env._game.players[1].life, 12)

        # Active passes priority.
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env.agent_selection, "agent_1")
        self.assertEqual(env._phase, Phase.INSTANT)
        self.assertEqual(len(env._instant_stack), 1)

        # Opponent passes → top of stack resolves. Stack now empty, but
        # window stays open (passes reset). Priority goes back to active.
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env._phase, Phase.INSTANT)
        self.assertEqual(len(env._instant_stack), 0)
        self.assertEqual(env.agent_selection, "agent_0")
        self.assertEqual(env._game.players[1].life, 15,
                         "Sigil of Solace should gain 3 life on resolution")
        # Card moved to graveyard.
        self.assertIn("Sigil of Solace",
                      [c.name for c in env._game.players[1].graveyard])

        # Two more passes close the window.
        env.step(Action(ActionType.PASS_PRIORITY))
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env._phase, Phase.ARSENAL)


class TestLIFOResolution(unittest.TestCase):
    """Two instants on the stack resolve in last-in-first-out order."""

    def test_lifo_order(self):
        sigil = CARD_CATALOG["sigil_of_solace_blue"]
        bauble = CARD_CATALOG["titanium_bauble_blue"]
        # Titanium Bauble is typed as RESOURCE, not INSTANT, so for this LIFO
        # test we use two copies of Sigil of Solace (one per player).
        env = _make_test_env(p0_hand=[sigil], p1_hand=[sigil],
                             p0_life=10, p1_life=10, active_idx=0)
        env._phase = Phase.ATTACK
        env.agent_selection = "agent_0"
        env._game.players[0].action_points = 1
        env.step(Action(ActionType.PASS))  # opens instant window

        # agent_0 plays Sigil (puts life-gain on the stack).
        legal = env.legal_actions()
        a0 = next(a for a in legal
                  if a.action_type == ActionType.PLAY_CARD)
        env.step(a0)
        # priority is now agent_1
        self.assertEqual(env.agent_selection, "agent_1")

        # agent_1 plays Sigil (on top).
        legal = env.legal_actions()
        a1 = next(a for a in legal
                  if a.action_type == ActionType.PLAY_CARD)
        env.step(a1)

        # Stack has p0's sigil at the bottom, p1's on top.
        self.assertEqual(len(env._instant_stack), 2)
        self.assertEqual(env._instant_stack[0][0], 0)
        self.assertEqual(env._instant_stack[1][0], 1)

        # Both pass — top (p1's) resolves first → p1 gains 3 life.
        env.step(Action(ActionType.PASS_PRIORITY))
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env._game.players[1].life, 13)
        self.assertEqual(env._game.players[0].life, 10)
        self.assertEqual(len(env._instant_stack), 1)

        # Another pair of passes — p0's sigil resolves.
        env.step(Action(ActionType.PASS_PRIORITY))
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env._game.players[0].life, 13)
        self.assertEqual(len(env._instant_stack), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)

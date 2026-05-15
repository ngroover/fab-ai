"""
Unit tests for the INSTANT phase & stack.

Covers:
  - An instant window opens after a defended attack and before arsenal.
  - Either player may play an instant during the window.
  - Instants go on a LIFO stack and only resolve when both players pass priority.
  - Sigil of Solace is playable as a test case and grants life on resolution.

With auto-execute enabled, the instant window collapses automatically when
neither player has an instant in hand (only PASS_PRIORITY available).  Tests
that need to observe the INSTANT phase must give at least the priority-holder
an instant card so auto-execute does not fire for that player.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CARD_CATALOG, CardType
from cards import build_rhinar_deck, build_dorinthea_deck
from game_state import Player


def _make_test_env(p0_hand=None, p1_hand=None, p0_life=20, p1_life=20,
                   active_idx=0):
    """Spin up a FaBEnv and force both players into a known state by
    overwriting ``_game`` after reset."""
    env = FaBEnv(verbose=False)
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=0)
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
        # Give agent_0 a Sigil so the window pauses at their priority rather
        # than being auto-executed away (both players having only PASS_PRIORITY
        # would let auto-execute close the window immediately).
        sigil = CARD_CATALOG["sigil_of_solace_blue"]
        env = _make_test_env(p0_hand=[sigil])
        env._phase = Phase.ATTACK
        env.agent_selection = "agent_0"
        env._game.players[0].action_points = 1

        # Active player passes their action phase — should open instant window.
        env.step(Action(ActionType.PASS))
        self.assertEqual(env._phase, Phase.INSTANT)
        # Active player has priority first and has a real choice (Sigil vs pass).
        self.assertEqual(env.agent_selection, "agent_0")

    def test_instant_window_closes_after_two_passes_with_empty_stack(self):
        # Give both players a Sigil so neither is auto-executed; each must
        # explicitly pass priority for the window to close.
        sigil = CARD_CATALOG["sigil_of_solace_blue"]
        env = _make_test_env(p0_hand=[sigil], p1_hand=[sigil])
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
        self.assertEqual(sigil.card_type, [CardType.INSTANT])

        # Give agent_1 a Sigil; give agent_0 a non-instant card so it has
        # only PASS_PRIORITY available (auto-execute fires for agent_0 on PASS,
        # handing priority directly to agent_1 who then has a real choice).
        bare_fangs = CARD_CATALOG["bare_fangs_red"]
        env = _make_test_env(p0_hand=[bare_fangs], p1_hand=[sigil],
                             p1_life=12, active_idx=0)
        env._phase = Phase.ATTACK
        env.agent_selection = "agent_0"
        env._game.players[0].action_points = 1

        # Active passes → auto-execute fires PASS_PRIORITY for agent_0 →
        # priority lands on agent_1 who has a real instant choice.
        env.step(Action(ActionType.PASS))
        self.assertEqual(env._phase, Phase.INSTANT)
        self.assertEqual(env.agent_selection, "agent_1")

        # Sigil of Solace should be a legal action for agent_1 (cost 0).
        legal = env.legal_actions()
        play_sigil = next(
            (a for a in legal
             if a.action_type == ActionType.PLAY_CARD and a.card is sigil),
            None,
        )
        self.assertIsNotNone(play_sigil, f"Sigil not playable; legal={legal}")

        # Play it — with auto-execute the stack resolves and window closes.
        # agent_0 and agent_1 both auto-pass (neither has instants left),
        # Sigil resolves gaining 1 life, then the window closes to ARSENAL.
        env.step(play_sigil)
        self.assertEqual(env._game.players[1].life, 13,
                         "Sigil of Solace should gain 1 life on resolution")
        self.assertIn("Sigil of Solace",
                      [c.name for c in env._game.players[1].graveyard])
        # Window closed; agent_0 holds the turn (has bare_fangs to store or not).
        self.assertEqual(env._phase, Phase.ARSENAL)


class TestLIFOResolution(unittest.TestCase):
    """Two instants on the stack resolve in last-in-first-out order."""

    def test_lifo_order(self):
        sigil = CARD_CATALOG["sigil_of_solace_blue"]
        # Give each player two Sigils so after each plays one they still hold
        # one, preventing auto-execute from firing when they have priority.
        env = _make_test_env(p0_hand=[sigil, sigil], p1_hand=[sigil, sigil],
                             p0_life=10, p1_life=10, active_idx=0)
        env._phase = Phase.ATTACK
        env.agent_selection = "agent_0"
        env._game.players[0].action_points = 1
        env.step(Action(ActionType.PASS))  # opens instant window

        # agent_0 plays Sigil (puts life-gain on the stack).
        legal = env.legal_actions()
        a0 = next(a for a in legal if a.action_type == ActionType.PLAY_CARD)
        env.step(a0)
        # priority is now agent_1 (still has a Sigil — not auto-executed)
        self.assertEqual(env.agent_selection, "agent_1")

        # agent_1 plays Sigil (on top).
        legal = env.legal_actions()
        a1 = next(a for a in legal if a.action_type == ActionType.PLAY_CARD)
        env.step(a1)

        # Stack has p0's sigil at the bottom, p1's on top.
        # Both players still hold one Sigil, so auto-execute does not fire.
        self.assertEqual(len(env._instant_stack), 2)
        self.assertEqual(env._instant_stack[0][0], 0)
        self.assertEqual(env._instant_stack[1][0], 1)

        # Both pass — top (p1's) resolves first → p1 gains 1 life.
        env.step(Action(ActionType.PASS_PRIORITY))
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env._game.players[1].life, 11)
        self.assertEqual(env._game.players[0].life, 10)
        self.assertEqual(len(env._instant_stack), 1)

        # Another pair of passes — p0's sigil resolves.
        env.step(Action(ActionType.PASS_PRIORITY))
        env.step(Action(ActionType.PASS_PRIORITY))
        self.assertEqual(env._game.players[0].life, 11)
        self.assertEqual(len(env._instant_stack), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)

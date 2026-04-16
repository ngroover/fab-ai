"""
Unit tests for Sigil of Solace timing and effect.

Seed 27 gives:
  Rhinar:     Chief Ruk'utan, Wild Ride, Beast Mode, Wrecker Romp
  Dorinthea:  On a Knife Edge, Sigil of Solace, Toughen Up, Ironsong Response

Tests verify:
  - Sigil of Solace appears in legal actions during the DEFEND phase (reaction window)
  - Playing it gives exactly 1 life (not 3)
  - DEFEND phase continues after playing it (defender still chooses blocks)
  - Sigil is consumed (removed from hand, put in graveyard)
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import ActionType


SEED = 27  # Dorinthea has Sigil of Solace; Rhinar has Wild Ride


def _advance_to_defend_after_wild_ride(env):
    """
    Reset at SEED and step to the DEFEND phase triggered by Rhinar's Wild Ride.

    Returns once env._phase == Phase.DEFEND with Dorinthea (agent_1) to act.
    """
    env.reset(seed=SEED)

    # CHOOSE_FIRST: Rhinar elects to go first
    legal = env.legal_actions()
    go_first = next(a for a in legal if a.action_type == ActionType.GO_FIRST)
    env.step(go_first)

    # ATTACK: Rhinar plays Wild Ride
    legal = env.legal_actions()
    wild_ride = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Wild Ride"
    )
    env.step(wild_ride)

    # PITCH: pay Wild Ride's cost (one pitch step covers cost 2)
    while env._phase == Phase.PITCH:
        env.step(env.legal_actions()[0])

    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"
    assert env.agent_selection == "agent_1", "Dorinthea should be defending"


class TestSigilOfSolaceCard(unittest.TestCase):
    """Verify Sigil of Solace is correctly defined in the card catalog."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_sigil_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Sigil of Solace", names)

    def test_sigil_card_properties(self):
        from cards import CardType, Color
        sigil = next(c for c in self.dorinthea.hand if c.name == "Sigil of Solace")
        self.assertEqual(sigil.card_type, CardType.INSTANT)
        self.assertEqual(sigil.cost, 0)
        self.assertEqual(sigil.color, Color.BLUE)
        self.assertTrue(sigil.no_block)
        self.assertEqual(sigil.text, "Gain 1 life.")


class TestSigilOfSolaceTiming(unittest.TestCase):
    """Dorinthea can play Sigil of Solace during the reaction window before committing blocks."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_defend_after_wild_ride(self.env)
        self.dorinthea = self.env._game.players[1]

    def test_sigil_is_legal_during_defend_phase(self):
        """PLAY_CARD for Sigil of Solace must appear in defend legal actions."""
        legal = self.env.legal_actions()
        sigil_actions = [
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD
            and a.card is not None
            and a.card.name == "Sigil of Solace"
        ]
        self.assertEqual(len(sigil_actions), 1,
                         f"Expected 1 PLAY_CARD(Sigil of Solace) in legal, got {legal}")

    def test_playing_sigil_gains_one_life(self):
        """Sigil of Solace must give exactly 1 life."""
        life_before = self.dorinthea.life
        legal = self.env.legal_actions()
        sigil_action = next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "Sigil of Solace"
        )
        self.env.step(sigil_action)
        self.assertEqual(self.dorinthea.life, life_before + 1)

    def test_defend_phase_continues_after_sigil(self):
        """DEFEND phase must remain active so Dorinthea can still choose blocks."""
        legal = self.env.legal_actions()
        sigil_action = next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "Sigil of Solace"
        )
        self.env.step(sigil_action)
        self.assertEqual(self.env._phase, Phase.DEFEND)
        self.assertEqual(self.env.agent_selection, "agent_1")

    def test_sigil_consumed_after_play(self):
        """Sigil is removed from hand and put in graveyard after being played."""
        legal = self.env.legal_actions()
        sigil_action = next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "Sigil of Solace"
        )
        self.env.step(sigil_action)
        hand_names = [c.name for c in self.dorinthea.hand]
        grave_names = [c.name for c in self.dorinthea.graveyard]
        self.assertNotIn("Sigil of Solace", hand_names)
        self.assertIn("Sigil of Solace", grave_names)

    def test_block_choices_still_available_after_sigil(self):
        """After playing Sigil, Dorinthea still has regular blocking options."""
        legal = self.env.legal_actions()
        sigil_action = next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "Sigil of Solace"
        )
        self.env.step(sigil_action)
        legal = self.env.legal_actions()
        defend_actions = [a for a in legal if a.action_type == ActionType.DEFEND]
        self.assertGreater(len(defend_actions), 0,
                           "Blocking options must remain after playing Sigil of Solace")

    def test_sigil_not_offered_as_blocking_card(self):
        """Sigil of Solace (no_block=True, INSTANT) must not appear as a DEFEND hand choice."""
        legal = self.env.legal_actions()
        for a in legal:
            if a.action_type == ActionType.DEFEND and a.defend_hand_indices:
                # Verify none of the offered block indices point to Sigil
                for idx in a.defend_hand_indices:
                    card = self.dorinthea.hand[idx]
                    self.assertNotEqual(card.name, "Sigil of Solace",
                                        "Sigil of Solace must not be offered as a block card")


if __name__ == "__main__":
    unittest.main(verbosity=2)

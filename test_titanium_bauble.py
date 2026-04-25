"""
Tests for Titanium Bauble resource card.

Seed 7 (via env.reset(seed=7)) gives:
  Rhinar (agent_0):  Beast Mode, Titanium Bauble, Wrecking Ball, Raging Onslaught
  Rhinar wins the coin flip and goes first.

Titanium Bauble is a RESOURCE card (cost 0, pitch 3, defense 3).
Resource cards cannot be played as actions from hand — they exist only
to be pitched for resources or used as blockers.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import ActionType
from cards import CardType, Color

SEED = 7  # Rhinar has Titanium Bauble; Rhinar wins coin flip and goes first


class TestTitaniumBaubleCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))
        self.rhinar = self.env._game.players[0]

    def test_titanium_bauble_in_rhinar_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Titanium Bauble", names)

    def test_card_type_is_resource(self):
        card = next(c for c in self.rhinar.hand if c.name == "Titanium Bauble")
        self.assertEqual(card.card_type, CardType.RESOURCE)

    def test_card_properties(self):
        card = next(c for c in self.rhinar.hand if c.name == "Titanium Bauble")
        self.assertEqual(card.cost, 0)
        self.assertEqual(card.pitch, 3)
        self.assertEqual(card.color, Color.BLUE)


class TestTitaniumBaubleNotPlayableFromHand(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))
        self.rhinar = self.env._game.players[0]

    def test_titanium_bauble_not_in_legal_actions(self):
        """Titanium Bauble must never appear as a PLAY_CARD legal action."""
        self.assertEqual(self.env._phase, Phase.ATTACK)
        legal = self.env.legal_actions()
        bauble_actions = [
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD
            and a.card is not None
            and a.card.name == "Titanium Bauble"
        ]
        self.assertEqual(
            len(bauble_actions), 0,
            f"Titanium Bauble must not be playable from hand, but found: {bauble_actions}",
        )

    def test_other_hand_cards_still_legal(self):
        """Non-resource hand cards should still appear as legal actions."""
        legal = self.env.legal_actions()
        playable_names = {
            a.card.name
            for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card is not None
        }
        # Rhinar's hand: Beast Mode, Titanium Bauble, Wrecking Ball, Raging Onslaught
        self.assertTrue(
            playable_names,
            "At least one non-resource card should be playable",
        )
        self.assertNotIn("Titanium Bauble", playable_names)


if __name__ == "__main__":
    unittest.main()

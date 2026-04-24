"""
Unit tests for Wrecker Romp's discard additional cost.

Wrecker Romp text: "As an additional cost to play Wrecker Romp, discard a card."

The card is only a legal PLAY_CARD action when the player has enough cards to
both cover the pitch cost (2 resources needed) AND keep at least one card in
hand to satisfy the discard.  During the PITCH phase for Wrecker Romp the
player must not be allowed to pitch the last card in hand.

Seed 14 gives:
  Rhinar:     Wrecker Romp, Wounded Bull, Smash Instinct, Chief Ruk'utan
  (3 non-Wrecker cards — enough to pitch 1 yellow card and still keep 2,
   one of which will be discarded as the additional cost)
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass
from card_effects import EffectAction


SEED = 14  # Rhinar: Wrecker Romp, Wounded Bull, Smash Instinct, Chief Ruk'utan


def _setup_rhinar_turn(env):
    """Reset at SEED and advance past CHOOSE_FIRST so Rhinar is in ATTACK phase."""
    env.reset(seed=SEED)
    legal = env.legal_actions()
    go_first = next(a for a in legal if a.action_type == ActionType.GO_FIRST)
    env.step(go_first)


class TestWreckerRompCardDefinition(unittest.TestCase):
    """Verify Wrecker Romp card stats, text, and effect are correctly defined."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.rhinar = self.env._game.players[0]
        self.card = next(c for c in self.rhinar.hand if c.name == "Wrecker Romp")

    def test_wrecker_romp_in_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Wrecker Romp", names)

    def test_card_type(self):
        self.assertEqual(self.card.card_type, CardType.ACTION_ATTACK)

    def test_card_stats(self):
        self.assertEqual(self.card.cost, 2)
        self.assertEqual(self.card.pitch, 3)
        self.assertEqual(self.card.power, 6)
        self.assertEqual(self.card.defense, 3)
        self.assertEqual(self.card.color, Color.BLUE)
        self.assertEqual(self.card.card_class, CardClass.BRUTE)

    def test_has_discard_cost_text(self):
        self.assertIn("discard", self.card.text.lower())

    def test_has_discard_cost_effect(self):
        effect_actions = [e.action for e in self.card.effects]
        self.assertIn(EffectAction.DISCARD_CARD_COST, effect_actions,
                      "Wrecker Romp must carry a DISCARD_CARD_COST effect")


class TestWreckerRompLegalActions(unittest.TestCase):
    """Verify Wrecker Romp's playability gating respects the discard cost."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _setup_rhinar_turn(self.env)
        self.rhinar = self.env._game.players[0]

    def _legal_wrecker_actions(self):
        return [a for a in self.env.legal_actions()
                if a.action_type == ActionType.PLAY_CARD
                and a.card is not None
                and a.card.name == "Wrecker Romp"]

    def test_legal_with_full_hand(self):
        """With 4 cards in hand (3 non-Wrecker), Wrecker Romp is legal."""
        self.assertEqual(len(self._legal_wrecker_actions()), 1,
                         "Expected exactly 1 Wrecker Romp PLAY_CARD action")

    def test_not_legal_with_only_one_other_card(self):
        """Only 1 other card: it must be used for pitch, leaving nothing for discard."""
        wrecker = next(c for c in self.rhinar.hand if c.name == "Wrecker Romp")
        wounded_bull = next(c for c in self.rhinar.hand if c.name == "Wounded Bull")
        # Exactly 1 non-Wrecker card; Wounded Bull (pitch 2) covers cost but
        # nothing remains to discard.
        self.rhinar.hand = [wrecker, wounded_bull]
        self.assertEqual(self._legal_wrecker_actions(), [],
                         "Wrecker Romp must not be legal with only 1 other card")

    def test_legal_with_two_other_cards(self):
        """2 other cards: pitch 1 (yellow covers cost 2), keep 1 for discard."""
        wrecker = next(c for c in self.rhinar.hand if c.name == "Wrecker Romp")
        wounded_bull = next(c for c in self.rhinar.hand if c.name == "Wounded Bull")
        smash = next(c for c in self.rhinar.hand if c.name == "Smash Instinct")
        self.rhinar.hand = [wrecker, wounded_bull, smash]
        self.assertEqual(len(self._legal_wrecker_actions()), 1,
                         "Wrecker Romp must be legal with 2 other cards")

    def test_not_legal_with_zero_other_cards(self):
        """Wrecker Romp alone in hand — nothing to pitch or discard."""
        wrecker = next(c for c in self.rhinar.hand if c.name == "Wrecker Romp")
        self.rhinar.hand = [wrecker]
        self.assertEqual(self._legal_wrecker_actions(), [],
                         "Wrecker Romp must not be legal when alone in hand")


class TestWreckerRompPitchConstraint(unittest.TestCase):
    """During PITCH phase for Wrecker Romp, the last card in hand must not be
    offered as a pitch option — it is reserved for the discard cost."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _setup_rhinar_turn(self.env)
        self.rhinar = self.env._game.players[0]

        # Enter PITCH phase by selecting Wrecker Romp
        legal = self.env.legal_actions()
        wr_action = next(a for a in legal
                         if a.action_type == ActionType.PLAY_CARD
                         and a.card is not None
                         and a.card.name == "Wrecker Romp")
        self.env.step(wr_action)

    def test_in_pitch_phase(self):
        self.assertEqual(self.env._phase, Phase.PITCH)
        self.assertEqual(self.env._pending_play_card.name, "Wrecker Romp")

    def test_pitch_not_offered_when_only_one_card_remains(self):
        """Reduce hand to 1 card: no pitch should be offered (that card is
        the only one available for the discard)."""
        # Simulate a state where two cards were already pitched and only one remains
        smash = next(c for c in self.rhinar.hand if c.name == "Smash Instinct")
        self.rhinar.hand = [smash]

        legal = self.env.legal_actions()
        pitch_with_card = [a for a in legal
                           if a.action_type == ActionType.PITCH
                           and a.pitch_indices]
        self.assertEqual(pitch_with_card, [],
                         "Must not offer pitching the last card when Wrecker Romp needs a discard")

    def test_pitch_offered_with_multiple_cards(self):
        """With 3 cards in hand (natural state after playing Wrecker Romp), pitching
        one still leaves 2, so pitch options should be available."""
        self.assertEqual(len(self.rhinar.hand), 3,
                         "Expected 3 cards remaining after Wrecker Romp was selected to play")
        legal = self.env.legal_actions()
        pitch_actions = [a for a in legal
                         if a.action_type == ActionType.PITCH and a.pitch_indices]
        self.assertGreater(len(pitch_actions), 0,
                           "Pitch options must be available when hand still has multiple cards")


class TestWreckerRompPlayEffect(unittest.TestCase):
    """Playing Wrecker Romp discards a card from hand as the additional cost."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _setup_rhinar_turn(self.env)
        self.rhinar = self.env._game.players[0]

        # Snapshot before playing
        self._hand_size_before = len(self.rhinar.hand)
        self._grave_size_before = len(self.rhinar.graveyard)

        # Play Wrecker Romp
        legal = self.env.legal_actions()
        wr_action = next(a for a in legal
                         if a.action_type == ActionType.PLAY_CARD
                         and a.card is not None
                         and a.card.name == "Wrecker Romp")
        self.env.step(wr_action)

        # Complete the pitch phase (one pitch step covers cost 2 with a yellow card)
        while self.env._phase == Phase.PITCH:
            self.env.step(self.env.legal_actions()[0])

    def test_phase_after_play(self):
        """After pitch completes, Wrecker Romp should be the pending attack,
        and we should be in INSTANT (pre-DEFEND) or DEFEND phase."""
        self.assertIn(self.env._phase, (Phase.INSTANT, Phase.DEFEND),
                      f"Unexpected phase: {self.env._phase}")

    def test_pending_attack_is_wrecker_romp(self):
        self.assertIsNotNone(self.env._pending_attack)
        self.assertEqual(self.env._pending_attack.name, "Wrecker Romp")

    def test_discard_cost_card_goes_to_graveyard(self):
        """Exactly one card must be in graveyard from the discard additional cost
        (pitched cards go to pitch_zone, not graveyard)."""
        self.assertEqual(len(self.rhinar.graveyard), self._grave_size_before + 1,
                         "Graveyard must grow by exactly 1 (the discard cost card)")

    def test_hand_shrinks_by_wrecker_plus_pitch_plus_discard(self):
        """Hand loses: Wrecker Romp (played) + 1 pitched card + 1 discard = 3 cards."""
        expected_hand_size = self._hand_size_before - 3  # WR + 1 pitch + 1 discard
        self.assertEqual(len(self.rhinar.hand), expected_hand_size,
                         f"Expected hand size {expected_hand_size}, got {len(self.rhinar.hand)}")


if __name__ == "__main__":
    unittest.main(verbosity=2)

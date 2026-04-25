"""
Tests for Bone Vizier equipment behavior.

Seed 3 gives:
  Rhinar:     Beast Mode, Barraging Beatdown, Pack Call, Bare Fangs
  Dorinthea:  En Garde, Flock of the Feather Walkers, Visit the Blacksmith, On a Knife Edge
  Dorinthea (agent_1) wins the coin flip and goes first.

Bone Vizier rules:
  HEAD equipment for Rhinar with defense 1.
  Blade Break — when used to defend, it is destroyed when the combat chain closes.
  When Bone Vizier is destroyed, reveal the top card of Rhinar's deck.
    If it has 6 or more power, put it on top. Otherwise, put it on the bottom.

At seed 3, Rhinar's top deck card is Wounded Bull (power 6).
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import Keyword
from card_effects import EffectTrigger, EffectAction

SEED = 3


def _advance_to_defend(env):
    """
    Reset at SEED and advance to the DEFEND phase where Rhinar can block
    Dorinthea's first Dawnblade attack.

    Step sequence
    -------------
    1. Dorinthea chooses GO_FIRST.
    2. Dorinthea attacks with Dawnblade (WEAPON).
    3. Pitch Visit the Blacksmith (index 2, pitch=3) to cover Dawnblade cost 1.
    -> Now in DEFEND phase, Rhinar's turn.

    Returns (rhinar, dorinthea).
    """
    env.reset(seed=SEED)
    rhinar = env._game.players[0]
    dorinthea = env._game.players[1]

    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))

    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

    legal = env.legal_actions()
    env.step(next(a for a in legal if a.pitch_indices == [2]))

    return rhinar, dorinthea


def _defend_with_bone_vizier(env):
    """Commit Bone Vizier (HEAD slot) as the sole blocker, then pass."""
    legal = env.legal_actions()
    env.step(next(a for a in legal
                  if a.action_type == ActionType.DEFEND and a.defend_equip_slots == ["head"]))
    legal = env.legal_actions()
    env.step(next(a for a in legal
                  if a.action_type == ActionType.DEFEND
                  and not a.defend_hand_indices
                  and not a.defend_equip_slots))


class TestBoneVizierCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.rhinar = self.env._game.players[0]

    def test_bone_vizier_in_head_slot(self):
        self.assertIn("head", self.rhinar.equipment)
        self.assertEqual(self.rhinar.equipment["head"].card.name, "Bone Vizier")

    def test_has_blade_break_keyword(self):
        eq = self.rhinar.equipment["head"]
        self.assertIn(Keyword.BLADE_BREAK, eq.card.keywords)

    def test_has_on_destroyed_effect(self):
        eq = self.rhinar.equipment["head"]
        matching = [
            e for e in eq.card.effects
            if e.trigger == EffectTrigger.ON_DESTROYED
            and e.action == EffectAction.REVEAL_TOP_DECK_POWER_CHECK
        ]
        self.assertEqual(len(matching), 1,
                         "Bone Vizier must have exactly one REVEAL_TOP_DECK_POWER_CHECK ON_DESTROYED effect")

    def test_defense_value(self):
        eq = self.rhinar.equipment["head"]
        self.assertEqual(eq.card.defense, 1)


class TestBoneVizierBladeBreak(unittest.TestCase):
    """Bone Vizier is destroyed when the combat chain closes after blocking."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_defend(self.env)

    def test_bone_vizier_available_before_defend(self):
        self.assertIn("head", self.rhinar.equipment)

    def test_bone_vizier_destroyed_after_defend(self):
        _defend_with_bone_vizier(self.env)
        self.assertNotIn("head", self.rhinar.equipment,
                         "Bone Vizier should be removed from equipment after combat chain closes (Blade Break)")

    def test_bone_vizier_in_graveyard_after_destroy(self):
        _defend_with_bone_vizier(self.env)
        grave_names = [c.name for c in self.rhinar.graveyard]
        self.assertIn("Bone Vizier", grave_names)

    def test_not_destroyed_before_defend(self):
        eq = self.rhinar.equipment.get("head")
        self.assertFalse(eq.destroyed)


class TestBoneVizierRevealHighPower(unittest.TestCase):
    """Top card is Wounded Bull (power 6) — should remain on top."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_defend(self.env)

    def test_deck_top_is_wounded_bull_before_defend(self):
        self.assertEqual(self.rhinar.deck[0].name, "Wounded Bull")
        self.assertEqual(self.rhinar.deck[0].power, 6)

    def test_high_power_card_stays_on_top(self):
        deck_before = [c.name for c in self.rhinar.deck]
        _defend_with_bone_vizier(self.env)
        self.assertEqual(self.rhinar.deck[0].name, "Wounded Bull",
                         "6-power card should stay on top after Bone Vizier reveal")
        self.assertEqual([c.name for c in self.rhinar.deck], deck_before,
                         "Deck order should be unchanged when top card has 6+ power")

    def test_deck_size_unchanged(self):
        deck_size = len(self.rhinar.deck)
        _defend_with_bone_vizier(self.env)
        self.assertEqual(len(self.rhinar.deck), deck_size)


class TestBoneVizierRevealLowPower(unittest.TestCase):
    """Manually place a low-power card on top; it should be moved to the bottom."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_defend(self.env)
        low_power = next(c for c in self.rhinar.deck if c.power < 6)
        self.rhinar.deck.remove(low_power)
        self.rhinar.deck.insert(0, low_power)
        self.low_power_card = low_power

    def test_low_power_card_moved_to_bottom(self):
        _defend_with_bone_vizier(self.env)
        self.assertEqual(self.rhinar.deck[-1].name, self.low_power_card.name,
                         "Card with less than 6 power should be placed at the bottom")

    def test_low_power_card_not_on_top(self):
        _defend_with_bone_vizier(self.env)
        self.assertNotEqual(self.rhinar.deck[0].name, self.low_power_card.name,
                            "Low-power card must not remain on top after Bone Vizier reveal")

    def test_deck_size_unchanged(self):
        deck_size = len(self.rhinar.deck)
        _defend_with_bone_vizier(self.env)
        self.assertEqual(len(self.rhinar.deck), deck_size)


class TestBoneVizierRevealEmptyDeck(unittest.TestCase):
    """Effect does nothing when the deck is empty."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_defend(self.env)
        self.rhinar.deck.clear()

    def test_no_error_with_empty_deck(self):
        try:
            _defend_with_bone_vizier(self.env)
        except Exception as e:
            self.fail(f"Bone Vizier on-destroy with empty deck raised {e}")

    def test_deck_stays_empty(self):
        _defend_with_bone_vizier(self.env)
        self.assertEqual(len(self.rhinar.deck), 0)

    def test_still_destroyed_with_empty_deck(self):
        _defend_with_bone_vizier(self.env)
        self.assertNotIn("head", self.rhinar.equipment,
                         "Bone Vizier should still be destroyed even with empty deck")


if __name__ == "__main__":
    unittest.main()

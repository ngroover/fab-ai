"""
Tests for Pack Call defend ability.

Seed 3 gives:
  Rhinar:     Beast Mode, Barraging Beatdown, Pack Call, Bare Fangs
  Dorinthea:  En Garde, Flock of the Feather Walkers, Visit the Blacksmith, On a Knife Edge
  Dorinthea (agent_1) wins the coin flip.

Pack Call defend effect:
  When used to block, reveal the top card of the defender's deck.
  If it has 6 or more power, put it back on top.
  Otherwise put it on the bottom.

Rhinar's deck top card at defend time is Wounded Bull (power 6).
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass
from card_effects import EffectTrigger, EffectAction

SEED = 3  # Rhinar has Pack Call; Dorinthea wins coin flip and goes first


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


def _defend_with_pack_call(env, rhinar):
    """Commit Pack Call (index 2 in Rhinar's hand) as the sole blocker."""
    legal = env.legal_actions()
    env.step(next(a for a in legal
                  if a.action_type == ActionType.DEFEND and a.defend_hand_indices == [2]))
    legal = env.legal_actions()
    env.step(next(a for a in legal
                  if a.action_type == ActionType.DEFEND
                  and not a.defend_hand_indices
                  and not a.defend_equip_slots))


class TestPackCallCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.rhinar = self.env._game.players[0]

    def test_in_rhinar_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Pack Call", names)

    def test_card_properties(self):
        card = next(c for c in self.rhinar.hand if c.name == "Pack Call")
        self.assertEqual(card.card_type, CardType.ACTION_ATTACK)
        self.assertEqual(card.cost, 3)
        self.assertEqual(card.power, 6)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.YELLOW)
        self.assertEqual(card.card_class, CardClass.BRUTE)

    def test_has_on_defend_effect(self):
        card = next(c for c in self.rhinar.hand if c.name == "Pack Call")
        matching = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_DEFEND
            and e.action == EffectAction.REVEAL_TOP_DECK_POWER_CHECK
        ]
        self.assertEqual(len(matching), 1,
                         "Pack Call must have exactly one REVEAL_TOP_DECK_POWER_CHECK ON_DEFEND effect")


class TestPackCallDefendTopCardStaysOnTop(unittest.TestCase):
    """Top card is Wounded Bull (power 6) — should remain on top."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_defend(self.env)

    def test_deck_top_is_wounded_bull_before_defend(self):
        self.assertEqual(self.rhinar.deck[0].name, "Wounded Bull")
        self.assertEqual(self.rhinar.deck[0].power, 6)

    def test_wounded_bull_stays_on_top_after_defend(self):
        deck_before = [c.name for c in self.rhinar.deck]
        _defend_with_pack_call(self.env, self.rhinar)
        self.assertEqual(self.rhinar.deck[0].name, "Wounded Bull",
                         "6-power card should stay on top of the deck")
        self.assertEqual([c.name for c in self.rhinar.deck], deck_before,
                         "Deck order should be unchanged when top card has 6+ power")

    def test_pack_call_in_graveyard_after_defend(self):
        _defend_with_pack_call(self.env, self.rhinar)
        # Combat chain closes and defending cards go to graveyard
        grave_names = [c.name for c in self.rhinar.graveyard]
        self.assertIn("Pack Call", grave_names)

    def test_rhinar_life_unchanged_after_full_block(self):
        # Dawnblade power=2, Pack Call defense=3 → fully blocked
        _defend_with_pack_call(self.env, self.rhinar)
        self.assertEqual(self.rhinar.life, 20)


class TestPackCallDefendLowPowerCardMovesToBottom(unittest.TestCase):
    """Manually place a low-power card on top; it should be moved to the bottom."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_defend(self.env)
        # Swap top card for a known low-power card (Dodge, power=0)
        low_power = next(c for c in self.rhinar.deck if c.power < 6)
        self.rhinar.deck.remove(low_power)
        self.rhinar.deck.insert(0, low_power)
        self.low_power_card = low_power

    def test_low_power_card_moved_to_bottom(self):
        deck_size = len(self.rhinar.deck)
        _defend_with_pack_call(self.env, self.rhinar)
        self.assertEqual(self.rhinar.deck[-1].name, self.low_power_card.name,
                         "Card with less than 6 power should move to the bottom of the deck")

    def test_deck_size_unchanged_after_move(self):
        deck_size = len(self.rhinar.deck)
        _defend_with_pack_call(self.env, self.rhinar)
        self.assertEqual(len(self.rhinar.deck), deck_size,
                         "Deck size must not change after Pack Call reveal")

    def test_low_power_card_not_on_top_after_defend(self):
        _defend_with_pack_call(self.env, self.rhinar)
        self.assertNotEqual(self.rhinar.deck[0].name, self.low_power_card.name,
                            "Low-power card must not remain on top after Pack Call defend")


class TestPackCallDefendEmptyDeck(unittest.TestCase):
    """Effect does nothing when the deck is empty."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_defend(self.env)
        self.rhinar.deck.clear()

    def test_no_error_with_empty_deck(self):
        try:
            _defend_with_pack_call(self.env, self.rhinar)
        except Exception as e:
            self.fail(f"Pack Call defend on empty deck raised {e}")

    def test_deck_stays_empty(self):
        _defend_with_pack_call(self.env, self.rhinar)
        self.assertEqual(len(self.rhinar.deck), 0)


if __name__ == "__main__":
    unittest.main()

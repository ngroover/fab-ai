"""
Unit tests for Bare Fangs (Red).

Card text: "When you attack with Bare Fangs, draw a card then discard a random card.
If a card with 6 or more power is discarded this way, Bare Fangs gets +2 power."
Bare Fangs (6 power) can't be used to defend.

Test seeds:
  SEED_BONUS = 42    — Bare Fangs discards Wild Ride (power 6) → +2 power (8 total)
  SEED_NO_BONUS = 9  — Bare Fangs discards Titanium Bauble (power 0) → stays at 6

A 6+ power discard during the action phase also triggers Rhinar's hero ability
(intimidate), so Dorinthea banishes one card in the bonus case.
"""

import unittest

from fab_env import FaBEnv, Phase
from cards import build_rhinar_deck, build_dorinthea_deck, CARD_CATALOG
from actions import ActionType


SEED_BONUS = 42
SEED_NO_BONUS = 9


def _setup_and_play_bare_fangs(seed: int) -> FaBEnv:
    """Reset at *seed*, make Rhinar go first, play Bare Fangs, pitch greedily."""
    env = FaBEnv()
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=seed)

    legal = env.legal_actions()
    chooser = env._game.players[env._game.active_player_idx]
    if chooser.name == "Rhinar":
        env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))
    else:
        env.step(next(a for a in legal if a.action_type == ActionType.GO_SECOND))

    env.step(next(
        a for a in env.legal_actions()
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Bare Fangs"
    ))

    while env._phase in (Phase.PITCH, Phase.INSTANT):
        env.step(env.legal_actions()[0])

    return env


def _commit_no_block(env: FaBEnv) -> None:
    env.step(next(
        a for a in env.legal_actions()
        if a.action_type == ActionType.DEFEND
        and a.hand_index is None
        and a.equip_slot is None
    ))


class TestBareFangsCardDefinition(unittest.TestCase):
    def test_stats(self):
        card = CARD_CATALOG["bare_fangs_red"]
        self.assertEqual(card.cost, 2)
        self.assertEqual(card.power, 6)
        self.assertEqual(card.pitch, 1)

    def test_cannot_block(self):
        self.assertTrue(CARD_CATALOG["bare_fangs_red"].no_block)


class TestBareFangsPowerBonus(unittest.TestCase):
    """Seed 42 — Bare Fangs discards Wild Ride (power 6) → +2 power."""

    def setUp(self):
        self.env = _setup_and_play_bare_fangs(SEED_BONUS)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

    def test_phase_is_defend(self):
        self.assertEqual(self.env._phase, Phase.DEFEND)

    def test_pending_attack_is_bare_fangs(self):
        self.assertEqual(self.env._pending_attack.name, "Bare Fangs")

    def test_drew_then_discarded(self):
        """Hand after play + pitch was 2; draw 1, discard 1 → still 2."""
        self.assertEqual(len(self.rhinar.hand), 2)

    def test_six_power_card_discarded(self):
        self.assertIn("Wild Ride", [c.name for c in self.rhinar.graveyard])

    def test_attack_power_is_8(self):
        self.assertEqual(self.env._pending_attack_power, 8)

    def test_catalog_card_not_mutated(self):
        self.assertEqual(CARD_CATALOG["bare_fangs_red"].power, 6)

    def test_hero_ability_intimidates(self):
        """Rhinar's hero ability fires on the 6+ power discard."""
        self.assertTrue(self.rhinar.intimidated_this_turn)
        self.assertEqual(len(self.dorinthea.banished), 1)
        self.assertEqual(len(self.dorinthea.hand), 3)

    def test_dorinthea_takes_8_damage(self):
        _commit_no_block(self.env)
        self.assertEqual(self.dorinthea.life, 12)


class TestBareFangsNoBonus(unittest.TestCase):
    """Seed 9 — Bare Fangs discards Titanium Bauble (power 0) → no bonus."""

    def setUp(self):
        self.env = _setup_and_play_bare_fangs(SEED_NO_BONUS)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

    def test_low_power_card_discarded(self):
        self.assertIn("Titanium Bauble", [c.name for c in self.rhinar.graveyard])

    def test_attack_power_stays_6(self):
        self.assertEqual(self.env._pending_attack_power, 6)

    def test_no_intimidate(self):
        self.assertFalse(self.rhinar.intimidated_this_turn)
        self.assertEqual(self.dorinthea.banished, [])

    def test_dorinthea_takes_6_damage(self):
        _commit_no_block(self.env)
        self.assertEqual(self.dorinthea.life, 14)


if __name__ == "__main__":
    unittest.main(verbosity=2)

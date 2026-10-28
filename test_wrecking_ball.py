"""
Unit tests for Wrecking Ball (Red).

Card text: "When you attack with Wrecking Ball, draw a card then discard a random card.
If a card with 6 or more power is discarded this way, intimidate."
Wrecking Ball (6 power, cost 3) can't be used to defend.

Test seeds:
  SEED_INTIMIDATE = 7      — discards Raging Onslaught (power 6) → intimidate
  SEED_NO_INTIMIDATE = 23  — discards Come to Fight (power 0) → no intimidate

A 6+ power discard during the action phase also triggers Rhinar's hero ability,
so in the intimidate case Dorinthea banishes two cards: one from Wrecking Ball
and one from Rhinar.
"""

import unittest

from fab_env import FaBEnv, Phase
from cards import build_rhinar_deck, build_dorinthea_deck, CARD_CATALOG
from actions import ActionType


SEED_INTIMIDATE = 7
SEED_NO_INTIMIDATE = 23


def _setup_and_play_wrecking_ball(seed: int) -> FaBEnv:
    """Reset at *seed*, make Rhinar go first, play Wrecking Ball, pitch greedily."""
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
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Wrecking Ball"
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


class TestWreckingBallCardDefinition(unittest.TestCase):
    def test_stats(self):
        card = CARD_CATALOG["wrecking_ball_red"]
        self.assertEqual(card.cost, 3)
        self.assertEqual(card.power, 6)
        self.assertEqual(card.pitch, 1)

    def test_cannot_block(self):
        self.assertTrue(CARD_CATALOG["wrecking_ball_red"].no_block)


class TestWreckingBallIntimidate(unittest.TestCase):
    """Seed 7 — discards Raging Onslaught (power 6) → intimidate."""

    def setUp(self):
        self.env = _setup_and_play_wrecking_ball(SEED_INTIMIDATE)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

    def test_phase_is_defend(self):
        self.assertEqual(self.env._phase, Phase.DEFEND)

    def test_six_power_card_discarded(self):
        self.assertIn("Raging Onslaught", [c.name for c in self.rhinar.graveyard])

    def test_intimidated_this_turn(self):
        self.assertTrue(self.rhinar.intimidated_this_turn)

    def test_card_and_hero_each_banish_one(self):
        """Wrecking Ball's intimidate plus Rhinar's hero intimidate → 2 banished."""
        self.assertEqual(len(self.dorinthea.banished), 2)
        self.assertEqual(len(self.dorinthea.hand), 2)

    def test_power_unchanged(self):
        """Wrecking Ball's bonus is intimidate, not power."""
        self.assertEqual(self.env._pending_attack_power, 6)

    def test_dorinthea_takes_6_damage(self):
        _commit_no_block(self.env)
        self.assertEqual(self.dorinthea.life, 14)


class TestWreckingBallNoIntimidate(unittest.TestCase):
    """Seed 23 — discards Come to Fight (power 0) → no intimidate."""

    def setUp(self):
        self.env = _setup_and_play_wrecking_ball(SEED_NO_INTIMIDATE)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

    def test_low_power_card_discarded(self):
        self.assertIn("Come to Fight", [c.name for c in self.rhinar.graveyard])

    def test_no_intimidate(self):
        self.assertFalse(self.rhinar.intimidated_this_turn)
        self.assertEqual(self.dorinthea.banished, [])
        self.assertEqual(len(self.dorinthea.hand), 4)


if __name__ == "__main__":
    unittest.main(verbosity=2)

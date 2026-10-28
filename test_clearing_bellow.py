"""
Unit tests for Clearing Bellow (Blue).

Card text: "Intimidate. Go again."
Clearing Bellow is a cost 0 Brute non-attack action. Intimidate makes the
defending hero banish a random card from hand face-down until end of turn.

Test seed:
  SEED = 39 — Rhinar opens with Clearing Bellow; intimidate banishes Titanium Bauble.
"""

import unittest

from fab_env import FaBEnv, Phase
from cards import build_rhinar_deck, build_dorinthea_deck, CARD_CATALOG, CardType, Keyword
from actions import ActionType


SEED = 39


def _setup_and_play(seed: int) -> FaBEnv:
    """Reset at *seed*, make Rhinar go first, play Clearing Bellow."""
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
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Clearing Bellow"
    ))

    while env._phase in (Phase.PITCH, Phase.INSTANT):
        env.step(env.legal_actions()[0])

    return env


def _finish_turn(env: FaBEnv) -> None:
    """Pass and store nothing until Rhinar's turn ends."""
    for _ in range(40):
        if env._game.active_player_idx != 0:
            return
        legal = env.legal_actions()
        pick = (
            next((a for a in legal if a.action_type in (ActionType.PASS_PRIORITY, ActionType.PASS)), None)
            or next((a for a in legal if a.action_type == ActionType.ARSENAL
                     and a.hand_index is None), None)
            or legal[0]
        )
        env.step(pick)


class TestClearingBellowCardDefinition(unittest.TestCase):
    def test_stats(self):
        card = CARD_CATALOG["clearing_bellow_blue"]
        self.assertEqual(card.cost, 0)
        self.assertEqual(card.pitch, 3)
        self.assertEqual(card.defense, 3)

    def test_is_non_attack_action(self):
        card_type = CARD_CATALOG["clearing_bellow_blue"].card_type
        self.assertIn(CardType.ACTION, card_type)
        self.assertNotIn(CardType.ATTACK, card_type)

    def test_keywords(self):
        keywords = CARD_CATALOG["clearing_bellow_blue"].keywords
        self.assertIn(Keyword.INTIMIDATE, keywords)
        self.assertIn(Keyword.GO_AGAIN, keywords)


class TestClearingBellowPlay(unittest.TestCase):
    def setUp(self):
        self.env = _setup_and_play(SEED)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

    def test_go_again_returns_to_attack_phase(self):
        """Clearing Bellow resolves without a combat step and Rhinar keeps acting."""
        self.assertEqual(self.env._phase, Phase.ATTACK)
        self.assertEqual(self.env._game.active_player_idx, 0)
        self.assertEqual(self.rhinar.action_points, 1)

    def test_no_resources_spent(self):
        """Cost 0 → no card pitched to play it."""
        self.assertEqual(self.rhinar.pitch_zone, [])

    def test_goes_to_graveyard(self):
        self.assertIn("Clearing Bellow", [c.name for c in self.rhinar.graveyard])

    def test_one_card_banished(self):
        self.assertEqual([c.name for c in self.dorinthea.banished], ["Titanium Bauble"])
        self.assertEqual(len(self.dorinthea.hand), 3)

    def test_intimidated_this_turn(self):
        self.assertTrue(self.rhinar.intimidated_this_turn)

    def test_deals_no_damage(self):
        self.assertEqual(self.dorinthea.life, 20)

    def test_can_play_another_card(self):
        plays = [a for a in self.env.legal_actions() if a.action_type == ActionType.PLAY_CARD]
        self.assertTrue(plays)

    def test_banished_card_returns_at_end_of_turn(self):
        _finish_turn(self.env)
        self.assertEqual(self.dorinthea.banished, [])
        self.assertEqual(len(self.dorinthea.hand), 4)
        self.assertIn("Titanium Bauble", [c.name for c in self.dorinthea.hand])


if __name__ == "__main__":
    unittest.main(verbosity=2)

"""
Unit tests for Smash Instinct (Yellow).

Card text: "Intimidate."
Smash Instinct is a 6 power Brute attack action (cost 3). Intimidate makes the defending
hero banish a random card from hand face-down until end of turn.

Test seed:
  SEED = 14 — Rhinar opens with Smash Instinct; intimidate banishes Titanium Bauble.
"""

import unittest

from fab_env import FaBEnv, Phase
from cards import build_rhinar_deck, build_dorinthea_deck, CARD_CATALOG, Keyword
from actions import ActionType


SEED = 14


def _setup_and_play(seed: int) -> FaBEnv:
    """Reset at *seed*, make Rhinar go first, play Smash Instinct, pitch greedily."""
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
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Smash Instinct"
    ))

    while env._phase in (Phase.PITCH, Phase.INSTANT):
        env.step(env.legal_actions()[0])

    return env


def _finish_turn(env: FaBEnv) -> None:
    """Pass, take no block and store nothing until Rhinar's turn ends."""
    for _ in range(40):
        if env._game.active_player_idx != 0:
            return
        legal = env.legal_actions()
        pick = (
            next((a for a in legal if a.action_type in (ActionType.PASS_PRIORITY, ActionType.PASS)), None)
            or next((a for a in legal if a.action_type == ActionType.DEFEND
                     and a.hand_index is None and a.equip_slot is None), None)
            or next((a for a in legal if a.action_type == ActionType.ARSENAL
                     and a.hand_index is None), None)
            or legal[0]
        )
        env.step(pick)


class TestSmashInstinctCardDefinition(unittest.TestCase):
    def test_stats(self):
        card = CARD_CATALOG["smash_instinct_yellow"]
        self.assertEqual(card.cost, 3)
        self.assertEqual(card.power, 6)
        self.assertEqual(card.pitch, 2)
        self.assertEqual(card.defense, 3)

    def test_has_intimidate(self):
        self.assertIn(Keyword.INTIMIDATE, CARD_CATALOG["smash_instinct_yellow"].keywords)

    def test_no_go_again(self):
        self.assertNotIn(Keyword.GO_AGAIN, CARD_CATALOG["smash_instinct_yellow"].keywords)


class TestSmashInstinctIntimidate(unittest.TestCase):
    def setUp(self):
        self.env = _setup_and_play(SEED)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

    def test_phase_is_defend(self):
        self.assertEqual(self.env._phase, Phase.DEFEND)

    def test_pending_attack(self):
        self.assertEqual(self.env._pending_attack.name, "Smash Instinct")

    def test_one_card_banished(self):
        self.assertEqual([c.name for c in self.dorinthea.banished], ["Titanium Bauble"])
        self.assertEqual(len(self.dorinthea.hand), 3)

    def test_intimidated_this_turn(self):
        self.assertTrue(self.rhinar.intimidated_this_turn)

    def test_no_go_again(self):
        """Without go again, combat resolving sends Rhinar straight to ARSENAL."""
        self.env.step(next(
            a for a in self.env.legal_actions()
            if a.action_type == ActionType.DEFEND
            and a.hand_index is None and a.equip_slot is None
        ))
        self.assertEqual(self.env._phase, Phase.ARSENAL)
        self.assertEqual(self.rhinar.action_points, 0)

    def test_dorinthea_takes_6_damage(self):
        _finish_turn(self.env)
        self.assertEqual(self.dorinthea.life, 14)

    def test_banished_card_returns_at_end_of_turn(self):
        _finish_turn(self.env)
        self.assertEqual(self.dorinthea.banished, [])
        self.assertEqual(len(self.dorinthea.hand), 4)
        self.assertIn("Titanium Bauble", [c.name for c in self.dorinthea.hand])


if __name__ == "__main__":
    unittest.main(verbosity=2)

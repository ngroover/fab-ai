"""
Tests for Come to Fight (Blue).

Seed 20 gives:
  Dorinthea wins the coin flip; GO_SECOND puts Rhinar first.
  Rhinar:     Wounded Bull, Pack Hunt, Beast Mode, Come to Fight
  Dorinthea:  (various)

Come to Fight should:
  - Cost 1 resource, pitch for 3, have Go Again keyword
  - On play: set next_attack_power_bonus += 1 (NOT next_attack_go_again)
  - Next attack ACTION card you play this turn gains +1 power
  - Weapon attacks do NOT receive the +1 power bonus
  - The +1 bonus is preserved (not consumed) after a weapon attack
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, Keyword
from cards import build_rhinar_deck, build_dorinthea_deck

SEED = 20  # Dorinthea wins coin flip; GO_SECOND → Rhinar goes first
           # Rhinar hand: Wounded Bull, Pack Hunt, Beast Mode, Come to Fight


def _setup_env():
    env = FaBEnv(verbose=False)
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    # Dorinthea is choosing player; GO_SECOND makes Rhinar go first
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.GO_SECOND))
    return env


def _play_come_to_fight(env):
    """Play Come to Fight and pitch Wounded Bull (index 0) to cover cost 1."""
    rhinar = env._game.players[0]
    ctf_action = next(a for a in env.legal_actions() if a.card and a.card.name == "Come to Fight")
    env.step(ctf_action)
    # Pitch Wounded Bull (first in hand, pitch=2 >= cost=1)
    env.step(env.legal_actions()[0])
    # Drain instant window (Dorinthea may pass priority)
    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])
    return rhinar


class TestComeToFightCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = _setup_env()
        self.rhinar = self.env._game.players[0]

    def test_in_rhinar_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Come to Fight", names)

    def test_card_properties(self):
        card = next(c for c in self.rhinar.hand if c.name == "Come to Fight")
        self.assertIn(CardType.ACTION, card.card_type)
        self.assertEqual(card.cost, 1)
        self.assertEqual(card.pitch, 3)
        self.assertEqual(card.power, 0)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.BLUE)

    def test_go_again_keyword(self):
        card = next(c for c in self.rhinar.hand if c.name == "Come to Fight")
        self.assertIn(Keyword.GO_AGAIN, card.keywords)


class TestComeToFightEffect(unittest.TestCase):
    """Come to Fight sets next_attack_power_bonus = 1 on play."""

    def setUp(self):
        self.env = _setup_env()
        self.rhinar = _play_come_to_fight(self.env)

    def test_sets_next_attack_power_bonus(self):
        self.assertEqual(self.rhinar.next_attack_power_bonus, 1,
                         "Come to Fight must set next_attack_power_bonus to 1")

    def test_does_not_set_next_attack_go_again(self):
        self.assertFalse(self.rhinar.next_attack_go_again,
                         "Come to Fight effect is +1 power, not go again")

    def test_phase_returns_to_attack(self):
        self.assertEqual(self.env._phase, Phase.ATTACK,
                         "After Come to Fight resolves (go again), phase must be ATTACK")

    def test_come_to_fight_in_graveyard(self):
        grave_names = [c.name for c in self.rhinar.graveyard]
        self.assertIn("Come to Fight", grave_names)


class TestComeToFightBonusOnAttackCard(unittest.TestCase):
    """The +1 power applies to the next attack action card played."""

    def setUp(self):
        self.env = _setup_env()
        self.rhinar = _play_come_to_fight(self.env)

        # Play Pack Hunt (base power=6) — Beast Mode auto-pitched for cost
        pack_action = next(
            a for a in self.env.legal_actions()
            if a.card and a.card.name == "Pack Hunt"
        )
        self.env.step(pack_action)
        while self.env._phase == Phase.PITCH:
            self.env.step(self.env.legal_actions()[0])
        # Drain instant window opened after attack declaration
        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

    def test_attack_card_gets_plus_one_power(self):
        self.assertEqual(self.env._pending_attack_power, 7,
                         "Pack Hunt (base 6) must gain +1 from Come to Fight → power 7")

    def test_bonus_consumed_after_attack_card(self):
        self.assertEqual(self.rhinar.next_attack_power_bonus, 0,
                         "next_attack_power_bonus must be consumed after the attack card is played")


class TestComeToFightBonusNotOnWeaponAttack(unittest.TestCase):
    """Weapon attacks must NOT receive the Come to Fight +1 power bonus."""

    def setUp(self):
        self.env = _setup_env()
        self.rhinar = _play_come_to_fight(self.env)

        # Use weapon attack (Bone Basher, base power=4, cost=2)
        weapon_action = next(
            a for a in self.env.legal_actions()
            if a.action_type == ActionType.WEAPON
        )
        self.env.step(weapon_action)
        while self.env._phase == Phase.PITCH:
            self.env.step(self.env.legal_actions()[0])
        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

    def test_weapon_attack_does_not_get_bonus(self):
        self.assertEqual(self.env._pending_attack_power, 4,
                         "Bone Basher (base 4) must NOT gain +1 from Come to Fight → power stays 4")

    def test_bonus_preserved_after_weapon_attack(self):
        self.assertEqual(self.rhinar.next_attack_power_bonus, 1,
                         "next_attack_power_bonus must NOT be consumed by a weapon attack")


if __name__ == "__main__":
    unittest.main()

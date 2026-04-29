"""
Tests for Dawnblade, Resplendent weapon behavior.

Dawnblade has base power 2. The second time Dorinthea attacks with it each
turn, it gains +1 power (total 3) until end of turn.
"""

import unittest

from fab_env import FaBEnv
from cards import build_rhinar_deck, build_dorinthea_deck


class TestDawnbladeBasePower(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=42)
        self.dorinthea = self.env._game.players[1]

    def test_base_power_is_2(self):
        self.assertEqual(self.dorinthea.weapon.power, 2)

    def test_first_attack_power_is_2(self):
        # weapon_attack_count starts at 0 each turn — first attack has no bonus
        self.assertEqual(self.dorinthea.weapon_attack_count, 0)
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), 2)

    def test_second_attack_power_is_3(self):
        # After the first attack, weapon_attack_count == 1 — second swing gets +1
        self.dorinthea.weapon_attack_count = 1
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), 3)

    def test_power_bonus_resets_each_turn(self):
        # Simulate a full turn cycle: bonus applies mid-turn, resets on new turn
        self.dorinthea.weapon_attack_count = 1
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), 3)

        self.dorinthea.reset_turn_resources()
        self.assertEqual(self.dorinthea.weapon_attack_count, 0)
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), 2)


class TestDawnbladeCounters(unittest.TestCase):
    """dawnblade_counters is a persistent +1 power counter placed by Glistening Steelblade."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=42)
        self.dorinthea = self.env._game.players[1]

    def test_dawnblade_counters_starts_at_zero(self):
        self.assertEqual(self.dorinthea.dawnblade_counters, 0)

    def test_counters_increase_effective_power(self):
        self.dorinthea.dawnblade_counters = 2
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), 4)

    def test_counters_not_reset_between_turns(self):
        self.dorinthea.dawnblade_counters = 3
        self.dorinthea.reset_turn_resources()
        self.assertEqual(self.dorinthea.dawnblade_counters, 3)


if __name__ == "__main__":
    unittest.main()

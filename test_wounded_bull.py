"""
Unit tests for Wounded Bull's conditional power bonus.

Card text: When you play Wounded Bull, if you have less health than an opposing
hero, it gains +1 power.

Seed 14 gives Rhinar's opening hand:
  Wrecker Romp, Wounded Bull, Smash Instinct, Chief Ruk'utan
  Rhinar wins the coin flip and goes first.

Test approach: reset at seed 14, advance past CHOOSE_FIRST, then manually set
life totals and pre-pay resources so Wounded Bull can be played in isolation.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import build_rhinar_deck, build_dorinthea_deck, CardType
from card_effects import EffectTrigger, EffectAction

SEED = 14


def _setup(env):
    """Reset at SEED, advance past CHOOSE_FIRST (Rhinar goes first).

    Returns (rhinar, dorinthea).
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))
    rhinar = env._game.players[0]
    dorinthea = env._game.players[1]
    return rhinar, dorinthea


def _play_wounded_bull(env, rhinar):
    """Isolate Wounded Bull in hand, pre-pay resources, and play it.

    Advances through any INSTANT windows so the ON_ATTACK effect fires.
    Leaves env in DEFEND phase with _pending_attack_power set.
    """
    card = next(c for c in rhinar.hand if c.name == "Wounded Bull")
    rhinar.hand = [card]
    rhinar.resource_points = 3  # Wounded Bull costs 3
    legal = env.legal_actions()
    action = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Wounded Bull"
    )
    env.step(action)
    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])


class TestWoundedBullCardDefinition(unittest.TestCase):
    """Verify the card's static properties and effect attachment."""

    def setUp(self):
        env = FaBEnv(verbose=False)
        env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        env.step(env.legal_actions()[0])  # resolve CHOOSE_FIRST
        rhinar = env._game.players[0]
        self.card = next(c for c in rhinar.hand if c.name == "Wounded Bull")

    def test_base_power_is_6(self):
        self.assertEqual(self.card.power, 6)

    def test_cost_is_3(self):
        self.assertEqual(self.card.cost, 3)

    def test_defense_is_2(self):
        self.assertEqual(self.card.defense, 2)

    def test_has_on_attack_effect(self):
        triggers = [e.trigger for e in self.card.effects]
        self.assertIn(EffectTrigger.ON_ATTACK, triggers)

    def test_effect_action_is_power_boost_if_lower_life(self):
        effect = next(e for e in self.card.effects if e.trigger == EffectTrigger.ON_ATTACK)
        self.assertEqual(effect.action, EffectAction.ATTACK_POWER_BOOST_IF_LOWER_LIFE)

    def test_effect_magnitude_is_1(self):
        effect = next(e for e in self.card.effects if e.trigger == EffectTrigger.ON_ATTACK)
        self.assertEqual(effect.magnitude, 1)

    def test_text_mentions_less_health(self):
        self.assertIn("less health", self.card.text.lower())


class TestWoundedBullLowerLifeBonus(unittest.TestCase):
    """Effect fires when attacker (Rhinar) has strictly less life than defender."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _setup(self.env)

    def test_power_is_7_when_rhinar_has_lower_life(self):
        self.rhinar.life = 15
        self.dorinthea.life = 20
        _play_wounded_bull(self.env, self.rhinar)
        self.assertEqual(self.env._pending_attack_power, 7)

    def test_power_is_7_when_rhinar_has_one_less_life(self):
        self.rhinar.life = 19
        self.dorinthea.life = 20
        _play_wounded_bull(self.env, self.rhinar)
        self.assertEqual(self.env._pending_attack_power, 7)

    def test_in_defend_phase_after_lower_life_attack(self):
        self.rhinar.life = 10
        self.dorinthea.life = 20
        _play_wounded_bull(self.env, self.rhinar)
        self.assertEqual(self.env._phase, Phase.DEFEND)


class TestWoundedBullNoBonus(unittest.TestCase):
    """Effect does NOT fire when attacker life is equal to or greater than defender's."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _setup(self.env)

    def test_power_is_6_when_life_is_equal(self):
        self.rhinar.life = 20
        self.dorinthea.life = 20
        _play_wounded_bull(self.env, self.rhinar)
        self.assertEqual(self.env._pending_attack_power, 6)

    def test_power_is_6_when_rhinar_has_more_life(self):
        self.rhinar.life = 20
        self.dorinthea.life = 15
        _play_wounded_bull(self.env, self.rhinar)
        self.assertEqual(self.env._pending_attack_power, 6)

    def test_power_is_6_when_dorinthea_has_one_less_life(self):
        self.rhinar.life = 20
        self.dorinthea.life = 19
        _play_wounded_bull(self.env, self.rhinar)
        self.assertEqual(self.env._pending_attack_power, 6)


if __name__ == "__main__":
    unittest.main()

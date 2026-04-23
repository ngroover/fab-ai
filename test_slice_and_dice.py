"""
Unit tests for Slice and Dice card effect.

Seed 24 gives:
  Rhinar:     Pack Hunt, Titanium Bauble, Smash Instinct, Beast Mode
  Dorinthea:  Slice and Dice, En Garde, Out for Blood, Titanium Bauble
  Dorinthea (agent_1) wins the coin flip and chooses to go first.

Slice and Dice should:
  - Activate a per-swing weapon power bonus (via WEAPON_ATTACK_BONUS_PER_SWING effect)
  - Give the first weapon attack this turn +1 power
  - Give the second weapon attack this turn +2 power
  - Have no bonus after the second weapon attack
  - Reset at the start of a new turn
  - Have go again so the action phase continues
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from card_effects import EffectTrigger, EffectAction

SEED = 24  # Dorinthea has Slice and Dice; she wins coin flip


def _advance_to_dorinthea_attack_phase(env):
    """Reset at SEED and step to Dorinthea's ATTACK phase."""
    env.reset(seed=SEED)
    # Dorinthea (agent_1) won the coin flip — GO_FIRST puts her active
    legal = env.legal_actions()
    go_first = next(a for a in legal if a.action_type == ActionType.GO_FIRST)
    env.step(go_first)
    assert env._phase == Phase.ATTACK
    assert env._game.active_player_idx == 1  # Dorinthea


def _play_slice_and_dice(env):
    """Play Slice and Dice (cost 0, go again — stays in ATTACK phase)."""
    legal = env.legal_actions()
    action = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Slice and Dice"
    )
    env.step(action)
    assert env._phase == Phase.ATTACK, f"Expected ATTACK after go-again, got {env._phase}"


class TestSliceAndDiceCardDefinition(unittest.TestCase):
    """Verify the card is correctly defined in the catalog."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Slice and Dice", names)

    def test_card_properties(self):
        from cards import CardType, Color
        card = next(c for c in self.dorinthea.hand if c.name == "Slice and Dice")
        self.assertEqual(card.card_type, CardType.ACTION)
        self.assertEqual(card.cost, 0)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.YELLOW)
        from cards import Keyword
        self.assertIn(Keyword.GO_AGAIN, card.keywords)

    def test_has_weapon_attack_bonus_per_swing_effect(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Slice and Dice")
        matching = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_PLAY
            and e.action == EffectAction.WEAPON_ATTACK_BONUS_PER_SWING
        ]
        self.assertEqual(len(matching), 1,
                         "Slice and Dice must have exactly one WEAPON_ATTACK_BONUS_PER_SWING ON_PLAY effect")


class TestSliceAndDiceActivatesState(unittest.TestCase):
    """After playing Slice and Dice the player's slice_and_dice_active flag is set."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_dorinthea_attack_phase(self.env)
        self.dorinthea = self.env._game.players[1]

    def test_inactive_before_play(self):
        self.assertFalse(self.dorinthea.slice_and_dice_active)

    def test_active_after_play(self):
        _play_slice_and_dice(self.env)
        self.assertTrue(self.dorinthea.slice_and_dice_active)

    def test_still_in_attack_phase_go_again(self):
        _play_slice_and_dice(self.env)
        self.assertEqual(self.env._phase, Phase.ATTACK)

    def test_resets_on_new_turn(self):
        _play_slice_and_dice(self.env)
        self.dorinthea.reset_turn_resources()
        self.assertFalse(self.dorinthea.slice_and_dice_active)


class TestSliceAndDiceWeaponPowerBonus(unittest.TestCase):
    """get_effective_weapon_power() returns the right value for each swing."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_dorinthea_attack_phase(self.env)
        self.dorinthea = self.env._game.players[1]
        _play_slice_and_dice(self.env)
        # Dawnblade base power is 2
        self._base = self.dorinthea.weapon.power  # 2

    def test_first_weapon_attack_plus_one(self):
        # weapon_attack_count == 0 → +1 from Slice and Dice
        self.dorinthea.weapon_attack_count = 0
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), self._base + 1)

    def test_second_weapon_attack_plus_two(self):
        # weapon_attack_count == 1 → Dawnblade +1 AND Slice and Dice +2
        self.dorinthea.weapon_attack_count = 1
        dawnblade_bonus = 1
        self.assertEqual(
            self.dorinthea.get_effective_weapon_power(),
            self._base + dawnblade_bonus + 2,
        )

    def test_no_bonus_after_two_attacks(self):
        # weapon_attack_count >= 2 → Slice and Dice exhausted; Dawnblade still +1
        self.dorinthea.weapon_attack_count = 2
        dawnblade_bonus = 1
        self.assertEqual(
            self.dorinthea.get_effective_weapon_power(),
            self._base + dawnblade_bonus,
        )

    def test_no_bonus_without_slice_and_dice(self):
        # Slice and Dice not played → no per-swing bonus
        self.env2 = FaBEnv(verbose=False)
        _advance_to_dorinthea_attack_phase(self.env2)
        d = self.env2._game.players[1]
        self.assertFalse(d.slice_and_dice_active)
        d.weapon_attack_count = 0
        self.assertEqual(d.get_effective_weapon_power(), self._base)
        d.weapon_attack_count = 1
        self.assertEqual(d.get_effective_weapon_power(), self._base + 1)  # Dawnblade only


if __name__ == "__main__":
    unittest.main(verbosity=2)

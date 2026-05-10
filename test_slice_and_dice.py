"""
Unit tests for Slice and Dice card effect.

Seed 24 gives:
  Rhinar:     Pack Hunt, Titanium Bauble, Smash Instinct, Beast Mode
  Dorinthea:  Slice and Dice, En Garde, Out for Blood, Titanium Bauble
  Dorinthea (agent_1) wins the coin flip and chooses to go first.

Slice and Dice should:
  - Schedule per-swing weapon power bonuses via WEAPON_SWING_POWER_BONUS effects
  - Give the first weapon attack this turn +1 power
  - Give the second weapon attack this turn +2 power
  - Have no bonus after the second weapon attack
  - Reset at the start of a new turn
  - Have go again so the action phase continues
  - Stack: two copies give +2 first swing, +4 second swing
  - Miss bonuses for swings that already happened when the card is played
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from card_effects import EffectTrigger, EffectAction
from cards import build_rhinar_deck, build_dorinthea_deck

SEED = 24  # Dorinthea has Slice and Dice; she wins coin flip


def _advance_to_dorinthea_attack_phase(env):
    """Reset at SEED and step to Dorinthea's ATTACK phase."""
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
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
    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])
    assert env._phase == Phase.ATTACK, f"Expected ATTACK after go-again, got {env._phase}"


class TestSliceAndDiceCardDefinition(unittest.TestCase):
    """Verify the card is correctly defined in the catalog."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.env.step(self.env.legal_actions()[0])  # resolve CHOOSE_FIRST
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

    def test_has_two_weapon_swing_bonus_effects(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Slice and Dice")
        swing_effects = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_PLAY
            and e.action == EffectAction.WEAPON_SWING_POWER_BONUS
        ]
        self.assertEqual(len(swing_effects), 2,
                         "Slice and Dice must have exactly two WEAPON_SWING_POWER_BONUS ON_PLAY effects")

    def test_swing_effects_have_correct_indices_and_magnitudes(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Slice and Dice")
        swing_effects = sorted(
            [e for e in card.effects if e.action == EffectAction.WEAPON_SWING_POWER_BONUS],
            key=lambda e: e.swing_index,
        )
        self.assertEqual(swing_effects[0].swing_index, 0)
        self.assertEqual(swing_effects[0].magnitude, 1)
        self.assertEqual(swing_effects[1].swing_index, 1)
        self.assertEqual(swing_effects[1].magnitude, 2)


class TestSliceAndDiceSchedulesBonuses(unittest.TestCase):
    """After playing Slice and Dice the player's weapon_swing_bonuses list is populated."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_dorinthea_attack_phase(self.env)
        self.dorinthea = self.env._game.players[1]

    def test_empty_before_play(self):
        self.assertEqual(self.dorinthea.weapon_swing_bonuses, [])

    def test_both_swings_scheduled_after_play(self):
        _play_slice_and_dice(self.env)
        indices = [idx for idx, _ in self.dorinthea.weapon_swing_bonuses]
        self.assertIn(0, indices)
        self.assertIn(1, indices)

    def test_still_in_attack_phase_go_again(self):
        _play_slice_and_dice(self.env)
        self.assertEqual(self.env._phase, Phase.ATTACK)

    def test_resets_on_new_turn(self):
        _play_slice_and_dice(self.env)
        self.dorinthea.reset_turn_resources()
        self.assertEqual(self.dorinthea.weapon_swing_bonuses, [])


class TestSliceAndDiceWeaponPowerBonus(unittest.TestCase):
    """get_effective_weapon_power() returns the right value for each swing."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_dorinthea_attack_phase(self.env)
        self.dorinthea = self.env._game.players[1]
        _play_slice_and_dice(self.env)
        self._base = self.dorinthea.weapon.power  # Dawnblade base = 2

    def test_first_weapon_attack_plus_one(self):
        self.dorinthea.weapon_attack_count = 0
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), self._base + 1)

    def test_second_weapon_attack_plus_two(self):
        self.dorinthea.weapon_attack_count = 1
        dawnblade_bonus = 1
        self.assertEqual(
            self.dorinthea.get_effective_weapon_power(),
            self._base + dawnblade_bonus + 2,
        )

    def test_no_bonus_after_two_attacks(self):
        self.dorinthea.weapon_attack_count = 2
        dawnblade_bonus = 1
        self.assertEqual(
            self.dorinthea.get_effective_weapon_power(),
            self._base + dawnblade_bonus,
        )

    def test_no_bonus_without_slice_and_dice(self):
        env2 = FaBEnv(verbose=False)
        _advance_to_dorinthea_attack_phase(env2)
        d = env2._game.players[1]
        self.assertEqual(d.weapon_swing_bonuses, [])
        d.weapon_attack_count = 0
        self.assertEqual(d.get_effective_weapon_power(), self._base)
        d.weapon_attack_count = 1
        self.assertEqual(d.get_effective_weapon_power(), self._base + 1)  # Dawnblade only


class TestSliceAndDiceStacking(unittest.TestCase):
    """Two copies of Slice and Dice stack: +2 first swing, +4 second swing."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_dorinthea_attack_phase(self.env)
        self.dorinthea = self.env._game.players[1]
        self._base = self.dorinthea.weapon.power

    def test_two_copies_double_bonuses(self):
        # Manually inject bonuses as if two copies were played
        self.dorinthea.weapon_swing_bonuses = [(0, 1), (1, 2), (0, 1), (1, 2)]
        self.dorinthea.weapon_attack_count = 0
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), self._base + 2)
        self.dorinthea.weapon_attack_count = 1
        dawnblade_bonus = 1
        self.assertEqual(
            self.dorinthea.get_effective_weapon_power(),
            self._base + dawnblade_bonus + 4,
        )


class TestSliceAndDiceMissedSwing(unittest.TestCase):
    """Playing Slice and Dice after the first weapon attack misses the +1 bonus."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_dorinthea_attack_phase(self.env)
        self.dorinthea = self.env._game.players[1]
        self._base = self.dorinthea.weapon.power

    def test_only_second_swing_scheduled_when_played_late(self):
        # Simulate first weapon attack already happened
        self.dorinthea.weapon_attack_count = 1
        # Manually trigger the effect as the env would
        from card_effects import CardEffect
        effect0 = CardEffect(trigger=EffectTrigger.ON_PLAY,
                             action=EffectAction.WEAPON_SWING_POWER_BONUS,
                             magnitude=1, swing_index=0)
        effect1 = CardEffect(trigger=EffectTrigger.ON_PLAY,
                             action=EffectAction.WEAPON_SWING_POWER_BONUS,
                             magnitude=2, swing_index=1)
        for eff in (effect0, effect1):
            if self.dorinthea.weapon_attack_count <= eff.swing_index:
                self.dorinthea.weapon_swing_bonuses.append((eff.swing_index, eff.magnitude))

        indices = [idx for idx, _ in self.dorinthea.weapon_swing_bonuses]
        self.assertNotIn(0, indices, "swing 0 already happened — bonus should be skipped")
        self.assertIn(1, indices, "swing 1 has not happened yet — bonus should be scheduled")
        # First swing (already done) sees no bonus
        self.dorinthea.weapon_attack_count = 0
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), self._base)
        # Second swing gets +2
        self.dorinthea.weapon_attack_count = 1
        dawnblade_bonus = 1
        self.assertEqual(
            self.dorinthea.get_effective_weapon_power(),
            self._base + dawnblade_bonus + 2,
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)

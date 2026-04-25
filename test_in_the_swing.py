"""
Unit tests for "In the Swing" card.

Seed 63 gives Dorinthea: In the Swing (x2), Blade Flash, On a Knife Edge.
Dorinthea wins the coin flip and goes first.

Setup for a second weapon attack:
  1. Play On a Knife Edge (cost 0, go again) → next_weapon_go_again = True
  2. Attack with Dawnblade (first attack) — pitch Blade Flash to cover cost 1;
     remaining 2 resources carry over.
  3. Both players pass the reaction window; go again fires.
  4. Attack with Dawnblade again (second attack) — resources already cover cost.
  5. Rhinar passes the defend step.
  6. In the reaction window play "In the Swing" → +3 power.

Card text: "Play only if you have attacked 2 or more times with weapons this turn.
            Target weapon attack gains +3 power."
  weapon_attack_count == 1 during the second attack's reaction window —
  that count reflects the one completed attack; the current one is the second.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import ActionType
from card_effects import EffectTrigger, EffectAction

SEED = 63  # Dorinthea: In the Swing x2, Blade Flash, On a Knife Edge; she wins flip


def _setup_to_second_weapon_reaction(env):
    """Step env from reset(SEED) to the reaction window of Dawnblade's second attack.

    Returns the pending_attack_power just before 'In the Swing' is played so
    callers can assert the delta applied by the card.
    """
    dorinthea = env._game.players[1]
    rhinar = env._game.players[0]

    # Dorinthea wins coin flip — she goes first
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))

    # Play On a Knife Edge (free, go again) — grants next sword attack go again
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "On a Knife Edge"
    ))

    # First Dawnblade attack — needs 1 resource; pitch Blade Flash (pitch value 3)
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

    bf_idx = next(i for i, c in enumerate(dorinthea.hand) if c.name == "Blade Flash")
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.PITCH and a.pitch_indices == [bf_idx]
    ))

    # Rhinar passes the defend step
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and a.defend_hand_indices == []
        and not a.defend_equip_slots
    ))

    # Both players pass priority in the first attack's reaction window
    for _ in range(2):
        if env._phase == Phase.REACTION:
            legal = env.legal_actions()
            env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

    # Second Dawnblade attack — 2 leftover resources cover the cost; no PITCH phase
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

    # Rhinar passes the defend step for the second attack
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and a.defend_hand_indices == []
        and not a.defend_equip_slots
    ))

    # Now in the reaction window of the second weapon attack
    assert env._phase == Phase.REACTION
    assert dorinthea.weapon_attack_count == 1


class TestInTheSwingCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("In the Swing", names)

    def test_card_properties(self):
        from cards import CardType, Color
        card = next(c for c in self.dorinthea.hand if c.name == "In the Swing")
        self.assertEqual(card.card_type, CardType.ATTACK_REACTION)
        self.assertEqual(card.cost, 0)
        self.assertEqual(card.pitch, 1)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.RED)

    def test_has_attack_power_boost_effect(self):
        card = next(c for c in self.dorinthea.hand if c.name == "In the Swing")
        matching = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_ATTACK_REACTION
            and e.action == EffectAction.ATTACK_POWER_BOOST
            and e.magnitude == 3
        ]
        self.assertEqual(len(matching), 1,
                         "In the Swing must have exactly one ATTACK_POWER_BOOST(3) effect")

    def test_effect_condition_passes_on_second_attack(self):
        card = next(c for c in self.dorinthea.hand if c.name == "In the Swing")
        effect = card.effects[0]
        self.assertTrue(effect.matches(EffectTrigger.ON_ATTACK_REACTION,
                                       {"weapon_attack_count": 1}))

    def test_effect_condition_fails_on_first_attack(self):
        card = next(c for c in self.dorinthea.hand if c.name == "In the Swing")
        effect = card.effects[0]
        self.assertFalse(effect.matches(EffectTrigger.ON_ATTACK_REACTION,
                                        {"weapon_attack_count": 0}))


class TestInTheSwingDuringSecondWeaponAttack(unittest.TestCase):
    """Integration test: In the Swing grants +3 power in the second weapon attack."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]
        self.rhinar = self.env._game.players[0]
        _setup_to_second_weapon_reaction(self.env)

    def test_in_the_swing_is_legal_during_second_attack(self):
        legal = self.env.legal_actions()
        names = [a.card.name for a in legal if a.card]
        self.assertIn("In the Swing", names)

    def test_weapon_attack_count_is_one_in_reaction_window(self):
        self.assertEqual(self.dorinthea.weapon_attack_count, 1)

    def test_second_dawnblade_attack_pending_power_before_reaction(self):
        # Dawnblade base 2 + second-attack bonus 1 = 3
        self.assertEqual(self.env._pending_attack_power, 3)

    def test_in_the_swing_adds_three_power(self):
        power_before = self.env._pending_attack_power

        legal = self.env.legal_actions()
        env = self.env
        env.step(next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "In the Swing"
        ))
        # After playing, the card is on the stack; priority passes to Rhinar.
        # Both pass → stack resolves → "In the Swing" fires its effect.
        for _ in range(2):
            if env._phase == Phase.REACTION:
                legal = env.legal_actions()
                env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        # After stack resolves, attack resolves and phase advances.
        # Total damage dealt to Rhinar: first attack 2, second attack 3+3=6
        self.assertEqual(self.rhinar.life, 20 - 2 - 6)


class TestInTheSwingNotLegalOnFirstAttack(unittest.TestCase):
    """Verify 'In the Swing' has a play restriction enforced during the first weapon attack."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_play_condition_fails_with_zero_weapon_attacks(self):
        """play_condition returns False when weapon_attack_count==0 (first attack)."""
        card = next(c for c in self.dorinthea.hand if c.name == "In the Swing")
        self.assertIsNotNone(card.play_condition,
                             "In the Swing must have a play_condition")
        self.assertFalse(card.play_condition({"weapon_attack_count": 0}),
                         "play_condition must block play on the first weapon attack")

    def test_in_the_swing_not_in_legal_reaction_actions_first_attack(self):
        """legal_reaction_actions excludes 'In the Swing' when weapon_attack_count==0."""
        from actions import legal_reaction_actions
        self.dorinthea.weapon_attack_count = 0
        legal = legal_reaction_actions(self.dorinthea, attacker_idx=1, priority_idx=1)
        names = [a.card.name for a in legal if a.card]
        self.assertNotIn("In the Swing", names,
                         "In the Swing must not be legal on the first weapon attack")

    def test_effect_condition_fails_on_first_attack(self):
        """The effect's condition also blocks the +3 power boost when count==0."""
        card = next(c for c in self.dorinthea.hand if c.name == "In the Swing")
        effect = card.effects[0]
        self.assertFalse(effect.matches(EffectTrigger.ON_ATTACK_REACTION,
                                        {"weapon_attack_count": 0}),
                         "Effect must NOT fire during first weapon attack")


if __name__ == "__main__":
    unittest.main(verbosity=2)

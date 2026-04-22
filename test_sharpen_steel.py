"""
Tests for Sharpen Steel action card.

Seed 1 gives:
  Dorinthea:  Run Through, Sharpen Steel, Flock of the Feather Walkers, Hala Goldenhelm
  Dorinthea (agent_1) wins the coin flip.

Sharpen Steel should:
  - Apply +3 power to the next weapon attack via the ON_PLAY / WEAPON_ATTACK_POWER_BONUS
    effect defined on the card (not via a name-check in fab_env.py).
  - Grant go again (action_points +1 after playing).
  - Be free (cost=0, no pitch required).
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass
from card_effects import EffectTrigger, EffectAction

SEED = 1  # Dorinthea has Sharpen Steel; Dorinthea wins coin flip


def _setup(env):
    """
    Reset at SEED and play Sharpen Steel as Dorinthea's first action.

    Step sequence
    -------------
    1. Dorinthea wins the coin flip and chooses GO_FIRST.
    2. Dorinthea plays Sharpen Steel (cost=0 — no pitch needed).
       → effect fires: next_weapon_power_bonus += 3, go_again grants +1 AP.

    Returns (dorinthea, rhinar).
    """
    env.reset(seed=SEED)
    dorinthea = env._game.players[1]
    rhinar = env._game.players[0]

    # Step 1: Dorinthea wins the flip → GO_FIRST → she acts first
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))

    # Step 2: Play Sharpen Steel
    legal = env.legal_actions()
    sharpen_action = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD
        and a.card is not None
        and a.card.name == "Sharpen Steel"
    )
    env.step(sharpen_action)

    return dorinthea, rhinar


class TestSharpenSteelCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Sharpen Steel", names)

    def test_card_properties(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Sharpen Steel")
        self.assertEqual(card.card_type, CardType.ACTION)
        self.assertEqual(card.cost, 0)
        self.assertEqual(card.pitch, 1)
        self.assertEqual(card.defense, 3)
        self.assertTrue(card.go_again)
        self.assertEqual(card.color, Color.RED)
        self.assertEqual(card.card_class, CardClass.WARRIOR)

    def test_has_weapon_attack_power_bonus_effect(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Sharpen Steel")
        matching = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_PLAY
            and e.action == EffectAction.WEAPON_ATTACK_POWER_BONUS
            and e.magnitude == 3
        ]
        self.assertEqual(len(matching), 1,
                         "Sharpen Steel must have exactly one ON_PLAY WEAPON_ATTACK_POWER_BONUS +3 effect")


class TestSharpenSteelEffect(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _setup(self.env)

    def test_next_weapon_power_bonus_is_three(self):
        self.assertEqual(
            self.dorinthea.next_weapon_power_bonus, 3,
            "Playing Sharpen Steel must set next_weapon_power_bonus to 3"
        )

    def test_go_again_grants_action_point(self):
        # After Sharpen Steel resolves, Dorinthea should still have an action point
        # (go_again gives +1 AP so she can continue acting)
        self.assertGreater(
            self.dorinthea.action_points, 0,
            "Sharpen Steel go again must grant +1 action point"
        )

    def test_still_in_attack_phase(self):
        self.assertEqual(self.env._phase, Phase.ATTACK,
                         "Should still be in ATTACK phase after playing a go-again action")

    def test_weapon_attack_uses_bonus(self):
        """Dawnblade (base power 2) should deal 5 damage after Sharpen Steel."""
        # Attack with Dawnblade
        legal = self.env.legal_actions()
        env = self.env
        env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

        # Pitch Run Through (pitch=1) to cover Dawnblade cost=1
        assert env._phase == Phase.PITCH
        legal = env.legal_actions()
        env.step(legal[0])

        # Rhinar does not defend
        assert env._phase == Phase.DEFEND
        legal = env.legal_actions()
        no_defend = next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and not a.defend_hand_indices
            and not a.defend_equip_slots
        )
        env.step(no_defend)

        # Pass through reaction window
        while env._phase == Phase.REACTION:
            legal = env.legal_actions()
            env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        # Dawnblade base power=2 + Sharpen Steel +3 = 5 damage
        self.assertEqual(self.rhinar.life, 20 - 5,
                         "Dawnblade with Sharpen Steel should deal 5 damage (2 base + 3 bonus)")

    def test_bonus_cleared_after_weapon_attack(self):
        """next_weapon_power_bonus resets to 0 after the weapon attack resolves."""
        env = self.env
        legal = env.legal_actions()
        env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

        assert env._phase == Phase.PITCH
        legal = env.legal_actions()
        env.step(legal[0])

        assert env._phase == Phase.DEFEND
        legal = env.legal_actions()
        no_defend = next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and not a.defend_hand_indices
            and not a.defend_equip_slots
        )
        env.step(no_defend)

        while env._phase == Phase.REACTION:
            legal = env.legal_actions()
            env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertEqual(
            self.dorinthea.next_weapon_power_bonus, 0,
            "next_weapon_power_bonus must reset to 0 after the weapon attack resolves"
        )


if __name__ == "__main__":
    unittest.main()

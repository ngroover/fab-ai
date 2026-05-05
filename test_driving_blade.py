"""
Unit tests for "Driving Blade" card.

Seed 9 gives Dorinthea: Hit and Run, Driving Blade, In the Swing, Hit and Run.
Rhinar wins the coin flip and goes second, so Dorinthea acts first.

Card text: "Your next weapon attack this turn gains +2 power and go again. Go again."

Expected behaviour:
  1. Playing Driving Blade sets next_weapon_power_bonus += 2.
  2. Playing Driving Blade sets next_weapon_go_again = True.
  3. Driving Blade itself has Go again (grants +1 action point after resolving).
  4. The next Dawnblade attack deals base 2 + bonus 2 = 4 damage.
  5. The weapon attack gains go again (Dorinthea may attack again).
  6. next_weapon_power_bonus and next_weapon_go_again are cleared after the attack.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass, Keyword
from cards import build_rhinar_deck, build_dorinthea_deck

SEED = 9  # Dorinthea: Hit and Run, Driving Blade, In the Swing, Hit and Run


def _play_driving_blade(env):
    """
    Reset at SEED and advance past Driving Blade being played and its pitch step.

    Steps:
      1. Rhinar chooses GO_SECOND → Dorinthea acts first.
      2. Dorinthea plays Driving Blade (cost 2).
      3. Pitch Hit and Run (pitch=3) to cover cost.

    Returns (dorinthea, rhinar).
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    dorinthea = env._game.players[1]
    rhinar = env._game.players[0]

    # Step 1: Rhinar goes second
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_SECOND))

    # Step 2: play Driving Blade
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD
        and a.card is not None
        and a.card.name == "Driving Blade"
    ))

    # Step 3: pitch Hit and Run (pitch=3) to cover cost=2
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.PITCH
        and sum(dorinthea.hand[i].pitch for i in a.pitch_indices) >= 2
    ))

    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])

    return dorinthea, rhinar


class TestDrivingBladeCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Driving Blade", names)

    def test_card_properties(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Driving Blade")
        self.assertEqual(card.card_type, CardType.ACTION)
        self.assertEqual(card.cost, 2)
        self.assertEqual(card.pitch, 2)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.YELLOW)
        self.assertEqual(card.card_class, CardClass.WARRIOR)

    def test_has_go_again_keyword(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Driving Blade")
        self.assertIn(Keyword.GO_AGAIN, card.keywords,
                      "Driving Blade must have GO_AGAIN keyword (the card itself has go again)")


class TestDrivingBladeEffect(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _play_driving_blade(self.env)

    def test_still_in_attack_phase_after_playing(self):
        self.assertEqual(self.env._phase, Phase.ATTACK,
                         "Go again from Driving Blade should keep turn in ATTACK phase")

    def test_grants_action_point_via_go_again(self):
        self.assertEqual(self.dorinthea.action_points, 1,
                         "Driving Blade should grant +1 action point via its own Go again")

    def test_next_weapon_power_bonus_set(self):
        self.assertEqual(self.dorinthea.next_weapon_power_bonus, 2,
                         "Driving Blade should set next_weapon_power_bonus = 2")

    def test_next_weapon_go_again_set(self):
        self.assertTrue(self.dorinthea.next_weapon_go_again,
                        "Driving Blade should set next_weapon_go_again = True")

    def test_weapon_attack_deals_boosted_damage(self):
        """Dawnblade base power=2 + Driving Blade bonus=2 → 4 damage to Rhinar."""
        rhinar_life_before = self.rhinar.life

        # Attack with Dawnblade
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

        # Pitch if needed
        if self.env._phase == Phase.PITCH:
            legal = self.env.legal_actions()
            self.env.step(legal[0])

        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

        # Rhinar does not defend
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and not a.defend_hand_indices
            and not a.defend_equip_slots
        ))

        # Pass priority to resolve reaction window
        for _ in range(2):
            if self.env._phase == Phase.REACTION:
                legal = self.env.legal_actions()
                self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertEqual(self.rhinar.life, rhinar_life_before - 4,
                         "Dawnblade with Driving Blade bonus should deal 4 damage")

    def test_weapon_gains_go_again(self):
        """After the Dawnblade attack resolves, Dorinthea should have go again."""
        # Attack with Dawnblade
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

        if self.env._phase == Phase.PITCH:
            legal = self.env.legal_actions()
            self.env.step(legal[0])

        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

        # Rhinar does not defend
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and not a.defend_hand_indices
            and not a.defend_equip_slots
        ))

        # Pass reaction window
        for _ in range(2):
            if self.env._phase == Phase.REACTION:
                legal = self.env.legal_actions()
                self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

        # After go again resolves, Dorinthea should have 1 action point and be in ATTACK phase
        self.assertEqual(self.dorinthea.action_points, 1,
                         "Driving Blade's go again on the weapon attack should grant +1 AP")
        self.assertEqual(self.env._phase, Phase.ATTACK)

    def test_bonuses_consumed_after_weapon_attack(self):
        """next_weapon_power_bonus and next_weapon_go_again are cleared after the attack fires."""
        # Attack with Dawnblade
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

        if self.env._phase == Phase.PITCH:
            legal = self.env.legal_actions()
            self.env.step(legal[0])

        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

        # Rhinar does not defend
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and not a.defend_hand_indices
            and not a.defend_equip_slots
        ))

        # Pass reaction window
        for _ in range(2):
            if self.env._phase == Phase.REACTION:
                legal = self.env.legal_actions()
                self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertEqual(self.dorinthea.next_weapon_power_bonus, 0,
                         "next_weapon_power_bonus must be cleared after weapon attack")
        self.assertFalse(self.dorinthea.next_weapon_go_again,
                         "next_weapon_go_again must be cleared after weapon attack")


if __name__ == "__main__":
    unittest.main(verbosity=2)

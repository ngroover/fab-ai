"""
Tests for Gallantry Gold equipment behavior.

Seed 1 gives:
  Rhinar:     Smash with Big Tree (Yellow), Awakening Bellow (Red), Wrecking Ball (Red), Awakening Bellow (Red)
  Dorinthea:  Run Through (Yellow, pitch 2), Sharpen Steel (Red, pitch 1),
              Flock of the Feather Walkers (Red, pitch 1), Hala Goldenhelm (pitch 0)
  Dorinthea (agent_1) wins the coin flip and goes first.

Gallantry Gold rules:
  ARMS equipment for Dorinthea with defense 1 and Battleworn.
  Action — 1: Destroy Gallantry Gold: Your weapon attacks gain +1 power this turn. Go again.
  Battleworn: when used to defend, gets a -1 defense counter instead of going to graveyard.
              Destroyed when defense reaches 0.

Test scenarios:
  1. Card definition: arms slot, defense=1, cost=1, BATTLEWORN keyword, ON_ACTIVATE effect.
  2. Activation: available as ACTIVATE_EQUIPMENT action, triggers PITCH phase to cover cost=1,
     after activation arms slot is empty, Gallantry Gold is in graveyard,
     weapon_attacks_power_bonus_all_turn = +1.
  3. Weapon power bonus: Dawnblade attacks gain +1 power after activation.
  4. Battleworn blocking: defense drops by 1 when used to block; goes to graveyard when defense reaches 0.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import Keyword, build_rhinar_deck, build_dorinthea_deck
from card_effects import EffectTrigger, EffectAction

SEED = 1


def _reset_and_go_first(env) -> tuple:
    """Reset at SEED and have Dorinthea go first. Returns (rhinar, dorinthea)."""
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    rhinar = env._game.players[0]
    dorinthea = env._game.players[1]
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST))
    return rhinar, dorinthea


def _activate_gallantry_gold(env, dorinthea):
    """
    Activate Gallantry Gold from the arms slot.

    Requires 1 resource. Pitches Sharpen Steel (index 1, pitch=1) to cover the cost.
    Returns after the activation resolves (env back in ATTACK phase).
    """
    # Step 1: select the ACTIVATE_EQUIPMENT action for arms slot
    legal = env.legal_actions()
    act = next(a for a in legal if a.action_type == ActionType.ACTIVATE_EQUIPMENT
               and a.equip_slot == "arms")
    env.step(act)
    assert env._phase == Phase.PITCH, "Expected PITCH phase to cover Gallantry Gold cost"

    # Step 2: pitch Sharpen Steel (hand index 1, pitch=1) to cover cost=1
    legal = env.legal_actions()
    pitch_sharpen = next(a for a in legal
                         if a.pitch_index is not None
                         and dorinthea.hand[a.pitch_index].name == "Sharpen Steel")
    env.step(pitch_sharpen)


def _advance_to_defend_phase(env):
    """
    Advance to Rhinar's DEFEND phase so Dorinthea can block.

    Sequence:
    1. Dorinthea passes (no attacks).
    2. Skip instant and arsenal phases.
    3. Rhinar attacks with Bone Basher.
    4. Skip pitch and instant phases.
    Returns (rhinar, dorinthea).
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    rhinar = env._game.players[0]
    dorinthea = env._game.players[1]
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST))

    # Dorinthea passes
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.PASS))
    while env._phase in (Phase.INSTANT, Phase.ARSENAL):
        env.step(env.legal_actions()[0])

    # Rhinar attacks with Bone Basher
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON))
    while env._phase in (Phase.PITCH, Phase.INSTANT):
        env.step(env.legal_actions()[0])

    assert env._phase == Phase.DEFEND
    return rhinar, dorinthea


def _block_with_gallantry_gold(env):
    """Commit Gallantry Gold (arms) as sole blocker, then pass to close the block."""
    legal = env.legal_actions()
    env.step(next(a for a in legal
                  if a.action_type == ActionType.DEFEND and a.equip_slot == "arms"))
    legal = env.legal_actions()
    env.step(next(a for a in legal
                  if a.action_type == ActionType.DEFEND
                  and a.hand_index is None and a.equip_slot is None))
    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])


# ─────────────────────────────────────────────
# Card definition tests
# ─────────────────────────────────────────────

class TestGallantryGoldCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _, self.dorinthea = _reset_and_go_first(self.env)
        self.eq = self.dorinthea.equipment.get("arms")

    def test_in_arms_slot(self):
        self.assertIsNotNone(self.eq)
        self.assertEqual(self.eq.card.name, "Gallantry Gold")

    def test_defense_value(self):
        self.assertEqual(self.eq.card.defense, 1)

    def test_activation_cost(self):
        self.assertEqual(self.eq.card.cost, 1)

    def test_battleworn_keyword(self):
        self.assertIn(Keyword.BATTLEWORN, self.eq.card.keywords)

    def test_has_on_activate_effect(self):
        matching = [e for e in self.eq.card.effects
                    if e.trigger == EffectTrigger.ON_ACTIVATE
                    and e.action == EffectAction.WEAPON_ATTACKS_POWER_BONUS_ALL_TURN
                    and e.magnitude == 1]
        self.assertEqual(len(matching), 1,
                         "Gallantry Gold must have exactly one WEAPON_ATTACKS_POWER_BONUS_ALL_TURN ON_ACTIVATE effect")

    def test_not_destroyed_at_start(self):
        self.assertFalse(self.eq.destroyed)

    def test_block_counters_zero_at_start(self):
        self.assertEqual(self.eq.block_counters, 0)


# ─────────────────────────────────────────────
# Activation legal action tests
# ─────────────────────────────────────────────

class TestGallantryGoldActivationLegal(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _, self.dorinthea = _reset_and_go_first(self.env)

    def test_activation_appears_in_legal_actions(self):
        legal = self.env.legal_actions()
        has_arms_activate = any(
            a.action_type == ActionType.ACTIVATE_EQUIPMENT and a.equip_slot == "arms"
            for a in legal
        )
        self.assertTrue(has_arms_activate,
                        "ACTIVATE_EQUIPMENT for arms slot must be a legal action at turn start")

    def test_activation_not_legal_after_destroy(self):
        _activate_gallantry_gold(self.env, self.dorinthea)
        legal = self.env.legal_actions()
        has_arms_activate = any(
            a.action_type == ActionType.ACTIVATE_EQUIPMENT and a.equip_slot == "arms"
            for a in legal
        )
        self.assertFalse(has_arms_activate,
                         "Gallantry Gold activation must not appear after it has been destroyed")


# ─────────────────────────────────────────────
# Activation effect tests
# ─────────────────────────────────────────────

class TestGallantryGoldActivation(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _, self.dorinthea = _reset_and_go_first(self.env)
        _activate_gallantry_gold(self.env, self.dorinthea)

    def test_arms_slot_empty_after_activation(self):
        self.assertNotIn("arms", self.dorinthea.equipment,
                         "Gallantry Gold must be removed from the arms slot on activation")

    def test_gallantry_gold_in_graveyard(self):
        grave_names = [c.name for c in self.dorinthea.graveyard]
        self.assertIn("Gallantry Gold", grave_names,
                      "Gallantry Gold must be in graveyard after activation")

    def test_weapon_attacks_power_bonus_applied(self):
        self.assertEqual(self.dorinthea.weapon_attacks_power_bonus_all_turn, 1,
                         "weapon_attacks_power_bonus_all_turn must be 1 after Gallantry Gold activation")

    def test_still_in_attack_phase(self):
        self.assertEqual(self.env._phase, Phase.ATTACK,
                         "Game must return to ATTACK phase after activation resolves")

    def test_resource_cost_deducted(self):
        # Pitched Sharpen Steel (pitch=1), paid cost=1: net 0 resource points
        self.assertEqual(self.dorinthea.resource_points, 0,
                         "Resource points must reflect 1 deducted for Gallantry Gold activation cost")


# ─────────────────────────────────────────────
# Weapon power bonus tests
# ─────────────────────────────────────────────

class TestGallantryGoldWeaponPowerBonus(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _, self.dorinthea = _reset_and_go_first(self.env)
        _activate_gallantry_gold(self.env, self.dorinthea)

    def test_dawnblade_attack_power_is_boosted(self):
        """Dawnblade (power=2) should attack with 3 power after Gallantry Gold activation."""
        # Attack with Dawnblade — needs 1 resource (we have 0 left, must pitch)
        env = self.env
        legal = env.legal_actions()
        env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))
        while env._phase == Phase.PITCH:
            env.step(env.legal_actions()[0])
        self.assertEqual(env._pending_attack_power, 3,
                         "Dawnblade attack power must be 3 (base 2 + Gallantry Gold +1)")

    def test_bonus_not_reset_after_first_weapon_attack(self):
        """weapon_attacks_power_bonus_all_turn persists after the first attack."""
        env = self.env
        # Attack with Dawnblade
        env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON))
        while env._phase == Phase.PITCH:
            env.step(env.legal_actions()[0])
        # The bonus is still set even after the attack is declared
        self.assertEqual(self.dorinthea.weapon_attacks_power_bonus_all_turn, 1,
                         "weapon_attacks_power_bonus_all_turn must not be reset after a weapon attack")


# ─────────────────────────────────────────────
# Battleworn blocking tests
# ─────────────────────────────────────────────

class TestGallantryGoldBattleworn(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_defend_phase(self.env)

    def test_gallantry_gold_available_before_block(self):
        self.assertIn("arms", self.dorinthea.equipment)

    def test_gallantry_gold_defense_before_block(self):
        eq = self.dorinthea.equipment["arms"]
        self.assertEqual(eq.defense, 1)

    def test_block_counter_incremented(self):
        eq = self.dorinthea.equipment["arms"]  # capture before block
        _block_with_gallantry_gold(self.env)
        # Gallantry Gold has defense=1: after one block, block_counters=1 → defense=0 → graveyard
        # Verify it was properly tracked as battleworn (not blade-break)
        grave_names = [c.name for c in self.dorinthea.graveyard]
        self.assertIn("Gallantry Gold", grave_names,
                      "Gallantry Gold (defense=1 Battleworn) must go to graveyard after one block")
        # Battleworn counter must be applied (not a direct Blade Break graveyard)
        self.assertEqual(eq.block_counters, 1,
                         "Battleworn must increment block_counters to 1 before destroying")
        self.assertTrue(eq.destroyed,
                        "Equipment must be marked destroyed after Battleworn reaches 0 DEF")
        self.assertEqual(eq.defense, 0,
                         "Defense must be 0 after the -1 Battleworn counter")

    def test_arms_slot_empty_after_block(self):
        _block_with_gallantry_gold(self.env)
        self.assertNotIn("arms", self.dorinthea.equipment,
                         "Arms slot must be empty after Gallantry Gold is destroyed by Battleworn")

    def test_battleworn_not_blade_break(self):
        """Gallantry Gold should be destroyed by Battleworn (defense 0), not Blade Break."""
        eq = self.dorinthea.equipment["arms"]
        self.assertIn(Keyword.BATTLEWORN, eq.card.keywords)
        self.assertNotIn(Keyword.BLADE_BREAK, eq.card.keywords)

    def test_not_in_graveyard_while_blocking(self):
        """Gallantry Gold must stay on the combat chain (not in graveyard) while blocking is committed."""
        eq = self.dorinthea.equipment["arms"]
        # Commit the equip to block (one step only — chain not yet closed)
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND and a.equip_slot == "arms"))
        # Gallantry Gold is now on the combat chain — must NOT be in graveyard yet
        grave_names = [c.name for c in self.dorinthea.graveyard]
        self.assertNotIn("Gallantry Gold", grave_names,
                         "Gallantry Gold must not be in graveyard at block-commit time (only at chain close)")
        self.assertIn(eq.card, self.dorinthea.combat_chain,
                      "Gallantry Gold card must be on the combat chain after being committed to block")

    def test_battleworn_stays_if_defense_remains(self):
        """A Battleworn item with defense > 1 stays in equipment zone after first block."""
        # Manually bump Gallantry Gold's defense to 2 (as if it were a 2-defense Battleworn)
        eq = self.dorinthea.equipment["arms"]
        original_defense = eq.card.defense

        from classic_battles import CARD_CATALOG
        from cards import Card, CardType, EquipSlot, CardClass
        from card_effects import CardEffect

        # Create a temporary 2-defense Gallantry Gold to test the mechanics
        two_def_eq = __import__('game_state').Equipment(
            Card("Gallantry Gold", CardType.EQUIPMENT, defense=2, cost=1,
                 equip_slot=EquipSlot.ARMS, card_class=CardClass.WARRIOR,
                 keywords=[Keyword.BATTLEWORN])
        )
        self.dorinthea.equipment["arms"] = two_def_eq
        _block_with_gallantry_gold(self.env)
        # After first block: block_counters=1, defense=max(0,2-1)=1 → stays
        self.assertIn("arms", self.dorinthea.equipment,
                      "Battleworn equipment with defense>1 must stay in arms slot after first block")
        self.assertEqual(self.dorinthea.equipment["arms"].block_counters, 1)
        self.assertEqual(self.dorinthea.equipment["arms"].defense, 1)


if __name__ == "__main__":
    unittest.main()

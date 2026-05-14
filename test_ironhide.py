"""
Tests for Ironhide Gauntlet / Ironhide Legs PAY_FOR_BLOCK_BONUS behavior.

Ironhide rules:
  ARMS / LEGS equipment for Rhinar with defense 0 and no special keywords.
  When you defend with Ironhide [slot], you may pay 1 resource. If you do,
  it gains +2 block and is destroyed when the combat chain closes.

Test scenarios:
  1. Card definition: arms/legs slot, defense=0, PAY_FOR_BLOCK_BONUS effect
     (magnitude=2, cost=1, trigger=ON_DEFEND).
  2. Reaction phase: PAY_FOR_BLOCK_BONUS action is offered only when the
     defender (a) committed the equipment as a blocker and (b) has at least
     1 resource available.
  3. Activating the pay action deducts 1 resource and adds +2 to the
     reaction defense bonus.
  4. The pay action is one-shot per equipment per combat — it disappears
     from legal actions after the defender pays.
  5. After paying, the equipment is destroyed (moved to graveyard) when the
     combat chain closes.
  6. Without paying, plain equipment (no Blade Break / no Battleworn /
     no payment) returns to its slot after the chain closes.
  7. The PAY_FOR_BLOCK_BONUS effect is reusable: both Ironhide Gauntlet
     and Ironhide Legs share the same effect definition.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import ActionType
from cards import Keyword, build_rhinar_deck, build_dorinthea_deck
from card_effects import EffectTrigger, EffectAction

SEED = 1


def _reset_dorinthea_first(env):
    """Reset at SEED 1 (Dorinthea wins the coin flip) and elect to go first.

    Returns (rhinar, dorinthea). Dorinthea will attack first so Rhinar can
    defend with Ironhide equipment.
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    rhinar = env._game.players[0]
    dorinthea = env._game.players[1]
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST))
    return rhinar, dorinthea


def _advance_to_rhinar_defend(env):
    """Advance the game state so that Rhinar is in the DEFEND phase against a
    Dawnblade attack. Returns (rhinar, dorinthea).

    The Dorinthea agent isn't used — Dorinthea swings Dawnblade directly,
    pitching whatever covers the 1-resource cost.
    """
    rhinar, dorinthea = _reset_dorinthea_first(env)
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON))
    while env._phase in (Phase.PITCH, Phase.INSTANT):
        env.step(env.legal_actions()[0])
    assert env._phase == Phase.DEFEND, f"expected DEFEND, got {env._phase}"
    assert env.agent_selection == "agent_0", "Rhinar must be the defender"
    return rhinar, dorinthea


def _commit_ironhide_block(env, slot: str):
    """Commit the Ironhide equipment in *slot* as the sole blocker, then close
    the block decision. Leaves the env in the REACTION phase (defender side).
    """
    legal = env.legal_actions()
    env.step(next(a for a in legal
                  if a.action_type == ActionType.DEFEND and a.equip_slot == slot))
    legal = env.legal_actions()
    env.step(next(a for a in legal
                  if a.action_type == ActionType.DEFEND
                  and a.hand_index is None and a.equip_slot is None))


def _let_attacker_pass_reactions(env):
    """In the REACTION phase, attacker (Dorinthea) passes priority so the
    defender (Rhinar) holds priority."""
    while (env._phase == Phase.REACTION
           and env.agent_selection == "agent_1"):
        env.step(next(a for a in env.legal_actions()
                      if a.action_type == ActionType.PASS_PRIORITY))


def _close_reaction_and_chain(env):
    """Have both players pass priority enough times to close the reaction
    window, then advance through any post-combat instant windows."""
    while env._phase == Phase.REACTION:
        env.step(next(a for a in env.legal_actions()
                      if a.action_type == ActionType.PASS_PRIORITY))
    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])


# ─────────────────────────────────────────────
# Card definition
# ─────────────────────────────────────────────

class TestIronhideCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, _ = _reset_dorinthea_first(self.env)

    def test_ironhide_gauntlet_in_arms_slot(self):
        eq = self.rhinar.equipment.get("arms")
        self.assertIsNotNone(eq)
        self.assertEqual(eq.card.name, "Ironhide Gauntlet")
        self.assertEqual(eq.card.defense, 0)

    def test_ironhide_legs_in_legs_slot(self):
        eq = self.rhinar.equipment.get("legs")
        self.assertIsNotNone(eq)
        self.assertEqual(eq.card.name, "Ironhide Legs")
        self.assertEqual(eq.card.defense, 0)

    def test_ironhide_has_no_blade_break_or_battleworn(self):
        for slot in ("arms", "legs"):
            eq = self.rhinar.equipment[slot]
            self.assertNotIn(Keyword.BLADE_BREAK, eq.card.keywords)
            self.assertNotIn(Keyword.BATTLEWORN, eq.card.keywords)

    def test_ironhide_pay_effect_definition(self):
        for slot in ("arms", "legs"):
            eq = self.rhinar.equipment[slot]
            matching = [e for e in eq.card.effects
                        if e.trigger == EffectTrigger.ON_DEFEND
                        and e.action == EffectAction.PAY_FOR_BLOCK_BONUS
                        and e.magnitude == 2 and e.cost == 1]
            self.assertEqual(
                len(matching), 1,
                f"{eq.card.name} must have exactly one PAY_FOR_BLOCK_BONUS effect "
                f"(magnitude=2, cost=1, trigger=ON_DEFEND)"
            )


# ─────────────────────────────────────────────
# Legal-action gating
# ─────────────────────────────────────────────

class TestIronhidePayLegalAction(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, _ = _advance_to_rhinar_defend(self.env)

    def test_pay_not_offered_without_resources(self):
        self.rhinar.resource_points = 0
        _commit_ironhide_block(self.env, "arms")
        _let_attacker_pass_reactions(self.env)
        legal = self.env.legal_actions()
        self.assertFalse(
            any(a.action_type == ActionType.PAY_FOR_BLOCK_BONUS for a in legal),
            "PAY_FOR_BLOCK_BONUS must not be legal when defender has 0 resources"
        )

    def test_pay_offered_when_resources_available(self):
        self.rhinar.resource_points = 1
        _commit_ironhide_block(self.env, "arms")
        _let_attacker_pass_reactions(self.env)
        legal = self.env.legal_actions()
        matching = [a for a in legal
                    if a.action_type == ActionType.PAY_FOR_BLOCK_BONUS
                    and a.card is not None and a.card.name == "Ironhide Gauntlet"]
        self.assertEqual(len(matching), 1,
                         "PAY_FOR_BLOCK_BONUS must be legal once Ironhide is committed and resources available")

    def test_pay_not_offered_when_equipment_not_committed(self):
        self.rhinar.resource_points = 5
        # Don't commit any equipment — just commit no blocks and close defend.
        env = self.env
        env.step(next(a for a in env.legal_actions()
                      if a.action_type == ActionType.DEFEND
                      and a.hand_index is None and a.equip_slot is None))
        _let_attacker_pass_reactions(env)
        legal = env.legal_actions()
        self.assertFalse(
            any(a.action_type == ActionType.PAY_FOR_BLOCK_BONUS for a in legal),
            "PAY_FOR_BLOCK_BONUS must not be legal when no Ironhide equipment is committed"
        )


# ─────────────────────────────────────────────
# Pay action effects
# ─────────────────────────────────────────────

class TestIronhidePayActivation(unittest.TestCase):
    """End-to-end pay-flow checks.

    Once the defender pays, the rest of the turn auto-resolves through
    forced single-legal-action states (attacker has no action points left,
    so the chain closes immediately). We check the post-paying state.
    """

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, _ = _advance_to_rhinar_defend(self.env)
        self.rhinar.resource_points = 1
        _commit_ironhide_block(self.env, "arms")
        _let_attacker_pass_reactions(self.env)
        # Hold a reference to the Equipment object so we can inspect it after
        # the chain has closed (it leaves _committed_defend_equip on resolve).
        self.eq_ref = next(e for e in self.env._committed_defend_equip
                           if e.card.name == "Ironhide Gauntlet")
        self.pay_action = next(a for a in self.env.legal_actions()
                               if a.action_type == ActionType.PAY_FOR_BLOCK_BONUS)
        self.env.step(self.pay_action)

    def test_resource_deducted(self):
        self.assertEqual(self.rhinar.resource_points, 0,
                         "1 resource must be deducted for the pay-for-block bonus")

    def test_reaction_defense_bonus_applied_to_combat(self):
        # The +2 reaction defense bonus must absorb the 2-power Dawnblade hit
        # (full block → 0 damage).
        self.assertEqual(self.rhinar.life, 20,
                         "Paid +2 block must fully absorb the Dawnblade attack")

    def test_equipment_destroyed_after_pay(self):
        # destroy_on_chain_close → destroyed=True when the chain closes.
        self.assertTrue(self.eq_ref.destroyed,
                        "Paid-for Ironhide must be destroyed after chain close")

    def test_pay_not_repeatable_for_same_equipment(self):
        # The pay action is one-shot — it doesn't reappear if the defender
        # somehow blocks with the same equipment again (sanity check via the
        # used-ids set carried inside the env).
        self.assertIn(id(self.eq_ref.card), self.env._pay_block_bonus_used_ids,
                      "Equipment id must be recorded as having used its pay ability")


# ─────────────────────────────────────────────
# Chain-close consequences
# ─────────────────────────────────────────────

class TestIronhidePaidEquipmentDestroyed(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, _ = _advance_to_rhinar_defend(self.env)
        self.rhinar.resource_points = 1
        _commit_ironhide_block(self.env, "arms")
        _let_attacker_pass_reactions(self.env)
        pay = next(a for a in self.env.legal_actions()
                   if a.action_type == ActionType.PAY_FOR_BLOCK_BONUS)
        self.env.step(pay)
        _close_reaction_and_chain(self.env)

    def test_ironhide_in_graveyard_after_chain_close(self):
        grave_names = [c.name for c in self.rhinar.graveyard]
        self.assertIn("Ironhide Gauntlet", grave_names,
                      "Paid-for Ironhide Gauntlet must be in graveyard once the chain closes")

    def test_ironhide_not_in_arms_slot_after_destruction(self):
        self.assertNotIn("arms", self.rhinar.equipment,
                         "Destroyed Ironhide Gauntlet must not remain in the arms slot")


class TestIronhideUnpaidStaysEquipped(unittest.TestCase):
    """Without paying, plain equipment must NOT be sent to graveyard."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, _ = _advance_to_rhinar_defend(self.env)
        self.rhinar.resource_points = 0  # cannot pay
        _commit_ironhide_block(self.env, "legs")
        _close_reaction_and_chain(self.env)

    def test_ironhide_returns_to_legs_slot(self):
        self.assertIn("legs", self.rhinar.equipment,
                      "Unpaid Ironhide Legs must return to the legs slot after chain close")
        self.assertEqual(self.rhinar.equipment["legs"].card.name, "Ironhide Legs")

    def test_ironhide_not_in_graveyard(self):
        grave_names = [c.name for c in self.rhinar.graveyard]
        self.assertNotIn("Ironhide Legs", grave_names,
                         "Unpaid Ironhide Legs must not be in the graveyard")

    def test_ironhide_not_destroyed_flag(self):
        eq = self.rhinar.equipment.get("legs")
        self.assertIsNotNone(eq)
        self.assertFalse(eq.destroyed)
        self.assertFalse(eq.destroy_on_chain_close,
                         "destroy_on_chain_close flag must be cleared after the chain closes")


# ─────────────────────────────────────────────
# Effect is reusable across cards
# ─────────────────────────────────────────────

class TestIronhidePayEffectReusable(unittest.TestCase):
    """Same PAY_FOR_BLOCK_BONUS effect should work on the legs slot too."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, _ = _advance_to_rhinar_defend(self.env)
        self.rhinar.resource_points = 1

    def test_pay_works_on_legs(self):
        _commit_ironhide_block(self.env, "legs")
        _let_attacker_pass_reactions(self.env)
        legal = self.env.legal_actions()
        pay = next((a for a in legal
                    if a.action_type == ActionType.PAY_FOR_BLOCK_BONUS
                    and a.card is not None and a.card.name == "Ironhide Legs"),
                   None)
        self.assertIsNotNone(pay,
                             "PAY_FOR_BLOCK_BONUS must be legal for Ironhide Legs")
        self.env.step(pay)
        self.assertEqual(self.rhinar.resource_points, 0)
        self.assertEqual(self.env._reaction_defense_bonus, 2)
        _close_reaction_and_chain(self.env)
        grave_names = [c.name for c in self.rhinar.graveyard]
        self.assertIn("Ironhide Legs", grave_names,
                      "Paid-for Ironhide Legs must be in graveyard once the chain closes")


# ─────────────────────────────────────────────
# Damage prevention sanity check
# ─────────────────────────────────────────────

class TestIronhidePayPreventsDamage(unittest.TestCase):
    """End-to-end check: paying for the +2 block must actually reduce damage."""

    def test_paid_block_absorbs_dawnblade_attack(self):
        # Dawnblade base power = 2. Pay-for-block bonus = +2 defense.
        # Expected: 2 - 2 = 0 damage to Rhinar.
        env = FaBEnv(verbose=False)
        rhinar, _ = _advance_to_rhinar_defend(env)
        rhinar.resource_points = 1
        life_before = rhinar.life
        attack_power = env._pending_attack_power
        _commit_ironhide_block(env, "arms")
        _let_attacker_pass_reactions(env)
        pay = next(a for a in env.legal_actions()
                   if a.action_type == ActionType.PAY_FOR_BLOCK_BONUS)
        env.step(pay)
        _close_reaction_and_chain(env)
        damage = max(0, attack_power - 2)
        self.assertEqual(rhinar.life, life_before - damage,
                         f"Expected {damage} damage after +2 block; "
                         f"got {life_before - rhinar.life}")


if __name__ == "__main__":
    unittest.main()

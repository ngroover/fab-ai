"""
Tests for Run Through attack reaction.

Seed 38 gives:
  Rhinar:     Chief Ruk'utan, Smash with Big Tree, Titanium Bauble, Barraging Beatdown
  Dorinthea:  Run Through, Driving Blade, Sharpen Steel, Thrust
  Rhinar (agent_0) wins the coin flip.

Run Through should:
  - Be playable as an attack reaction during a Dawnblade weapon attack
  - Grant go again to the current Dawnblade attack (via SWORD_ATTACK_GO_AGAIN effect)
  - Give +2 power to the next sword attack this turn (via NEXT_SWORD_ATTACK_POWER_BONUS effect)
  - The +2 bonus must survive the first attack resolving and apply to the second attack
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass
from card_effects import EffectTrigger, EffectAction

SEED = 38  # Dorinthea has Run Through; Rhinar wins coin flip


def _setup(env):
    """
    Reset at SEED and advance through Dorinthea's first Dawnblade attack with
    Run Through played and resolved, returning with Dorinthea in go-again ATTACK phase.

    Step sequence
    -------------
    1. Rhinar chooses GO_SECOND → Dorinthea goes first.
    2. Dorinthea attacks with Dawnblade (WEAPON).
    3. Pitch Sharpen Steel (index 2, pitch=1) to cover Dawnblade cost=1.
    4. Rhinar does not defend (empty DEFEND).
    5. Dorinthea plays Run Through (ATTACK_REACTION, cost=1) in reaction window.
    6. Pitch Thrust (index 1, pitch=1) to cover Run Through cost=1.
       → Run Through resolves: go again + next_sword_attack_power_bonus=2.
       → Combat: Rhinar takes 2 damage; Dorinthea gains 1 AP (go again).
       → Phase returns to ATTACK for Dorinthea.

    Returns (dorinthea, rhinar).
    """
    env.reset(seed=SEED)
    dorinthea = env._game.players[1]
    rhinar = env._game.players[0]

    # Step 1: Rhinar chooses GO_SECOND so Dorinthea acts first
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_SECOND))

    # Step 2: Dorinthea attacks with Dawnblade
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

    # Step 3: Pitch Sharpen Steel (index 2, pitch=1) to cover Dawnblade cost=1
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.pitch_indices == [2]))

    # Step 4: Rhinar does not defend
    legal = env.legal_actions()
    no_defend = next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and not a.defend_hand_indices
        and not a.defend_equip_slots
    )
    env.step(no_defend)

    # Step 5: Dorinthea plays Run Through in the REACTION window
    assert env._phase == Phase.REACTION, f"Expected REACTION, got {env._phase}"
    legal = env.legal_actions()
    run_through = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD
        and a.card is not None
        and a.card.name == "Run Through"
    )
    env.step(run_through)

    # Step 6: Pitch Thrust (index 1 in remaining hand, pitch=1) to cover Run Through cost=1
    # Remaining hand after Sharpen Steel pitched: [Driving Blade(0), Thrust(1)]
    assert env._phase == Phase.PITCH, f"Expected PITCH for Run Through cost, got {env._phase}"
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.pitch_indices == [1]))
    # Run Through resolves; combat resolves; Dorinthea enters go-again ATTACK phase

    return dorinthea, rhinar


class TestRunThroughCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Run Through", names)

    def test_card_properties(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Run Through")
        self.assertEqual(card.card_type, CardType.ATTACK_REACTION)
        self.assertEqual(card.cost, 1)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.YELLOW)
        self.assertEqual(card.card_class, CardClass.WARRIOR)

    def test_has_sword_attack_go_again_effect(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Run Through")
        matching = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_ATTACK_REACTION
            and e.action == EffectAction.SWORD_ATTACK_GO_AGAIN
        ]
        self.assertEqual(len(matching), 1,
                         "Run Through must have exactly one SWORD_ATTACK_GO_AGAIN effect")

    def test_has_next_sword_attack_power_bonus_effect(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Run Through")
        matching = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_ATTACK_REACTION
            and e.action == EffectAction.NEXT_SWORD_ATTACK_POWER_BONUS
            and e.magnitude == 2
        ]
        self.assertEqual(len(matching), 1,
                         "Run Through must have exactly one NEXT_SWORD_ATTACK_POWER_BONUS effect with magnitude=2")


class TestRunThroughEffects(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _setup(self.env)

    def test_first_dawnblade_dealt_two_damage(self):
        """The first Dawnblade attack (power=2, unblocked) deals 2 damage."""
        self.assertEqual(self.rhinar.life, 18)

    def test_go_again_granted(self):
        """Run Through gives Dorinthea go again — she has 1 action point."""
        self.assertEqual(self.dorinthea.action_points, 1)

    def test_dorinthea_in_attack_phase_after_resolution(self):
        """After resolution Dorinthea is back in ATTACK phase with go again."""
        self.assertEqual(self.env._phase, Phase.ATTACK)
        self.assertEqual(self.env.agent_selection, "agent_1")

    def test_next_sword_attack_power_bonus_set(self):
        """next_sword_attack_power_bonus is 2 before the second Dawnblade attack."""
        self.assertEqual(self.dorinthea.next_sword_attack_power_bonus, 2)

    def test_second_dawnblade_has_plus_two_power(self):
        """Second Dawnblade attack gains +1 (Dawnblade 2nd-hit) +2 (Run Through) = 5 power."""
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))
        # Dawnblade power=2 + 1 (second-attack Dawnblade bonus) + 2 (Run Through) = 5
        self.assertEqual(self.env._pending_attack_power, 5)

    def test_next_sword_attack_power_bonus_consumed_after_use(self):
        """next_sword_attack_power_bonus resets to 0 once used by the second attack."""
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))
        self.assertEqual(self.dorinthea.next_sword_attack_power_bonus, 0)


if __name__ == "__main__":
    unittest.main()

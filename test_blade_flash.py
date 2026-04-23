"""
Tests for Blade Flash attack reaction.

Seed 19 gives:
  Rhinar:     Barraging Beatdown, Pack Call, Smash Instinct, Beast Mode
  Dorinthea:  Blade Flash, Thrust, Sharpen Steel, Ironsong Response
  Rhinar (agent_0) wins the coin flip.

Blade Flash should:
  - Be playable as an attack reaction during a Dawnblade weapon attack
  - Grant go again to the current Dawnblade attack (once)
  - NOT permanently set go_again on the Dawnblade Card object (which would
    cause every subsequent weapon attack to also receive free go again)
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass
from card_effects import EffectTrigger, EffectAction

SEED = 19  # Dorinthea has Blade Flash; Rhinar wins coin flip


def _setup(env):
    """
    Reset at SEED and advance to the REACTION phase during Dorinthea's first
    Dawnblade attack with Blade Flash already on the reaction stack.

    Step sequence
    -------------
    1. Rhinar chooses GO_SECOND → Dorinthea goes first.
    2. Dorinthea attacks with Dawnblade (WEAPON).
    3. Pitch Thrust (pitch=1) to cover Dawnblade cost=1.
       [Pre-DEFEND instant window auto-collapses — neither player holds instants.]
    4. Rhinar does not defend (empty DEFEND).
    5. Dorinthea plays Blade Flash (ATTACK_REACTION, cost=1) in reaction window.
    6. Pitch Sharpen Steel (pitch=1) to cover Blade Flash cost=1.
       → Blade Flash is now on the reaction stack; phase=REACTION.

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

    # Step 3: Pitch Thrust (legal[1] = PITCH [1]) to cover weapon cost
    legal = env.legal_actions()
    env.step(legal[1])
    # Pre-DEFEND instant window auto-collapses; now in DEFEND phase

    # Step 4: Rhinar does not defend
    legal = env.legal_actions()
    no_defend = next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and not a.defend_hand_indices
        and not a.defend_equip_slots
    )
    env.step(no_defend)

    # Step 5: Dorinthea plays Blade Flash in the REACTION window
    assert env._phase == Phase.REACTION, f"Expected REACTION, got {env._phase}"
    legal = env.legal_actions()
    blade_flash = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD
        and a.card is not None
        and a.card.name == "Blade Flash"
    )
    env.step(blade_flash)

    # Step 6: Pitch Sharpen Steel (legal[0] = PITCH [0]) to cover Blade Flash cost
    assert env._phase == Phase.PITCH, f"Expected PITCH for Blade Flash cost, got {env._phase}"
    legal = env.legal_actions()
    env.step(legal[0])
    # Blade Flash is now on the reaction stack; back in REACTION phase

    return dorinthea, rhinar


class TestBladeFlashCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Blade Flash", names)

    def test_card_properties(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Blade Flash")
        self.assertEqual(card.card_type, CardType.ATTACK_REACTION)
        self.assertEqual(card.cost, 1)
        self.assertEqual(card.defense, 2)
        self.assertEqual(card.color, Color.BLUE)
        self.assertEqual(card.card_class, CardClass.WARRIOR)

    def test_has_sword_attack_go_again_effect(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Blade Flash")
        matching = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_ATTACK_REACTION
            and e.action == EffectAction.SWORD_ATTACK_GO_AGAIN
        ]
        self.assertEqual(len(matching), 1,
                         "Blade Flash must have exactly one SWORD_ATTACK_GO_AGAIN effect")


class TestBladeFlashGoAgain(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _setup(self.env)

    def test_blade_flash_on_reaction_stack(self):
        self.assertEqual(self.env._phase, Phase.REACTION)
        self.assertIsNotNone(self.env._instant_stack)
        stack_names = [card.name for _, card in self.env._instant_stack]
        self.assertIn("Blade Flash", stack_names)

    def test_dawnblade_card_go_again_not_mutated_before_resolution(self):
        # Before the reaction resolves, the weapon Card object must be clean
        from cards import Keyword
        self.assertNotIn(Keyword.GO_AGAIN, self.dorinthea.weapon.keywords,
                         "Dawnblade must not have GO_AGAIN keyword before Blade Flash resolves")

    def test_pending_attack_go_again_flag_set(self):
        # After pitching for Blade Flash (stack pending), the flag is not yet set
        # (it is set during _resolve_reaction_from_stack, which fires when both pass)
        self.assertFalse(self.env._pending_attack_go_again)

    def test_blade_flash_grants_go_again_once(self):
        """Resolving Blade Flash gives Dorinthea +1 AP via go again."""
        ap_before = self.dorinthea.action_points  # 0 after spending on weapon attack
        rhinar_life_before = self.rhinar.life

        # Pass priority twice to resolve Blade Flash and close the reaction window.
        # First PASS: Dorinthea passes → Rhinar auto-passes → Blade Flash resolves.
        # Second PASS: Dorinthea passes → Rhinar auto-passes → combat resolves.
        for _ in range(2):
            legal = self.env.legal_actions()
            pass_action = next(
                a for a in legal if a.action_type == ActionType.PASS_PRIORITY
            )
            self.env.step(pass_action)

        # Go again granted by Blade Flash
        self.assertEqual(self.dorinthea.action_points, ap_before + 1,
                         "Blade Flash should grant exactly +1 action point via go again")
        # Dawnblade hit Rhinar (power=2, no blocks)
        self.assertEqual(self.rhinar.life, rhinar_life_before - 2)
        self.assertEqual(self.dorinthea.weapon_attack_count, 1)

    def test_dawnblade_go_again_not_permanently_set(self):
        """After Blade Flash resolves, Dawnblade.go_again must remain False.

        The bug was that _pending_attack.go_again = True was set directly on the
        weapon Card object, which persists across attacks and turns. With the fix,
        a separate _pending_attack_go_again flag is used instead.
        """
        # Resolve Blade Flash and combat
        for _ in range(2):
            legal = self.env.legal_actions()
            pass_action = next(
                a for a in legal if a.action_type == ActionType.PASS_PRIORITY
            )
            self.env.step(pass_action)

        from cards import Keyword
        self.assertNotIn(Keyword.GO_AGAIN, self.dorinthea.weapon.keywords,
                         "Dawnblade must not have GO_AGAIN keyword after Blade Flash resolves — "
                         "the flag must not be permanently set on the Card object")

    def test_pending_attack_go_again_cleared_after_combat(self):
        """The temporary go-again flag must be reset after the attack resolves."""
        for _ in range(2):
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertFalse(self.env._pending_attack_go_again,
                         "_pending_attack_go_again must be False after combat resolves")


if __name__ == "__main__":
    unittest.main()

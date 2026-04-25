"""
Tests for Warrior's Valor action card.

Seed 5 gives:
  Dorinthea:  Flock of the Feather Walkers, Ironsong Response, Sigil of Solace, Warrior's Valor
  Rhinar:     Smash with Big Tree x2, Bare Fangs, Raging Onslaught
  Dorinthea wins the coin flip.

Warrior's Valor card text:
  "Your next weapon attack this turn gains +3 power and
   'When this attack hits, it gains go again.' Go again."

Expected behaviour:
  - Playing Warrior's Valor sets next_weapon_go_again_if_hits (not next_weapon_go_again).
  - The weapon attack gains +3 power.
  - If the weapon attack HITS  → the weapon gains go again (AP +1, weapon_additional_attack set).
  - If the weapon attack MISSES → no go again is granted.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass, Keyword

SEED = 5  # Dorinthea has Warrior's Valor; Dorinthea wins coin flip


def _play_warriors_valor(env):
    """
    Reset at SEED and advance to the point after Warrior's Valor has been played
    and pitched for.

    Step sequence
    -------------
    1. Dorinthea chooses GO_FIRST.
    2. Dorinthea plays Warrior's Valor (cost 1).
    3. Pitch Sigil of Solace [index 2] (pitch=3) to over-pay, leaving RP=2 for
       the subsequent weapon attack (cost 1) without needing another pitch step.

    Returns (dorinthea, rhinar).
    """
    env.reset(seed=SEED)
    dorinthea = env._game.players[1]
    rhinar = env._game.players[0]

    # Step 1: Dorinthea wins coin flip and goes first
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))

    # Step 2: Play Warrior's Valor
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD
        and a.card is not None
        and a.card.name == "Warrior's Valor"
    ))

    # Step 3: Pitch Sigil of Solace (pitch=3) to over-pay; RP=2 remains.
    # This avoids a second PITCH step when attacking with Dawnblade (cost=1).
    assert env._phase == Phase.PITCH
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.pitch_indices == [2]))

    return dorinthea, rhinar


def _setup_weapon_attack_hit(env):
    """
    Advance past _play_warriors_valor through to the DEFEND step where Rhinar
    does not block (guaranteed hit: Dawnblade 2 + WV bonus 3 = 5 power,
    Rhinar max single-card block at seed 5 is 3).

    Returns (dorinthea, rhinar).
    """
    dorinthea, rhinar = _play_warriors_valor(env)

    # Step 4: Attack with Dawnblade (cost 1; RP=2 left from Sigil of Solace pitch)
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

    # Reaction window auto-collapses; now in DEFEND phase
    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"

    # Step 5: Rhinar does not defend
    legal = env.legal_actions()
    no_def = next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and not a.defend_hand_indices
        and not a.defend_equip_slots
    )
    env.step(no_def)

    return dorinthea, rhinar


def _setup_weapon_attack_miss(env):
    """
    Advance past _play_warriors_valor through to the DEFEND step, then
    artificially lower the pending attack power so Rhinar can fully block it.
    Rhinar defends with Raging Onslaught (def=3), which fully blocks the
    lowered power of 3, resulting in 0 damage (a miss).

    The defend mechanism is two-step: first add a card (stays in DEFEND),
    then commit with an empty action (opens post-defend reaction window).

    Returns (dorinthea, rhinar).
    """
    dorinthea, rhinar = _play_warriors_valor(env)

    # Step 4: Attack with Dawnblade
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"

    # Lower attack power so it can be fully blocked by Raging Onslaught (def=3)
    env._pending_attack_power = 3

    # Step 5a: Rhinar adds Raging Onslaught (index 3, def=3) to the block pile
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and a.defend_hand_indices == [3]
        and not a.defend_equip_slots
    ))
    assert env._phase == Phase.DEFEND, f"Expected DEFEND after adding blocker, got {env._phase}"

    # Step 5b: Empty DEFEND commits the block and opens the post-defend reaction window
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and not a.defend_hand_indices
        and not a.defend_equip_slots
    ))

    return dorinthea, rhinar


class TestWarriorsValorDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Warrior's Valor", names)

    def test_card_properties(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Warrior's Valor")
        self.assertEqual(card.card_type, CardType.ACTION)
        self.assertEqual(card.cost, 1)
        self.assertEqual(card.pitch, 1)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.RED)
        self.assertEqual(card.card_class, CardClass.WARRIOR)
        self.assertIn(Keyword.GO_AGAIN, card.keywords)

    def test_card_text_mentions_on_hit(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Warrior's Valor")
        self.assertIn("hits", card.text.lower(),
                      "Card text must describe the on-hit go-again condition")


class TestWarriorsValorFlags(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _play_warriors_valor(self.env)

    def test_weapon_power_bonus_applied(self):
        self.assertEqual(self.dorinthea.next_weapon_power_bonus, 3)

    def test_conditional_go_again_flag_set(self):
        self.assertTrue(self.dorinthea.next_weapon_go_again_if_hits,
                        "next_weapon_go_again_if_hits must be True after playing Warrior's Valor")

    def test_unconditional_go_again_flag_not_set(self):
        self.assertFalse(self.dorinthea.next_weapon_go_again,
                         "next_weapon_go_again must NOT be set by Warrior's Valor "
                         "(go again is conditional on the attack hitting)")


class TestWarriorsValorHit(unittest.TestCase):
    """Weapon attack deals damage → Warrior's Valor go again is granted."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _setup_weapon_attack_hit(self.env)

    def test_rhinar_takes_damage(self):
        # Collapse post-defend reaction window, then check damage
        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        # Dawnblade 2 + WV +3 = 5 power, no block → 5 damage
        self.assertEqual(self.rhinar.life, 15,
                         "Rhinar should take 5 damage from unblocked Dawnblade+WV attack")

    def test_go_again_granted_on_hit(self):
        # Reaction window after combat: both players pass → back to ATTACK
        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertEqual(self.dorinthea.action_points, 1,
                         "Warrior's Valor: weapon attack that hits should grant go again (+1 AP)")

    def test_weapon_additional_attack_set_on_hit(self):
        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertTrue(self.dorinthea.weapon_additional_attack,
                        "weapon_additional_attack should be set after go again on first weapon attack")

    def test_if_hits_flag_cleared_after_resolution(self):
        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertFalse(self.dorinthea.next_weapon_go_again_if_hits,
                         "next_weapon_go_again_if_hits must be cleared after weapon resolves")


class TestWarriorsValorMiss(unittest.TestCase):
    """Weapon attack is fully blocked → Warrior's Valor go again is NOT granted."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _setup_weapon_attack_miss(self.env)

    def test_rhinar_takes_no_damage(self):
        self.assertEqual(self.rhinar.life, 20,
                         "Fully blocked attack should deal 0 damage")

    def test_go_again_not_granted_on_miss(self):
        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertEqual(self.dorinthea.action_points, 0,
                         "Warrior's Valor: weapon attack that misses must NOT grant go again")

    def test_if_hits_flag_cleared_after_resolution(self):
        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertFalse(self.dorinthea.next_weapon_go_again_if_hits,
                         "next_weapon_go_again_if_hits must be cleared even when attack misses")


if __name__ == "__main__":
    unittest.main()

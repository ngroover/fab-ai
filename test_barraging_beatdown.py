"""
Tests for Barraging Beatdown (Yellow).

Seed 3 gives:
  Rhinar:     Beast Mode, Barraging Beatdown, Pack Call, Bare Fangs
  Dorinthea:  En Garde, Flock of the Feather Walkers, Visit the Blacksmith, On a Knife Edge
  Dorinthea wins the coin flip; GO_SECOND puts Rhinar first.

Barraging Beatdown should:
  - Set next_brute_attack_conditional_bonus = 3 (not next_brute_attack_bonus)
  - Give +3 power to the next Brute attack when defended by 0 non-equipment cards
  - Give +3 power to the next Brute attack when defended by exactly 1 non-equipment card
  - NOT give +3 power when defended by 2 or more non-equipment cards
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass, Keyword
from card_effects import EffectTrigger, EffectAction

SEED = 3


def _setup_after_bb_and_bare_fangs(env):
    """
    Reset at SEED, advance to DEFEND phase with Barraging Beatdown played and
    Bare Fangs declared as the attacking card.

    Step sequence
    -------------
    1. GO_SECOND (Dorinthea passes priority so Rhinar goes first)
    2. Rhinar plays Barraging Beatdown (cost 0, go again, intimidate fires)
    3. Rhinar plays Bare Fangs (cost 2, attack)
    4. Pitch Pack Call (pitch 2) to cover Bare Fangs cost
       → Bare Fangs draws/discards, power becomes 8 (6 + 2 DRAW_DISCARD_POWER_BONUS)
       → DEFEND phase opens

    Returns (rhinar, dorinthea).
    """
    env.reset(seed=SEED)
    rhinar = env._game.players[0]
    dorinthea = env._game.players[1]

    # Step 1: Rhinar goes first
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_SECOND))

    # Step 2: Play Barraging Beatdown (cost 0, no pitch needed)
    legal = env.legal_actions()
    bb = next(a for a in legal
              if a.action_type == ActionType.PLAY_CARD
              and a.card is not None
              and a.card.name == "Barraging Beatdown")
    env.step(bb)

    # Step 3: Play Bare Fangs (cost 2, attack action)
    legal = env.legal_actions()
    bf = next(a for a in legal
              if a.action_type == ActionType.PLAY_CARD
              and a.card is not None
              and a.card.name == "Bare Fangs")
    env.step(bf)

    # Step 4: Pitch Pack Call (index 1, pitch value 2) to cover cost 2
    legal = env.legal_actions()
    pitch = next(a for a in legal
                 if a.action_type == ActionType.PITCH and 1 in a.pitch_indices)
    env.step(pitch)

    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"
    return rhinar, dorinthea


def _commit_no_block(env):
    """Defender commits with no blocking cards."""
    legal = env.legal_actions()
    no_block = next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and not a.defend_hand_indices
        and not a.defend_equip_slots
    )
    env.step(no_block)


def _pass_reactions(env):
    """Pass through any REACTION phase until it closes."""
    while env._phase.name in ("REACTION", "INSTANT"):
        legal = env.legal_actions()
        pass_a = next(
            (a for a in legal if a.action_type == ActionType.PASS_PRIORITY), None
        )
        if pass_a:
            env.step(pass_a)
        else:
            break


class TestBarragingBeatdownCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.rhinar = self.env._game.players[0]

    def test_in_rhinar_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Barraging Beatdown", names)

    def test_card_properties(self):
        card = next(c for c in self.rhinar.hand if c.name == "Barraging Beatdown")
        self.assertEqual(card.card_type, CardType.ACTION)
        self.assertEqual(card.cost, 0)
        self.assertEqual(card.pitch, 2)
        self.assertEqual(card.power, 0)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.YELLOW)
        self.assertEqual(card.card_class, CardClass.BRUTE)
        self.assertIn(Keyword.GO_AGAIN, card.keywords)

    def test_has_intimidate_on_play_effect(self):
        card = next(c for c in self.rhinar.hand if c.name == "Barraging Beatdown")
        matching = [
            e for e in card.effects
            if e.trigger == EffectTrigger.ON_PLAY
            and e.action == EffectAction.INTIMIDATE
        ]
        self.assertEqual(len(matching), 1,
                         "Barraging Beatdown must have exactly one ON_PLAY INTIMIDATE effect")


class TestBarragingBeatdownOnPlay(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.GO_SECOND))

        legal = self.env.legal_actions()
        bb = next(a for a in legal
                  if a.action_type == ActionType.PLAY_CARD
                  and a.card is not None
                  and a.card.name == "Barraging Beatdown")
        self.env.step(bb)

    def test_sets_conditional_bonus_not_unconditional(self):
        self.assertEqual(self.rhinar.next_brute_attack_conditional_bonus, 3,
                         "Barraging Beatdown must set next_brute_attack_conditional_bonus=3")
        self.assertEqual(self.rhinar.next_brute_attack_bonus, 0,
                         "Barraging Beatdown must NOT set next_brute_attack_bonus")

    def test_go_again_grants_action_point(self):
        self.assertGreaterEqual(self.rhinar.action_points, 1,
                                "Barraging Beatdown go again must grant an action point")

    def test_intimidate_banishes_opponent_card(self):
        self.assertEqual(len(self.dorinthea.banished), 1,
                         "Intimidate must banish one card from Dorinthea's hand")


class TestBarragingBeatdownBonusApplies_NoBlock(unittest.TestCase):
    """When defender blocks with 0 non-equipment cards, +3 must apply."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _setup_after_bb_and_bare_fangs(self.env)
        self.life_before = self.dorinthea.life

        _commit_no_block(self.env)
        _pass_reactions(self.env)

    def test_bonus_applied_no_block(self):
        # Bare Fangs base 6 + DRAW_DISCARD_POWER_BONUS +2 + BB conditional +3 = 11 damage
        expected_damage = 11
        self.assertEqual(self.dorinthea.life, self.life_before - expected_damage,
                         f"Dorinthea should take {expected_damage} damage (BB +3 applies with 0 blockers)")

    def test_conditional_bonus_consumed(self):
        self.assertEqual(self.rhinar.next_brute_attack_conditional_bonus, 0,
                         "Conditional bonus must be cleared after attack resolves")


class TestBarragingBeatdownBonusApplies_OneBlock(unittest.TestCase):
    """When defender blocks with exactly 1 non-equipment card, +3 must apply."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _setup_after_bb_and_bare_fangs(self.env)
        self.life_before = self.dorinthea.life

        # Block with Flock of the Feather Walkers (index 0, defense 2)
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and a.defend_hand_indices == [0]))
        # Commit
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and not a.defend_hand_indices
                           and not a.defend_equip_slots))
        _pass_reactions(self.env)

    def test_bonus_applied_one_block(self):
        # Bare Fangs base 6 + DRAW_DISCARD_POWER_BONUS +2 + BB conditional +3 = 11 attack power
        # Flock of the Feather Walkers defense = 2 → damage = 11 - 2 = 9
        expected_damage = 9
        self.assertEqual(self.dorinthea.life, self.life_before - expected_damage,
                         f"Dorinthea should take {expected_damage} damage (BB +3 applies with 1 blocker)")


class TestBarragingBeatdownBonusNullified_TwoBlocks(unittest.TestCase):
    """When defender blocks with 2 non-equipment cards, +3 must NOT apply."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _setup_after_bb_and_bare_fangs(self.env)
        self.life_before = self.dorinthea.life

        # Block with Flock of the Feather Walkers (index 0, defense 2)
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and a.defend_hand_indices == [0]))

        # Block with On a Knife Edge (index 1, defense 2)
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and a.defend_hand_indices == [1]))

        # Commit
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and not a.defend_hand_indices
                           and not a.defend_equip_slots))
        _pass_reactions(self.env)

    def test_bonus_nullified_two_blocks(self):
        # No BB conditional bonus: Bare Fangs 6 + DRAW_DISCARD_POWER_BONUS +2 = 8 attack power
        # Flock (def 2) + On a Knife Edge (def 2) = 4 total defense → damage = 8 - 4 = 4
        expected_damage = 4
        self.assertEqual(self.dorinthea.life, self.life_before - expected_damage,
                         f"Dorinthea should take {expected_damage} damage (BB +3 nullified with 2 blockers)")

    def test_conditional_bonus_consumed_even_when_nullified(self):
        self.assertEqual(self.rhinar.next_brute_attack_conditional_bonus, 0,
                         "Conditional bonus must be cleared even when nullified")


if __name__ == "__main__":
    unittest.main()

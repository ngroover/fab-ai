"""
Tests for Barraging Beatdown (Yellow).

Seed 3 gives:
  Rhinar:     Beast Mode, Barraging Beatdown, Pack Call, Bare Fangs
  Dorinthea:  En Garde, Flock of the Feather Walkers, Visit the Blacksmith, On a Knife Edge
  Dorinthea wins the coin flip; GO_SECOND puts Rhinar first.

Seed 155 gives:
  Rhinar:     Barraging Beatdown x2, Bare Fangs, Come to Fight
  Rhinar wins the coin flip and goes first with GO_FIRST.

Barraging Beatdown should:
  - Set next_brute_attack_conditional_bonus = 3 (not next_brute_attack_bonus)
  - Give +3 power to the next Brute attack when defended by 0 non-equipment cards
  - Give +3 power to the next Brute attack when defended by exactly 1 non-equipment card
  - NOT give +3 power when defended by 2 or more non-equipment cards
  - Stack: two BBs played in the same turn give +6 conditional bonus
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass, Keyword
from cards import build_rhinar_deck, build_dorinthea_deck

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
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
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

    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])

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
                 if a.action_type == ActionType.PITCH and a.pitch_index == 1)
    env.step(pitch)

    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])

    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"
    return rhinar, dorinthea


def _commit_no_block(env):
    """Defender commits with no blocking cards."""
    legal = env.legal_actions()
    no_block = next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and a.hand_index is None
        and a.equip_slot is None
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
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.env.step(self.env.legal_actions()[0])  # resolve CHOOSE_FIRST
        self.rhinar = self.env._game.players[0]

    def test_in_rhinar_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Barraging Beatdown", names)

    def test_card_properties(self):
        card = next(c for c in self.rhinar.hand if c.name == "Barraging Beatdown")
        self.assertEqual(card.card_type, [CardType.ACTION])
        self.assertEqual(card.cost, 0)
        self.assertEqual(card.pitch, 2)
        self.assertEqual(card.power, 0)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.YELLOW)
        self.assertEqual(card.card_class, CardClass.BRUTE)
        self.assertIn(Keyword.GO_AGAIN, card.keywords)

    def test_has_intimidate_keyword(self):
        card = next(c for c in self.rhinar.hand if c.name == "Barraging Beatdown")
        self.assertIn(Keyword.INTIMIDATE, card.keywords,
                      "Barraging Beatdown must have the Intimidate keyword")


class TestBarragingBeatdownOnPlay(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
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

        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

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
                           and a.hand_index == 0))
        # Commit
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and a.hand_index is None
                           and a.equip_slot is None))
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

        # Block with Flock of the Feather Walkers
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and a.hand_index is not None
                           and self.dorinthea.hand[a.hand_index].name == "Flock of the Feather Walkers"))

        # Block with On a Knife Edge (now shifted in hand after previous removal)
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and a.hand_index is not None
                           and self.dorinthea.hand[a.hand_index].name == "On a Knife Edge"))

        # Commit
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal
                           if a.action_type == ActionType.DEFEND
                           and a.hand_index is None
                           and a.equip_slot is None))
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


SEED_TWO_BB = 155


def _setup_two_bb_then_bare_fangs(env):
    """
    Seed 155: Rhinar has BB x2, Bare Fangs, Come to Fight.
    Sequence: GO_FIRST → BB1 → BB2 → Bare Fangs (auto-pitches Come to Fight).
    Returns (rhinar, dorinthea) with env in DEFEND phase.
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED_TWO_BB)
    rhinar = env._game.players[0]
    dorinthea = env._game.players[1]

    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST))

    for _ in range(2):
        legal = env.legal_actions()
        env.step(next(a for a in legal if a.action_type == ActionType.PLAY_CARD
                      and a.card and a.card.name == "Barraging Beatdown"))
        while env._phase == Phase.INSTANT:
            env.step(env.legal_actions()[0])

    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.PLAY_CARD
                  and a.card and a.card.name == "Bare Fangs"))
    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])

    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"
    return rhinar, dorinthea


class TestBarragingBeatdownStacking(unittest.TestCase):
    """Two BBs played in one turn must stack to +6 conditional bonus."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _setup_two_bb_then_bare_fangs(self.env)
        self.life_before = self.dorinthea.life

        legal = self.env.legal_actions()
        no_block = next(a for a in legal if a.action_type == ActionType.DEFEND
                        and a.hand_index is None and a.equip_slot is None)
        self.env.step(no_block)

        while self.env._phase.name in ("REACTION", "INSTANT"):
            legal = self.env.legal_actions()
            pass_a = next((a for a in legal if a.action_type == ActionType.PASS_PRIORITY), None)
            if pass_a:
                self.env.step(pass_a)
            else:
                break

    def test_two_bb_bonus_stacks_to_six(self):
        # Bare Fangs base 6 + 2x BB conditional +6 = 12 damage (no block)
        expected_damage = 12
        self.assertEqual(self.dorinthea.life, self.life_before - expected_damage,
                         f"Two BBs must give +6 bonus: expected {expected_damage} damage, "
                         f"got {self.life_before - self.dorinthea.life}")

    def test_conditional_bonus_consumed_after_attack(self):
        self.assertEqual(self.rhinar.next_brute_attack_conditional_bonus, 0,
                         "Conditional bonus must be cleared after attack resolves")


# ─────────────────────────────────────────────────────────────────────────
# Brute-class targeting: the conditional bonus must apply only to Brute
# attacks. Generic action cards (e.g. Raging Onslaught) must not benefit
# and must not consume the bonus. Brute weapons (Bone Basher) DO benefit.
# ─────────────────────────────────────────────────────────────────────────

SEED_BB_RO = 21
# Seed 21: Rhinar wins coin flip; hand:
#   ['Smash Instinct', 'Barraging Beatdown', 'Bare Fangs', 'Raging Onslaught']


class TestBarragingBeatdownDoesNotApplyToGenericAttack(unittest.TestCase):
    """BB conditional bonus must NOT apply to (or be consumed by) a Generic
    attack action card (Raging Onslaught)."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED_BB_RO)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

        self.env.step(next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.GO_FIRST))

        # Play BB (cost 0).
        bb = next(a for a in self.env.legal_actions()
                  if a.action_type == ActionType.PLAY_CARD
                  and a.card is not None
                  and a.card.name == "Barraging Beatdown")
        self.env.step(bb)
        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

        # Play Raging Onslaught (Generic 6/3, cost 3). Pitch Smash Instinct (2)
        # then Bare Fangs (1) to cover.
        ro = next(a for a in self.env.legal_actions()
                  if a.action_type == ActionType.PLAY_CARD
                  and a.card is not None
                  and a.card.name == "Raging Onslaught")
        self.env.step(ro)

        for name in ("Smash Instinct", "Bare Fangs"):
            if self.env._phase != Phase.PITCH:
                break
            pitch_a = next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.PITCH
                           and a.pitch_index is not None
                           and self.rhinar.hand[a.pitch_index].name == name)
            self.env.step(pitch_a)

        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

        self.life_before = self.dorinthea.life

    def test_pending_attack_is_raging_onslaught(self):
        self.assertEqual(self.env._pending_attack.name, "Raging Onslaught")
        self.assertEqual(self.env._pending_attack.card_class, CardClass.GENERIC)

    def test_generic_takes_no_bonus_no_block(self):
        _commit_no_block(self.env)
        _pass_reactions(self.env)
        # Raging Onslaught 6 power, no block, no bonus → 6 damage.
        self.assertEqual(self.dorinthea.life, self.life_before - 6,
                         "Generic attack must NOT get BB's +3 conditional bonus")

    def test_bonus_not_consumed_by_generic_attack(self):
        _commit_no_block(self.env)
        _pass_reactions(self.env)
        self.assertEqual(self.rhinar.next_brute_attack_conditional_bonus, 3,
                         "Conditional bonus must NOT be consumed by a non-Brute attack")


class TestBarragingBeatdownAppliesToBruteWeapon(unittest.TestCase):
    """Unlike Awakening Bellow, Barraging Beatdown's wording 'next Brute attack'
    DOES include weapon attacks. Bone Basher is a Brute weapon, so the
    conditional bonus must apply and be consumed."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED_BB_RO)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

        self.env.step(next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.GO_FIRST))

        # Play BB (cost 0).
        bb = next(a for a in self.env.legal_actions()
                  if a.action_type == ActionType.PLAY_CARD
                  and a.card is not None
                  and a.card.name == "Barraging Beatdown")
        self.env.step(bb)
        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

        # WEAPON (Bone Basher cost 2). Pitch Smash Instinct (2).
        w = next(a for a in self.env.legal_actions()
                 if a.action_type == ActionType.WEAPON)
        self.env.step(w)
        pitch_a = next(a for a in self.env.legal_actions()
                       if a.action_type == ActionType.PITCH
                       and a.pitch_index is not None
                       and self.rhinar.hand[a.pitch_index].name == "Smash Instinct")
        self.env.step(pitch_a)
        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

        self.life_before = self.dorinthea.life

    def test_pending_attack_is_bone_basher(self):
        self.assertEqual(self.env._pending_attack.name, "Bone Basher")
        self.assertEqual(self.env._pending_attack.card_class, CardClass.BRUTE)

    def test_brute_weapon_gets_bonus_no_block(self):
        _commit_no_block(self.env)
        _pass_reactions(self.env)
        # Bone Basher base 4 + BB conditional +3 (no block, <2 blockers) = 7 damage.
        self.assertEqual(self.dorinthea.life, self.life_before - 7,
                         "Brute weapon attack must get BB's +3 conditional bonus")

    def test_bonus_consumed_by_brute_weapon(self):
        _commit_no_block(self.env)
        _pass_reactions(self.env)
        self.assertEqual(self.rhinar.next_brute_attack_conditional_bonus, 0,
                         "Conditional bonus must be consumed by a Brute weapon attack")


if __name__ == "__main__":
    unittest.main()

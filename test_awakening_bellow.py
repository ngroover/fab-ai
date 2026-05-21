"""
Tests for Awakening Bellow (Red).

Awakening Bellow text:
  "Go again. Intimidate. Your next Brute attack action card this turn
  has +3 power."

The bonus:
  - Applies only to attacks whose card is a Brute attack ACTION card.
  - Does NOT apply to Generic attack action cards (e.g. Raging Onslaught).
  - Does NOT apply to weapon attacks (even Brute weapons like Bone Basher).
  - Persists across non-matching attacks until consumed by a matching one.

Seed 70 → Rhinar hand: ['Smash with Big Tree', 'Awakening Bellow',
                        'Raging Onslaught', 'Wrecker Romp']
  - Awakening Bellow: Brute action, cost 1, pitch 1
  - Smash with Big Tree: Brute attack action, 6/0 no_block, cost 2, pitch 2
  - Raging Onslaught: Generic attack action, 6/3, cost 3, pitch 2
  - Wrecker Romp: Brute attack action, 6/3, cost 2, pitch 3
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass, Keyword
from cards import build_rhinar_deck, build_dorinthea_deck

SEED = 70


def _go_first(env):
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST))


def _play_ab_pitching(env, name_to_pitch):
    """Play Awakening Bellow, pitching the card with the given name."""
    rhinar = env._game.players[0]
    ab = next(a for a in env.legal_actions()
              if a.action_type == ActionType.PLAY_CARD
              and a.card is not None
              and a.card.name == "Awakening Bellow")
    env.step(ab)

    pitch_a = next(a for a in env.legal_actions()
                   if a.action_type == ActionType.PITCH
                   and a.pitch_index is not None
                   and rhinar.hand[a.pitch_index].name == name_to_pitch)
    env.step(pitch_a)

    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])


def _pass_reactions(env):
    while env._phase.name in ("REACTION", "INSTANT"):
        legal = env.legal_actions()
        pass_a = next((a for a in legal if a.action_type == ActionType.PASS_PRIORITY), None)
        if pass_a:
            env.step(pass_a)
        else:
            break


def _commit_no_block(env):
    no_block = next(
        a for a in env.legal_actions()
        if a.action_type == ActionType.DEFEND
        and a.hand_index is None
        and a.equip_slot is None
    )
    env.step(no_block)


class TestAwakeningBellowCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.env.step(self.env.legal_actions()[0])  # CHOOSE_FIRST
        self.rhinar = self.env._game.players[0]

    def test_in_rhinar_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Awakening Bellow", names)

    def test_card_properties(self):
        card = next(c for c in self.rhinar.hand if c.name == "Awakening Bellow")
        self.assertEqual(card.card_type, [CardType.ACTION])
        self.assertEqual(card.cost, 1)
        self.assertEqual(card.pitch, 1)
        self.assertEqual(card.color, Color.RED)
        self.assertEqual(card.card_class, CardClass.BRUTE)
        self.assertIn(Keyword.GO_AGAIN, card.keywords)
        self.assertIn(Keyword.INTIMIDATE, card.keywords)


class TestAwakeningBellowOnPlay(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.rhinar = self.env._game.players[0]
        _go_first(self.env)
        _play_ab_pitching(self.env, "Raging Onslaught")

    def test_sets_next_brute_attack_bonus(self):
        self.assertEqual(self.rhinar.next_brute_attack_bonus, 3,
                         "Awakening Bellow must set next_brute_attack_bonus=3")

    def test_does_not_set_conditional_bonus(self):
        self.assertEqual(self.rhinar.next_brute_attack_conditional_bonus, 0,
                         "Awakening Bellow must NOT set conditional bonus")

    def test_go_again_grants_action_point(self):
        self.assertGreaterEqual(self.rhinar.action_points, 1,
                                "Awakening Bellow go again must grant an action point")


class TestAwakeningBellowAppliesToBruteAttackAction(unittest.TestCase):
    """+3 must apply when the next attack is a Brute attack action card."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]
        _go_first(self.env)
        _play_ab_pitching(self.env, "Raging Onslaught")

        # Play Smash with Big Tree (Brute attack action, base 6).
        # cost 2, resources 1, only Wrecker Romp (pitch 3) available → auto-pitch.
        swbt = next(a for a in self.env.legal_actions()
                    if a.action_type == ActionType.PLAY_CARD
                    and a.card is not None
                    and a.card.name == "Smash with Big Tree")
        self.env.step(swbt)
        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

    def test_pending_attack_is_smash_with_big_tree(self):
        self.assertEqual(self.env._pending_attack.name, "Smash with Big Tree")

    def test_brute_attack_gets_bonus(self):
        # Smash with Big Tree base 6 + Awakening Bellow +3 = 9
        self.assertEqual(self.env._pending_attack_power, 9,
                         "Brute attack action card must get Awakening Bellow's +3 power")

    def test_bonus_consumed_after_resolution(self):
        _commit_no_block(self.env)
        _pass_reactions(self.env)
        self.assertEqual(self.rhinar.next_brute_attack_bonus, 0,
                         "Bonus must be cleared after a Brute attack action resolves")


class TestAwakeningBellowDoesNotApplyToGenericAttack(unittest.TestCase):
    """The user-reported bug:

    Awakening Bellow + Raging Onslaught (Generic) was attacking for 9 power.
    Correct behavior: Raging Onslaught is Generic, so the bonus must NOT
    apply, and must NOT be consumed (it waits for the next Brute attack).
    """

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]
        _go_first(self.env)
        # Pitch Wrecker Romp (3) for Awakening Bellow → 2 resources remain.
        _play_ab_pitching(self.env, "Wrecker Romp")

        # Play Raging Onslaught (Generic 6/3, cost 3).
        # Need 1 more from pitch; only Smash with Big Tree (pitch 2) available
        # → env auto-pitches.
        ro = next(a for a in self.env.legal_actions()
                  if a.action_type == ActionType.PLAY_CARD
                  and a.card is not None
                  and a.card.name == "Raging Onslaught")
        self.env.step(ro)
        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

    def test_pending_attack_is_raging_onslaught(self):
        self.assertEqual(self.env._pending_attack.name, "Raging Onslaught")
        self.assertEqual(self.env._pending_attack.card_class, CardClass.GENERIC)

    def test_generic_attack_does_not_get_brute_bonus(self):
        # The bug: would have been 9 (6 base + 3). Correct: 6.
        self.assertEqual(self.env._pending_attack_power, 6,
                         "Generic attack must NOT get Awakening Bellow's +3 power")

    def test_bonus_not_consumed_by_generic_attack(self):
        _commit_no_block(self.env)
        _pass_reactions(self.env)
        self.assertEqual(self.rhinar.next_brute_attack_bonus, 3,
                         "Bonus must NOT be consumed when a non-Brute attack resolves")


class TestAwakeningBellowDoesNotApplyToWeapon(unittest.TestCase):
    """Bone Basher is a Brute *weapon*, not an action card.

    Awakening Bellow specifies 'Brute attack action card' so the bonus must
    NOT apply to (or be consumed by) a weapon attack.
    """

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.rhinar = self.env._game.players[0]
        _go_first(self.env)
        _play_ab_pitching(self.env, "Raging Onslaught")

        # WEAPON — Bone Basher costs 2, have 1, pitch Wrecker Romp (3).
        w = next(a for a in self.env.legal_actions()
                 if a.action_type == ActionType.WEAPON)
        self.env.step(w)
        pitch_a = next(a for a in self.env.legal_actions()
                       if a.action_type == ActionType.PITCH
                       and a.pitch_index is not None
                       and self.rhinar.hand[a.pitch_index].name == "Wrecker Romp")
        self.env.step(pitch_a)
        while self.env._phase == Phase.INSTANT:
            self.env.step(self.env.legal_actions()[0])

    def test_pending_attack_is_bone_basher(self):
        self.assertEqual(self.env._pending_attack.name, "Bone Basher")

    def test_weapon_attack_does_not_get_bonus(self):
        # Bone Basher base power is 4.
        self.assertEqual(self.env._pending_attack_power, 4,
                         "Bone Basher (weapon) must NOT get Awakening Bellow's +3")

    def test_bonus_not_consumed_by_weapon_attack(self):
        _commit_no_block(self.env)
        _pass_reactions(self.env)
        self.assertEqual(self.rhinar.next_brute_attack_bonus, 3,
                         "Bonus must NOT be consumed by a weapon attack")


if __name__ == "__main__":
    unittest.main()

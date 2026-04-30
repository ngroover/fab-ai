"""
Tests for Hala Goldenhelm mentor card behavior.

Seed 1 gives:
  Dorinthea: Run Through, Sharpen Steel, Flock of the Feather Walkers, Hala Goldenhelm
  Dorinthea (agent_1) wins the coin flip and goes first.

Hala Goldenhelm rules:
  MENTOR card for Dorinthea with defense 3.
  While face-down in arsenal, at the start of your turn you may flip her face-up.
  While face-up in arsenal, whenever a sword attack you control hits:
    - it gains go again
    - put a lesson counter on Hala
  If there are 2+ lesson counters: banish Hala, search deck for Glistening Steelblade,
  put it face-up in arsenal, shuffle.

At seed 1, Dorinthea stores Hala in arsenal on turn 1 (pitching Sharpen Steel).
On turn 3 the MENTOR_FLIP option fires. After flipping, two unblocked Dawnblade hits
trigger both lesson counters, banishing Hala and tutoring Glistening Steelblade.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import build_rhinar_deck, build_dorinthea_deck, CardType

SEED = 1


def _skip_to(env, phases, max_steps=300):
    """Auto-step (preferring pass/pass-priority) until one of the target phases is reached."""
    for _ in range(max_steps):
        if env._phase in phases:
            return True
        legal = env.legal_actions()
        if not legal:
            return False
        passables = [
            a for a in legal
            if a.action_type in (ActionType.PASS, ActionType.PASS_PRIORITY, ActionType.DEFEND)
            and not getattr(a, 'defend_hand_indices', [])
            and not getattr(a, 'defend_equip_slots', [])
        ]
        env.step(passables[0] if passables else legal[0])
    return False


def _advance_to_mentor_flip(env):
    """
    Reset at SEED and advance through to the MENTOR_FLIP phase at the start of
    Dorinthea's second turn (turn 3).

    Step sequence:
    1. Dorinthea wins coin flip → chooses GO_FIRST.
    2. Turn 1 (Dorinthea): weapon attack, then ARSENAL → store Hala Goldenhelm.
    3. Turn 2 (Rhinar): passes, stores nothing.
    4. Turn 3 (Dorinthea): MENTOR_FLIP phase fires.

    Returns (rhinar, dorinthea).
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    rhinar = env._game.players[0]
    dorinthea = env._game.players[1]

    # CHOOSE_FIRST: Dorinthea goes first
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST))

    # Turn 1: skip to ARSENAL then store Hala
    _skip_to(env, [Phase.ARSENAL])
    legal = env.legal_actions()
    hala_act = next(
        (a for a in legal
         if a.action_type == ActionType.ARSENAL
         and a.arsenal_hand_index >= 0
         and dorinthea.hand[a.arsenal_hand_index].name == 'Hala Goldenhelm'),
        None
    )
    env.step(hala_act or next(a for a in legal if a.arsenal_hand_index == -1))
    while env._phase == Phase.PITCH_ORDER:
        env.step(env.legal_actions()[0])

    # Turn 2 (Rhinar): skip to ARSENAL, store nothing
    _skip_to(env, [Phase.ARSENAL])
    env.step(next(a for a in env.legal_actions()
                  if a.action_type == ActionType.ARSENAL and a.arsenal_hand_index == -1))
    while env._phase == Phase.PITCH_ORDER:
        env.step(env.legal_actions()[0])

    # Now at start of Turn 3 (Dorinthea) → MENTOR_FLIP
    return rhinar, dorinthea


def _flip_and_hit_twice(env, dorinthea):
    """Flip Hala face-up, then execute two unblocked Dawnblade sword attacks."""
    # Flip face-up
    env.step(next(a for a in env.legal_actions()
                  if a.action_type == ActionType.MENTOR_FLIP and a.flip))

    # 1st sword hit
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON))
    _skip_to(env, [Phase.ATTACK])  # no block → hit fires; go-again returns us to ATTACK

    # 2nd sword hit (weapon_additional_attack granted by go-again)
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON))
    _skip_to(env, [Phase.ATTACK])


# ── Card definition tests ────────────────────────────────────────────────────

class TestHalaGoldenheimCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_hala_is_mentor_type(self):
        deck = build_dorinthea_deck()
        hala = next(c for c in deck if c.name == 'Hala Goldenhelm')
        self.assertEqual(hala.card_type, CardType.MENTOR)

    def test_hala_defense_value(self):
        deck = build_dorinthea_deck()
        hala = next(c for c in deck if c.name == 'Hala Goldenhelm')
        self.assertEqual(hala.defense, 3)

    def test_hala_in_dorinthea_opening_hand(self):
        self.assertIn('Hala Goldenhelm', [c.name for c in self.dorinthea.hand])


# ── Arsenal placement: face-down ─────────────────────────────────────────────

class TestHalaStartsFaceDown(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

        # GO_FIRST, skip to ARSENAL on turn 1
        self.env.step(next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.GO_FIRST))
        _skip_to(self.env, [Phase.ARSENAL])

    def test_hala_in_hand_before_storing(self):
        self.assertIn('Hala Goldenhelm', [c.name for c in self.dorinthea.hand])

    def test_hala_stored_face_down(self):
        legal = self.env.legal_actions()
        hala_act = next(
            a for a in legal
            if a.action_type == ActionType.ARSENAL
            and a.arsenal_hand_index >= 0
            and self.dorinthea.hand[a.arsenal_hand_index].name == 'Hala Goldenhelm'
        )
        self.env.step(hala_act)
        self.assertIsNotNone(self.dorinthea.arsenal)
        self.assertEqual(self.dorinthea.arsenal.name, 'Hala Goldenhelm')
        self.assertFalse(self.dorinthea.mentor_face_up,
                         "Hala should be face-down when first placed in arsenal")

    def test_no_mentor_flip_before_next_turn(self):
        """MENTOR_FLIP does not fire during the same turn Hala is stored."""
        legal = self.env.legal_actions()
        hala_act = next(
            a for a in legal
            if a.action_type == ActionType.ARSENAL
            and a.arsenal_hand_index >= 0
            and self.dorinthea.hand[a.arsenal_hand_index].name == 'Hala Goldenhelm'
        )
        self.env.step(hala_act)
        while self.env._phase == Phase.PITCH_ORDER:
            self.env.step(self.env.legal_actions()[0])
        # Turn 2 starts for Rhinar — no MENTOR_FLIP yet
        self.assertNotEqual(self.env._phase, Phase.MENTOR_FLIP)


# ── Start-of-turn flip option ────────────────────────────────────────────────

class TestMentorFlipPhase(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_mentor_flip(self.env)

    def test_mentor_flip_phase_fires(self):
        self.assertEqual(self.env._phase, Phase.MENTOR_FLIP)

    def test_mentor_flip_legal_actions(self):
        legal = self.env.legal_actions()
        types = {a.action_type for a in legal}
        self.assertEqual(types, {ActionType.MENTOR_FLIP})

    def test_both_flip_options_available(self):
        legal = self.env.legal_actions()
        flips = {a.flip for a in legal if a.action_type == ActionType.MENTOR_FLIP}
        self.assertIn(True, flips)
        self.assertIn(False, flips)

    def test_flip_true_sets_mentor_face_up(self):
        self.env.step(next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.MENTOR_FLIP and a.flip))
        self.assertTrue(self.dorinthea.mentor_face_up)

    def test_flip_false_keeps_face_down(self):
        self.env.step(next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.MENTOR_FLIP and not a.flip))
        self.assertFalse(self.dorinthea.mentor_face_up)

    def test_flip_transitions_to_attack_phase(self):
        self.env.step(next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.MENTOR_FLIP and a.flip))
        self.assertEqual(self.env._phase, Phase.ATTACK)

    def test_no_flip_transitions_to_attack_phase(self):
        self.env.step(next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.MENTOR_FLIP and not a.flip))
        self.assertEqual(self.env._phase, Phase.ATTACK)


# ── Sword hit while face-up: go again ────────────────────────────────────────

class TestHalaGoAgainOnSwordHit(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_mentor_flip(self.env)
        # Flip Hala face-up
        self.env.step(next(a for a in self.env.legal_actions()
                           if a.action_type == ActionType.MENTOR_FLIP and a.flip))

    def test_weapon_action_available_after_first_hit_go_again(self):
        """After Dawnblade hits while Hala is face-up, WEAPON action becomes available again."""
        self.env.step(next(a for a in self.env.legal_actions() if a.action_type == ActionType.WEAPON))
        _skip_to(self.env, [Phase.ATTACK])
        legal = self.env.legal_actions()
        self.assertTrue(any(a.action_type == ActionType.WEAPON for a in legal),
                        "Go again should make WEAPON available a second time after hit")

    def test_weapon_additional_attack_granted(self):
        """The weapon_additional_attack flag must be set after the hit to enable the 2nd swing."""
        self.env.step(next(a for a in self.env.legal_actions() if a.action_type == ActionType.WEAPON))
        _skip_to(self.env, [Phase.ATTACK])
        self.assertTrue(self.dorinthea.weapon_additional_attack)

    def test_lesson_counter_added_on_first_hit(self):
        self.env.step(next(a for a in self.env.legal_actions() if a.action_type == ActionType.WEAPON))
        _skip_to(self.env, [Phase.ATTACK])
        self.assertEqual(self.dorinthea.mentor_lesson_counters, 1)

    def test_hala_still_in_arsenal_after_first_hit(self):
        self.env.step(next(a for a in self.env.legal_actions() if a.action_type == ActionType.WEAPON))
        _skip_to(self.env, [Phase.ATTACK])
        self.assertIsNotNone(self.dorinthea.arsenal)
        self.assertEqual(self.dorinthea.arsenal.name, 'Hala Goldenhelm')

    def test_no_trigger_when_face_down(self):
        """Sword attack while Hala is still face-down must not add a lesson counter."""
        env2 = FaBEnv(verbose=False)
        rhinar2, dorinthea2 = _advance_to_mentor_flip(env2)
        # Do NOT flip (keep face-down)
        env2.step(next(a for a in env2.legal_actions()
                       if a.action_type == ActionType.MENTOR_FLIP and not a.flip))
        env2.step(next(a for a in env2.legal_actions() if a.action_type == ActionType.WEAPON))
        _skip_to(env2, [Phase.ATTACK])
        self.assertEqual(dorinthea2.mentor_lesson_counters, 0)
        self.assertFalse(dorinthea2.weapon_additional_attack,
                         "No go-again when Hala is face-down")


# ── Two hits: banish Hala, tutor Glistening Steelblade ───────────────────────

class TestHalaTwoLessonCounters(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_mentor_flip(self.env)
        _flip_and_hit_twice(self.env, self.dorinthea)

    def test_hala_banished(self):
        self.assertIn('Hala Goldenhelm', [c.name for c in self.dorinthea.permanently_banished])

    def test_hala_no_longer_in_arsenal(self):
        if self.dorinthea.arsenal:
            self.assertNotEqual(self.dorinthea.arsenal.name, 'Hala Goldenhelm')

    def test_glistening_steelblade_in_arsenal(self):
        self.assertIsNotNone(self.dorinthea.arsenal)
        self.assertEqual(self.dorinthea.arsenal.name, 'Glistening Steelblade')

    def test_lesson_counters_reset(self):
        self.assertEqual(self.dorinthea.mentor_lesson_counters, 0)

    def test_mentor_face_up_reset(self):
        self.assertFalse(self.dorinthea.mentor_face_up)

    def test_glistening_steelblade_not_in_deck(self):
        """Glistening Steelblade was removed from deck when tutored."""
        deck_names = [c.name for c in self.dorinthea.deck]
        self.assertNotIn('Glistening Steelblade', deck_names)


if __name__ == "__main__":
    unittest.main()

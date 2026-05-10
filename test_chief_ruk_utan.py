"""
Unit tests for Chief Ruk'utan's mentor effect.

Card text: While Ruk'utan is face up in arsenal, whenever you play a card with
6 or more power, intimidate and put a lesson counter on him.  Then if there are
2 or more lesson counters on Rok'utan, banish him, search your deck for Alpha
Rampage, put it face up in arsenal and shuffle.

Seed 14 gives Rhinar's opening hand:
  Wrecker Romp, Wounded Bull, Smash Instinct, Chief Ruk'utan

Test approach: advance to ATTACK phase with seed 14, then manually place
Chief Ruk'utan face-up in arsenal and give Rhinar pre-paid resources so we
can play a single attack card directly without a separate PITCH step.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import build_rhinar_deck, build_dorinthea_deck, CardType, Color, CardClass


SEED = 14


def _setup(env, mentor_face_up=True):
    """
    Reset at SEED, advance past CHOOSE_FIRST (Rhinar goes first), then:
      - Move Chief Ruk'utan from hand to arsenal
      - Set mentor_face_up as requested
      - Return (rhinar, dori) player objects
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    legal = env.legal_actions()
    go_first = next(a for a in legal if a.action_type == ActionType.GO_FIRST)
    env.step(go_first)

    rhinar = env._game.players[0]
    dori = env._game.players[1]

    chief = next(c for c in rhinar.hand if c.name == "Chief Ruk'utan")
    rhinar.hand.remove(chief)
    rhinar.arsenal = chief
    rhinar.mentor_face_up = mentor_face_up

    return rhinar, dori


def _play_attack(env, rhinar, card_name, required_resources):
    """Remove all hand cards except card_name, pre-pay resources, and play it."""
    card = next(c for c in rhinar.hand if c.name == card_name)
    rhinar.hand = [card]
    rhinar.resource_points = required_resources
    legal = env.legal_actions()
    action = next(a for a in legal
                  if a.action_type == ActionType.PLAY_CARD and a.card.name == card_name)
    env.step(action)
    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])


class TestChiefRukutanCardDefinition(unittest.TestCase):

    def setUp(self):
        env = FaBEnv()
        env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        env.step(env.legal_actions()[0])  # resolve CHOOSE_FIRST
        rhinar = env._game.players[0]
        self.card = next(c for c in rhinar.hand if c.name == "Chief Ruk'utan")

    def test_card_type_is_mentor(self):
        self.assertEqual(self.card.card_type, CardType.MENTOR)

    def test_card_is_in_opening_hand(self):
        env = FaBEnv()
        env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        env.step(env.legal_actions()[0])  # resolve CHOOSE_FIRST
        rhinar = env._game.players[0]
        names = [c.name for c in rhinar.hand]
        self.assertIn("Chief Ruk'utan", names)

    def test_card_class_is_brute(self):
        self.assertEqual(self.card.card_class, CardClass.BRUTE)

    def test_text_mentions_lesson_counter(self):
        self.assertIn("lesson counter", self.card.text.lower())

    def test_text_mentions_intimidate(self):
        self.assertIn("intimidate", self.card.text.lower())

    def test_text_mentions_alpha_rampage(self):
        self.assertIn("Alpha Rampage", self.card.text)


class TestChiefRukutanIntimidateEffect(unittest.TestCase):
    """Mentor face-up + 6-power attack → intimidate fires before defend."""

    def test_intimidate_fires_on_6_power_attack(self):
        env = FaBEnv()
        rhinar, dori = _setup(env, mentor_face_up=True)
        dori_hand_before = len(dori.hand)

        _play_attack(env, rhinar, "Wounded Bull", required_resources=3)

        self.assertEqual(env._phase, Phase.DEFEND,
                         "Should be in DEFEND phase after attack is declared")
        self.assertEqual(len(dori.banished), 1,
                         "Mentor intimidate must banish exactly 1 card from Dorinthea's hand")
        self.assertEqual(len(dori.hand), dori_hand_before - 1,
                         "Dorinthea's hand should shrink by 1 due to intimidate")

    def test_no_intimidate_when_mentor_face_down(self):
        env = FaBEnv()
        rhinar, dori = _setup(env, mentor_face_up=False)

        _play_attack(env, rhinar, "Wounded Bull", required_resources=3)

        self.assertEqual(len(dori.banished), 0,
                         "No intimidate when mentor is face-down")

    def test_no_intimidate_on_sub_6_power_attack(self):
        """Rally the Rearguard has power 4 — should not trigger mentor."""
        env = FaBEnv()
        rhinar, dori = _setup(env, mentor_face_up=True)
        # Rally the Rearguard is not in the opening hand; grab it from the deck
        rally = next(c for c in rhinar.deck if c.name == "Rally the Rearguard")
        rhinar.deck.remove(rally)
        rhinar.hand.append(rally)

        _play_attack(env, rhinar, "Rally the Rearguard", required_resources=2)

        self.assertEqual(len(dori.banished), 0,
                         "No intimidate for attacks with power < 6")
        self.assertEqual(rhinar.mentor_lesson_counters, 0,
                         "No lesson counter for attacks with power < 6")


class TestChiefRukutanLessonCounter(unittest.TestCase):
    """Lesson counter increments on each qualifying attack."""

    def test_lesson_counter_added_on_first_6_power_attack(self):
        env = FaBEnv()
        rhinar, dori = _setup(env, mentor_face_up=True)

        _play_attack(env, rhinar, "Wounded Bull", required_resources=3)

        self.assertEqual(rhinar.mentor_lesson_counters, 1,
                         "One lesson counter after first 6-power attack")

    def test_no_lesson_counter_when_mentor_face_down(self):
        env = FaBEnv()
        rhinar, dori = _setup(env, mentor_face_up=False)

        _play_attack(env, rhinar, "Wounded Bull", required_resources=3)

        self.assertEqual(rhinar.mentor_lesson_counters, 0,
                         "No lesson counter when mentor is face-down")


class TestChiefRukutanMentorFires(unittest.TestCase):
    """Second lesson counter triggers the mentor ability."""

    def test_mentor_fires_on_second_counter(self):
        """With 1 lesson counter already on mentor, playing a 6-power card fires the ability."""
        env = FaBEnv()
        rhinar, dori = _setup(env, mentor_face_up=True)
        rhinar.mentor_lesson_counters = 1  # already have 1 counter

        _play_attack(env, rhinar, "Wounded Bull", required_resources=3)

        # Chief Ruk'utan should be banished permanently
        banished_names = [c.name for c in rhinar.permanently_banished]
        self.assertIn("Chief Ruk'utan", banished_names,
                      "Chief Ruk'utan must be permanently banished when mentor fires")
        # Arsenal is replaced by Alpha Rampage when mentor fires
        if rhinar.arsenal is not None:
            self.assertNotEqual(rhinar.arsenal.name, "Chief Ruk'utan",
                                "Chief Ruk'utan must no longer be in arsenal after mentor fires")

    def test_alpha_rampage_placed_in_arsenal_after_mentor_fires(self):
        """When mentor fires, Alpha Rampage is fetched from deck into arsenal."""
        env = FaBEnv()
        rhinar, dori = _setup(env, mentor_face_up=True)
        rhinar.mentor_lesson_counters = 1

        _play_attack(env, rhinar, "Wounded Bull", required_resources=3)

        # Alpha Rampage should now be face-up in arsenal (or just placed)
        self.assertIsNotNone(rhinar.arsenal,
                             "Alpha Rampage must be in arsenal after mentor fires")
        self.assertEqual(rhinar.arsenal.name, "Alpha Rampage",
                         "Arsenal must contain Alpha Rampage after mentor fires")

    def test_lesson_counters_reset_after_mentor_fires(self):
        """Lesson counters reset to 0 once the mentor ability fires."""
        env = FaBEnv()
        rhinar, dori = _setup(env, mentor_face_up=True)
        rhinar.mentor_lesson_counters = 1

        _play_attack(env, rhinar, "Wounded Bull", required_resources=3)

        self.assertEqual(rhinar.mentor_lesson_counters, 0,
                         "Lesson counters must reset to 0 after mentor fires")
        self.assertFalse(rhinar.mentor_face_up,
                         "mentor_face_up must be False after mentor is banished")


if __name__ == "__main__":
    unittest.main(verbosity=2)

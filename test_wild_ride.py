"""
Unit tests for Wild Ride (Red).

Card text: "When you attack with Wild Ride, draw a card then discard a random card.
If a card with 6 or more power is discarded this way, Wild Ride gains go again."

Test seeds:
  SEED_GO_AGAIN = 42   — Wild Ride discards Bare Fangs (power 6) → go again fires
  SEED_NO_GO_AGAIN = 23 — Wild Ride discards Come to Fight (power 0) → no go again

Key invariant: the shared CARD_CATALOG card object must never be mutated — go again
is tracked via env._pending_attack_go_again, not by appending to card.keywords.
"""

import unittest

from fab_env import FaBEnv, Phase
from cards import build_rhinar_deck, build_dorinthea_deck, CARD_CATALOG, Keyword
from actions import ActionType, Action


SEED_GO_AGAIN = 42
SEED_NO_GO_AGAIN = 23


def _setup_and_play_wild_ride(seed: int) -> FaBEnv:
    """Reset at *seed*, make Rhinar go first, play Wild Ride, pitch greedily."""
    env = FaBEnv()
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=seed)

    # CHOOSE_FIRST: make Rhinar go first
    legal = env.legal_actions()
    chooser = env._game.players[env._game.active_player_idx]
    if chooser.name == "Rhinar":
        env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))
    else:
        env.step(next(a for a in legal if a.action_type == ActionType.GO_SECOND))

    # ATTACK: play Wild Ride from hand
    legal = env.legal_actions()
    wra = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Wild Ride"
    )
    env.step(wra)

    # PITCH: greedy (highest pitch card first) until cost is covered
    while env._phase == Phase.PITCH:
        env.step(env.legal_actions()[0])

    return env


class TestWildRideGoAgain(unittest.TestCase):
    """Seed 42 — Wild Ride discards Bare Fangs (power 6) → go again fires."""

    def setUp(self):
        CARD_CATALOG["wild_ride_red"].keywords.clear()  # guarantee clean catalog
        self.env = _setup_and_play_wild_ride(SEED_GO_AGAIN)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

    def test_phase_is_defend_after_on_attack_effect(self):
        """After the instant window closes and ON_ATTACK fires, phase is DEFEND."""
        self.assertEqual(self.env._phase, Phase.DEFEND)

    def test_pending_attack_is_wild_ride(self):
        self.assertIsNotNone(self.env._pending_attack)
        self.assertEqual(self.env._pending_attack.name, "Wild Ride")

    def test_wild_ride_drew_a_card(self):
        """Wild Ride's effect draws 1 card: hand grew from 2 (after play+pitch) to 2
        (draw 1, discard 1). Net hand size should be 2 cards."""
        self.assertEqual(len(self.rhinar.hand), 2)

    def test_bare_fangs_discarded(self):
        """Bare Fangs (power 6) was randomly discarded from hand."""
        graveyard_names = [c.name for c in self.rhinar.graveyard]
        self.assertIn("Bare Fangs", graveyard_names)

    def test_go_again_flag_set(self):
        """_pending_attack_go_again is True because a 6+ power card was discarded."""
        self.assertTrue(self.env._pending_attack_go_again)

    def test_catalog_card_not_mutated(self):
        """CARD_CATALOG['wild_ride_red'].keywords must NOT be mutated by go again."""
        self.assertNotIn(Keyword.GO_AGAIN, CARD_CATALOG["wild_ride_red"].keywords)

    def test_go_again_grants_extra_action_point(self):
        """After combat resolves with go again, Rhinar gets action_points=1."""
        # Dorinthea takes no block
        self.env.step(self.env.legal_actions()[0])
        self.assertEqual(self.env._phase, Phase.ATTACK)
        self.assertEqual(self.rhinar.action_points, 1)

    def test_dorinthea_takes_6_damage(self):
        """Wild Ride has power 6, no blocks committed — Dorinthea takes 6 damage."""
        self.env.step(self.env.legal_actions()[0])  # no block
        self.assertEqual(self.dorinthea.life, 14)


class TestWildRideNoGoAgain(unittest.TestCase):
    """Seed 23 — Wild Ride discards Come to Fight (power 0) → no go again."""

    def setUp(self):
        CARD_CATALOG["wild_ride_red"].keywords.clear()
        self.env = _setup_and_play_wild_ride(SEED_NO_GO_AGAIN)
        self.rhinar = self.env._game.players[0]

    def test_phase_is_defend(self):
        self.assertEqual(self.env._phase, Phase.DEFEND)

    def test_come_to_fight_discarded(self):
        """Come to Fight (power 0) was discarded — threshold not met."""
        graveyard_names = [c.name for c in self.rhinar.graveyard]
        self.assertIn("Come to Fight", graveyard_names)

    def test_go_again_flag_not_set(self):
        """No card with 6+ power discarded → _pending_attack_go_again stays False."""
        self.assertFalse(self.env._pending_attack_go_again)

    def test_catalog_card_not_mutated(self):
        self.assertNotIn(Keyword.GO_AGAIN, CARD_CATALOG["wild_ride_red"].keywords)

    def test_no_extra_action_point_after_combat(self):
        """No go again — Rhinar ends the attack phase with action_points=0."""
        self.env.step(self.env.legal_actions()[0])  # no block
        # Step through reaction/arsenal until back in ATTACK or turn ends
        for _ in range(10):
            if self.env._phase in (Phase.ATTACK, Phase.ARSENAL):
                break
            self.env.step(self.env.legal_actions()[0])
        self.assertEqual(self.rhinar.action_points, 0)


class TestWildRideInvalidCardGuard(unittest.TestCase):
    """The env must not crash when a PLAY_CARD action references a card not in hand.
    It should log a warning and pass the turn instead."""

    def setUp(self):
        CARD_CATALOG["wild_ride_red"].keywords.clear()
        self.env = FaBEnv()
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED_GO_AGAIN)
        # Rhinar goes first
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))

    def test_invalid_card_action_passes_turn(self):
        """A PLAY_CARD action for Wild Ride when Wild Ride is not in hand must
        gracefully end the attack phase rather than crash."""
        wild_ride_card = CARD_CATALOG["wild_ride_red"]
        rhinar = self.env._game.players[0]

        # Manually remove Wild Ride from hand so it is genuinely not there
        rhinar.hand = [c for c in rhinar.hand if c.name != "Wild Ride"]
        self.assertNotIn(wild_ride_card, rhinar.hand)

        # Submit a stale PLAY_CARD action for Wild Ride
        stale_action = Action(ActionType.PLAY_CARD, card=wild_ride_card)
        life_before = self.env._game.players[1].life

        # Should not raise
        self.env.step(stale_action)

        # Turn ended (attack phase passed) — Dorinthea's life unchanged (no attack landed)
        self.assertEqual(self.env._game.players[1].life, life_before)


if __name__ == "__main__":
    unittest.main(verbosity=2)

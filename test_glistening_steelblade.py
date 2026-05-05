"""
Tests for Glistening Steelblade action card.

Seed 20 gives:
  Dorinthea:  Glistening Steelblade, Sigil of Solace, Run Through, Visit the Blacksmith
  Rhinar:     Wounded Bull, Pack Hunt, Beast Mode, Come to Fight
  Dorinthea (agent_1) wins the coin flip.

Glistening Steelblade card text:
  "Dorinthea Specialization. Your next Dawnblade attack this turn has go again.
   Whenever Dawnblade hits a hero this turn, put a +1 counter on it. Go again."

Expected behaviour:
  - Playing Glistening Steelblade sets next_weapon_go_again (for the go again effect).
  - Playing Glistening Steelblade sets glistening_steelblade_active (for the on-hit counter effect).
  - Whenever Dawnblade hits this turn (glistening_steelblade_active = True), dawnblade_counters += 1.
  - dawnblade_counters is permanent — it persists across turns and is never reset.
  - glistening_steelblade_active is per-turn — it resets in reset_turn_resources().
  - dawnblade_counters adds to Dawnblade's effective power permanently.
  - Counter is NOT added when the attack misses (fully blocked).
  - Counter is NOT added when glistening_steelblade_active is False (GSB not played this turn).
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass, Keyword
from cards import build_rhinar_deck, build_dorinthea_deck

SEED = 20  # Dorinthea has Glistening Steelblade; she wins coin flip


def _advance_to_attack_phase(env):
    """Reset at SEED and step to Dorinthea's ATTACK phase."""
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))
    assert env._phase == Phase.ATTACK
    assert env._game.active_player_idx == 1


def _play_glistening_steelblade(env):
    """Play Glistening Steelblade and pitch Sigil of Solace to cover its cost.

    Pitching Sigil of Solace (pitch=3) overpays by 2, leaving RP=2 — enough
    to attack with Dawnblade (cost=1) without a second pitch step.
    Returns (dorinthea, rhinar).
    """
    dorinthea = env._game.players[1]
    rhinar = env._game.players[0]

    # Play GSB
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD
        and a.card is not None
        and a.card.name == "Glistening Steelblade"
    ))

    # Pitch Sigil of Solace (index 0 in remaining hand, pitch=3)
    assert env._phase == Phase.PITCH
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.pitch_indices == [0]))

    assert env._phase == Phase.ATTACK
    return dorinthea, rhinar


def _play_gsb_and_attack_hit(env):
    """Play GSB, then attack with Dawnblade; Rhinar commits no blocks (guaranteed hit).

    Dawnblade power 2 is not blocked → 2 damage to Rhinar.
    Returns (dorinthea, rhinar).
    """
    _advance_to_attack_phase(env)
    dorinthea, rhinar = _play_glistening_steelblade(env)

    # Attack with Dawnblade (RP=2 >= cost 1, no pitch needed)
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"

    # Rhinar commits no blocks
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and a.hand_index is None
        and not a.defend_equip_slots
    ))

    # Collapse reaction window
    while env._phase == Phase.REACTION:
        legal = env.legal_actions()
        env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

    return dorinthea, rhinar


def _play_gsb_and_attack_miss(env):
    """Play GSB, then attack with Dawnblade; Rhinar blocks with Wounded Bull (def=2).

    Power 2 blocked by def 2 → 0 damage → miss, no counter placed.
    Returns (dorinthea, rhinar).
    """
    _advance_to_attack_phase(env)
    dorinthea, rhinar = _play_glistening_steelblade(env)

    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"

    # Rhinar adds Wounded Bull (index 0, def=2) to block pile
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and a.hand_index == 0
        and not a.defend_equip_slots
    ))
    assert env._phase == Phase.DEFEND

    # Commit block
    legal = env.legal_actions()
    env.step(next(
        a for a in legal
        if a.action_type == ActionType.DEFEND
        and a.hand_index is None
        and not a.defend_equip_slots
    ))

    while env._phase == Phase.REACTION:
        legal = env.legal_actions()
        env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

    return dorinthea, rhinar


class TestGlisteningSteebladeDefinition(unittest.TestCase):
    """Verify card definition in catalog."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Glistening Steelblade", names)

    def test_card_properties(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Glistening Steelblade")
        self.assertEqual(card.card_type, CardType.ACTION)
        self.assertEqual(card.cost, 1)
        self.assertEqual(card.pitch, 2)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.YELLOW)
        self.assertEqual(card.card_class, CardClass.WARRIOR)
        self.assertIn(Keyword.GO_AGAIN, card.keywords)

    def test_card_text_mentions_go_again(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Glistening Steelblade")
        self.assertIn("go again", card.text.lower())

    def test_card_text_mentions_counter(self):
        card = next(c for c in self.dorinthea.hand if c.name == "Glistening Steelblade")
        self.assertIn("counter", card.text.lower())


class TestGlisteningSteebladeFlags(unittest.TestCase):
    """Flags set after playing Glistening Steelblade."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_attack_phase(self.env)
        self.dorinthea = self.env._game.players[1]

    def test_flags_clear_before_play(self):
        self.assertFalse(self.dorinthea.next_weapon_go_again)
        self.assertFalse(self.dorinthea.glistening_steelblade_active)

    def test_weapon_go_again_set_after_play(self):
        _play_glistening_steelblade(self.env)
        self.assertTrue(self.dorinthea.next_weapon_go_again,
                        "next_weapon_go_again must be True after playing Glistening Steelblade")

    def test_gsb_active_flag_set_after_play(self):
        _play_glistening_steelblade(self.env)
        self.assertTrue(self.dorinthea.glistening_steelblade_active,
                        "glistening_steelblade_active must be True after playing Glistening Steelblade")

    def test_still_in_attack_phase_go_again(self):
        _play_glistening_steelblade(self.env)
        self.assertEqual(self.env._phase, Phase.ATTACK,
                         "Glistening Steelblade has Go Again — must remain in ATTACK phase")

    def test_gsb_active_resets_on_new_turn(self):
        _play_glistening_steelblade(self.env)
        self.assertTrue(self.dorinthea.glistening_steelblade_active)
        self.dorinthea.reset_turn_resources()
        self.assertFalse(self.dorinthea.glistening_steelblade_active,
                         "glistening_steelblade_active must reset at the start of a new turn")


class TestGlisteningSteebladeHit(unittest.TestCase):
    """Counter is placed when Dawnblade hits with glistening_steelblade_active."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _play_gsb_and_attack_hit(self.env)

    def test_rhinar_takes_damage(self):
        self.assertEqual(self.rhinar.life, 18,
                         "Rhinar should take 2 damage from unblocked Dawnblade")

    def test_dawnblade_counter_added_on_hit(self):
        self.assertEqual(self.dorinthea.dawnblade_counters, 1,
                         "dawnblade_counters must be 1 after Dawnblade hits with GSB active")

    def test_go_again_fired(self):
        self.assertEqual(self.dorinthea.action_points, 1,
                         "Glistening Steelblade: next_weapon_go_again must fire — +1 AP")

    def test_weapon_go_again_flag_cleared(self):
        self.assertFalse(self.dorinthea.next_weapon_go_again,
                         "next_weapon_go_again must be consumed after the weapon attack")

    def test_weapon_additional_attack_set(self):
        self.assertTrue(self.dorinthea.weapon_additional_attack,
                        "weapon_additional_attack must be set after go again on first weapon attack")


class TestGlisteningSteebladeCounterPower(unittest.TestCase):
    """dawnblade_counters permanently increase Dawnblade's effective power."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_zero_counters_at_start(self):
        self.assertEqual(self.dorinthea.dawnblade_counters, 0)

    def test_one_counter_adds_one_power(self):
        self.dorinthea.dawnblade_counters = 1
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), 3,
                         "1 counter: Dawnblade power should be 2 + 1 = 3")

    def test_two_counters_add_two_power(self):
        self.dorinthea.dawnblade_counters = 2
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), 4,
                         "2 counters: Dawnblade power should be 2 + 2 = 4")

    def test_counters_stack_with_second_attack_bonus(self):
        self.dorinthea.dawnblade_counters = 1
        self.dorinthea.weapon_attack_count = 1  # second attack
        self.assertEqual(self.dorinthea.get_effective_weapon_power(), 4,
                         "Second attack with 1 counter: 2 (base) + 1 (counter) + 1 (2nd swing) = 4")

    def test_counters_persist_across_turn_reset(self):
        self.dorinthea.dawnblade_counters = 3
        self.dorinthea.reset_turn_resources()
        self.assertEqual(self.dorinthea.dawnblade_counters, 3,
                         "dawnblade_counters must NOT reset at the start of a new turn")


class TestGlisteningSteebladeMultipleHits(unittest.TestCase):
    """Each Dawnblade hit this turn adds a counter when GSB is active."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_attack_phase(self.env)
        self.dorinthea, self.rhinar = _play_glistening_steelblade(self.env)

    def test_two_hits_give_two_counters(self):
        # First weapon attack — Rhinar no block → hit → counter 1
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and a.hand_index is None
        ))
        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertEqual(self.dorinthea.dawnblade_counters, 1)
        self.assertEqual(self.dorinthea.action_points, 1)

        # Second weapon attack (weapon_additional_attack=True) — Rhinar no block → hit → counter 2
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and a.hand_index is None
        ))
        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertEqual(self.dorinthea.dawnblade_counters, 2,
                         "Two Dawnblade hits with GSB active should place 2 counters total")


class TestGlisteningSteebladeNoCounterOnMiss(unittest.TestCase):
    """Counter is NOT placed when Dawnblade is fully blocked."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.dorinthea, self.rhinar = _play_gsb_and_attack_miss(self.env)

    def test_rhinar_takes_no_damage(self):
        self.assertEqual(self.rhinar.life, 20,
                         "Fully blocked Dawnblade should deal 0 damage")

    def test_no_counter_on_miss(self):
        self.assertEqual(self.dorinthea.dawnblade_counters, 0,
                         "dawnblade_counters must NOT increase when attack is fully blocked")


class TestGlisteningSteebladeNoCounterWithoutFlag(unittest.TestCase):
    """Counter is NOT placed when Dawnblade hits but GSB was not played this turn."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_attack_phase(self.env)
        self.dorinthea = self.env._game.players[1]
        self.rhinar = self.env._game.players[0]

    def test_no_counter_without_gsb(self):
        # Attack with Dawnblade directly (no GSB played)
        self.assertFalse(self.dorinthea.glistening_steelblade_active)

        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.action_type == ActionType.WEAPON))

        # Dawnblade costs 1; pitch Sigil of Solace (index 1, pitch=3) to cover it
        assert self.env._phase == Phase.PITCH
        legal = self.env.legal_actions()
        self.env.step(next(a for a in legal if a.pitch_indices == [1]))

        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and a.hand_index is None
        ))

        while self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.env.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))

        self.assertEqual(self.rhinar.life, 18,
                         "Unblocked Dawnblade should still deal 2 damage")
        self.assertEqual(self.dorinthea.dawnblade_counters, 0,
                         "No counter should be added when GSB was not played this turn")


if __name__ == "__main__":
    unittest.main(verbosity=2)

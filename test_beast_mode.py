"""
Tests for Beast Mode (Red).

Seed 3 gives:
  Rhinar:     Beast Mode, Barraging Beatdown, Pack Call, Bare Fangs
  Dorinthea:  En Garde, Flock of the Feather Walkers, Visit the Blacksmith, On a Knife Edge
  Dorinthea wins the coin flip; GO_SECOND puts Rhinar first.

Seed 7 gives:
  Rhinar:     Beast Mode, Titanium Bauble, Wrecking Ball, Raging Onslaught
  Dorinthea:  Flock of the Feather Walkers, On a Knife Edge, Hit and Run, Sigil of Solace
  Rhinar wins the coin flip; GO_FIRST puts Rhinar first.

Beast Mode should:
  - Have base power 6, cost 3, pitch 1
  - Gain +2 power if the active player has intimidated this turn (power becomes 8)
  - NOT gain +2 power if the player has not intimidated this turn (power stays 6)
  - Track intimidation via Player.intimidated_this_turn flag, reset each turn
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass, build_rhinar_deck, build_dorinthea_deck

SEED_WITH_BB = 3   # Beast Mode + Barraging Beatdown in opening hand
SEED_NO_INTIM = 7  # Beast Mode in opening hand, no Intimidate source played


def _make_env():
    return FaBEnv(verbose=False)


def _pass_instant(env):
    """Exhaust any open INSTANT window by passing once for each player."""
    while env._phase == Phase.INSTANT:
        legal = env.legal_actions()
        pass_a = next(
            (a for a in legal if a.action_type == ActionType.PASS_PRIORITY), None
        )
        if pass_a is None:
            break
        env.step(pass_a)


class TestBeastModeCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = _make_env()
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED_WITH_BB)
        self.env.step(Action(ActionType.GO_SECOND))
        self.rhinar = self.env._game.players[0]

    def test_in_rhinar_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Beast Mode", names)

    def test_card_properties(self):
        card = next(c for c in self.rhinar.hand if c.name == "Beast Mode")
        self.assertEqual(card.card_type, [CardType.ATTACK, CardType.ACTION])
        self.assertEqual(card.cost, 3)
        self.assertEqual(card.pitch, 1)
        self.assertEqual(card.power, 6)
        self.assertEqual(card.defense, 3)
        self.assertEqual(card.color, Color.RED)
        self.assertEqual(card.card_class, CardClass.BRUTE)


class TestBeastModeWithIntimidateBonus(unittest.TestCase):
    """
    Sequence (seed 3): Barraging Beatdown (Intimidate + Go Again) → Beast Mode.
    After pitching Pack Call (pitch 2), the game auto-pitches Bare Fangs (pitch 1)
    to cover cost 3, declares the attack, and lands in DEFEND phase.
    Expected pending_attack_power = 8 (base 6 + +2 intimidate bonus).
    """

    def setUp(self):
        self.env = _make_env()
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED_WITH_BB)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

        # Rhinar goes first
        self.env.step(Action(ActionType.GO_SECOND))

        # Play Barraging Beatdown: cost 0, Intimidate fires, Go Again granted
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD
            and a.card and a.card.name == "Barraging Beatdown"
        ))

        # Select Beast Mode (cost 3). Hand is now [Beast Mode, Pack Call, Bare Fangs]
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD
            and a.card and a.card.name == "Beast Mode"
        ))

        # Pitch Pack Call (index 0, pitch 2). The engine auto-pitches Bare Fangs
        # (the only remaining legal pitch) and declares the attack.
        # After this step: DEFEND phase, _pending_attack_power = 8.
        self.env.step(Action(ActionType.PITCH, pitch_index=0))
        self.life_before = self.dorinthea.life

    def test_intimidated_this_turn_flag_set(self):
        self.assertTrue(
            self.rhinar.intimidated_this_turn,
            "Barraging Beatdown Intimidate must set intimidated_this_turn=True"
        )

    def test_beast_mode_gains_two_power(self):
        self.assertEqual(
            self.env._pending_attack_power, 8,
            "Beast Mode should have power 8 (base 6 + 2 from intimidate this turn)"
        )

    def test_beast_mode_in_defend_phase(self):
        self.assertEqual(self.env._phase, Phase.DEFEND,
                         "Game should be in DEFEND phase after attack is declared")

    def test_beast_mode_deals_correct_damage_no_block(self):
        # Dorinthea commits no blocks
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.DEFEND
            and a.hand_index is None and a.equip_slot is None
        ))
        _pass_instant(self.env)

        # Beast Mode 8 + Barraging Beatdown conditional +3 (0 blockers) = 11 damage
        self.assertEqual(
            self.dorinthea.life, self.life_before - 11,
            "Dorinthea should take 11 damage (Beast Mode 8 + BB conditional 3, no block)"
        )


class TestBeastModeWithoutIntimidateBonus(unittest.TestCase):
    """
    Beast Mode played WITHOUT any prior intimidation this turn (seed 7).
    Pitch Titanium Bauble (pitch 3) covers cost 3 in one step → INSTANT phase.
    Expected pending_attack_power = 6 (base only).
    """

    def setUp(self):
        self.env = _make_env()
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED_NO_INTIM)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

        # Rhinar wins flip and goes first
        self.env.step(Action(ActionType.GO_FIRST))

        # Select Beast Mode. Hand: [Beast Mode, Titanium Bauble, Wrecking Ball, Raging Onslaught]
        legal = self.env.legal_actions()
        self.env.step(next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD
            and a.card and a.card.name == "Beast Mode"
        ))

        # Pitch Titanium Bauble (index 0, pitch 3): covers cost 3 in one pitch.
        # After this step: INSTANT phase (attack reaction window), _pending_attack_power = 6.
        self.env.step(Action(ActionType.PITCH, pitch_index=0))

    def test_not_intimidated_this_turn(self):
        self.assertFalse(
            self.rhinar.intimidated_this_turn,
            "No intimidate was triggered; intimidated_this_turn must be False"
        )

    def test_beast_mode_base_power_only(self):
        self.assertEqual(
            self.env._pending_attack_power, 6,
            "Beast Mode should have base power 6 when no intimidation has occurred this turn"
        )

    def test_beast_mode_in_instant_phase(self):
        self.assertEqual(self.env._phase, Phase.INSTANT,
                         "Game should be in INSTANT (attack reaction) phase after attack declared")


class TestIntimidatedFlagResetsEachTurn(unittest.TestCase):
    """intimidated_this_turn must be False at the start of each new turn."""

    def test_flag_reset_after_turn(self):
        env = _make_env()
        env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED_WITH_BB)
        rhinar = env._game.players[0]

        # Rhinar goes first
        env.step(Action(ActionType.GO_SECOND))

        # Play Barraging Beatdown → sets intimidated_this_turn
        legal = env.legal_actions()
        env.step(next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD
            and a.card and a.card.name == "Barraging Beatdown"
        ))
        self.assertTrue(rhinar.intimidated_this_turn,
                        "Flag must be True immediately after intimidate fires")

        # End Rhinar's turn by passing
        legal = env.legal_actions()
        env.step(next(a for a in legal if a.action_type == ActionType.PASS))

        # Step through end-of-turn and Dorinthea's full turn until Rhinar's turn begins again
        max_steps = 60
        for _ in range(max_steps):
            if env._game.is_over():
                break
            if env.agent_selection == "agent_0" and env._phase == Phase.ATTACK:
                break
            legal = env.legal_actions()
            if not legal:
                break
            env.step(legal[0])

        # Now it is Rhinar's second turn; reset_turn_resources was called, flag must be False
        if not env._game.is_over():
            self.assertFalse(
                rhinar.intimidated_this_turn,
                "intimidated_this_turn must reset to False at the start of Rhinar's next turn"
            )


if __name__ == "__main__":
    unittest.main()

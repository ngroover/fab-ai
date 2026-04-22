"""
Tests for Dodge defense reaction.

Seed 12 gives:
  Rhinar:     Dodge, Awakening Bellow, Beast Mode, Come to Fight
  Dorinthea:  Titanium Bauble, Warrior's Valor, Toughen Up, Second Swing
  Rhinar (agent_0) wins the coin flip.

Dodge should:
  - NOT be offered as a blocking card during the DEFEND phase
  - BE offered as a PLAY_CARD action during the REACTION phase (when Rhinar is the defender)
  - Add its defense value (+2) to _reaction_defense_bonus when resolved
  - Reduce damage taken from the incoming attack when played
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color

SEED = 12  # Rhinar has Dodge; Rhinar wins coin flip


def _advance_to_defend(env):
    """
    Reset at SEED and advance to the DEFEND phase during Dorinthea's first
    Dawnblade attack.

    Step sequence
    -------------
    1. Rhinar chooses GO_SECOND -> Dorinthea goes first.
    2. Dorinthea attacks with Dawnblade (WEAPON, cost=1).
    3. Dorinthea pitches Titanium Bauble (pitch=3, index 0) to cover cost.
       [Auto-step collapses pre-DEFEND instant window.]
    -> Returns with phase=DEFEND, agent=agent_0 (Rhinar defending).
    """
    env.reset(seed=SEED)
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.GO_SECOND))
    env.step(next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON))
    env.step(env.legal_actions()[0])  # pitch Titanium Bauble to cover Dawnblade cost


def _advance_to_reaction(env):
    """
    Advance past the DEFEND phase (no blocks committed) and into the REACTION
    phase. Dorinthea auto-passes (no attack reactions), leaving Rhinar with
    priority and Dodge available to play.

    Returns (rhinar, dorinthea).
    """
    _advance_to_defend(env)
    no_defend = next(
        a for a in env.legal_actions()
        if a.action_type == ActionType.DEFEND
        and not a.defend_hand_indices
        and not a.defend_equip_slots
    )
    env.step(no_defend)
    return env._game.players[0], env._game.players[1]


class TestDodgeCardDefinition(unittest.TestCase):
    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.rhinar = self.env._game.players[0]

    def test_in_rhinar_opening_hand(self):
        names = [c.name for c in self.rhinar.hand]
        self.assertIn("Dodge", names)

    def test_card_properties(self):
        card = next(c for c in self.rhinar.hand if c.name == "Dodge")
        self.assertEqual(card.card_type, CardType.DEFENSE_REACTION)
        self.assertEqual(card.cost, 0)
        self.assertEqual(card.defense, 2)
        self.assertEqual(card.color, Color.BLUE)

    def test_card_pitch_value(self):
        card = next(c for c in self.rhinar.hand if c.name == "Dodge")
        self.assertEqual(card.pitch, 3)


class TestDodgeNotBlockableInDefendPhase(unittest.TestCase):
    """Dodge must NOT appear as a DEFEND option during the block step."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_defend(self.env)
        self.rhinar = self.env._game.players[0]

    def test_phase_is_defend(self):
        self.assertEqual(self.env._phase, Phase.DEFEND)
        self.assertEqual(self.env.agent_selection, "agent_0")

    def test_dodge_not_offered_as_block(self):
        legal = self.env.legal_actions()
        defend_card_indices = set()
        for a in legal:
            if a.action_type == ActionType.DEFEND:
                defend_card_indices.update(a.defend_hand_indices)

        blocked_names = {self.rhinar.hand[i].name for i in defend_card_indices}
        self.assertNotIn(
            "Dodge", blocked_names,
            "Dodge (a DEFENSE_REACTION) must not be offered as a blocking card in the DEFEND phase",
        )

    def test_defend_actions_only_non_reaction_cards(self):
        """Every card offered as a block must not be a DEFENSE_REACTION."""
        legal = self.env.legal_actions()
        for a in legal:
            if a.action_type == ActionType.DEFEND and a.defend_hand_indices:
                for idx in a.defend_hand_indices:
                    card = self.rhinar.hand[idx]
                    self.assertNotEqual(
                        card.card_type, CardType.DEFENSE_REACTION,
                        f"{card.name} is a DEFENSE_REACTION and must not be offered as a block",
                    )


class TestDodgePlayableInReactionPhase(unittest.TestCase):
    """Dodge must appear as a PLAY_CARD option in the REACTION phase."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_reaction(self.env)

    def test_phase_is_reaction(self):
        self.assertEqual(self.env._phase, Phase.REACTION)

    def test_rhinar_has_priority(self):
        self.assertEqual(self.env.agent_selection, "agent_0")

    def test_dodge_offered_in_reaction_phase(self):
        legal = self.env.legal_actions()
        play_names = [a.card.name for a in legal if a.action_type == ActionType.PLAY_CARD and a.card]
        self.assertIn("Dodge", play_names,
                      "Dodge must be a legal PLAY_CARD action in the REACTION phase")

    def test_dodge_has_zero_cost_no_pitch_needed(self):
        """Dodge costs 0 — it should be playable with no resources."""
        dodge_action = next(
            a for a in self.env.legal_actions()
            if a.action_type == ActionType.PLAY_CARD and a.card and a.card.name == "Dodge"
        )
        self.assertEqual(dodge_action.card.cost, 0)


class TestDodgeDefenseEffect(unittest.TestCase):
    """Dodge played in the REACTION phase reduces damage by its defense value."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.rhinar, self.dorinthea = _advance_to_reaction(self.env)
        self.rhinar_life_before = self.rhinar.life

    def _play_dodge_and_resolve(self):
        """Play Dodge; cost=0 auto-pitches, and both players have no further
        reactions so the entire stack resolves via auto-step before returning."""
        dodge_action = next(
            a for a in self.env.legal_actions()
            if a.action_type == ActionType.PLAY_CARD and a.card and a.card.name == "Dodge"
        )
        self.env.step(dodge_action)
        # All subsequent REACTION steps (pitch empty, both players pass, Dodge
        # resolves, both pass again, combat resolves) are auto-executed.

    def test_dodge_adds_two_to_reaction_defense_bonus(self):
        dodge_action = next(
            a for a in self.env.legal_actions()
            if a.action_type == ActionType.PLAY_CARD and a.card and a.card.name == "Dodge"
        )
        self.env.step(dodge_action)
        # After Dodge resolves off the stack, check the bonus
        self.assertEqual(self.env._reaction_defense_bonus, 2,
                         "Dodge must add exactly 2 to _reaction_defense_bonus")

    def test_dodge_fully_blocks_dawnblade_two_power(self):
        """Dawnblade hits for 2; Dodge defends for 2 — Rhinar takes 0 damage."""
        self._play_dodge_and_resolve()
        self.assertEqual(
            self.rhinar.life, self.rhinar_life_before,
            "Dodge (defense 2) should fully absorb Dawnblade (power 2), leaving Rhinar at full life",
        )

    def test_rhinar_takes_damage_without_dodge(self):
        """Control: if Rhinar passes in REACTION (no Dodge), he takes 2 damage.

        After _advance_to_reaction, Dorinthea already auto-passed (_reaction_passes=1).
        One manual Rhinar PASS_PRIORITY raises the count to 2 → combat resolves.
        """
        env2 = FaBEnv(verbose=False)
        rhinar2, _ = _advance_to_reaction(env2)
        life_before = rhinar2.life
        # One pass needed — Dorinthea already auto-passed once
        legal = env2.legal_actions()
        env2.step(next(a for a in legal if a.action_type == ActionType.PASS_PRIORITY))
        self.assertEqual(rhinar2.life, life_before - 2,
                         "Without Dodge, Rhinar should take 2 damage from Dawnblade")

    def test_dodge_removed_from_hand_after_play(self):
        """Dodge must be removed from Rhinar's hand when played."""
        self._play_dodge_and_resolve()
        hand_names = [c.name for c in self.rhinar.hand]
        self.assertNotIn("Dodge", hand_names,
                         "Dodge must be removed from hand after being played")

    def test_dodge_in_graveyard_after_resolution(self):
        """Dodge must end up in Rhinar's graveyard after resolving."""
        self._play_dodge_and_resolve()
        grave_names = [c.name for c in self.rhinar.graveyard]
        self.assertIn("Dodge", grave_names,
                      "Dodge must go to the graveyard after resolving off the stack")


if __name__ == "__main__":
    unittest.main()

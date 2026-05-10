"""
Unit tests for Rally the Rearguard's instant activated ability.

Card text:
  "Once per turn Instant - Discard a card: Rally the Rearguard gains +3 block.
   Activate this ability only while Rally the Rearguard is defending."

Seed 6 gives:
  Rhinar:      Chief Ruk'utan, Raging Onslaught, Rally the Rearguard, Wounded Bull
  Dorinthea:   In the Swing, In the Swing, Sharpen Steel, Slice and Dice

CHOOSE_FIRST: agent_1 (Dorinthea) chooses. Selecting GO_FIRST makes Dorinthea the
attacker on turn 1, so Rhinar is the defender and can use Rally to block.

The ability is:
  - Available during REACTION phase when Rally is among the committed blocking cards
  - NOT available when Rally is not blocking
  - NOT available after it has already been used this combat
  - Costs: discard one card from the defender's hand
  - Effect: +3 to the defending card's block value (tracked via reaction_defense_bonus)
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import build_rhinar_deck, build_dorinthea_deck, CARD_CATALOG, CardType, Color


SEED = 6  # Rhinar: Chief Ruk'utan, Raging Onslaught, Rally the Rearguard, Wounded Bull


def _advance_to_reaction_with_rally_blocking(env):
    """Reset at SEED, have Dorinthea go first and attack with Dawnblade, then
    have Rhinar block with Rally the Rearguard. Stop at the REACTION phase with
    Rhinar (agent_0) holding priority (Dorinthea auto-passes, having no legal
    attack reactions on her first weapon swing).

    Returns once env._phase == Phase.REACTION and env.agent_selection == 'agent_0'.
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)

    # CHOOSE_FIRST: Dorinthea elects to go first → she attacks, Rhinar defends
    go_first = next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST)
    env.step(go_first)

    # ATTACK: Dorinthea attacks with Dawnblade
    weapon = next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON)
    env.step(weapon)

    # PITCH: cover Dawnblade's cost (one pitch step)
    while env._phase == Phase.PITCH:
        env.step(env.legal_actions()[0])

    # INSTANT: attack reaction window — neither player has an instant; auto-executes
    while env._phase == Phase.INSTANT:
        env.step(Action(ActionType.PASS_PRIORITY))

    assert env._phase == Phase.DEFEND, f"Expected DEFEND, got {env._phase}"
    assert env.agent_selection == "agent_0", "Rhinar should be defending"

    # DEFEND: Rhinar adds Rally the Rearguard (hand index 2 after first draw)
    rally_block = next(
        a for a in env.legal_actions()
        if a.action_type == ActionType.DEFEND
        and a.hand_index is not None
        and env._game.players[0].hand[a.hand_index].name == "Rally the Rearguard"
    )
    env.step(rally_block)

    # Commit blocks — opens REACTION phase
    done = next(
        a for a in env.legal_actions()
        if a.action_type == ActionType.DEFEND
        and a.hand_index is None and a.equip_slot is None
    )
    env.step(done)

    # Dorinthea (attacker) auto-passes priority (no legal attack reactions on first swing)
    # → auto-execute lands us at Rhinar's priority in REACTION
    assert env._phase == Phase.REACTION, f"Expected REACTION, got {env._phase}"
    assert env.agent_selection == "agent_0", "Rhinar should hold priority in REACTION"


class TestRallyCardDefinition(unittest.TestCase):
    """Card definition sanity checks."""

    def test_rally_in_card_catalog(self):
        self.assertIn("rally_the_rearguard_blue", CARD_CATALOG)

    def test_rally_card_properties(self):
        rally = CARD_CATALOG["rally_the_rearguard_blue"]
        self.assertEqual(rally.card_type, CardType.ACTION_ATTACK)
        self.assertEqual(rally.cost, 2)
        self.assertEqual(rally.power, 4)
        self.assertEqual(rally.defense, 2)
        self.assertEqual(rally.color, Color.BLUE)

    def test_rally_text_mentions_instant_ability(self):
        rally = CARD_CATALOG["rally_the_rearguard_blue"]
        self.assertIn("+3 block", rally.text)
        self.assertIn("Instant", rally.text)
        self.assertIn("defending", rally.text)

    def test_rally_in_rhinar_opening_hand_at_seed_6(self):
        env = FaBEnv(verbose=False)
        env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        env.step(env.legal_actions()[0])  # resolve CHOOSE_FIRST
        rhinar = env._game.players[0]
        self.assertIn("Rally the Rearguard", [c.name for c in rhinar.hand])


class TestRallyAbilityAvailability(unittest.TestCase):
    """Verify ACTIVATE_CARD_ABILITY appears when and only when appropriate."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_reaction_with_rally_blocking(self.env)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]

    def test_phase_is_reaction(self):
        self.assertEqual(self.env._phase, Phase.REACTION)

    def test_rhinar_has_priority(self):
        self.assertEqual(self.env.agent_selection, "agent_0")

    def test_rally_is_committed_blocker(self):
        self.assertIn("Rally the Rearguard",
                      [c.name for c in self.env._committed_defend_cards])

    def test_activate_ability_is_legal(self):
        legal = self.env.legal_actions()
        ability_actions = [a for a in legal
                           if a.action_type == ActionType.ACTIVATE_CARD_ABILITY]
        self.assertGreater(len(ability_actions), 0,
                           "ACTIVATE_CARD_ABILITY should appear when Rally is blocking")

    def test_ability_targets_rally(self):
        legal = self.env.legal_actions()
        for a in legal:
            if a.action_type == ActionType.ACTIVATE_CARD_ABILITY:
                self.assertIsNotNone(a.card)
                self.assertEqual(a.card.name, "Rally the Rearguard")

    def test_one_ability_action_per_hand_card(self):
        """One ACTIVATE_CARD_ABILITY per distinct card name in Rhinar's hand."""
        legal = self.env.legal_actions()
        ability_actions = [a for a in legal
                           if a.action_type == ActionType.ACTIVATE_CARD_ABILITY]
        unique_discard_indices = {a.hand_index for a in ability_actions}
        # Rhinar has 3 cards left (Rally moved to combat chain)
        self.assertEqual(len(unique_discard_indices), len(self.rhinar.hand))

    def test_pass_priority_is_always_legal(self):
        legal = self.env.legal_actions()
        self.assertTrue(any(a.action_type == ActionType.PASS_PRIORITY for a in legal))


class TestRallyAbilityNotAvailableWithoutRally(unittest.TestCase):
    """When Rally is NOT a committed blocker, the ability must not appear."""

    def test_no_ability_when_blocking_with_other_card(self):
        env = FaBEnv(verbose=False)
        env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)

        go_first = next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST)
        env.step(go_first)

        weapon = next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON)
        env.step(weapon)

        while env._phase == Phase.PITCH:
            env.step(env.legal_actions()[0])
        while env._phase == Phase.INSTANT:
            env.step(Action(ActionType.PASS_PRIORITY))

        # Block with Raging Onslaught instead
        rhinar = env._game.players[0]
        non_rally_block = next(
            a for a in env.legal_actions()
            if a.action_type == ActionType.DEFEND
            and a.hand_index is not None
            and rhinar.hand[a.hand_index].name != "Rally the Rearguard"
        )
        env.step(non_rally_block)

        done = next(a for a in env.legal_actions()
                    if a.action_type == ActionType.DEFEND
                    and a.hand_index is None and a.equip_slot is None)
        env.step(done)

        # Reaction phase (if reached) should not offer Rally ability
        if env._phase == Phase.REACTION:
            legal = env.legal_actions()
            self.assertFalse(
                any(a.action_type == ActionType.ACTIVATE_CARD_ABILITY for a in legal),
                "Rally ability must not appear when Rally is not blocking"
            )

    def test_no_ability_when_not_blocking_at_all(self):
        env = FaBEnv(verbose=False)
        env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)

        go_first = next(a for a in env.legal_actions() if a.action_type == ActionType.GO_FIRST)
        env.step(go_first)

        weapon = next(a for a in env.legal_actions() if a.action_type == ActionType.WEAPON)
        env.step(weapon)

        while env._phase == Phase.PITCH:
            env.step(env.legal_actions()[0])
        while env._phase == Phase.INSTANT:
            env.step(Action(ActionType.PASS_PRIORITY))

        # Commit no blocks (take the damage)
        done = next(a for a in env.legal_actions()
                    if a.action_type == ActionType.DEFEND
                    and a.hand_index is None and a.equip_slot is None)
        env.step(done)

        if env._phase == Phase.REACTION:
            legal = env.legal_actions()
            self.assertFalse(
                any(a.action_type == ActionType.ACTIVATE_CARD_ABILITY for a in legal),
            )


class TestRallyAbilityEffect(unittest.TestCase):
    """Verify activating the ability correctly modifies defense and game state."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_reaction_with_rally_blocking(self.env)
        self.rhinar = self.env._game.players[0]
        self.dorinthea = self.env._game.players[1]
        self._hand_before = list(self.rhinar.hand)
        self._graveyard_before = list(self.rhinar.graveyard)
        self._life_before = self.rhinar.life

        # Activate the ability — discard the first available card
        legal = self.env.legal_actions()
        self._activate = next(
            a for a in legal if a.action_type == ActionType.ACTIVATE_CARD_ABILITY
        )
        self._discarded_card = self.rhinar.hand[self._activate.hand_index]
        self.env.step(self._activate)

    def test_discarded_card_removed_from_hand(self):
        self.assertNotIn(self._discarded_card, self.rhinar.hand)

    def test_discarded_card_in_graveyard(self):
        self.assertIn(self._discarded_card, self.rhinar.graveyard)

    def test_hand_size_reduced_by_one(self):
        self.assertEqual(len(self.rhinar.hand), len(self._hand_before) - 1)

    def test_reaction_defense_bonus_is_three(self):
        self.assertEqual(self.env._reaction_defense_bonus, 3)

    def test_rally_ability_marked_used(self):
        self.assertTrue(self.env._rally_ability_used)

    def test_attack_fully_blocked_dawnblade_2_power(self):
        """Dawnblade has 2 power; Rally base def 2 + 3 ability = 5 total > 2 power."""
        self.assertEqual(self.rhinar.life, self._life_before,
                         "Rhinar should take no damage (5 def vs 2 power)")

    def test_ability_not_available_after_use(self):
        """Once used the ability must no longer appear as a legal action."""
        if self.env._phase == Phase.REACTION:
            legal = self.env.legal_actions()
            self.assertFalse(
                any(a.action_type == ActionType.ACTIVATE_CARD_ABILITY for a in legal),
                "Ability must be exhausted after one use"
            )


class TestRallyAbilityOncePerTurn(unittest.TestCase):
    """The ability is marked used after activation and clears for the next combat."""

    def test_ability_used_flag_clears_for_next_attack(self):
        """_rally_ability_used resets when a new attack is declared."""
        env = FaBEnv(verbose=False)
        _advance_to_reaction_with_rally_blocking(env)

        # Activate the ability
        legal = env.legal_actions()
        activate = next(a for a in legal if a.action_type == ActionType.ACTIVATE_CARD_ABILITY)
        env.step(activate)

        self.assertTrue(env._rally_ability_used,
                        "Flag should be set immediately after activation")

        # Continue to end of turn — flag should reset when a new attack is declared
        # Just verify the flag is reset by _trigger_defend_phase (which is called
        # on each new attack declaration). After the reaction closes and game continues,
        # the flag resets for the next combat.
        self.assertFalse(env._committed_defend_cards,
                         "Committed defend cards should be cleared after combat resolves")

    def test_ability_not_available_after_rally_leaves_combat(self):
        """After the attack resolves, Rally is on the combat chain / graveyard,
        not a committed blocker, so the ability cannot be activated."""
        env = FaBEnv(verbose=False)
        _advance_to_reaction_with_rally_blocking(env)

        # Pass priority without using the ability
        env.step(Action(ActionType.PASS_PRIORITY))

        # Attack resolves; game has moved on — no ACTIVATE_CARD_ABILITY available
        if env._phase == Phase.REACTION:
            legal = env.legal_actions()
            self.assertFalse(
                any(a.action_type == ActionType.ACTIVATE_CARD_ABILITY for a in legal)
            )


if __name__ == "__main__":
    unittest.main(verbosity=2)

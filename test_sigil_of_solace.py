"""
Unit tests for Sigil of Solace timing and effect.

Seed 27 gives:
  Rhinar:     Chief Ruk'utan, Wild Ride, Beast Mode, Wrecker Romp
  Dorinthea:  On a Knife Edge, Sigil of Solace, Toughen Up, Ironsong Response

Instants use the stack; blocking does not. When Rhinar plays Wild Ride, an
attack reaction INSTANT window opens BEFORE the DEFEND phase. Dorinthea can
respond by playing Sigil of Solace, which goes onto the stack. The stack
resolves in LIFO order, so Sigil of Solace resolves (gain 1 life) BEFORE Wild
Ride's own ON_ATTACK trigger resolves (draw / discard / intimidate). Only
after the reaction window has fully closed does the DEFEND phase begin, and
at that point instants are NOT offered as defend choices — only blocking
cards and equipment are.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType


SEED = 27  # Dorinthea has Sigil of Solace; Rhinar has Wild Ride


def _advance_to_attack_reaction_after_wild_ride(env):
    """Reset at SEED and step through to the attack reaction INSTANT window
    opened by Rhinar's Wild Ride.

    Returns once env._phase == Phase.INSTANT with Dorinthea (agent_1) holding
    priority so she can respond to Wild Ride by playing Sigil of Solace."""
    env.reset(seed=SEED)

    # CHOOSE_FIRST: Rhinar elects to go first
    legal = env.legal_actions()
    go_first = next(a for a in legal if a.action_type == ActionType.GO_FIRST)
    env.step(go_first)

    # ATTACK: Rhinar plays Wild Ride
    legal = env.legal_actions()
    wild_ride = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD and a.card.name == "Wild Ride"
    )
    env.step(wild_ride)

    # PITCH: pay Wild Ride's cost (one pitch step covers cost 2)
    while env._phase == Phase.PITCH:
        env.step(env.legal_actions()[0])

    assert env._phase == Phase.INSTANT, f"Expected INSTANT, got {env._phase}"
    assert env.agent_selection == "agent_1", (
        "Dorinthea should have priority first in the attack reaction window"
    )


class TestSigilOfSolaceCard(unittest.TestCase):
    """Verify Sigil of Solace is correctly defined in the card catalog."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        self.env.reset(seed=SEED)
        self.dorinthea = self.env._game.players[1]

    def test_sigil_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Sigil of Solace", names)

    def test_sigil_card_properties(self):
        from cards import CardType, Color
        sigil = next(c for c in self.dorinthea.hand if c.name == "Sigil of Solace")
        self.assertEqual(sigil.card_type, CardType.INSTANT)
        self.assertEqual(sigil.cost, 0)
        self.assertEqual(sigil.color, Color.BLUE)
        from cards import Keyword
        self.assertIn(Keyword.NO_BLOCK, sigil.keywords)
        self.assertEqual(sigil.text, "Gain 1 life.")


class TestAttackReactionWindow(unittest.TestCase):
    """After Wild Ride is played, an INSTANT window opens BEFORE DEFEND so
    Dorinthea can respond with Sigil of Solace."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_attack_reaction_after_wild_ride(self.env)
        self.dorinthea = self.env._game.players[1]
        self.rhinar = self.env._game.players[0]

    def test_window_is_instant_phase_not_defend(self):
        """The attack reaction step is a proper INSTANT window, not DEFEND."""
        self.assertEqual(self.env._phase, Phase.INSTANT)

    def test_defender_has_priority_first(self):
        """Dorinthea reacts to the declared attack first."""
        self.assertEqual(self.env.agent_selection, "agent_1")

    def test_pending_attack_is_wild_ride(self):
        """Wild Ride is the pending attack awaiting resolution."""
        self.assertIsNotNone(self.env._pending_attack)
        self.assertEqual(self.env._pending_attack.name, "Wild Ride")

    def test_stack_starts_empty(self):
        self.assertEqual(len(self.env._instant_stack), 0)

    def test_sigil_is_legal_in_instant_window(self):
        """PLAY_CARD for Sigil of Solace must appear as a legal INSTANT action."""
        legal = self.env.legal_actions()
        sigil_actions = [
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD
            and a.card is not None
            and a.card.name == "Sigil of Solace"
        ]
        self.assertEqual(len(sigil_actions), 1,
                         f"Expected 1 PLAY_CARD(Sigil of Solace) in legal, got {legal}")

    def test_pass_priority_is_available(self):
        """The defender may also decline to play an instant."""
        legal = self.env.legal_actions()
        self.assertTrue(any(a.action_type == ActionType.PASS_PRIORITY for a in legal))


class TestSigilGoesOnStack(unittest.TestCase):
    """Playing Sigil of Solace pushes it onto the stack rather than resolving
    synchronously — LIFO resolution is what makes it resolve before Wild
    Ride's ON_ATTACK effect."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_attack_reaction_after_wild_ride(self.env)
        self.dorinthea = self.env._game.players[1]
        self.rhinar = self.env._game.players[0]
        # Snapshot state just before Dorinthea plays Sigil
        self._life_before = self.dorinthea.life
        self._rhinar_hand_before = list(self.rhinar.hand)
        self._rhinar_grave_before = list(self.rhinar.graveyard)

        legal = self.env.legal_actions()
        sigil_action = next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "Sigil of Solace"
        )
        self.env.step(sigil_action)

    def test_sigil_on_stack_after_play(self):
        self.assertEqual(len(self.env._instant_stack), 1)
        owner_idx, card = self.env._instant_stack[0]
        self.assertEqual(owner_idx, 1, "Dorinthea owns the stacked Sigil")
        self.assertEqual(card.name, "Sigil of Solace")

    def test_sigil_removed_from_hand_but_not_graveyard_yet(self):
        """When on the stack the card has left hand but has not yet resolved,
        so it is NOT in the graveyard yet."""
        self.assertNotIn("Sigil of Solace",
                         [c.name for c in self.dorinthea.hand])
        self.assertNotIn("Sigil of Solace",
                         [c.name for c in self.dorinthea.graveyard])

    def test_life_unchanged_until_sigil_resolves(self):
        """Playing Sigil does not gain life — resolution does."""
        self.assertEqual(self.dorinthea.life, self._life_before)

    def test_wild_ride_effect_has_not_fired_yet(self):
        """Wild Ride's ON_ATTACK trigger (draw / discard / intimidate) does
        NOT fire while the reaction window is still open."""
        self.assertEqual(list(self.rhinar.hand), self._rhinar_hand_before)
        self.assertEqual(list(self.rhinar.graveyard), self._rhinar_grave_before)

    def test_priority_passes_to_attacker_after_play(self):
        """After Dorinthea plays Sigil, Rhinar gets priority to respond."""
        self.assertEqual(self.env.agent_selection, "agent_0")
        self.assertEqual(self.env._phase, Phase.INSTANT)


class TestSigilResolvesBeforeWildRide(unittest.TestCase):
    """LIFO: Sigil of Solace resolves off the stack BEFORE Wild Ride's own
    ON_ATTACK effect fires when the reaction window closes."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_attack_reaction_after_wild_ride(self.env)
        self.dorinthea = self.env._game.players[1]
        self.rhinar = self.env._game.players[0]

        # Dorinthea plays Sigil of Solace onto the stack.
        legal = self.env.legal_actions()
        sigil_action = next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "Sigil of Solace"
        )
        self.env.step(sigil_action)

        # Snapshot Rhinar's state right before any passes — Wild Ride's
        # effect has not fired yet at this point.
        self._rhinar_hand_before = list(self.rhinar.hand)
        self._rhinar_grave_before = list(self.rhinar.graveyard)

        # Rhinar passes priority. Dorinthea then passes: with both passing on
        # a non-empty stack, the top entry (Sigil of Solace) resolves.
        self.env.step(Action(ActionType.PASS_PRIORITY))  # Rhinar passes
        self.env.step(Action(ActionType.PASS_PRIORITY))  # Dorinthea passes → Sigil resolves

    def test_sigil_resolved_into_graveyard(self):
        self.assertEqual(len(self.env._instant_stack), 0)
        self.assertIn("Sigil of Solace",
                      [c.name for c in self.dorinthea.graveyard])

    def test_life_gained_exactly_one(self):
        """Sigil of Solace says 'Gain 1 life.' — not 3."""
        self.assertEqual(self.dorinthea.life, 21)

    def test_wild_ride_effect_still_has_not_fired(self):
        """Sigil resolved first; Wild Ride's effect is still pending — the
        window is still open and ON_ATTACK does not fire until it closes."""
        self.assertEqual(self.env._phase, Phase.INSTANT)
        self.assertEqual(list(self.rhinar.hand), self._rhinar_hand_before)
        self.assertEqual(list(self.rhinar.graveyard), self._rhinar_grave_before)

    def test_window_still_open_with_priority_to_active(self):
        """After a stack entry resolves, priority returns to the active
        player. The window stays open so more reactions can be played."""
        self.assertEqual(self.env._phase, Phase.INSTANT)
        self.assertEqual(self.env.agent_selection, "agent_0")


class TestWildRideEffectFiresAtWindowClose(unittest.TestCase):
    """When both players pass with an empty stack, the reaction window closes.
    ONLY THEN does Wild Ride's ON_ATTACK effect fire, and THEN the DEFEND
    phase begins."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_attack_reaction_after_wild_ride(self.env)
        self.dorinthea = self.env._game.players[1]
        self.rhinar = self.env._game.players[0]

        legal = self.env.legal_actions()
        sigil_action = next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "Sigil of Solace"
        )
        self.env.step(sigil_action)
        # Resolve Sigil off the stack
        self.env.step(Action(ActionType.PASS_PRIORITY))  # Rhinar passes
        self.env.step(Action(ActionType.PASS_PRIORITY))  # Dorinthea passes → Sigil resolves
        # Close the window: both pass again with empty stack → Wild Ride fires
        self.env.step(Action(ActionType.PASS_PRIORITY))  # Rhinar passes
        self.env.step(Action(ActionType.PASS_PRIORITY))  # Dorinthea passes → window closes

    def test_phase_is_now_defend(self):
        """After the reaction window closes and ON_ATTACK fires, the
        defender gets the DEFEND step."""
        self.assertEqual(self.env._phase, Phase.DEFEND)
        self.assertEqual(self.env.agent_selection, "agent_1")

    def test_pending_attack_still_wild_ride(self):
        self.assertIsNotNone(self.env._pending_attack)
        self.assertEqual(self.env._pending_attack.name, "Wild Ride")

    def test_wild_ride_draw_and_discard_resolved(self):
        """Wild Ride's ON_ATTACK: draw a card then discard random. Net hand
        size is unchanged but graveyard gains exactly one card."""
        self.assertEqual(len(self.rhinar.graveyard), 1)

    def test_defend_actions_do_not_include_instant_plays(self):
        """Instants are played on the stack in the INSTANT window, never as
        part of the DEFEND step. No PLAY_CARD action should be offered here."""
        legal = self.env.legal_actions()
        play_card_actions = [a for a in legal
                             if a.action_type == ActionType.PLAY_CARD]
        self.assertEqual(play_card_actions, [],
                         f"DEFEND phase must not offer PLAY_CARD; got {legal}")

    def test_blocking_actions_are_available(self):
        """Regular blocking options remain: equipment, defense-value cards,
        and the 'done' (empty DEFEND) action."""
        legal = self.env.legal_actions()
        defend_actions = [a for a in legal if a.action_type == ActionType.DEFEND]
        self.assertGreater(len(defend_actions), 0)


class TestSigilNotPlayableDuringDefendPhase(unittest.TestCase):
    """Once the DEFEND phase has started, instants in hand must not appear as
    legal defend actions — they belong on the stack during the instant
    window, not in the blocking step."""

    def test_instant_in_hand_not_offered_as_defend_choice(self):
        env = FaBEnv(verbose=False)
        _advance_to_attack_reaction_after_wild_ride(env)
        dorinthea = env._game.players[1]

        # Skip the reaction window entirely without playing Sigil — so Sigil
        # is still in hand when DEFEND begins.
        env.step(Action(ActionType.PASS_PRIORITY))  # Dorinthea passes
        env.step(Action(ActionType.PASS_PRIORITY))  # Rhinar passes → window closes

        self.assertEqual(env._phase, Phase.DEFEND)
        self.assertIn("Sigil of Solace", [c.name for c in dorinthea.hand])

        legal = env.legal_actions()
        # No PLAY_CARD actions in DEFEND at all
        self.assertFalse(any(a.action_type == ActionType.PLAY_CARD for a in legal))
        # No DEFEND action points to the Sigil (it has no_block=True anyway)
        for a in legal:
            if a.action_type == ActionType.DEFEND:
                for idx in a.defend_hand_indices:
                    self.assertNotEqual(dorinthea.hand[idx].name, "Sigil of Solace")


if __name__ == "__main__":
    unittest.main(verbosity=2)

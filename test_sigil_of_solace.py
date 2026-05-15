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

With auto-execute enabled, the window collapses automatically once neither
player has an instant remaining.  Tests check end-state (life totals, phase,
graveyard contents) rather than intermediate stack state.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import build_rhinar_deck, build_dorinthea_deck


SEED = 27  # Dorinthea has Sigil of Solace; Rhinar has Wild Ride


def _advance_to_attack_reaction_after_wild_ride(env):
    """Reset at SEED and step through to the attack reaction INSTANT window
    opened by Rhinar's Wild Ride.

    Returns once env._phase == Phase.INSTANT with Dorinthea (agent_1) holding
    priority so she can respond to Wild Ride by playing Sigil of Solace.
    Auto-execute does not collapse the window here because Dorinthea has Sigil
    of Solace in hand (two legal actions: PASS_PRIORITY and PLAY_CARD)."""
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)

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
        self.env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
        self.env.step(self.env.legal_actions()[0])  # resolve CHOOSE_FIRST
        self.dorinthea = self.env._game.players[1]

    def test_sigil_in_dorinthea_opening_hand(self):
        names = [c.name for c in self.dorinthea.hand]
        self.assertIn("Sigil of Solace", names)

    def test_sigil_card_properties(self):
        from cards import CardType, Color
        sigil = next(c for c in self.dorinthea.hand if c.name == "Sigil of Solace")
        self.assertEqual(sigil.card_type, [CardType.INSTANT])
        self.assertEqual(sigil.cost, 0)
        self.assertEqual(sigil.color, Color.BLUE)
        self.assertTrue(sigil.no_block)
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


class TestSigilEffectAndTiming(unittest.TestCase):
    """Playing Sigil of Solace in the attack reaction window resolves it via
    the LIFO stack (gaining Dorinthea 1 life) BEFORE Wild Ride's ON_ATTACK
    effect (draw/discard/intimidate) fires when the window closes.

    With auto-execute, these effects all happen inside a single step() call.
    Tests verify the net game state rather than intermediate stack state."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _advance_to_attack_reaction_after_wild_ride(self.env)
        self.dorinthea = self.env._game.players[1]
        self.rhinar = self.env._game.players[0]
        self._life_before = self.dorinthea.life

        legal = self.env.legal_actions()
        sigil_action = next(
            a for a in legal
            if a.action_type == ActionType.PLAY_CARD and a.card.name == "Sigil of Solace"
        )
        # Dorinthea plays Sigil. With auto-execute: Sigil is pushed to the
        # stack, Rhinar and Dorinthea auto-pass (neither has instants left),
        # Sigil resolves (gaining 1 life via ON_PLAY), both auto-pass again,
        # the window closes, Wild Ride's ON_ATTACK fires, and DEFEND begins.
        self.env.step(sigil_action)

    def test_sigil_gained_exactly_one_life(self):
        """Sigil of Solace resolves gaining exactly 1 life — not 0 or 2."""
        self.assertEqual(self.dorinthea.life, self._life_before + 1)

    def test_sigil_in_graveyard_after_resolution(self):
        """Once resolved, Sigil moves from the stack to Dorinthea's graveyard."""
        self.assertIn("Sigil of Solace",
                      [c.name for c in self.dorinthea.graveyard])

    def test_sigil_not_in_hand_after_play(self):
        """Sigil is removed from hand when played."""
        self.assertNotIn("Sigil of Solace",
                         [c.name for c in self.dorinthea.hand])

    def test_phase_is_defend_after_wild_ride_fires(self):
        """Wild Ride's ON_ATTACK fires after Sigil resolves; DEFEND begins."""
        self.assertEqual(self.env._phase, Phase.DEFEND)
        self.assertEqual(self.env.agent_selection, "agent_1")

    def test_wild_ride_draw_and_discard_resolved(self):
        """Wild Ride's ON_ATTACK effect (draw then discard random) fired:
        Rhinar's graveyard gains exactly one card."""
        self.assertEqual(len(self.rhinar.graveyard), 1)


class TestWildRideEffectFiresAtWindowClose(unittest.TestCase):
    """When both players pass with an empty stack, the reaction window closes.
    ONLY THEN does Wild Ride's ON_ATTACK effect fire, and THEN the DEFEND
    phase begins.

    With auto-execute: after Dorinthea plays Sigil, neither player has an
    instant left so both auto-pass; Sigil resolves, then both auto-pass again
    to close the window; Wild Ride fires; DEFEND begins — all inside one step()."""

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
        # Single step: Sigil resolves via stack, Wild Ride fires, DEFEND begins.
        self.env.step(sigil_action)

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
        # Dorinthea passes; with auto-execute Rhinar also passes (no instants),
        # the window closes, Wild Ride fires, and DEFEND begins.
        env.step(Action(ActionType.PASS_PRIORITY))  # Dorinthea passes

        self.assertEqual(env._phase, Phase.DEFEND)
        self.assertIn("Sigil of Solace", [c.name for c in dorinthea.hand])

        legal = env.legal_actions()
        # No PLAY_CARD actions in DEFEND at all
        self.assertFalse(any(a.action_type == ActionType.PLAY_CARD for a in legal))
        # No DEFEND action points to the Sigil (it has no_block=True anyway)
        for a in legal:
            if a.action_type == ActionType.DEFEND:
                if a.hand_index is not None:
                    self.assertNotEqual(dorinthea.hand[a.hand_index].name, "Sigil of Solace")


if __name__ == "__main__":
    unittest.main(verbosity=2)

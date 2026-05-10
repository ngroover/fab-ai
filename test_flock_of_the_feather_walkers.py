"""
Tests for Flock of the Feather Walkers.

Seed 3 gives:
  Dorinthea (active): En Garde, Flock of the Feather Walkers, Visit the Blacksmith, On a Knife Edge
  Dorinthea wins the coin flip and goes first.

Card text: "As an additional cost to play Flock of the Feather Walkers, reveal a card in your hand
with cost 1 or less. When you attack with Flock of the Feather Walkers, create a Quicken token."

Expected behaviour:
  - Playing Flock triggers a REVEAL phase where the player must select a cost ≤ 1 card.
  - Legal REVEAL actions contain only hand cards with cost ≤ 1.
  - After the player chooses a card, it appears in hand_revealed (not discarded).
  - After the reveal, the game enters the DEFEND phase (attack declared).
  - A Quicken token is created when the attack resolves.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import build_rhinar_deck, build_dorinthea_deck

SEED = 3  # Dorinthea active: [En Garde(1), Flock(1), Visit the Blacksmith(0), On a Knife Edge(0)]


def _play_flock(env):
    """
    Reset at SEED, have Dorinthea play Flock of the Feather Walkers, and stop
    just before the REVEAL choice.  Returns (dorinthea, rhinar).
    """
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    dorinthea = env._game.players[1]

    # CHOOSE_FIRST: Dorinthea won the flip → GO_FIRST
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))

    # ATTACK: play Flock of the Feather Walkers
    legal = env.legal_actions()
    flock_action = next(
        a for a in legal
        if a.action_type == ActionType.PLAY_CARD
        and a.card is not None
        and a.card.name == "Flock of the Feather Walkers"
    )
    env.step(flock_action)

    # PITCH: Flock costs 1 — pitch one card to cover it
    if env._phase == Phase.PITCH:
        legal = env.legal_actions()
        env.step(legal[0])

    return dorinthea, env._game.players[0]


class TestFlockRevealPhase(unittest.TestCase):

    def setUp(self):
        self.env = FaBEnv()

    def test_reveal_phase_entered(self):
        """Playing Flock transitions to Phase.REVEAL before attacking."""
        dorinthea, _ = _play_flock(self.env)
        self.assertEqual(self.env._phase, Phase.REVEAL)

    def test_reveal_legal_actions_cost_1_or_less_only(self):
        """REVEAL legal actions contain only cards with cost ≤ 1."""
        dorinthea, _ = _play_flock(self.env)
        legal = self.env.legal_actions()
        self.assertTrue(all(a.action_type == ActionType.REVEAL for a in legal),
                        "All legal actions in REVEAL phase should be REVEAL type")
        self.assertGreater(len(legal), 0, "There must be at least one revealable card")
        for a in legal:
            card = dorinthea.hand[a.hand_index]
            self.assertLessEqual(card.cost, 1,
                                 f"{card.name} has cost {card.cost} > 1 but appears as legal reveal")

    def test_no_high_cost_cards_in_reveal_actions(self):
        """Cards with cost > 1 are not offered as reveal options."""
        dorinthea, _ = _play_flock(self.env)
        legal = self.env.legal_actions()
        revealed_cards = [dorinthea.hand[a.hand_index] for a in legal]
        high_cost = [c for c in revealed_cards if c.cost > 1]
        self.assertEqual(high_cost, [],
                         f"High-cost cards offered as reveal options: {[c.name for c in high_cost]}")

    def test_chosen_card_appears_in_hand_revealed(self):
        """After the REVEAL action, the selected card appears in hand_revealed."""
        dorinthea, _ = _play_flock(self.env)
        self.assertEqual(dorinthea.hand_revealed, [])

        legal = self.env.legal_actions()
        reveal_action = legal[0]
        chosen_card = dorinthea.hand[reveal_action.hand_index]
        self.env.step(reveal_action)

        self.assertIn(chosen_card, dorinthea.hand_revealed,
                      f"{chosen_card.name} should be in hand_revealed after reveal")

    def test_revealed_card_stays_in_hand(self):
        """Revealed card is not discarded — it remains in hand."""
        dorinthea, _ = _play_flock(self.env)
        legal = self.env.legal_actions()
        reveal_action = legal[0]
        chosen_card = dorinthea.hand[reveal_action.hand_index]
        self.env.step(reveal_action)

        self.assertIn(chosen_card, dorinthea.hand,
                      f"{chosen_card.name} should remain in hand after being revealed")
        self.assertNotIn(chosen_card, dorinthea.graveyard,
                         f"{chosen_card.name} should not be in graveyard after reveal")

    def test_defend_phase_follows_reveal(self):
        """After the reveal, the game enters the DEFEND phase (attack is declared)."""
        dorinthea, _ = _play_flock(self.env)
        legal = self.env.legal_actions()
        self.env.step(legal[0])
        self.assertEqual(self.env._phase, Phase.DEFEND)

    def test_quicken_token_created_on_attack(self):
        """Flock creates a Quicken token in the arena when the attack is declared."""
        dorinthea, rhinar = _play_flock(self.env)
        legal = self.env.legal_actions()
        self.env.step(legal[0])  # complete the reveal

        # Quicken token is placed in the attacker's arena (not hand)
        quicken_tokens = [c for c in dorinthea.arena if c.name == "Quicken"]
        self.assertGreater(len(quicken_tokens), 0, "Flock should create a Quicken token in arena")

    def test_player_can_choose_different_cards(self):
        """Player can select any legal cost ≤ 1 card — not just a fixed one."""
        dorinthea, _ = _play_flock(self.env)
        legal = self.env.legal_actions()
        # Should have multiple revealable options (En Garde cost 1, On a Knife Edge cost 0)
        self.assertGreater(len(legal), 1,
                           "Player should have multiple reveal choices with this hand")

        # Pick the second option (different card from legal[0])
        second_choice = legal[1]
        second_card = dorinthea.hand[second_choice.hand_index]
        self.env.step(second_choice)

        self.assertIn(second_card, dorinthea.hand_revealed,
                      f"Second choice {second_card.name} should appear in hand_revealed")


if __name__ == "__main__":
    unittest.main()

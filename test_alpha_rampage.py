"""
Unit tests for Alpha Rampage (red).

Card text: "Rhinar Specialization. As an additional cost to play Alpha Rampage,
discard a random card. Intimidate."

Behaviour under test:
  - Card stats: cost 3, pitch 1, power 9, defense 3, red, Brute, attack-action
    with the Intimidate keyword and a Rhinar Specialization keyword.
  - DISCARD_CARD_COST attached as an ADDITIONAL_COST effect.
  - Legal-action gating: Alpha Rampage is only offered as a PLAY_CARD action
    when the player has enough cards to both cover the 3-resource pitch cost
    AND keep at least one card in hand to satisfy the additional discard cost
    (whether played from hand or arsenal).
  - PITCH phase gating: the last card in hand must not be offered for pitch
    while Alpha Rampage is pending — it is reserved for the discard cost.
  - On play: exactly one card is discarded as the additional cost (no double
    discard). The attack's INTIMIDATE keyword banishes one card from the
    defender's hand. If the discarded card has 6+ power, Rhinar's hero
    ability fires an additional intimidate (two banished cards total).

Alpha Rampage is a single copy at the bottom of the deck in normal play, so
each test pulls it from the deck into a controlled hand using seed 14.
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import CardType, Color, CardClass, Keyword
from cards import build_rhinar_deck, build_dorinthea_deck
from card_effects import EffectTrigger, EffectAction


SEED = 14  # Rhinar wins coin flip; deterministic for hand manipulation


def _setup_rhinar_turn(env):
    """Reset at SEED and resolve CHOOSE_FIRST so Rhinar is the active player
    in ATTACK phase."""
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=SEED)
    legal = env.legal_actions()
    env.step(next(a for a in legal if a.action_type == ActionType.GO_FIRST))


def _pull_from_deck(player, name):
    card = next(c for c in player.deck if c.name == name)
    player.deck.remove(card)
    return card


def _pull_n_pitch_one_reds(player, n):
    """Pull n distinct red cards with pitch=1 from the deck so tests can build
    hands that pitch exactly 1 resource per card."""
    pulled = []
    for c in list(player.deck):
        if len(pulled) >= n:
            break
        if c.color is not None and c.color.name == "RED" and c.pitch == 1 \
                and c.name != "Alpha Rampage":
            player.deck.remove(c)
            pulled.append(c)
    if len(pulled) < n:
        raise RuntimeError(f"Could not pull {n} red pitch-1 cards from deck (got {len(pulled)})")
    return pulled


def _legal_play_alpha(env):
    return [a for a in env.legal_actions()
            if a.action_type == ActionType.PLAY_CARD
            and a.card is not None
            and a.card.name == "Alpha Rampage"]


def _play_alpha_through_to_defend(env):
    """Step the env from a state where Alpha Rampage is a legal PLAY_CARD
    action through any PITCH and INSTANT (attack reaction) phases, landing in
    Phase.DEFEND with Alpha Rampage as the pending attack."""
    alpha_action = _legal_play_alpha(env)[0]
    env.step(alpha_action)
    # Auto-walk through any auto-pitch chain (one PITCH step per legal[0]).
    while env._phase == Phase.PITCH:
        env.step(env.legal_actions()[0])
    # Pass through the attack reaction instant window.
    while env._phase == Phase.INSTANT:
        env.step(env.legal_actions()[0])


# ──────────────────────────────────────────────────────────────────────────
# Card definition
# ──────────────────────────────────────────────────────────────────────────

class TestAlphaRampageCardDefinition(unittest.TestCase):
    """Verify Alpha Rampage's stats, keywords, and effect attachment."""

    def setUp(self):
        env = FaBEnv(verbose=False)
        _setup_rhinar_turn(env)
        self.rhinar = env._game.players[0]
        self.card = _pull_from_deck(self.rhinar, "Alpha Rampage")

    def test_card_type_is_attack_action(self):
        self.assertEqual(self.card.card_type, [CardType.ATTACK, CardType.ACTION])

    def test_cost_is_3(self):
        self.assertEqual(self.card.cost, 3)

    def test_pitch_is_1(self):
        self.assertEqual(self.card.pitch, 1)

    def test_power_is_9(self):
        self.assertEqual(self.card.power, 9)

    def test_defense_is_3(self):
        self.assertEqual(self.card.defense, 3)

    def test_color_is_red(self):
        self.assertEqual(self.card.color, Color.RED)

    def test_class_is_brute(self):
        self.assertEqual(self.card.card_class, CardClass.BRUTE)

    def test_has_intimidate_keyword(self):
        self.assertIn(Keyword.INTIMIDATE, self.card.keywords)

    def test_has_rhinar_specialization_keyword(self):
        self.assertIn(Keyword.RHINAR_SPECIALIZATION, self.card.keywords)

    def test_has_discard_cost_effect(self):
        match = [e for e in self.card.effects
                 if e.trigger == EffectTrigger.ADDITIONAL_COST
                 and e.action == EffectAction.DISCARD_CARD_COST]
        self.assertEqual(len(match), 1,
                         "Alpha Rampage must have exactly one ADDITIONAL_COST "
                         "→ DISCARD_CARD_COST effect")

    def test_text_mentions_additional_cost_discard(self):
        text = self.card.text.lower()
        self.assertIn("additional cost", text)
        self.assertIn("discard", text)

    def test_text_mentions_intimidate(self):
        self.assertIn("intimidate", self.card.text.lower())


# ──────────────────────────────────────────────────────────────────────────
# Legal action gating
# ──────────────────────────────────────────────────────────────────────────

class TestAlphaRampageLegalActions(unittest.TestCase):
    """Alpha Rampage is only a legal PLAY_CARD when the player can cover the
    pitch cost AND still keep one card in hand for the discard."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _setup_rhinar_turn(self.env)
        self.rhinar = self.env._game.players[0]
        self.alpha = _pull_from_deck(self.rhinar, "Alpha Rampage")

    def test_legal_when_hand_covers_pitch_and_discard(self):
        """4 red pitch-1 cards: 3 pitch to cover cost 3, 1 left over for the
        additional discard cost."""
        reds = _pull_n_pitch_one_reds(self.rhinar, 4)
        self.rhinar.hand = [self.alpha] + reds
        self.rhinar.resource_points = 0
        self.assertEqual(len(_legal_play_alpha(self.env)), 1,
                         "Alpha Rampage must be legal when pitch + discard are both satisfiable")

    def test_not_legal_when_alone_in_hand(self):
        """No pitch available and no extra card for the discard."""
        self.rhinar.hand = [self.alpha]
        self.rhinar.resource_points = 0
        self.assertEqual(_legal_play_alpha(self.env), [],
                         "Alpha Rampage must not be legal with no other cards in hand")

    def test_not_legal_when_pitch_consumes_every_other_card(self):
        """Hand has Alpha Rampage + exactly 3 red cards: pitching all three
        covers cost 3 but leaves nothing to discard."""
        reds = _pull_n_pitch_one_reds(self.rhinar, 3)
        self.rhinar.hand = [self.alpha] + reds
        self.rhinar.resource_points = 0
        # 3 reds at pitch 1 each = 3 resources; exactly enough — but nothing left to discard
        self.assertEqual(_legal_play_alpha(self.env), [],
                         "Alpha Rampage must not be legal when all non-Alpha cards "
                         "are needed for pitch")

    def test_legal_with_pre_paid_resources_and_one_other_card(self):
        """Pre-paying cost 3 means no pitch is needed; one extra card suffices
        for the discard."""
        other = _pull_from_deck(self.rhinar, "Dodge")
        self.rhinar.hand = [self.alpha, other]
        self.rhinar.resource_points = 3
        self.assertEqual(len(_legal_play_alpha(self.env)), 1,
                         "Alpha Rampage must be legal when cost is pre-paid and "
                         "hand has at least one other card to discard")

    def test_not_legal_with_pre_paid_resources_and_no_other_cards(self):
        """Pre-paid cost still requires at least one other card in hand to
        satisfy the discard additional cost."""
        self.rhinar.hand = [self.alpha]
        self.rhinar.resource_points = 3
        self.assertEqual(_legal_play_alpha(self.env), [],
                         "Alpha Rampage must not be legal when cost is pre-paid "
                         "but no card is available to discard")

    def test_legal_from_arsenal_with_two_pitch_cards(self):
        """From arsenal: cost 3 covered by one blue (pitch 3), one card left
        in hand for the discard."""
        self.rhinar.hand = []
        self.rhinar.arsenal = self.alpha
        blue = _pull_from_deck(self.rhinar, "Wrecker Romp")  # blue pitch 3
        discardable = _pull_from_deck(self.rhinar, "Dodge")
        self.rhinar.hand = [blue, discardable]
        self.rhinar.resource_points = 0

        arsenal_actions = [a for a in self.env.legal_actions()
                           if a.action_type == ActionType.PLAY_CARD
                           and a.card is not None
                           and a.card.name == "Alpha Rampage"
                           and a.from_arsenal]
        self.assertEqual(len(arsenal_actions), 1,
                         "Alpha Rampage must be legal from arsenal with sufficient "
                         "pitch and a spare card to discard")

    def test_not_legal_from_arsenal_with_one_card_in_hand(self):
        """From arsenal: a single blue covers the pitch but leaves nothing to
        discard (the arsenal card is not in hand to discard from)."""
        self.rhinar.hand = []
        self.rhinar.arsenal = self.alpha
        blue = _pull_from_deck(self.rhinar, "Wrecker Romp")  # blue pitch 3
        self.rhinar.hand = [blue]
        self.rhinar.resource_points = 0

        arsenal_actions = [a for a in self.env.legal_actions()
                           if a.action_type == ActionType.PLAY_CARD
                           and a.card is not None
                           and a.card.name == "Alpha Rampage"
                           and a.from_arsenal]
        self.assertEqual(arsenal_actions, [],
                         "Alpha Rampage from arsenal must not be legal when the "
                         "only pitch card is also the only candidate for discard")


# ──────────────────────────────────────────────────────────────────────────
# PITCH phase gating
# ──────────────────────────────────────────────────────────────────────────

class TestAlphaRampagePitchPhase(unittest.TestCase):
    """During PITCH for Alpha Rampage, the last card in hand must not be a
    pitch option — it is reserved for the additional discard cost."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _setup_rhinar_turn(self.env)
        self.rhinar = self.env._game.players[0]
        self.alpha = _pull_from_deck(self.rhinar, "Alpha Rampage")
        # Arrange hand: Alpha + 3 reds (pitch 1 each). Cost 3, no resources paid.
        reds = _pull_n_pitch_one_reds(self.rhinar, 3)
        extra = _pull_from_deck(self.rhinar, "Dodge")  # spare card for discard
        self.rhinar.hand = [self.alpha] + reds + [extra]
        self.rhinar.resource_points = 0

        # Step into PITCH phase by selecting Alpha Rampage.
        self.env.step(_legal_play_alpha(self.env)[0])

    def test_in_pitch_phase_with_alpha_pending(self):
        self.assertEqual(self.env._phase, Phase.PITCH)
        self.assertEqual(self.env._pending_play_card.name, "Alpha Rampage")

    def test_pitch_not_offered_when_only_one_card_remains(self):
        """Reduce hand to a single card mid-pitch; that card must be reserved
        for the discard cost — no pitch action should be offered."""
        last = next(c for c in self.rhinar.hand if c.name != "Alpha Rampage")
        self.rhinar.hand = [last]
        pitch_with_card = [a for a in self.env.legal_actions()
                           if a.action_type == ActionType.PITCH
                           and a.pitch_index is not None]
        self.assertEqual(pitch_with_card, [],
                         "Pitching the last card must not be offered while "
                         "Alpha Rampage still owes a discard")


# ──────────────────────────────────────────────────────────────────────────
# Play effect: discard count
# ──────────────────────────────────────────────────────────────────────────

class TestAlphaRampageDiscardCount(unittest.TestCase):
    """Playing Alpha Rampage discards exactly one card from hand — the
    DISCARD_CARD_COST effect must not double-fire."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _setup_rhinar_turn(self.env)
        self.rhinar = self.env._game.players[0]
        self.alpha = _pull_from_deck(self.rhinar, "Alpha Rampage")
        # Pre-pay cost so no pitch step is needed; isolate the discard event.
        other = _pull_from_deck(self.rhinar, "Dodge")
        self.rhinar.hand = [self.alpha, other]
        self.rhinar.resource_points = 3

        _play_alpha_through_to_defend(self.env)

    def test_exactly_one_card_in_graveyard(self):
        self.assertEqual(len(self.rhinar.graveyard), 1,
                         "Alpha Rampage's additional cost must discard exactly 1 card "
                         "(not 2 — regression for ADDITIONAL_COST double-fire)")

    def test_hand_is_empty_after_play(self):
        """Hand started as [Alpha Rampage, Dodge]. Alpha is played (consumed),
        Dodge is discarded → hand is empty."""
        self.assertEqual(len(self.rhinar.hand), 0)

    def test_pending_attack_is_alpha_rampage(self):
        self.assertIsNotNone(self.env._pending_attack)
        self.assertEqual(self.env._pending_attack.name, "Alpha Rampage")

    def test_attack_power_is_9(self):
        self.assertEqual(self.env._pending_attack_power, 9)


# ──────────────────────────────────────────────────────────────────────────
# Play effect: Intimidate keyword + Rhinar hero ability
# ──────────────────────────────────────────────────────────────────────────

class TestAlphaRampageIntimidateOnPlay(unittest.TestCase):
    """Alpha Rampage's INTIMIDATE keyword banishes one card from the defender's
    hand when the attack is declared (after the attack-reaction window)."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _setup_rhinar_turn(self.env)
        self.rhinar = self.env._game.players[0]
        self.dori = self.env._game.players[1]
        self.alpha = _pull_from_deck(self.rhinar, "Alpha Rampage")
        # Pre-pay and seed a NON-6+ power card so Rhinar's hero ability does
        # NOT fire — isolating the keyword INTIMIDATE.
        dodge = _pull_from_deck(self.rhinar, "Dodge")  # power 0
        self.rhinar.hand = [self.alpha, dodge]
        self.rhinar.resource_points = 3
        self.dori_hand_before = len(self.dori.hand)
        self.dori_banished_before = len(self.dori.banished)

        _play_alpha_through_to_defend(self.env)

    def test_discarded_card_was_under_six_power(self):
        """Sanity check: the discarded card must have power < 6 so the Rhinar
        hero ability is guaranteed not to fire."""
        self.assertEqual(len(self.rhinar.graveyard), 1)
        self.assertLess(self.rhinar.graveyard[0].power, 6)

    def test_one_card_banished_from_defender(self):
        self.assertEqual(len(self.dori.banished), self.dori_banished_before + 1,
                         "Alpha Rampage's Intimidate keyword must banish exactly 1 "
                         "card from the defender's hand")

    def test_defender_hand_shrinks_by_one(self):
        self.assertEqual(len(self.dori.hand), self.dori_hand_before - 1)

    def test_attacker_intimidated_flag_set(self):
        self.assertTrue(self.rhinar.intimidated_this_turn,
                        "Keyword.INTIMIDATE must set intimidated_this_turn=True")


class TestAlphaRampageRhinarHeroIntimidate(unittest.TestCase):
    """When the discarded card has 6+ power, Rhinar's hero ability fires an
    ADDITIONAL intimidate — combined with the attack's INTIMIDATE keyword the
    defender loses two cards to banish."""

    def setUp(self):
        self.env = FaBEnv(verbose=False)
        _setup_rhinar_turn(self.env)
        self.rhinar = self.env._game.players[0]
        self.dori = self.env._game.players[1]
        self.alpha = _pull_from_deck(self.rhinar, "Alpha Rampage")
        # Force the discard to be a 6-power card so Rhinar's hero ability fires.
        bare_fangs = _pull_from_deck(self.rhinar, "Bare Fangs")  # power 6
        self.assertEqual(bare_fangs.power, 6, "Test precondition: Bare Fangs is 6 power")
        self.rhinar.hand = [self.alpha, bare_fangs]
        self.rhinar.resource_points = 3
        # Ensure defender has at least two cards to lose so both banishes resolve.
        self.assertGreaterEqual(len(self.dori.hand), 2,
                                "Test precondition: defender starts with ≥ 2 cards")
        self.dori_banished_before = len(self.dori.banished)

        _play_alpha_through_to_defend(self.env)

    def test_six_power_card_was_discarded(self):
        """The only non-Alpha card in hand was Bare Fangs (power 6)."""
        self.assertEqual(len(self.rhinar.graveyard), 1)
        self.assertGreaterEqual(self.rhinar.graveyard[0].power, 6)

    def test_two_cards_banished_from_defender(self):
        """Banish 1 from Rhinar hero (6+ power discard) + 1 from keyword INTIMIDATE."""
        self.assertEqual(len(self.dori.banished), self.dori_banished_before + 2,
                         "Defender should have 2 banished cards: one from Rhinar's "
                         "hero ability on the 6-power discard, one from Alpha "
                         "Rampage's Intimidate keyword")


class TestAlphaRampageNoHeroIntimidateWithoutSixPower(unittest.TestCase):
    """Rhinar's hero ability does NOT fire when the discarded card has < 6 power
    — only the attack's INTIMIDATE keyword banishes a defender card (1 total)."""

    def test_only_one_banish_when_discard_is_low_power(self):
        env = FaBEnv(verbose=False)
        _setup_rhinar_turn(env)
        rhinar = env._game.players[0]
        dori = env._game.players[1]
        alpha = _pull_from_deck(rhinar, "Alpha Rampage")
        # Two non-6+ candidates so the random discard is guaranteed sub-6.
        dodge = _pull_from_deck(rhinar, "Dodge")              # power 0
        rally = _pull_from_deck(rhinar, "Rally the Rearguard")  # power 4
        rhinar.hand = [alpha, dodge, rally]
        rhinar.resource_points = 3
        banished_before = len(dori.banished)

        _play_alpha_through_to_defend(env)

        # Whichever sub-6 card was randomly discarded, only the keyword
        # INTIMIDATE should have fired (no Rhinar hero ability).
        self.assertEqual(len(rhinar.graveyard), 1)
        self.assertLess(rhinar.graveyard[0].power, 6)
        self.assertEqual(len(dori.banished), banished_before + 1,
                         "Only the INTIMIDATE keyword should banish — "
                         "Rhinar hero ability requires a 6+ power discard")


if __name__ == "__main__":
    unittest.main(verbosity=2)

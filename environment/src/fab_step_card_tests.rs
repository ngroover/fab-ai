//! Per-card unit tests for on-play effects.
//!
//! Declared as a child module of `fab_step` (via `#[path]` in `fab_step.rs`),
//! so `use super::*` reaches `fab_step`'s private helpers — notably
//! `apply_on_play_effect`, `detach_from_current_zone`, and
//! `attach_to_front_of_zone` — exactly as the main `tests` module does.
//!
//! Cards covered here:
//!   - Bare Fangs          (`Card::BareFangsR`)
//!   - Wild Ride           (`Card::WildRideR`)
//!   - Alpha Rampage       (`Card::AlphaRampageR`)
//!   - Wrecker Romp        (`Card::WreckerRompB`)
//!   - Awakening Bellow    (`Card::AwakeningBellowR`)
//!   - Beast Mode          (`Card::BeastModeR`)
//!   - Rhinar (hero)       (`Card::Rhinar`)
//!   - Pack Hunt           (`Card::PackHuntR`)
//!   - Smash Instinct      (`Card::SmashInstinctY`)
//!   - Wrecking Ball       (`Card::WreckingBallR`)
//!   - Muscle Mutt         (`Card::MuscleMuttY`)
//!   - Raging Onslaught    (`Card::RagingOnslaughtY`)
//!   - Smash with Big Tree (`Card::SmashWithBigTreeY`)
//!   - Clearing Bellow     (`Card::ClearingBellowB`)
//!   - Wounded Bull        (`Card::WoundedBullY`)
//!   - Sigil of Solace     (`Card::SigilofSolaceB`)
//!   - Come to Fight       (`Card::ComeToFightB`)
//!   - Titanium Bauble     (`Card::TitaniumBaubleB`)
//!   - Pack Call           (`Card::PackCallY`)

use super::*;
use crate::cards::Card;
use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
use crate::fab_game::{gamestate_from_decklists, reset};
use crate::legal_actions::legal_actions;
use std::collections::HashSet;

/// Empty `pid`'s hand into its graveyard so that a subsequent draw produces a
/// single, known card. Each card is detached while still tagged as a hand
/// card (so the hand list is fixed up) and then retagged to the graveyard,
/// which is tracked purely by `location`.
fn move_hand_to_graveyard(gs: &mut Gamestate, pid: PlayerIndex) {
    let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    while let Some(head) = player.hand_idx {
        let idx = head.get();
        detach_from_current_zone(player, &mut gs.cards, idx);
        gs.cards[idx].location = CardLocation::graveyard(pid);
    }
}

/// Move the deck card at global index `idx` to the top of `pid`'s deck so it
/// is the very next card drawn. The card is already in the deck, so only its
/// position changes.
fn put_on_top_of_deck(gs: &mut Gamestate, pid: PlayerIndex, idx: usize) {
    let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    detach_from_current_zone(player, &mut gs.cards, idx);
    attach_to_front_of_zone(
        &mut gs.cards,
        &mut player.top_deck_idx,
        Some(&mut player.bottom_deck_idx),
        Some(&mut player.deck_size),
        idx,
    );
}

/// Find a card sitting in p1's deck whose power satisfies `pred`, returning
/// its global index. Only p1's half of the `cards` array is searched.
fn find_p1_deck_card(gs: &Gamestate, pred: impl Fn(u8) -> bool) -> usize {
    gs.cards
        .iter()
        .enumerate()
        .find(|(i, cs)| {
            *i < PLAYER_CARDS
                && cs.location == CardLocation::P1Deck
                && pred(cs.card.data().power)
        })
        .map(|(i, _)| i)
        .expect("p1's deck should contain a card matching the power predicate")
}

#[test]
fn bare_fangs_discarding_power6_grants_plus2_power() {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Force the draw-then-discard to land on a power-6 card: empty p1's hand,
    // then seat a 6-power card on top of the deck. After the forced draw the
    // hand holds exactly that card, so it is the one discarded.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power >= 6);
    assert!(gs.cards[pick].card.data().power >= 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::BareFangsR
        .data()
        .play_effect
        .as_ref()
        .expect("Bare Fangs should carry an on-play effect");
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);

    // Discarding a 6-power card satisfies the condition, banking +2 power for
    // the attack; the discarded card is now in p1's graveyard.
    assert_eq!(gs.p1.attack_power_bonus, 2);
    assert_eq!(gs.cards[pick].location, CardLocation::P1Graveyard);
}

#[test]
fn bare_fangs_discarding_below_power6_grants_no_bonus() {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Same setup, but seat a sub-6-power card on top so the forced discard
    // fails the threshold.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power < 6);
    assert!(gs.cards[pick].card.data().power < 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::BareFangsR
        .data()
        .play_effect
        .as_ref()
        .expect("Bare Fangs should carry an on-play effect");
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);

    // The discard missed the power-6 threshold, so no power is banked; the
    // card still moves to the graveyard.
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert_eq!(gs.cards[pick].location, CardLocation::P1Graveyard);
}

#[test]
fn wild_ride_discarding_power6_grants_go_again() {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Force the draw-then-discard to land on a power-6 card: empty p1's hand,
    // then seat a 6-power card on top of the deck. After the forced draw the
    // hand holds exactly that card, so it is the one discarded.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power >= 6);
    assert!(gs.cards[pick].card.data().power >= 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::WildRideR
        .data()
        .play_effect
        .as_ref()
        .expect("Wild Ride should carry an on-play effect");
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);

    // Discarding a 6-power card satisfies the condition, banking Go Again for
    // the attack (and no power bonus); the discarded card is now in p1's
    // graveyard.
    assert!(gs.p1.attack_go_again_bonus);
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert_eq!(gs.cards[pick].location, CardLocation::P1Graveyard);
}

#[test]
fn wild_ride_discarding_below_power6_grants_no_go_again() {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Same setup, but seat a sub-6-power card on top so the forced discard
    // fails the threshold.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power < 6);
    assert!(gs.cards[pick].card.data().power < 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::WildRideR
        .data()
        .play_effect
        .as_ref()
        .expect("Wild Ride should carry an on-play effect");
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);

    // The discard missed the power-6 threshold, so no Go Again is banked; the
    // card still moves to the graveyard.
    assert!(!gs.p1.attack_go_again_bonus);
    assert_eq!(gs.cards[pick].location, CardLocation::P1Graveyard);
}

// ─────────────────────────────────────────────────────────────────────────────
// Alpha Rampage (Card::AlphaRampageR) and Wrecker Romp (Card::WreckerRompB)
//
// Both carry a "discard a card" additional cost. The tests below cover the
// legal-action gating (a card is only playable, and a pitch only legal, when the
// hand can both cover the cost and keep a card back to discard) and the payment
// of the discard at play time.
// ─────────────────────────────────────────────────────────────────────────────

/// Build a fresh game with Rhinar (p1) on his action phase, ready for his hand
/// to be set up for a legal-actions check.
fn setup_rhinar_action_phase() -> Gamestate {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    gs
}

/// Replace `pid`'s hand with exactly `desired`, in order. The opening hand is
/// trimmed from the front — surplus cards are parked in the graveyard — down to
/// `desired.len()` cards, then the survivors are relabelled. Panics if the hand
/// is smaller than requested.
fn set_hand(gs: &mut Gamestate, pid: PlayerIndex, desired: &[Card]) {
    {
        let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
        assert!(player.hand_size as usize >= desired.len(),
            "opening hand too small to set to the requested cards");
        // Pop cards off the front of the hand list until only `desired.len()`
        // remain. The hand is a singly-linked list whose tail points at itself;
        // advancing `hand_idx` to the head's `next_card` drops the head, mirroring
        // how the engine detaches a hand card.
        while player.hand_size as usize > desired.len() {
            let head = player.hand_idx.expect("hand non-empty while trimming").get();
            let next = gs.cards[head].next_card.get();
            player.hand_idx = if next == head { None } else { Some(CardIdx::new(next)) };
            player.hand_size -= 1;
            gs.cards[head].location = CardLocation::graveyard(pid);
        }
    }
    let survivors: Vec<usize> = {
        let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
        player.hand_iter(&gs.cards).map(|(idx, _)| idx).collect()
    };
    assert_eq!(survivors.len(), desired.len());
    for (slot, &card) in survivors.iter().zip(desired) {
        gs.cards[*slot].card = card;
    }
}

/// The set of distinct cards offered as `PlayCard` actions for the active player
/// in the current phase.
fn playable_cards(gs: &Gamestate) -> HashSet<Card> {
    legal_actions(gs).iter()
        .filter(|a| a.typ == ActionType::PlayCard)
        .map(|a| gs.cards[a.card_index()].card)
        .collect()
}

#[test]
fn discard_cost_card_unplayable_when_paying_leaves_no_card_to_discard() {
    // Both Alpha Rampage and Wrecker Romp carry a "discard a card" additional
    // cost, so a card must be kept back from the pitch pool to discard. In a
    // two-card hand the only partner is a single pitch-3 card: it alone covers
    // the cost, so each card would be playable but for the discard cost — paying
    // it consumes the one card that would otherwise be discarded.

    // Alpha Rampage (cost 3) + Clearing Bellow (pitch 3): not playable.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::AlphaRampageR, Card::ClearingBellowB]);
    gs.p1.resources = 0;
    assert!(!playable_cards(&gs).contains(&Card::AlphaRampageR));

    // Control: Muscle Mutt is also cost 3 but has no additional cost, so the
    // identical hand shape (Muscle Mutt + a pitch-3 card) leaves it playable —
    // confirming the block above comes from the discard cost, not the pitch.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::MuscleMuttY, Card::ClearingBellowB]);
    gs.p1.resources = 0;
    assert!(playable_cards(&gs).contains(&Card::MuscleMuttY));

    // Wrecker Romp (cost 2) + Clearing Bellow (pitch 3): not playable.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::WreckerRompB, Card::ClearingBellowB]);
    gs.p1.resources = 0;
    assert!(!playable_cards(&gs).contains(&Card::WreckerRompB));
}

#[test]
fn discard_cost_card_playable_with_a_spare_card_to_discard() {
    // With pitch enough to both pay the cost and keep a card back to discard, the
    // discard-cost cards are offered. The hand holds Alpha Rampage, Wrecker Romp,
    // a pitch-3 card (Clearing Bellow) and a pitch-1 card (Bare Fangs): the
    // lowest-pitch card is set aside for the discard and the rest still covers
    // each card's cost.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1,
        &[Card::AlphaRampageR, Card::WreckerRompB, Card::ClearingBellowB, Card::BareFangsR]);
    gs.p1.resources = 0;

    let playable = playable_cards(&gs);
    assert!(playable.contains(&Card::AlphaRampageR));
    assert!(playable.contains(&Card::WreckerRompB));
}

#[test]
fn discard_cost_card_playable_when_floating_resources_cover_the_gap() {
    // Floating resources count toward the cost, so a single point of banked
    // resource can be the difference. Hand: Alpha Rampage (cost 3, pitch 1) plus
    // three pitch-1 cards. Setting one pitch-1 card aside to discard leaves only
    // 2 pitch — short of the cost by itself, but with 1 floating resource the
    // remaining 2 pitch covers the other 2 owed.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1,
        &[Card::AlphaRampageR, Card::AlphaRampageR, Card::AwakeningBellowR, Card::BareFangsR]);

    // Without the floating resource the remaining 2 pitch can't cover cost 3.
    gs.p1.resources = 0;
    assert!(!playable_cards(&gs).contains(&Card::AlphaRampageR));

    // One floating resource drops what's owed to 2, which the spare pitch covers
    // — so Alpha Rampage becomes playable.
    gs.p1.resources = 1;
    assert!(playable_cards(&gs).contains(&Card::AlphaRampageR));
}

#[test]
fn pitch_for_discard_cost_card_excludes_the_card_needed_to_discard() {
    // Paying for a card with a "discard a card" additional cost must leave a card
    // in hand to discard. Hand: Alpha Rampage (cost 3) plus a red 1-pitch card
    // and a blue 3-pitch card.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1,
        &[Card::AlphaRampageR, Card::BareFangsR, Card::ClearingBellowB]);
    gs.p1.resources = 0;

    // Play Alpha Rampage: it can't be paid outright, so we drop into the pitch
    // phase with Alpha Rampage pending in hand.
    let ar_idx = gs.p1.hand_iter(&gs.cards)
        .find(|(_, cs)| cs.card == Card::AlphaRampageR)
        .map(|(idx, _)| idx)
        .expect("Alpha Rampage should be in hand");
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(ar_idx))});
    assert_eq!(gs.phase, Phase::ActionPitch);

    // Only the blue 3-pitch card is a legal pitch: pitching it covers the cost
    // and leaves the red card to discard. Pitching the red 1-pitch card would
    // force the blue card to be pitched too, leaving nothing to discard — so the
    // red card is not offered.
    let pitchable: HashSet<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::Pitch)
        .map(|a| gs.cards[a.card_index()].card)
        .collect();
    assert_eq!(pitchable, HashSet::from([Card::ClearingBellowB]));
    assert!(!pitchable.contains(&Card::BareFangsR));
}

#[test]
fn discard_cost_card_discards_a_hand_card_when_played() {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);

    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Relabel Muscle Mutt to Alpha Rampage (cost 3, carries a "discard a card"
    // additional cost). Rhinar's opening hand is then Alpha Rampage, Pack Call,
    // Raging Onslaught and Clearing Bellow.
    let ar_idx = gs.p1.hand_iter(&gs.cards)
            .find(|(_, cs)| cs.card == Card::MuscleMuttY)
            .map(|(idx, _)| idx)
            .expect("Muscle Mutt should be in the opening hand");
    gs.cards[ar_idx].card = Card::AlphaRampageR;

    // The two cards that will be left in hand once Alpha Rampage is committed and
    // Clearing Bellow is pitched — the discard must land on one of them.
    let pack_idx = gs.p1.hand_iter(&gs.cards)
            .find(|(_, cs)| cs.card == Card::PackCallY)
            .map(|(idx, _)| idx)
            .expect("Pack Call should be in the opening hand");
    let ro_idx = gs.p1.hand_iter(&gs.cards)
            .find(|(_, cs)| cs.card == Card::RagingOnslaughtY)
            .map(|(idx, _)| idx)
            .expect("Raging Onslaught should be in the opening hand");

    // Play Alpha Rampage; it can't be paid outright so it stays pending.
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(ar_idx))});
    assert_eq!(gs.phase, Phase::ActionPitch);

    // Before paying, Pack Call and Raging Onslaught are both still in hand — the
    // discard cost has not been paid yet.
    assert_eq!(gs.cards[pack_idx].location, CardLocation::P1Hand);
    assert_eq!(gs.cards[ro_idx].location, CardLocation::P1Hand);

    // Pitch Clearing Bellow (pitch 3) to cover the cost. This commits Alpha
    // Rampage to the stack, and the "discard a card" additional cost is paid as
    // it is played: one random card from the rest of the hand (Pack Call or
    // Raging Onslaught) is discarded to the graveyard immediately — well before
    // the card resolves.
    let cb_idx = gs.p1.hand_iter(&gs.cards)
            .find(|(_, cs)| cs.card == Card::ClearingBellowB)
            .map(|(idx, _)| idx)
            .expect("Clearing Bellow should be in the opening hand");
    step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});
    assert_eq!(gs.phase, Phase::ActionInstant);
    assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(ar_idx));

    // Exactly one of the two remaining cards was discarded at play time; the
    // other stays in hand, leaving a single card in hand.
    let discarded_at_play = [pack_idx, ro_idx]
            .into_iter()
            .filter(|&idx| gs.cards[idx].location == CardLocation::P1Graveyard)
            .count();
    assert_eq!(discarded_at_play, 1, "exactly one hand card should be discarded when played");
    assert_eq!(gs.p1.hand_size, 1);

    // Both players pass: Alpha Rampage resolves onto the combat chain. No further
    // discard happens at resolution — the cost was already paid.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.cards[ar_idx].location, CardLocation::P1CombatChain);

    let discarded_after_resolve = [pack_idx, ro_idx]
            .into_iter()
            .filter(|&idx| gs.cards[idx].location == CardLocation::P1Graveyard)
            .count();
    assert_eq!(discarded_after_resolve, 1, "the discard cost is paid once, at play time");
    assert_eq!(gs.p1.hand_size, 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Awakening Bellow (Card::AwakeningBellowR)
//
// An action with Go Again and Intimidate whose on-play effect banks +3 power for
// the next *brute* attack the owner plays this turn. The tests below verify the
// keywords, that a follow-up brute attack gets the +3, and that a follow-up
// generic (non-brute) attack does not (and leaves the bonus banked).
// ─────────────────────────────────────────────────────────────────────────────

/// Seat `card` on `pid`'s combat-chain link 0 as the attacking card, ready for
/// `resolve_combat_damage`. The head of `pid`'s hand is relabelled to `card`,
/// detached from the hand, and attached to the chain; its global index (the
/// attacking card) is returned.
fn place_attacker_on_chain(gs: &mut Gamestate, pid: PlayerIndex, card: Card) -> usize {
    let idx = {
        let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
        player.hand_idx.expect("hand should have a card to seat as the attacker").get()
    };
    gs.cards[idx].card = card;
    let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    detach_from_current_zone(player, &mut gs.cards, idx);
    gs.cards[idx].location = CardLocation::combat_chain(pid);
    attach_to_front_of_zone(&mut gs.cards, &mut player.chain_link[0], None, None, idx);
    idx
}

#[test]
fn awakening_bellow_has_go_again_and_intimidate() {
    use crate::cards::Keyword;
    let data = Card::AwakeningBellowR.data();
    assert!(data.keyword.contains(Keyword::GoAgain));
    assert!(data.keyword.contains(Keyword::Intimidate));
}

#[test]
fn awakening_bellow_banks_plus3_for_the_next_brute_attack() {
    let mut gs = setup_rhinar_action_phase();

    // Resolving Awakening Bellow's on-play effect banks +3 for the next brute.
    let effect = Card::AwakeningBellowR
        .data()
        .play_effect
        .as_ref()
        .expect("Awakening Bellow should carry an on-play effect");
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);
}

#[test]
fn awakening_bellow_followup_brute_attack_gets_plus3() {
    let mut gs = setup_rhinar_action_phase();

    // Bank the +3 brute bonus from Awakening Bellow.
    let effect = Card::AwakeningBellowR.data().play_effect.as_ref().unwrap();
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);

    // A follow-up brute attack — Bare Fangs (brute, 6 power) — is seated on the
    // chain against an undefended opponent and resolves combat damage.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    assert_eq!(gs.cards[attacker].card.data().card_class, CardClass::Brute);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // Brute attack: it gets the +3, so 6 + 3 = 9 damage, and the bonus is spent.
    assert_eq!(gs.p2.life, life_before - 9);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 0);
}

#[test]
fn awakening_bellow_followup_generic_attack_does_not_get_plus3() {
    let mut gs = setup_rhinar_action_phase();

    // Bank the +3 brute bonus from Awakening Bellow.
    let effect = Card::AwakeningBellowR.data().play_effect.as_ref().unwrap();
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);

    // A follow-up generic (non-brute) attack — Muscle Mutt (generic, 6 power) —
    // is seated on the chain against an undefended opponent and resolves.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
    assert_eq!(gs.cards[attacker].card.data().card_class, CardClass::Generic);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // Generic attack: no +3, so just 6 damage — and the bonus stays banked for a
    // later brute attack this turn rather than being consumed.
    assert_eq!(gs.p2.life, life_before - 6);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);
}

#[test]
fn awakening_bellow_brute_weapon_swing_does_not_get_plus3() {
    let mut gs = setup_rhinar_action_phase();

    // Bank the +3 brute bonus from Awakening Bellow.
    let effect = Card::AwakeningBellowR.data().play_effect.as_ref().unwrap();
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);

    // Bone Basher is a Brute, but it is a *weapon* (not an attack action card),
    // so swinging it does not qualify for the bonus. Seat it on the chain (4
    // power) against an undefended opponent and resolve.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BoneBasher);
    let data = gs.cards[attacker].card.data();
    assert_eq!(data.card_class, CardClass::Brute);
    assert_eq!(data.typ, CardType::Weapon);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // Brute weapon: no +3, so just 4 damage — and the bonus stays banked for a
    // later brute attack action this turn rather than being consumed.
    assert_eq!(gs.p2.life, life_before - 4);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Beast Mode (Card::BeastModeR)
//
// A 6-power brute attack action whose on-play effect grants +2 power "if you've
// intimidated this turn". The owner's `has_intimidated` flag, set whenever an
// Intimidate trigger resolves for them, gates the bonus. The tests below verify
// the attack lands for 8 when the owner has intimidated and for 6 when it hasn't.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn beast_mode_gets_plus2_when_player_has_intimidated() {
    let mut gs = setup_rhinar_action_phase();

    // The owner intimidated earlier this turn.
    gs.p1.has_intimidated = true;

    // Resolving Beast Mode's on-play effect banks +2 onto the resolving attack.
    let effect = Card::BeastModeR
        .data()
        .play_effect
        .as_ref()
        .expect("Beast Mode should carry an on-play effect");
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.attack_power_bonus, 2);

    // Seat Beast Mode (brute, 6 power) on the chain against an undefended
    // opponent and resolve: 6 + 2 = 8 damage, and the bonus is consumed.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BeastModeR);
    assert_eq!(gs.cards[attacker].card.data().card_class, CardClass::Brute);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 8);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn beast_mode_no_bonus_when_player_has_not_intimidated() {
    let mut gs = setup_rhinar_action_phase();

    // The owner has not intimidated this turn (the flag starts cleared).
    assert!(!gs.p1.has_intimidated);

    // Resolving Beast Mode's on-play effect does nothing: the condition fails.
    let effect = Card::BeastModeR.data().play_effect.as_ref().unwrap();
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.attack_power_bonus, 0);

    // Seat Beast Mode (brute, 6 power) on the chain against an undefended
    // opponent and resolve: just its base 6 damage, with no bonus.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BeastModeR);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 6);
}

// ─────────────────────────────────────────────────────────────────────────────
// Rhinar's constant hero ability: OnDiscard6Intimidate
//
// Whenever Rhinar discards a card with 6 or more power, he Intimidates (the
// opponent banishes a card from hand). This fires for every discard path — the
// "discard a card" additional cost (Alpha Rampage, Wrecker Romp) and the
// play-effect draw-then-discard (Bare Fangs, Wild Ride) — and stacks with a
// card's own Intimidate keyword.
// ─────────────────────────────────────────────────────────────────────────────

/// Count the cards sitting in `pid`'s Intimidate banish zone by location tag.
fn intimidate_banish_count(gs: &Gamestate, pid: PlayerIndex) -> usize {
    let loc = CardLocation::intimidate_banish(pid);
    gs.cards.iter().filter(|cs| cs.location == loc).count()
}

#[test]
fn rhinar_constant_ability_is_on_discard6_intimidate() {
    use crate::card_effects::ConstantEffect;
    assert!(matches!(
        Card::Rhinar.data().constant_effect,
        Some(ConstantEffect::OnDiscard6Intimidate)
    ));
    // The opposing hero (Dorinthea) carries no such constant effect.
    assert!(Card::Dorinthea.data().constant_effect.is_none());
}

#[test]
fn discard_cost_of_power6_card_triggers_intimidate() {
    let mut gs = setup_rhinar_action_phase();

    // Seat a single power-6 card in Rhinar's hand so the "discard a card" cost
    // is forced to discard it. Wrecker Romp is power 6.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::WreckerRompB]);
    assert!(Card::WreckerRompB.data().power >= 6);
    assert!(gs.p2.hand_size > 0, "opponent needs a card to be intimidated");
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);

    apply_discard_cost(&mut gs, PlayerIndex::P1);

    // The 6-power discard triggered Rhinar's constant ability: the opponent
    // banished one card from hand to their Intimidate banish zone.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
}

#[test]
fn discard_cost_of_sub_power6_card_does_not_trigger_intimidate() {
    let mut gs = setup_rhinar_action_phase();

    // Clearing Bellow is power 0, below the threshold, so the discard cost pays
    // out without triggering the constant ability.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ClearingBellowB]);
    assert!(Card::ClearingBellowB.data().power < 6);

    apply_discard_cost(&mut gs, PlayerIndex::P1);

    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
}

#[test]
fn bare_fangs_play_effect_power6_discard_also_intimidates() {
    // The play-effect draw-then-discard path (Bare Fangs / Wild Ride) routes
    // through the same constant-ability check, so a power-6 discard intimidates
    // on top of banking the card's own +2 power.
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power >= 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);

    let effect = Card::BareFangsR.data().play_effect.as_ref().unwrap();
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);

    // The play effect banked its +2, and the 6-power discard also intimidated.
    assert_eq!(gs.p1.attack_power_bonus, 2);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
}

#[test]
fn non_rhinar_hero_power6_discard_does_not_intimidate() {
    // The constant ability keys off the discarding hero, not the discard itself.
    // Dorinthea (p2) discarding a 6-power card does not intimidate p1.
    let mut gs = setup_rhinar_action_phase();
    assert!(Card::Dorinthea.data().constant_effect.is_none());

    set_hand(&mut gs, PlayerIndex::P2, &[Card::WreckerRompB]);
    assert!(Card::WreckerRompB.data().power >= 6);

    apply_discard_cost(&mut gs, PlayerIndex::P2);

    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P1), 0);
}

#[test]
fn alpha_rampage_power6_discard_intimidates_twice() {
    // Alpha Rampage carries the Intimidate keyword *and* a "discard a card" cost.
    // When the discarded card is power 6+, the keyword Intimidate (at resolution)
    // and Rhinar's constant ability (at the discard) both fire — two cards leave
    // the opponent's hand.
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Make every non-Alpha-Rampage hand card a power-6 card so whichever one the
    // discard cost picks is guaranteed to clear the threshold. Rhinar's opening
    // hand is Muscle Mutt, Pack Call, Raging Onslaught and Clearing Bellow.
    let mut ar_idx = None;
    let hand: Vec<usize> = gs.p1.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
    for (slot, &idx) in hand.iter().enumerate() {
        if slot == 0 {
            gs.cards[idx].card = Card::AlphaRampageR;
            ar_idx = Some(idx);
        } else {
            gs.cards[idx].card = Card::WreckerRompB; // power 6
        }
    }
    let ar_idx = ar_idx.expect("opening hand should be non-empty");
    assert!(Card::AlphaRampageR.data().keyword.contains(crate::cards::Keyword::Intimidate));

    // Pay the cost 3 outright so no pitching disturbs the hand.
    gs.p1.resources = 3;
    assert!(gs.p2.hand_size >= 2, "opponent needs two cards to be intimidated twice");

    // Play Alpha Rampage: paid from resources, it commits to the stack and the
    // discard cost is paid from the (all power-6) remainder of the hand — the
    // constant ability intimidates once here.
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(ar_idx))});
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1,
        "the power-6 discard cost should intimidate once");

    // Both players pass so Alpha Rampage resolves; its Intimidate keyword fires
    // as it lands on the chain, intimidating a second time.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    assert_eq!(gs.cards[ar_idx].location, CardLocation::P1CombatChain);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 2,
        "keyword Intimidate plus the constant ability should intimidate twice in total");
}

// ─────────────────────────────────────────────────────────────────────────────
// Pack Hunt (Card::PackHuntR) and Smash Instinct (Card::SmashInstinctY)
//
// Both are brute attack actions carrying the Intimidate keyword. Intimidate is
// applied generically as a card resolves, so when either attack resolves onto
// the combat chain the defending player banishes a random card from hand into
// their Intimidate banish zone. Neither card has a discard cost or discard play
// effect, so Rhinar's OnDiscard6Intimidate does not also fire — the keyword is
// the sole source of the single Intimidate.
// ─────────────────────────────────────────────────────────────────────────────

/// Play `card` (an attack action costing at most 3) from p1's hand and resolve
/// it onto the combat chain. The first two opening-hand cards are relabelled to
/// `card` and to Clearing Bellow (pitch 3, used to pay the cost); both players
/// then pass so the attack resolves. Returns the game and the played card's
/// global index.
fn play_and_resolve_attack(card: Card) -> (Gamestate, usize) {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    let hand: Vec<usize> = gs.p1.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
    let atk_idx = hand[0];
    let pitch_idx = hand[1];
    gs.cards[atk_idx].card = card;
    gs.cards[pitch_idx].card = Card::ClearingBellowB; // pitch 3 covers cost <= 3
    assert!(card.data().cost <= 3, "helper only pays costs up to a single pitch-3 card");

    // Play the attack, pitch to cover its cost, then both players pass so it
    // resolves onto p1's combat chain (its Intimidate fires as it resolves).
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(atk_idx))});
    assert_eq!(gs.phase, Phase::ActionPitch);
    step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(pitch_idx))});
    assert_eq!(gs.phase, Phase::ActionInstant);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    (gs, atk_idx)
}

#[test]
fn pack_hunt_has_intimidate() {
    use crate::cards::Keyword;
    assert!(Card::PackHuntR.data().keyword.contains(Keyword::Intimidate));
}

#[test]
fn smash_instinct_has_intimidate() {
    use crate::cards::Keyword;
    assert!(Card::SmashInstinctY.data().keyword.contains(Keyword::Intimidate));
}

#[test]
fn pack_hunt_intimidates_on_resolve() {
    let (gs, atk_idx) = play_and_resolve_attack(Card::PackHuntR);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.cards[atk_idx].location, CardLocation::P1CombatChain);
    // The Intimidate keyword fired once as the attack resolved.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
}

#[test]
fn smash_instinct_intimidates_on_resolve() {
    let (gs, atk_idx) = play_and_resolve_attack(Card::SmashInstinctY);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.cards[atk_idx].location, CardLocation::P1CombatChain);
    // The Intimidate keyword fired once as the attack resolved.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Wrecking Ball (Card::WreckingBallR)
//
// Brute attack action. On play, draw a card then discard a card; if the
// discarded card has 6 or more power, Intimidate. Same DrawDiscardHit6 condition
// as Bare Fangs / Wild Ride, but the payoff is a conditional Intimidate rather
// than a power / Go Again buff. Played by Rhinar, a 6-power discard Intimidates
// twice — once from his OnDiscard6Intimidate constant ability (which fires inside
// the discard) and once from Wrecking Ball's own conditional Intimidate.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wrecking_ball_discarding_power6_intimidates_twice_under_rhinar() {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Force the draw-then-discard to land on a power-6 card: empty p1's hand,
    // then seat a 6-power card on top of the deck so it is the one drawn and
    // discarded.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power >= 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);
    assert!(gs.p2.hand_size >= 2, "opponent needs two cards to be intimidated twice");
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);

    let effect = Card::WreckingBallR
        .data()
        .play_effect
        .as_ref()
        .expect("Wrecking Ball should carry an on-play effect");
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);

    // The 6-power discard intimidates twice (constant ability + the card's own
    // conditional Intimidate) and banks no power; the card is now in the
    // graveyard.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 2);
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert_eq!(gs.cards[pick].location, CardLocation::P1Graveyard);
}

#[test]
fn wrecking_ball_discarding_below_power6_does_not_intimidate() {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Seat a sub-6-power card on top so the forced discard fails the threshold.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power < 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::WreckingBallR.data().play_effect.as_ref().unwrap();
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);

    // The discard missed the power-6 threshold, so neither the constant ability
    // nor the conditional Intimidate fires; the card still moves to the
    // graveyard.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
    assert_eq!(gs.cards[pick].location, CardLocation::P1Graveyard);
}

// ─────────────────────────────────────────────────────────────────────────────
// Vanilla cards: Muscle Mutt (Card::MuscleMuttY), Raging Onslaught
// (Card::RagingOnslaughtY) and Smash with Big Tree (Card::SmashWithBigTreeY)
//
// None of the three has rules text. They are covered here to pin down that they
// are *deliberately* effectless — a later card that adds a shared mechanic must
// not quietly give them one — and that each hits for exactly its printed power.
// Smash with Big Tree additionally carries `no_block`, so it cannot be used to
// block even though it sits in hand like any other card.
// ─────────────────────────────────────────────────────────────────────────────

/// Assert `card` is an attack action with the given printed stats and class and
/// carries no rules text at all: no keywords, no on-play effect, no additional
/// cost, no activated ability, and no defend / next-attack / target effect.
fn assert_vanilla_attack(card: Card, cost: u8, power: u8, defense: u8, class: CardClass) {
    let data = card.data();
    assert_eq!(data.typ, CardType::AttackAction);
    assert_eq!(data.cost, cost);
    assert_eq!(data.power, power);
    assert_eq!(data.defense, defense);
    assert_eq!(data.card_class, class);
    assert!(data.keyword.is_empty(), "{:?} should carry no keywords", card);
    assert!(data.play_effect.is_none(), "{:?} should have no on-play effect", card);
    assert!(data.additional_cost.is_none(), "{:?} should have no additional cost", card);
    assert!(data.ability.is_none(), "{:?} should have no activated ability", card);
    assert!(data.defend_effect.is_none(), "{:?} should have no defend effect", card);
    assert!(data.next_attack_effect.is_none(), "{:?} should have no next-attack effect", card);
    assert!(data.target_effect.is_none(), "{:?} should have no target effect", card);
}

/// Seat `card` on p1's combat chain against an undefended opponent and resolve
/// combat damage, asserting the opponent lost exactly `expected` life and that
/// nothing was intimidated along the way.
fn assert_undefended_hit(card: Card, expected: u8) {
    let mut gs = setup_rhinar_action_phase();
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, card);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);
    assert_eq!(gs.p2.life, life_before - expected);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
}

#[test]
fn muscle_mutt_is_a_vanilla_6_power_generic_attack() {
    assert_vanilla_attack(Card::MuscleMuttY, 3, 6, 2, CardClass::Generic);
    assert_undefended_hit(Card::MuscleMuttY, 6);
}

#[test]
fn raging_onslaught_is_a_vanilla_6_power_generic_attack() {
    assert_vanilla_attack(Card::RagingOnslaughtY, 3, 6, 3, CardClass::Generic);
    assert_undefended_hit(Card::RagingOnslaughtY, 6);
}

#[test]
fn smash_with_big_tree_is_a_vanilla_6_power_brute_attack() {
    assert_vanilla_attack(Card::SmashWithBigTreeY, 2, 6, 0, CardClass::Brute);
    assert_undefended_hit(Card::SmashWithBigTreeY, 6);
}

#[test]
fn smash_with_big_tree_cannot_be_used_to_block() {
    // Smash with Big Tree has 0 defense and `no_block`, so it must not be
    // offered as a Defend action even while sitting in the defender's hand.
    let (mut gs, _) = play_and_resolve_attack(Card::MuscleMuttY);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.active_player, PlayerIndex::P2);

    let blocker = gs.p2.hand_idx.expect("defender should have a hand card").get();
    gs.cards[blocker].card = Card::SmashWithBigTreeY;
    assert!(Card::SmashWithBigTreeY.data().no_block);

    let defends: Vec<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::Defend)
        .map(|a| gs.cards[a.card_index()].card)
        .collect();
    assert!(!defends.contains(&Card::SmashWithBigTreeY),
        "a no_block card must not be offered as a block");
    // The rest of the hand is still blockable, so the exclusion is card-specific
    // rather than the defend options having gone empty.
    assert!(!defends.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Clearing Bellow (Card::ClearingBellowB)
//
// A 0-cost brute *action* (not an attack) with Intimidate and Go Again. It has
// no on-play effect: both halves of its text are keywords the engine already
// applies generically — Intimidate fires as the card resolves, whether it goes
// on to the combat chain or, as here, straight to the graveyard, and Go Again
// spares the owner's action point.
// ─────────────────────────────────────────────────────────────────────────────

/// Play the 0-cost action `card` from the head of p1's hand and resolve it. A
/// 0-cost card is affordable outright, so it commits straight to the stack with
/// no pitching; both players then pass so it resolves. Returns the game and the
/// played card's global index.
fn play_and_resolve_zero_cost_action(card: Card) -> (Gamestate, usize) {
    let mut gs = setup_rhinar_action_phase();
    assert_eq!(card.data().cost, 0, "helper only plays 0-cost cards");

    let idx = gs.p1.hand_idx.expect("hand should have a card to relabel").get();
    gs.cards[idx].card = card;

    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(idx))});
    assert_eq!(gs.phase, Phase::ActionInstant);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    (gs, idx)
}

#[test]
fn clearing_bellow_has_go_again_and_intimidate() {
    use crate::cards::Keyword;
    let data = Card::ClearingBellowB.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!(data.card_class, CardClass::Brute);
    assert!(data.keyword.contains(Keyword::GoAgain));
    assert!(data.keyword.contains(Keyword::Intimidate));
    // Both halves of the text are keywords: there is no on-play effect.
    assert!(data.play_effect.is_none());
}

#[test]
fn clearing_bellow_intimidates_on_resolve_and_keeps_the_action_point() {
    let (gs, idx) = play_and_resolve_zero_cost_action(Card::ClearingBellowB);

    // A non-attack action resolves to the graveyard rather than the chain, and
    // its Intimidate still fires on the way there.
    assert_eq!(gs.cards[idx].location, CardLocation::P1Graveyard);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
    assert!(gs.p1.has_intimidated);

    // Go Again: the action point is not spent, so Rhinar may act again.
    assert_eq!(gs.p1.action_points, 1);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Wounded Bull (Card::WoundedBullY)
//
// "When you play Wounded Bull, if you have less health than an opposing hero,
// it gains +1 power." A generic 6-power attack action whose on-play effect is
// the `HasLessLife` condition driving the shared `ConditionalPower` payoff: the
// life totals are compared as the card resolves, and the +1 is banked on the
// player and folded into the chain's power when combat damage resolves. The
// comparison is strict, so being level on life grants nothing.
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve Wounded Bull's on-play effect for `owner` with the two heroes on
/// `p1_life` / `p2_life`, returning the game so the banked bonus can be checked.
fn resolve_wounded_bull_at(owner: PlayerIndex, p1_life: u8, p2_life: u8) -> Gamestate {
    let mut gs = setup_rhinar_action_phase();
    gs.p1.life = p1_life;
    gs.p2.life = p2_life;

    let effect = Card::WoundedBullY
        .data()
        .play_effect
        .as_ref()
        .expect("Wounded Bull should carry an on-play effect");
    apply_on_play_effect(&mut gs, owner, effect);
    gs
}

#[test]
fn wounded_bull_effect_is_conditional_power_on_less_life() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::WoundedBullY.data();
    assert_eq!(data.typ, CardType::AttackAction);
    assert_eq!(data.power, 6);
    assert_eq!(data.card_class, CardClass::Generic);
    let effect = data.play_effect.as_ref().expect("Wounded Bull should carry an on-play effect");
    assert!(matches!(effect.condition, OnPlayConditionType::HasLessLife));
    assert!(matches!(effect.effectType, OnPlayEffectType::ConditionalPower));
    assert_eq!(effect.magnitude, 1);
}

#[test]
fn wounded_bull_banks_plus1_when_behind_on_life() {
    let gs = resolve_wounded_bull_at(PlayerIndex::P1, 15, 20);
    assert_eq!(gs.p1.attack_power_bonus, 1);
    assert_eq!(gs.p2.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_banks_nothing_when_level_on_life() {
    // "Less health" is a strict comparison: level on life grants nothing.
    let gs = resolve_wounded_bull_at(PlayerIndex::P1, 20, 20);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_banks_nothing_when_ahead_on_life() {
    let gs = resolve_wounded_bull_at(PlayerIndex::P1, 20, 15);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_condition_is_relative_to_its_owner() {
    // The condition compares the *owner's* life against their opponent's, so the
    // same life totals that deny p1 the bonus grant it to p2.
    let gs = resolve_wounded_bull_at(PlayerIndex::P2, 20, 15);
    assert_eq!(gs.p2.attack_power_bonus, 1);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_hits_for_7_when_behind_on_life() {
    let mut gs = resolve_wounded_bull_at(PlayerIndex::P1, 15, 20);
    assert_eq!(gs.p1.attack_power_bonus, 1);

    // Seat Wounded Bull on the chain against an undefended opponent: 6 printed
    // power plus the banked +1.
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::WoundedBullY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 7);
    // The on-play bonus is single-use: a follow-up attack does not inherit it.
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_hits_for_6_when_ahead_on_life() {
    let mut gs = resolve_wounded_bull_at(PlayerIndex::P1, 20, 15);
    assert_eq!(gs.p1.attack_power_bonus, 0);

    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::WoundedBullY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 6);
}

// ─────────────────────────────────────────────────────────────────────────────
// Sigil of Solace (Card::SigilofSolaceB)
//
// "Gain 1 life." A 0-cost blue *instant* with no power or defense, so it never
// blocks and never attacks — it simply resolves to the graveyard, healing its
// owner on the way. Being an instant it costs no action point, and there is no
// maximum life total, so the gain applies even at full health.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sigil_of_solace_effect_is_gain_1_life() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::SigilofSolaceB.data();
    assert_eq!(data.typ, CardType::Instant);
    assert_eq!(data.cost, 0);
    assert_eq!(data.pitch, 3);
    assert_eq!(data.power, 0);
    // No defense and flagged no_block: it cannot be used to block.
    assert_eq!(data.defense, 0);
    assert!(data.no_block);

    let effect = data.play_effect.as_ref().expect("Sigil of Solace should carry an on-play effect");
    // Unconditional: the life is gained whenever it resolves.
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::GainLife));
    assert_eq!(effect.magnitude, 1);
}

#[test]
fn sigil_of_solace_gains_1_life_on_resolve() {
    let mut gs = setup_rhinar_action_phase();
    let idx = gs.p1.hand_idx.expect("hand should have a card to relabel").get();
    gs.cards[idx].card = Card::SigilofSolaceB;

    // An instant is playable at action speed, so it is offered here.
    assert!(playable_cards(&gs).contains(&Card::SigilofSolaceB));

    let life_before = gs.p1.life;
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(idx))});
    assert_eq!(gs.phase, Phase::ActionInstant);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    // Resolved to the graveyard, one life richer. Rhinar starts at his full 20,
    // so this also pins down that there is no maximum life total to cap at.
    assert_eq!(gs.cards[idx].location, CardLocation::P1Graveyard);
    assert_eq!(life_before, 20);
    assert_eq!(gs.p1.life, life_before + 1);

    // An instant costs no action point, so Rhinar may still act.
    assert_eq!(gs.p1.action_points, 1);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
}

#[test]
fn sigil_of_solace_heals_only_its_owner() {
    let mut gs = setup_rhinar_action_phase();
    let (p1_before, p2_before) = (gs.p1.life, gs.p2.life);

    let effect = Card::SigilofSolaceB.data().play_effect.as_ref().unwrap();
    apply_on_play_effect(&mut gs, PlayerIndex::P2, effect);

    // The gain follows the resolving card's owner, leaving the opponent alone.
    assert_eq!(gs.p2.life, p2_before + 1);
    assert_eq!(gs.p1.life, p1_before);
}

// ─────────────────────────────────────────────────────────────────────────────
// Come to Fight (Card::ComeToFightB)
//
// "Your next attack action card you play this turn gains +1 power. Go again."
// A 1-cost blue action. Go again is the keyword; the +1 is banked on the player
// as `next_attack_action_bonus` and folded into the chain's power when an attack
// action card resolves combat damage. It is the class-agnostic sibling of
// Awakening Bellow's brute-only bonus: any attack action card takes it, but a
// weapon swing — not an attack action card — leaves it banked.
// ─────────────────────────────────────────────────────────────────────────────

/// Bank Come to Fight's +1 for p1 by resolving its on-play effect directly.
fn bank_come_to_fight_bonus() -> Gamestate {
    let mut gs = setup_rhinar_action_phase();
    let effect = Card::ComeToFightB
        .data()
        .play_effect
        .as_ref()
        .expect("Come to Fight should carry an on-play effect");
    apply_on_play_effect(&mut gs, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_attack_action_bonus, 1);
    gs
}

#[test]
fn come_to_fight_effect_is_next_attack_power() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    use crate::cards::Keyword;
    let data = Card::ComeToFightB.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!(data.cost, 1);
    assert_eq!(data.pitch, 3);
    assert_eq!(data.defense, 3);
    // "Go again" is the keyword half of the text.
    assert!(data.keyword.contains(Keyword::GoAgain));

    let effect = data.play_effect.as_ref().expect("Come to Fight should carry an on-play effect");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::NextAttackPower));
    assert_eq!(effect.magnitude, 1);
}

#[test]
fn come_to_fight_banks_plus1_for_the_next_attack() {
    let gs = bank_come_to_fight_bonus();
    assert_eq!(gs.p1.next_attack_action_bonus, 1);
    assert_eq!(gs.p2.next_attack_action_bonus, 0);
    // The brute-only bank is a separate pot and stays untouched.
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 0);
}

#[test]
fn come_to_fight_followup_generic_attack_gets_plus1() {
    let mut gs = bank_come_to_fight_bonus();

    // Muscle Mutt (generic, 6 power) against an undefended opponent.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
    assert_eq!(gs.cards[attacker].card.data().card_class, CardClass::Generic);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 6 + 1 = 7 damage, and the bonus is spent.
    assert_eq!(gs.p2.life, life_before - 7);
    assert_eq!(gs.p1.next_attack_action_bonus, 0);
}

#[test]
fn come_to_fight_followup_brute_attack_also_gets_plus1() {
    let mut gs = bank_come_to_fight_bonus();

    // Unlike Awakening Bellow's brute-only +3, this bonus is class-agnostic: a
    // brute attack action card takes it just the same.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    assert_eq!(gs.cards[attacker].card.data().card_class, CardClass::Brute);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 7);
    assert_eq!(gs.p1.next_attack_action_bonus, 0);
}

#[test]
fn come_to_fight_weapon_swing_does_not_get_plus1() {
    let mut gs = bank_come_to_fight_bonus();

    // Bone Basher is an attack, but a *weapon* rather than an attack action
    // card, so the text does not cover it. Seat it (4 power) and resolve.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BoneBasher);
    assert_eq!(gs.cards[attacker].card.data().typ, CardType::Weapon);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 4 damage, and the bonus stays banked for a later attack action this turn.
    assert_eq!(gs.p2.life, life_before - 4);
    assert_eq!(gs.p1.next_attack_action_bonus, 1);
}

#[test]
fn come_to_fight_bonus_does_not_survive_the_turn() {
    let mut gs = bank_come_to_fight_bonus();

    // "This turn": an unspent bonus is cleared at the turn boundary rather than
    // leaking into a later turn's attacks.
    begin_turn(&mut gs);
    assert_eq!(gs.p1.next_attack_action_bonus, 0);
    assert_eq!(gs.p2.next_attack_action_bonus, 0);
}

#[test]
fn come_to_fight_played_from_hand_banks_the_bonus_and_keeps_the_action_point() {
    let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut gs, false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Relabel Muscle Mutt to Come to Fight (cost 1); Rhinar's seed-42 opening
    // hand is then Come to Fight, Pack Call, Raging Onslaught, Clearing Bellow.
    let ctf_idx = gs.p1.hand_iter(&gs.cards)
            .find(|(_, cs)| cs.card == Card::MuscleMuttY)
            .map(|(idx, _)| idx)
            .expect("Muscle Mutt should be in the opening hand");
    gs.cards[ctf_idx].card = Card::ComeToFightB;

    // Cost 1 isn't covered outright, so it waits on a pitch.
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(ctf_idx))});
    assert_eq!(gs.phase, Phase::ActionPitch);

    // Clearing Bellow (pitch 3) covers it, committing Come to Fight to the stack.
    let cb_idx = gs.p1.hand_iter(&gs.cards)
            .find(|(_, cs)| cs.card == Card::ClearingBellowB)
            .map(|(idx, _)| idx)
            .expect("Clearing Bellow should be in the opening hand");
    step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});
    assert_eq!(gs.phase, Phase::ActionInstant);

    // Both pass: it resolves to the graveyard and banks the +1.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.cards[ctf_idx].location, CardLocation::P1Graveyard);
    assert_eq!(gs.p1.next_attack_action_bonus, 1);

    // Go Again: the action point survives, so the pumped attack can follow.
    assert_eq!(gs.p1.action_points, 1);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);

    // Raging Onslaught (generic, 6 power) then lands for 6 + 1 = 7.
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::RagingOnslaughtY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);
    assert_eq!(gs.p2.life, life_before - 7);
}

// ─────────────────────────────────────────────────────────────────────────────
// Dodge (Card::DodgeB) and Toughen Up (Card::ToughenUpB)
//
// Both are text-free blue defense reactions — Dodge blocks 2 for 0, Toughen Up
// blocks 4 for 2 — and both are covered here as a pair because everything that
// makes them work is the defense-reaction rules rather than card-specific
// effects:
//
//   * they are never declared as blockers in the defend step, whatever their
//     defense value (`legal_defend_phase` filters the card type out);
//   * only the defender is offered them, and only in the defend reaction step
//     (`is_defender_reaction_playable`), where they go on the stack like an
//     instant and can be responded to;
//   * a cost is paid by pitching, in the reaction window's own pitch phase —
//     Dodge is free and commits straight to the stack, Toughen Up's 2 has to be
//     pitched for;
//   * as the reaction resolves it joins the chain link the attack is on, so its
//     block adds to whatever was declared in the defend step, and it leaves for
//     the graveyard with the blockers when the chain closes.
// ─────────────────────────────────────────────────────────────────────────────

/// Assert `card` is a defense reaction with the given printed cost and defense
/// and carries no rules text at all: no keywords, no on-play effect, no
/// additional cost, no activated ability, and no defend / next-attack / target
/// effect. It blocks with its printed defense and nothing more.
fn assert_vanilla_defense_reaction(card: Card, cost: u8, defense: u8) {
    let data = card.data();
    assert_eq!(data.typ, CardType::DefenseReaction);
    assert_eq!(data.cost, cost);
    assert_eq!(data.defense, defense);
    assert_eq!(data.power, 0);
    assert!(data.keyword.is_empty(), "{:?} should carry no keywords", card);
    assert!(data.play_effect.is_none(), "{:?} should have no on-play effect", card);
    assert!(data.additional_cost.is_none(), "{:?} should have no additional cost", card);
    assert!(data.ability.is_none(), "{:?} should have no activated ability", card);
    assert!(data.defend_effect.is_none(), "{:?} should have no defend effect", card);
    assert!(data.next_attack_effect.is_none(), "{:?} should have no next-attack effect", card);
    assert!(data.target_effect.is_none(), "{:?} should have no target effect", card);
}

/// Rhinar (p1) attacks with Muscle Mutt (a vanilla 6-power generic attack) and
/// Dorinthea (p2) is on defense, in the Defend phase with the attack on link 0
/// of p1's chain. The starting point for every defense-reaction test below: 6
/// power against an untouched 20 life makes the arithmetic of a block obvious.
fn dorinthea_defending_muscle_mutt() -> Gamestate {
    let (gs, _) = play_and_resolve_attack(Card::MuscleMuttY);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert_eq!(gs.p2.life, 20);
    gs
}

/// Step from the Defend phase into the defend reaction step with the defender
/// (p2) holding priority: she declares no blockers, then the attacker passes
/// his first crack at the reaction window.
fn pass_to_defender_reaction(gs: &mut Gamestate) {
    step(gs, Action{ typ: ActionType::Pass, card: None}); // no blockers declared
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    step(gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes priority
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P2);
}

/// The cards offered to the active player as `Defend` actions (blockers).
fn blockable_cards(gs: &Gamestate) -> HashSet<Card> {
    legal_actions(gs).iter()
        .filter(|a| a.typ == ActionType::Defend)
        .map(|a| gs.cards[a.card_index()].card)
        .collect()
}

#[test]
fn dodge_is_a_vanilla_0_cost_2_block_defense_reaction() {
    assert_vanilla_defense_reaction(Card::DodgeB, 0, 2);
}

#[test]
fn toughen_up_is_a_vanilla_2_cost_4_block_defense_reaction() {
    assert_vanilla_defense_reaction(Card::ToughenUpB, 2, 4);
}

#[test]
fn defense_reactions_cannot_be_declared_as_blockers() {
    let mut gs = dorinthea_defending_muscle_mutt();

    // Dorinthea holds both defense reactions and one ordinary blocker.
    set_hand(&mut gs, PlayerIndex::P2,
        &[Card::DodgeB, Card::ToughenUpB, Card::DrivingBladeY]);

    // Neither reaction is a legal block, however much defense it prints: a
    // defense reaction is played in the reaction step, never declared here.
    let blocks = blockable_cards(&gs);
    assert!(!blocks.contains(&Card::DodgeB),
        "Dodge is a defense reaction and must not be offered as a block");
    assert!(!blocks.contains(&Card::ToughenUpB),
        "Toughen Up is a defense reaction and must not be offered as a block");
    // The ordinary attack action in the same hand still blocks, so the two are
    // excluded by their card type rather than the defend options having gone
    // empty.
    assert!(blocks.contains(&Card::DrivingBladeY));
}

#[test]
fn only_the_defender_is_offered_defense_reactions_in_the_reaction_step() {
    let mut gs = dorinthea_defending_muscle_mutt();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);

    // The attacker (p1) holds priority first. Hand him a defense reaction and
    // an attack reaction: only the attack reaction is his to play.
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::DodgeB, Card::InTheSwingR]);
    let attacker_options = playable_cards(&gs);
    assert!(!attacker_options.contains(&Card::DodgeB),
        "the attacking player must not be offered a defense reaction");
    assert!(attacker_options.contains(&Card::InTheSwingR),
        "the attacking player keeps his own attack reactions");

    // Priority passes to the defender, who is offered both of hers.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.active_player, PlayerIndex::P2);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::DodgeB, Card::ToughenUpB]);
    let defender_options = playable_cards(&gs);
    assert!(defender_options.contains(&Card::DodgeB));
    assert!(defender_options.contains(&Card::ToughenUpB));
}

#[test]
fn dodge_is_free_and_commits_straight_to_the_stack() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    set_hand(&mut gs, PlayerIndex::P2, &[Card::DodgeB]);
    let dodge_idx = gs.p2.hand_idx.expect("Dodge should be the defender's only card").get();

    // Cost 0: nothing to pitch for, so it goes on the stack and the window
    // stays open for a response rather than dropping into a pitch phase.
    assert_eq!(Card::DodgeB.data().cost, 0);
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(dodge_idx))});
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.cards[dodge_idx].location, CardLocation::Stack);
    assert!(!gs.stack_is_empty());
    assert_eq!(gs.p2.pitch_idx, None, "a free reaction pitches nothing");
}

#[test]
fn toughen_up_is_paid_for_by_pitching_in_the_reaction_window() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Two pitch-1 reds pay Toughen Up's cost of 2.
    assert_eq!(Card::ToughenUpB.data().cost, 2);
    assert_eq!(Card::SharpenSteelR.data().pitch, 1);
    set_hand(&mut gs, PlayerIndex::P2,
        &[Card::ToughenUpB, Card::SharpenSteelR, Card::SharpenSteelR]);
    let hand: Vec<usize> = gs.p2.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();

    // Playing it drops into the reaction window's own pitch phase; it is still
    // in hand (pending, not on the stack) until the cost is covered.
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(hand[0]))});
    assert_eq!(gs.phase, Phase::ReactionPitch);
    assert_eq!(gs.cards[hand[0]].location, CardLocation::P2Hand);

    // One pitch leaves a resource short, the second covers it and returns to
    // the reaction window with the card on the stack and the cost spent.
    step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(hand[1]))});
    assert_eq!(gs.phase, Phase::ReactionPitch);
    step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(hand[2]))});
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.cards[hand[0]].location, CardLocation::Stack);
    assert_eq!(gs.p2.resources, 0);
    assert_eq!(gs.cards[hand[1]].location, CardLocation::P2Pitch);
    assert_eq!(gs.cards[hand[2]].location, CardLocation::P2Pitch);
}

/// Play `card` (a 0-cost defense reaction) from the defender's hand in the
/// defend reaction step and resolve the window out: both players pass, so the
/// reaction resolves and, with the stack empty again, the window closes and
/// combat damage is dealt. Returns the game and the reaction's global index.
fn defender_reacts_with(gs: &mut Gamestate, card: Card) -> usize {
    assert_eq!(card.data().cost, 0, "helper only plays a free reaction");
    set_hand(gs, PlayerIndex::P2, &[card]);
    let idx = gs.p2.hand_idx.expect("the reaction should be the only card in hand").get();
    step(gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(idx))});
    step(gs, Action{ typ: ActionType::Pass, card: None});
    step(gs, Action{ typ: ActionType::Pass, card: None});
    idx
}

#[test]
fn dodge_adds_its_block_to_the_combat_chain() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);
    let dodge_idx = defender_reacts_with(&mut gs, Card::DodgeB);

    // Dodge resolved onto the defender's side of the link the attack is on,
    // rather than to her graveyard.
    assert_eq!(gs.cards[dodge_idx].location, CardLocation::P2CombatChain);
    assert_eq!(gs.p2.chain_link[0], Some(CardIdx::new(dodge_idx)));

    // Its 2 block came off Muscle Mutt's 6 power: 4 damage, not 6.
    assert_eq!(gs.p2.life, 20 - 4);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
}

#[test]
fn toughen_up_adds_its_block_to_the_combat_chain() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Give the defender the two resources up front so the reaction commits
    // straight to the stack; the pitching path is covered above.
    gs.p2.resources = 2;
    let tu_idx = {
        set_hand(&mut gs, PlayerIndex::P2, &[Card::ToughenUpB]);
        let idx = gs.p2.hand_idx.expect("Toughen Up should be the only card in hand").get();
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(idx))});
        assert_eq!(gs.phase, Phase::Reaction);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        idx
    };

    // 4 block against 6 power leaves 2 damage.
    assert_eq!(gs.cards[tu_idx].location, CardLocation::P2CombatChain);
    assert_eq!(gs.p2.chain_link[0], Some(CardIdx::new(tu_idx)));
    assert_eq!(gs.p2.life, 20 - 2);
}

#[test]
fn a_defense_reaction_stacks_with_a_declared_blocker() {
    let mut gs = dorinthea_defending_muscle_mutt();

    // Declare Driving Blade (defense 3) as a blocker in the defend step...
    set_hand(&mut gs, PlayerIndex::P2, &[Card::DrivingBladeY, Card::DodgeB]);
    let hand: Vec<usize> = gs.p2.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
    let (blocker_idx, dodge_idx) = (hand[0], hand[1]);
    assert_eq!(Card::DrivingBladeY.data().defense, 3);
    step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(blocker_idx))});
    assert_eq!(gs.cards[blocker_idx].location, CardLocation::P2CombatChain);

    // ...then add Dodge (2) from the same hand in the reaction step.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    assert_eq!(gs.active_player, PlayerIndex::P2);
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(dodge_idx))});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    // Both sit on the same chain link — the reaction on top of the blocker —
    // so their defense is summed: 3 + 2 = 5 against 6 power leaves 1 damage.
    assert_eq!(gs.p2.chain_link[0], Some(CardIdx::new(dodge_idx)));
    assert_eq!(gs.cards[dodge_idx].next_card, CardIdx::new(blocker_idx));
    assert_eq!(gs.p2.life, 20 - 1);
}

#[test]
fn a_defense_reaction_leaves_the_chain_for_the_graveyard_when_it_closes() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);
    let dodge_idx = defender_reacts_with(&mut gs, Card::DodgeB);
    assert_eq!(gs.cards[dodge_idx].location, CardLocation::P2CombatChain);

    // The turn player passes his action phase: the combat chain closes and
    // every card on it goes to its owner's graveyard, the reaction included.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Arsenal);
    assert_eq!(gs.cards[dodge_idx].location, CardLocation::P2Graveyard);
    assert_eq!(gs.p2.chain_link[0], None);
}

// ─────────────────────────────────────────────────────────────────────────────
// Defense reactions played from the arsenal
//
// A card in the arsenal is played as though it were in hand, so a defense
// reaction sitting there is available to the defender in the defend reaction
// step exactly as one in hand would be — paid for by pitching from hand (the
// arsenal card is not in the hand pool, so all of it can pay), and resolving
// onto the chain link the attack sits on.
// ─────────────────────────────────────────────────────────────────────────────

/// Move the head of `pid`'s hand into their (empty) arsenal, relabelled as
/// `card`, and return its global index. Mirrors what the Arsenal phase does at
/// the end of a turn, without having to play one out.
fn put_in_arsenal(gs: &mut Gamestate, pid: PlayerIndex, card: Card) -> usize {
    let idx = {
        let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
        assert!(player.arsenal_idx.is_none(), "the arsenal slot should be free");
        player.hand_idx.expect("hand should hold a card to arsenal").get()
    };
    gs.cards[idx].card = card;
    let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    detach_from_current_zone(player, &mut gs.cards, idx);
    gs.cards[idx].location = CardLocation::arsenal(pid);
    player.arsenal_idx = Some(CardIdx::new(idx));
    idx
}

#[test]
fn dodge_can_be_played_from_the_arsenal() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Dodge waits in the arsenal rather than the hand, and is free to play.
    let dodge_idx = put_in_arsenal(&mut gs, PlayerIndex::P2, Card::DodgeB);
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(dodge_idx))});
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.cards[dodge_idx].location, CardLocation::Stack);
    assert_eq!(gs.p2.arsenal_idx, None, "playing it empties the arsenal slot");

    // It blocks from the chain just as it would played from hand: 6 - 2 = 4.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.cards[dodge_idx].location, CardLocation::P2CombatChain);
    assert_eq!(gs.p2.chain_link[0], Some(CardIdx::new(dodge_idx)));
    assert_eq!(gs.p2.life, 20 - 4);
}

#[test]
fn toughen_up_from_the_arsenal_is_paid_for_by_pitching_the_whole_hand() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Two pitch-1 reds in hand, Toughen Up (cost 2) in the arsenal. The arsenal
    // card is not part of the hand's pitch pool, so both hand cards are free to
    // pay for it.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::SharpenSteelR, Card::SharpenSteelR]);
    let tu_idx = put_in_arsenal(&mut gs, PlayerIndex::P2, Card::ToughenUpB);
    let hand: Vec<usize> = gs.p2.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
    assert_eq!(hand.len(), 1, "one of the two reds went to the arsenal slot");

    // One pitch-1 card can't cover a cost of 2, so it isn't offered yet.
    let offered: Vec<usize> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::PlayCard)
        .map(|a| a.card_index())
        .collect();
    assert!(!offered.contains(&tu_idx));

    // Bank a resource and it becomes payable with the single card in hand.
    gs.p2.resources = 1;
    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(tu_idx))});
    assert_eq!(gs.phase, Phase::ReactionPitch);
    assert_eq!(gs.cards[tu_idx].location, CardLocation::P2Arsenal,
        "it stays in the arsenal until the cost is covered");
    step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(hand[0]))});
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.cards[tu_idx].location, CardLocation::Stack);
    assert_eq!(gs.p2.arsenal_idx, None);
    assert_eq!(gs.p2.resources, 0);

    // 4 block against Muscle Mutt's 6 power leaves 2 damage.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.cards[tu_idx].location, CardLocation::P2CombatChain);
    assert_eq!(gs.p2.life, 20 - 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Titanium Bauble (Card::TitaniumBaubleB)
//
// A blue *resource* card: cost 0, pitch 3, 0 power, 3 defense, no rules text.
// It is the catalog's only `CardType::Resource`, and what makes it work is a
// rule rather than an effect — a resource is never played. It contributes the
// two ways any card can without being put on the stack: pitched from hand for
// its 3 resources, or declared as a blocker for its 3 defense. The tests below
// pin both halves: that it is offered in neither the action, instant nor
// reaction window, from hand or from the arsenal (`can_ever_be_played`), and
// that it still pitches and blocks for its printed 3.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn titanium_bauble_is_a_vanilla_3_pitch_3_block_resource() {
    use crate::cards::Color;
    let data = Card::TitaniumBaubleB.data();
    assert_eq!(data.typ, CardType::Resource);
    assert_eq!(data.cost, 0);
    assert_eq!(data.pitch, 3);
    assert_eq!(data.power, 0);
    assert_eq!(data.defense, 3);
    assert_eq!(data.card_class, CardClass::Generic);
    assert!(matches!(data.color, Some(Color::Blue)));
    // Nothing about it is an effect — it has no text at all.
    assert!(!data.no_block, "a resource with defense still blocks");
    assert!(data.keyword.is_empty(), "Titanium Bauble should carry no keywords");
    assert!(data.play_effect.is_none(), "Titanium Bauble should have no on-play effect");
    assert!(data.additional_cost.is_none(), "Titanium Bauble should have no additional cost");
    assert!(data.ability.is_none(), "Titanium Bauble should have no activated ability");
    assert!(data.defend_effect.is_none(), "Titanium Bauble should have no defend effect");
    assert!(data.next_attack_effect.is_none(), "Titanium Bauble should have no next-attack effect");
    assert!(data.target_effect.is_none(), "Titanium Bauble should have no target effect");
    assert!(data.constant_effect.is_none(), "Titanium Bauble should have no constant effect");
}

#[test]
fn titanium_bauble_is_never_offered_as_a_play_from_hand() {
    let mut gs = setup_rhinar_action_phase();
    // A 0-cost action alongside it, so the action window is demonstrably live
    // and affordability is not what rules the bauble out.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::TitaniumBaubleB, Card::ClearingBellowB]);
    assert_eq!(gs.p1.action_points, 1);

    let playable = playable_cards(&gs);
    assert!(playable.contains(&Card::ClearingBellowB),
        "the action window should be offering the 0-cost action next to it");
    assert!(!playable.contains(&Card::TitaniumBaubleB),
        "a resource card must never be offered as a play");
}

#[test]
fn titanium_bauble_is_never_offered_as_a_play_from_the_arsenal() {
    let mut gs = setup_rhinar_action_phase();

    // Park a 0-cost action in the arsenal: it is offered from there, exactly as
    // Dodge is in the reaction tests above.
    let idx = put_in_arsenal(&mut gs, PlayerIndex::P1, Card::ClearingBellowB);
    let offered: Vec<usize> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::PlayCard)
        .map(|a| a.card_index())
        .collect();
    assert!(offered.contains(&idx), "an arsenal card of the right speed is offered");

    // Relabel that same arsenal slot to Titanium Bauble — nothing else about the
    // game changes — and the offer disappears. The type is what rules it out.
    gs.cards[idx].card = Card::TitaniumBaubleB;
    let offered: Vec<usize> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::PlayCard)
        .map(|a| a.card_index())
        .collect();
    assert!(!offered.contains(&idx),
        "a resource card must never be offered as a play from the arsenal either");
}

#[test]
fn titanium_bauble_is_not_offered_in_the_defender_reaction_window() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Dodge is free and legal here, so the reaction window is live.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::TitaniumBaubleB, Card::DodgeB]);
    let playable = playable_cards(&gs);
    assert!(playable.contains(&Card::DodgeB),
        "the defender's reaction window should be offering Dodge next to it");
    assert!(!playable.contains(&Card::TitaniumBaubleB),
        "a resource card is not a reaction and must not be offered in the reaction step");
}

#[test]
fn titanium_bauble_pitches_for_3() {
    let mut gs = setup_rhinar_action_phase();
    // Muscle Mutt costs exactly 3, so the bauble's pitch covers it on its own.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::MuscleMuttY, Card::TitaniumBaubleB]);
    let hand: Vec<usize> = gs.p1.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
    let (mutt_idx, bauble_idx) = (hand[0], hand[1]);
    assert_eq!(Card::MuscleMuttY.data().cost, 3);

    step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mutt_idx))});
    assert_eq!(gs.phase, Phase::ActionPitch);

    // Pitching the bauble pays the whole cost in one card: the attack leaves the
    // stack for the reaction window with nothing left over.
    step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(bauble_idx))});
    assert_eq!(gs.phase, Phase::ActionInstant,
        "3 pitch covers a cost of 3, so no further pitching is asked for");
    assert_eq!(gs.cards[bauble_idx].location, CardLocation::P1Pitch);
    assert_eq!(gs.p1.resources, 0, "all 3 went into the cost, none floats");
}

#[test]
fn titanium_bauble_blocks_for_3() {
    let mut gs = dorinthea_defending_muscle_mutt();
    set_hand(&mut gs, PlayerIndex::P2, &[Card::TitaniumBaubleB]);
    let bauble_idx = gs.p2.hand_idx.expect("the bauble should be the only card in hand").get();

    // Unlike a defense reaction, a resource is declared as an ordinary blocker
    // in the defend step.
    assert!(blockable_cards(&gs).contains(&Card::TitaniumBaubleB),
        "a resource with defense is a legal blocker");
    step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(bauble_idx))});
    assert_eq!(gs.cards[bauble_idx].location, CardLocation::P2CombatChain);

    // Its 3 block came off Muscle Mutt's 6 power: 3 damage, not 6.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // defender passes
    assert_eq!(gs.p2.life, 20 - 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Pack Call (Card::PackCallY)
//
// "When you defend with Pack Call, reveal the top card of your deck. If it has
// 6 or more power, put it on top of your deck. Otherwise, put it on the
// bottom." A 6-power brute attack action that also carries the catalog's first
// `defend_effect`: a trigger that fires as the card is *declared* as a blocker
// (`apply_defend_effect` in `commit_blocker`), not at damage.
//
// "Your deck" is the deck of whoever defends with it — which is never the turn
// player, so the tests below block with it as p2 and check that p1's deck is
// left alone.
// ─────────────────────────────────────────────────────────────────────────────

/// The order of `pid`'s deck, top first, as global card indices. Walks the
/// linked list to its tail terminator (a node whose `next_card` points at
/// itself), the convention `attach_to_bottom_of_deck` maintains.
fn deck_order(gs: &Gamestate, pid: PlayerIndex) -> Vec<usize> {
    let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    let mut out = Vec::new();
    let mut cur = player.top_deck_idx.map(|i| i.get());
    while let Some(idx) = cur {
        out.push(idx);
        let next = gs.cards[idx].next_card.get();
        cur = if next == idx { None } else { Some(next) };
    }
    out
}

/// Relabel the card on top of `pid`'s deck to `card`, returning its index. The
/// deck is left in place — only the identity of its top card changes.
fn set_top_of_deck(gs: &mut Gamestate, pid: PlayerIndex, card: Card) -> usize {
    let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    let idx = player.top_deck_idx.expect("deck should not be empty").get();
    gs.cards[idx].card = card;
    idx
}

/// Declare `card` as p2's sole blocker in the Defend phase, returning its index.
fn declare_sole_blocker(gs: &mut Gamestate, card: Card) -> usize {
    set_hand(gs, PlayerIndex::P2, &[card]);
    let idx = gs.p2.hand_idx.expect("the blocker should be the only card in hand").get();
    step(gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(idx))});
    idx
}

#[test]
fn pack_call_is_a_6_power_brute_attack_with_a_defend_trigger() {
    let data = Card::PackCallY.data();
    assert_eq!(data.typ, CardType::AttackAction);
    assert_eq!(data.cost, 3);
    assert_eq!(data.power, 6);
    assert_eq!(data.defense, 3);
    assert_eq!(data.card_class, CardClass::Brute);
    assert!(data.keyword.is_empty(), "Pack Call should carry no keywords");
    // Its whole text is the defend trigger — nothing happens when it is played.
    assert!(matches!(data.defend_effect, Some(DefendEffect::Reveal6BottomOtherwise)));
    assert!(data.play_effect.is_none(), "Pack Call should have no on-play effect");
    assert!(data.additional_cost.is_none(), "Pack Call should have no additional cost");
    assert!(data.ability.is_none(), "Pack Call should have no activated ability");
}

#[test]
fn pack_call_keeps_a_power6_card_on_top() {
    let mut gs = dorinthea_defending_muscle_mutt();
    // Wounded Bull sits exactly on the 6-power boundary, which is inclusive.
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::WoundedBullY);
    assert_eq!(Card::WoundedBullY.data().power, 6);
    let before = deck_order(&gs, PlayerIndex::P2);

    declare_sole_blocker(&mut gs, Card::PackCallY);

    assert_eq!(gs.p2.top_deck_idx, Some(CardIdx::new(top)),
        "a 6-power card stays on top");
    assert_eq!(deck_order(&gs, PlayerIndex::P2), before,
        "the deck order is untouched when the top card has 6 or more power");
}

#[test]
fn pack_call_bottoms_a_sub_power6_card() {
    let mut gs = dorinthea_defending_muscle_mutt();
    // Rally the Rearguard has 4 power — under the line, so it is bottomed.
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    assert_eq!(Card::RallyTheRearguardB.data().power, 4);
    let before = deck_order(&gs, PlayerIndex::P2);
    let size_before = gs.p2.deck_size;

    declare_sole_blocker(&mut gs, Card::PackCallY);

    let after = deck_order(&gs, PlayerIndex::P2);
    assert_eq!(*after.last().expect("deck is non-empty"), top,
        "a sub-6-power card goes to the bottom");
    assert_eq!(gs.p2.bottom_deck_idx, Some(CardIdx::new(top)),
        "the bottom pointer follows it");
    assert_ne!(gs.p2.top_deck_idx, Some(CardIdx::new(top)));

    // The deck was reordered, not resized: the rest keeps its order, with the
    // old top moved from the front to the back.
    assert_eq!(gs.p2.deck_size, size_before);
    assert_eq!(after.len(), before.len());
    let mut expected = before[1..].to_vec();
    expected.push(top);
    assert_eq!(after, expected);
}

#[test]
fn pack_call_reveals_its_own_controllers_deck() {
    let mut gs = dorinthea_defending_muscle_mutt();
    // The defender (p2) is not the turn player, so wiring the reveal to the
    // wrong side would show up here.
    set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let attacker_deck_before = deck_order(&gs, PlayerIndex::P1);
    let defender_deck_before = deck_order(&gs, PlayerIndex::P2);

    declare_sole_blocker(&mut gs, Card::PackCallY);

    assert_eq!(deck_order(&gs, PlayerIndex::P1), attacker_deck_before,
        "the attacker's deck must not be touched");
    assert_ne!(deck_order(&gs, PlayerIndex::P2), defender_deck_before,
        "the defender's own deck is the one revealed from");
}

#[test]
fn pack_call_reveal_is_public_whichever_way_it_goes() {
    // Kept on top: revealing it tells both players what is there.
    let mut gs = dorinthea_defending_muscle_mutt();
    let kept = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::WoundedBullY);
    gs.cards[kept].visible = CardVisibleState::Hidden;
    declare_sole_blocker(&mut gs, Card::PackCallY);
    assert_eq!(gs.cards[kept].visible, CardVisibleState::BothKnow);

    // Bottomed: it stays known there, as a face-up pitched card does.
    let mut gs = dorinthea_defending_muscle_mutt();
    let bottomed = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    gs.cards[bottomed].visible = CardVisibleState::Hidden;
    declare_sole_blocker(&mut gs, Card::PackCallY);
    assert_eq!(gs.cards[bottomed].visible, CardVisibleState::BothKnow);
}

#[test]
fn pack_call_on_an_empty_deck_does_nothing() {
    let mut gs = dorinthea_defending_muscle_mutt();
    // A decked player can still declare blockers, so an empty deck is a legal
    // state for the trigger to fire in — it must no-op rather than panic.
    gs.p2.top_deck_idx = None;
    gs.p2.bottom_deck_idx = None;
    gs.p2.deck_size = 0;

    let pc_idx = declare_sole_blocker(&mut gs, Card::PackCallY);

    assert_eq!(gs.p2.deck_size, 0);
    assert_eq!(gs.p2.top_deck_idx, None);
    assert_eq!(gs.p2.bottom_deck_idx, None);
    // The block itself still happened.
    assert_eq!(gs.cards[pc_idx].location, CardLocation::P2CombatChain);
}

#[test]
fn a_blocker_without_a_defend_trigger_leaves_the_deck_alone() {
    let mut gs = dorinthea_defending_muscle_mutt();
    // The same sub-6-power top card that Pack Call would bottom.
    set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let before = deck_order(&gs, PlayerIndex::P2);

    // Driving Blade has no defend effect, so declaring it changes nothing.
    assert!(Card::DrivingBladeY.data().defend_effect.is_none());
    declare_sole_blocker(&mut gs, Card::DrivingBladeY);

    assert_eq!(deck_order(&gs, PlayerIndex::P2), before,
        "only a card with a defend trigger disturbs the deck");
}

#[test]
fn pack_call_fires_once_for_each_declared_blocker() {
    let mut gs = dorinthea_defending_muscle_mutt();

    // Two sub-6-power cards on top, so each Pack Call bottoms one in turn.
    let order = deck_order(&gs, PlayerIndex::P2);
    let (first, second) = (order[0], order[1]);
    gs.cards[first].card = Card::RallyTheRearguardB;
    gs.cards[second].card = Card::RallyTheRearguardB;

    set_hand(&mut gs, PlayerIndex::P2, &[Card::PackCallY, Card::PackCallY]);
    let hand: Vec<usize> = gs.p2.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
    step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(hand[0]))});
    step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(hand[1]))});

    // Each declaration fired its own trigger, in declaration order: the deck
    // has rotated by two, both revealed cards now at the bottom in that order.
    let after = deck_order(&gs, PlayerIndex::P2);
    assert_eq!(after[after.len() - 2], first);
    assert_eq!(after[after.len() - 1], second);
    let mut expected = order[2..].to_vec();
    expected.push(first);
    expected.push(second);
    assert_eq!(after, expected);
}

#[test]
fn pack_call_still_blocks_for_3() {
    let mut gs = dorinthea_defending_muscle_mutt();
    set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let pc_idx = declare_sole_blocker(&mut gs, Card::PackCallY);
    assert_eq!(gs.cards[pc_idx].location, CardLocation::P2CombatChain);

    // Its 3 block comes off Muscle Mutt's 6 power: 3 damage, not 6. The trigger
    // is a bonus on top of an ordinary block, not a replacement for it.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // defender passes
    assert_eq!(gs.p2.life, 20 - 3);
}

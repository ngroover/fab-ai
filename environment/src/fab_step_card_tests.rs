//! Per-card unit tests for on-play effects.
//!
//! Declared as a child module of `fab_step` (via `#[path]` in `fab_step.rs`),
//! so `use super::*` reaches `fab_step`'s private helpers — notably
//! `apply_on_play_effect`, `detach_from_current_zone`, and
//! `attach_to_front_of_zone` — exactly as the main `tests` module does.
//!
//! Cards covered here:
//!   - Bare Fangs     (`Card::BareFangsR`)
//!   - Wild Ride      (`Card::WildRideR`)
//!   - Alpha Rampage  (`Card::AlphaRampageR`)
//!   - Wrecker Romp   (`Card::WreckerRompB`)

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

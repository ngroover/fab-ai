//! Per-card unit tests for on-play effects.
//!
//! Declared as a child module of `fab_step` (via `#[path]` in `fab_step.rs`),
//! so `use super::*` reaches `fab_step`'s private helpers — notably
//! `apply_on_play_effect`, `detach_from_current_zone`, and
//! `attach_to_front_of_zone` — exactly as the main `tests` module does.
//!
//! Cards covered here:
//!   - Bare Fangs (`Card::BareFangsR`)
//!   - Wild Ride  (`Card::WildRideR`)

use super::*;
use crate::cards::Card;
use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
use crate::fab_game::{gamestate_from_decklists, reset};

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

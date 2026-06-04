use crate::action::{Action,ActionType};
use crate::game_state::{Gamestate,Phase,Player,PendingCard,CardLocation,CardVisibleState,CardState,TOTAL_CARDS};
use crate::cards::CardData;
use crate::classic_battles::get_card_catalog;

pub fn step(gs: &mut Gamestate, act: Action) {
    match gs.phase {
        Phase::ChooseFirst => handle_choose_first(gs, act),
        Phase::Action => handle_action_phase(gs, act),
        _ => {}
    }
}

fn handle_choose_first(gs: &mut Gamestate, act: Action) {
    // only need to switch the player if they choose go second
    if act.typ == ActionType::ChooseSecond {
        // xor to flip the player
        gs.active_player = gs.active_player ^ 1;
    }
    // begin turn logic
    gs.phase = Phase::Action;
    // The cards live in the shared `gs.cards` array; draw each player's opening
    // hand by passing that array alongside the player and its id.
    draw_to_intellect(&mut gs.p1, &mut gs.cards, 0);
    draw_to_intellect(&mut gs.p2, &mut gs.cards, 1);
}

fn handle_action_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        // Playing a card or activating equipment/a weapon both commit a card
        // that must then be paid for. The played/activated card goes onto the
        // stack, then we record it as pending and move to the Pitch phase where
        // the player pitches to cover its cost.
        ActionType::PlayCard | ActionType::Activate => {
            // Cost of committing this card, read while it still sits in its
            // source zone (the location distinguishes a weapon attack / played
            // card from an activated armor ability).
            let catalog = get_card_catalog();
            let cs = gs.cards[act.index];
            let cost = action_cost(cs.location, &catalog[cs.card as usize]);

            // Borrow the active player and the shared cards/stack head as
            // disjoint fields so we can move the card onto the stack in one step.
            let player = if gs.active_player == 0 { &mut gs.p1 } else { &mut gs.p2 };
            // If banked resources already cover the cost there is nothing to
            // pitch for, so skip straight to the Instant phase; otherwise the
            // player still has to pitch to pay, in the Pitch phase.
            let already_paid = player.resources >= cost;
            detach_from_current_zone(player, &mut gs.cards, act.index);
            gs.cards[act.index].location = CardLocation::Stack;
            attach_to_front_of_zone(&mut gs.cards, &mut gs.stack_idx, None, None, act.index);
            gs.pending_card = Some(PendingCard {
                index: act.index,
                typ: act.typ,
            });
            gs.phase = if already_paid { Phase::Instant } else { Phase::Pitch };
        }
        // Passing ends the action phase; nothing is pending.
        ActionType::Pass => {
            gs.pending_card = None;
        }
        _ => {}
    }
}

/// Resource cost of committing a card from `location`. Activating a worn armor
/// piece costs its ability's resource cost; everything else (playing a card
/// from hand, or a weapon attack) costs the card's own `cost`. Mirrors the
/// affordability checks in `legal_actions`.
fn action_cost(location: CardLocation, data: &CardData) -> u8 {
    match location {
        CardLocation::P1Head | CardLocation::P2Head
        | CardLocation::P1Chest | CardLocation::P2Chest
        | CardLocation::P1Arms | CardLocation::P2Arms
        | CardLocation::P1Legs | CardLocation::P2Legs => {
            data.ability.as_ref().map_or(0, |a| a.resource_cost())
        }
        _ => data.cost,
    }
}

/// Prepend the card at global index `idx` to the front of a linked-list zone,
/// making it the new `head`. The old head (if any) is linked as `idx`'s
/// successor; an empty zone makes `idx` its own tail terminator (`next_card`
/// points at itself).
///
/// `head` is the zone's head index (e.g. `hand_idx`, `pitch_idx`, or the stack
/// head). `bottom`, when supplied, is the zone's tail pointer and is set to
/// `idx` only when the zone was previously empty. `count`, when supplied, is the
/// zone's size counter and is incremented. The caller is responsible for setting
/// the card's `location`.
fn attach_to_front_of_zone(
    cards: &mut [CardState; TOTAL_CARDS],
    head: &mut Option<u8>,
    bottom: Option<&mut Option<u8>>,
    count: Option<&mut u8>,
    idx: usize,
) {
    if let Some(old_head) = *head {
        cards[idx].next_card = old_head;
        cards[old_head as usize].prev_card = idx as u8;
    } else {
        // Empty zone: the card becomes the sole node, its own tail, and the
        // zone's bottom.
        cards[idx].next_card = idx as u8;
        if let Some(bottom) = bottom {
            *bottom = Some(idx as u8);
        }
    }
    *head = Some(idx as u8);
    if let Some(count) = count {
        *count += 1;
    }
}

/// Remove the card at global index `idx` from the bookkeeping of its current
/// location so it can be placed elsewhere. The card's `location` already encodes
/// its owner, so the same `player` (its owner) handles both `P1*` and `P2*`
/// variants. Linked-list zones (hand, deck, pitch) are relinked via
/// `detach_from_linked_list`; the weapon and equipment slots are single indices
/// that simply clear.
fn detach_from_current_zone(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS], idx: usize) {
    let location = cards[idx].location;
    match location {
        CardLocation::P1Hand | CardLocation::P2Hand => {
            detach_from_linked_list(
                cards,
                &mut player.hand_idx,
                None,
                Some(&mut player.hand_size),
                idx,
            );
        }
        CardLocation::P1Deck | CardLocation::P2Deck => {
            // The deck tracks both ends and a count; let the helper fix them all.
            detach_from_linked_list(
                cards,
                &mut player.top_deck_idx,
                Some(&mut player.bottom_deck_idx),
                Some(&mut player.deck_size),
                idx,
            );
        }
        CardLocation::P1Pitch | CardLocation::P2Pitch => {
            detach_from_linked_list(cards, &mut player.pitch_idx, None, None, idx);
        }
        CardLocation::P1Weapon | CardLocation::P2Weapon => player.weapon_idx = None,
        CardLocation::P1Head | CardLocation::P2Head => player.head_idx = None,
        CardLocation::P1Chest | CardLocation::P2Chest => player.chest_idx = None,
        CardLocation::P1Arms | CardLocation::P2Arms => player.arms_idx = None,
        CardLocation::P1Legs | CardLocation::P2Legs => player.legs_idx = None,
        CardLocation::P1Arsenal | CardLocation::P2Arsenal => player.arsenal_idx = None,
        CardLocation::P1BanishZone | CardLocation::P2BanishZone => player.banish_idx = None,
        _ => {}
    }
}

/// Unlink the card at global index `idx` from a doubly-linked list of
/// `CardState`s, fixing up the `head` pointer, the neighbours' links and the
/// tail terminator (a node whose `next_card` points at itself).
///
/// `head` is the zone's head index (e.g. `hand_idx` or `pitch_idx`). `bottom`,
/// when supplied, is the zone's tail pointer and is updated when the removed
/// card was the tail (or the only card). `count`, when supplied, is the zone's
/// size counter and is decremented.
fn detach_from_linked_list(
    cards: &mut [CardState; TOTAL_CARDS],
    head: &mut Option<u8>,
    bottom: Option<&mut Option<u8>>,
    count: Option<&mut u8>,
    idx: usize,
) {
    let next = cards[idx].next_card as usize;
    let is_head = *head == Some(idx as u8);
    let is_tail = next == idx;

    if is_head && is_tail {
        // Only card in the list; both ends clear.
        *head = None;
        if let Some(bottom) = bottom {
            *bottom = None;
        }
    } else if is_head {
        // Head of a multi-card list; the next card becomes the new head. The
        // tail is unchanged.
        *head = Some(next as u8);
    } else {
        // Non-head node always has a valid prev_card.
        let prev = cards[idx].prev_card as usize;
        if is_tail {
            // Removing the tail: prev becomes the new tail (points to itself).
            cards[prev].next_card = prev as u8;
            if let Some(bottom) = bottom {
                *bottom = Some(prev as u8);
            }
        } else {
            // Middle node: splice prev and next together.
            cards[prev].next_card = next as u8;
            cards[next].prev_card = prev as u8;
        }
    }
    if let Some(count) = count {
        *count -= 1;
    }
}

fn draw_to_intellect(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS], pid: u8) {
    let need = (player.intellect - player.hand_size).max(0) as usize;
    draw_cards(player, cards, pid, need);
}

fn draw_cards(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS], pid: u8, num: usize) {
    let mut drawn = 0;
    while drawn < num {
        // Re-read the top each iteration: drawing a card updates `top_deck_idx`,
        // and it becomes `None` once the (formerly last) card is drawn.
        let Some(current_idx) = player.top_deck_idx.map(|x| x as usize) else {
            break; // deck is empty
        };
        move_from_deck_to_hand(player, cards, pid, current_idx);
        drawn += 1;
    }
}

fn move_from_deck_to_hand(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS], pid: u8, card_idx : usize) {
    // Pull the card off the deck (updates top/bottom pointers and deck_size),
    // then prepend it to the hand and mark it as known to its owner.
    detach_from_current_zone(player, cards, card_idx);
    cards[card_idx].location = CardLocation::hand(pid);
    cards[card_idx].visible = if pid == 0 {
        CardVisibleState::P1Knows
    } else {
        CardVisibleState::P2Knows
    };
    attach_to_front_of_zone(
        cards,
        &mut player.hand_idx,
        None,
        Some(&mut player.hand_size),
        card_idx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset,get_card_states_from_location};
    use crate::cards::Card;
    use crate::legal_actions::legal_actions;

    #[test]
    fn test_go_first_step() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        assert_eq!(gs.active_player, 0);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        assert_eq!(gs.active_player, 0);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_go_second_step() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        assert_eq!(gs.active_player, 0);

        let go_second = Action{ typ: ActionType::ChooseSecond, index : 0};
        step(&mut gs, go_second);

        assert_eq!(gs.active_player, 1);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_play_card_moves_to_pitch() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);
        assert_eq!(gs.phase, Phase::Action);

        // Play Muscle Mutt (cost 3). With no banked resources the player can't
        // afford it outright, so the pending card is recorded and the phase
        // advances to Pitch to pay for it.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        let play = Action{ typ: ActionType::PlayCard, index: mm_idx};
        step(&mut gs, play);

        assert_eq!(gs.phase, Phase::Pitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index, mm_idx);
        assert_eq!(pending.typ, ActionType::PlayCard);
    }

    #[test]
    fn test_play_affordable_card_moves_to_instant() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);
        assert_eq!(gs.phase, Phase::Action);

        // Clearing Bellow costs 0, so the player already has enough resources
        // (zero) to pay for it. Committing it should skip pitching and advance
        // straight to the Instant phase, while still recording it as pending on
        // the stack.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");

        let actions = legal_actions(&gs);
        let play = actions.into_iter()
                .find(|a| a.typ == ActionType::PlayCard && a.index == cb_idx)
                .expect("playing Clearing Bellow should be a legal action");

        step(&mut gs, play);

        assert_eq!(gs.phase, Phase::Instant);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index, cb_idx);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::Stack);
        assert_eq!(gs.stack_idx, Some(cb_idx as u8));
    }

    #[test]
    fn test_play_card_moves_to_stack() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        let hand_idx = gs.p1.hand_idx.unwrap() as usize;
        let hand_size_before = gs.p1.hand_size;
        let play = Action{ typ: ActionType::PlayCard, index: hand_idx};
        step(&mut gs, play);

        // The played card sits on top of the stack and has been removed from the
        // hand.
        assert_eq!(gs.stack_idx, Some(hand_idx as u8));
        assert_eq!(gs.cards[hand_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.hand_size, hand_size_before - 1);
        // The hand linked list no longer contains the played card.
        assert!(gs.p1.hand_iter(&gs.cards).all(|(idx, _)| idx != hand_idx));
    }

    #[test]
    fn test_activate_weapon_moves_to_stack() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        let weapon_idx = gs.p1.weapon_idx.unwrap() as usize;
        let activate = Action{ typ: ActionType::Activate, index: weapon_idx};
        step(&mut gs, activate);

        // The activated weapon is now on top of the stack and the weapon slot has
        // been vacated.
        assert_eq!(gs.stack_idx, Some(weapon_idx as u8));
        assert_eq!(gs.cards[weapon_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.weapon_idx, None);
    }

    #[test]
    fn test_activate_card_moves_to_pitch() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Activate the equipped weapon. Same flow: record pending, go to Pitch.
        let weapon_idx = gs.p1.weapon_idx.unwrap() as usize;
        let activate = Action{ typ: ActionType::Activate, index: weapon_idx};
        step(&mut gs, activate);

        assert_eq!(gs.phase, Phase::Pitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index, weapon_idx);
        assert_eq!(pending.typ, ActionType::Activate);
    }

    #[test]
    fn test_play_packcall() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);
        assert_eq!(gs.phase, Phase::Action);

        // Seed 42 deals Rhinar an opening hand containing Pack Call. Locate its
        // slot, then find the legal PlayCard action that targets it.
        let packcall_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::PackCallY)
                .map(|(idx, _)| idx)
                .expect("Pack Call should be in the opening hand");

        let actions = legal_actions(&gs);
        let play = actions.into_iter()
                .find(|a| a.typ == ActionType::PlayCard && a.index == packcall_idx)
                .expect("playing Pack Call should be a legal action");

        step(&mut gs, play);

        // Pack Call is now the pending card, committed via PlayCard, and the
        // game has advanced to the Pitch phase to pay for it.
        assert_eq!(gs.phase, Phase::Pitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index, packcall_idx);
        assert_eq!(pending.typ, ActionType::PlayCard);
        assert_eq!(gs.cards[pending.index].card, Card::PackCallY);
    }

    #[test]
    fn test_pass_clears_pending_card() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        let pass = Action{ typ: ActionType::Pass, index: 0};
        step(&mut gs, pass);

        assert_eq!(gs.pending_card, None);
    }

    #[test]
    fn test_initial_hand() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        assert_eq!(gs.p1.hand_size, 4);
        assert_eq!(gs.p1.deck_size, 36);
        assert_eq!(gs.p2.hand_size, 4);
        assert_eq!(gs.p2.deck_size, 36);

        let hand = get_card_states_from_location(&gs, 0, CardLocation::P1Hand);

        assert_eq!(hand.len(), 4);

        assert_eq!(hand[0].card, Card::MuscleMuttY);
        assert_eq!(hand[1].card, Card::PackCallY);
        assert_eq!(hand[2].card, Card::RagingOnslaughtY);
        assert_eq!(hand[3].card, Card::ClearingBellowB);

        let hand2 = get_card_states_from_location(&gs, 1, CardLocation::P2Hand);

        assert_eq!(hand2.len(), 4);

        assert_eq!(hand2[0].card, Card::InTheSwingR);
        assert_eq!(hand2[1].card, Card::SecondSwingR);
        assert_eq!(hand2[2].card, Card::SharpenSteelR);
        assert_eq!(hand2[3].card, Card::DrivingBladeY);
    }

    #[test]
    fn test_draw_cards_includes_last_card() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // After the opening draw p1 has 36 cards left in the deck.
        let deck_before = gs.p1.deck_size;
        assert_eq!(deck_before, 36);
        let hand_before = gs.p1.hand_size;

        // Draw the entire deck, including the final (tail) card.
        draw_cards(&mut gs.p1, &mut gs.cards, 0, deck_before as usize);

        // The whole deck moved to hand; nothing is left behind and the head
        // pointer is cleared.
        assert_eq!(gs.p1.deck_size, 0);
        assert_eq!(gs.p1.top_deck_idx, None);
        assert_eq!(gs.p1.bottom_deck_idx, None);
        assert_eq!(gs.p1.hand_size, hand_before + deck_before);

        // Drawing further from an empty deck is a safe no-op.
        draw_cards(&mut gs.p1, &mut gs.cards, 0, 5);
        assert_eq!(gs.p1.deck_size, 0);
        assert_eq!(gs.p1.hand_size, hand_before + deck_before);
    }
}

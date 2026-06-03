use crate::action::{Action,ActionType};
use crate::game_state::{Gamestate,Phase,Player,PendingCard,CardLocation,CardVisibleState,CardState};
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
    for player in [&mut gs.p1, &mut gs.p2] {
        draw_to_intellect(player);
    }
}

fn handle_action_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        // Playing a card or activating equipment/a weapon both commit a card
        // that must then be paid for. The attacking card/weapon goes onto the
        // first link of the combat chain, then we record it as pending and move
        // to the Pitch phase where the player pitches to cover its cost.
        ActionType::PlayCard | ActionType::Activate => {
            let player = active_player_mut(gs);
            detach_from_current_zone(player, act.index);
            player.cards[act.index].location = CardLocation::CombatChain;
            attach_to_front_of_zone(&mut player.cards, &mut player.chain_link[0], act.index);
            gs.pending_card = Some(PendingCard {
                index: act.index,
                typ: act.typ,
            });
            gs.phase = Phase::Pitch;
        }
        // Passing ends the action phase; nothing is pending.
        ActionType::Pass => {
            gs.pending_card = None;
        }
        _ => {}
    }
}

/// Mutable reference to the player whose turn it is.
fn active_player_mut(gs: &mut Gamestate) -> &mut Player {
    if gs.active_player == 0 { &mut gs.p1 } else { &mut gs.p2 }
}

/// Prepend the card at `idx` to the front of a linked-list zone, making it the
/// new `head`. The old head (if any) is linked as `idx`'s successor; an empty
/// zone makes `idx` its own tail terminator (`next_card` points at itself).
/// `head` is the zone's head index (e.g. `hand_idx`, `pitch_idx`, or a combat
/// chain link); the caller is responsible for setting the card's `location`.
fn attach_to_front_of_zone(cards: &mut [CardState; 45], head: &mut Option<u8>, idx: usize) {
    if let Some(old_head) = *head {
        cards[idx].next_card = old_head;
        cards[old_head as usize].prev_card = idx as u8;
    } else {
        // Empty zone: the card becomes the sole node and its own tail.
        cards[idx].next_card = idx as u8;
    }
    *head = Some(idx as u8);
}

/// Remove the card at `idx` from the bookkeeping of its current location so it
/// can be placed elsewhere. Linked-list zones (hand, pitch) are relinked via
/// `detach_from_linked_list`; the weapon and equipment slots are single indices
/// that simply clear.
fn detach_from_current_zone(player: &mut Player, idx: usize) {
    match player.cards[idx].location {
        CardLocation::Hand => {
            detach_from_linked_list(&mut player.cards, &mut player.hand_idx, idx);
            player.hand_size -= 1;
        }
        CardLocation::Pitch => {
            detach_from_linked_list(&mut player.cards, &mut player.pitch_idx, idx);
        }
        CardLocation::Weapon => player.weapon_idx = None,
        CardLocation::Head => player.head_idx = None,
        CardLocation::Chest => player.chest_idx = None,
        CardLocation::Arms => player.arms_idx = None,
        CardLocation::Legs => player.legs_idx = None,
        CardLocation::Arsenal => player.arsenal_idx = None,
        CardLocation::BanishZone => player.banish_idx = None,
        _ => {}
    }
}

/// Unlink the card at `idx` from a doubly-linked list of `CardState`s, fixing up
/// the `head` pointer, the neighbours' links and the tail terminator (a node
/// whose `next_card` points at itself). `head` is the zone's head index (e.g.
/// `hand_idx` or `pitch_idx`); pass whichever zone the card lives in.
fn detach_from_linked_list(cards: &mut [CardState; 45], head: &mut Option<u8>, idx: usize) {
    let next = cards[idx].next_card as usize;
    let is_head = *head == Some(idx as u8);
    let is_tail = next == idx;

    if is_head && is_tail {
        // Only card in the list.
        *head = None;
    } else if is_head {
        // Head of a multi-card list; the next card becomes the new head.
        *head = Some(next as u8);
    } else {
        // Non-head node always has a valid prev_card.
        let prev = cards[idx].prev_card as usize;
        if is_tail {
            // Removing the tail: prev becomes the new tail (points to itself).
            cards[prev].next_card = prev as u8;
        } else {
            // Middle node: splice prev and next together.
            cards[prev].next_card = next as u8;
            cards[next].prev_card = prev as u8;
        }
    }
}

fn draw_to_intellect(player: &mut Player) {
    let need = (player.intellect - player.hand_size).max(0) as usize;
    draw_cards(player, need);
}

fn draw_cards(player: &mut Player, num: usize) {
    let catalog = get_card_catalog();
    if let Some(mut current_idx) = player.top_deck_idx.map(|x| x as usize) {
        let mut drawn = 0;
        loop {
            let mycard = player.cards[current_idx].card;

            let next = player.cards[current_idx].next_card as usize;
            if next == current_idx ||
                drawn == num {
                break;
            }
            move_from_deck_to_hand(player, current_idx);

            current_idx = next;
            drawn += 1;
        }
    }
}

fn move_from_deck_to_hand(player: &mut Player, card_idx : usize) {
    let next_card_on_deck = player.cards[card_idx].next_card;
    // modify deck
    if next_card_on_deck == card_idx as u8 {
        // the deck has no more cards
        player.top_deck_idx = None;
        player.bottom_deck_idx = None;
    }
    else {
        // assign top_deck_idx to next card
        player.top_deck_idx = Some(next_card_on_deck);
        player.cards[next_card_on_deck as usize].prev_card = next_card_on_deck
    }

    // modify hand
    if let Some(hand_idx) = player.hand_idx {
        // cards exist in hand; assign current card to point to hand
        player.cards[card_idx as usize].next_card = hand_idx as u8;
        // assign hand card to point backwards to current card
        player.cards[hand_idx as usize].prev_card = card_idx as u8;
    }
    else {
        player.cards[card_idx as usize].next_card = card_idx as u8;
    }
    player.hand_idx = Some(card_idx as u8);

    player.cards[card_idx].location = CardLocation::Hand;
    player.cards[card_idx].visible = CardVisibleState::SelfKnows;
    player.hand_size += 1;
    player.deck_size -= 1;
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

        // Play the first card in hand. The pending card should be recorded and
        // the phase advanced to Pitch.
        let hand_idx = gs.p1.hand_idx.unwrap() as usize;
        let play = Action{ typ: ActionType::PlayCard, index: hand_idx};
        step(&mut gs, play);

        assert_eq!(gs.phase, Phase::Pitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index, hand_idx);
        assert_eq!(pending.typ, ActionType::PlayCard);
    }

    #[test]
    fn test_play_card_moves_to_combat_chain() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        let hand_idx = gs.p1.hand_idx.unwrap() as usize;
        let hand_size_before = gs.p1.hand_size;
        let play = Action{ typ: ActionType::PlayCard, index: hand_idx};
        step(&mut gs, play);

        // The played card sits on the first link of the combat chain and has
        // been removed from the hand.
        assert_eq!(gs.p1.chain_link[0], Some(hand_idx as u8));
        assert_eq!(gs.p1.cards[hand_idx].location, CardLocation::CombatChain);
        assert_eq!(gs.p1.hand_size, hand_size_before - 1);
        // The hand linked list no longer contains the played card.
        assert!(gs.p1.hand_iter().all(|(idx, _)| idx != hand_idx));
    }

    #[test]
    fn test_activate_weapon_moves_to_combat_chain() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        let weapon_idx = gs.p1.weapon_idx.unwrap() as usize;
        let activate = Action{ typ: ActionType::Activate, index: weapon_idx};
        step(&mut gs, activate);

        // The activated weapon is now the first link of the combat chain and the
        // weapon slot has been vacated.
        assert_eq!(gs.p1.chain_link[0], Some(weapon_idx as u8));
        assert_eq!(gs.p1.cards[weapon_idx].location, CardLocation::CombatChain);
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
        let packcall_idx = gs.p1.hand_iter()
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
        assert_eq!(gs.p1.cards[pending.index].card, Card::PackCallY);
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

        let hand = get_card_states_from_location(&gs.p1, CardLocation::Hand);

        assert_eq!(hand.len(), 4);
        
        assert_eq!(hand[0].card, Card::MuscleMuttY);
        assert_eq!(hand[1].card, Card::PackCallY);
        assert_eq!(hand[2].card, Card::RagingOnslaughtY);
        assert_eq!(hand[3].card, Card::ClearingBellowB);

        let hand2 = get_card_states_from_location(&gs.p2, CardLocation::Hand);

        assert_eq!(hand2.len(), 4);
        
        assert_eq!(hand2[0].card, Card::InTheSwingR);
        assert_eq!(hand2[1].card, Card::SecondSwingR);
        assert_eq!(hand2[2].card, Card::SharpenSteelR);
        assert_eq!(hand2[3].card, Card::DrivingBladeY);
    }
}

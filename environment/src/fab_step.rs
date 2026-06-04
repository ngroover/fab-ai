use crate::action::{Action,ActionType};
use crate::game_state::{Gamestate,Phase,Player,PendingCard,CardLocation,CardVisibleState,CardState,TOTAL_CARDS};
use crate::cards::CardData;
use crate::classic_battles::get_card_catalog;

pub fn step(gs: &mut Gamestate, act: Action) {
    match gs.phase {
        Phase::ChooseFirst => handle_choose_first(gs, act),
        Phase::Action => handle_action_phase(gs, act),
        Phase::Pitch => handle_pitch_phase(gs, act),
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
        // that must then be paid for. The card is recorded as `pending_card`
        // (still sitting in its source zone, not yet on the stack). If banked
        // resources already cover its cost it is committed to the stack and the
        // Instant phase immediately; otherwise it stays pending and we drop into
        // the Pitch phase to pitch for the rest.
        ActionType::PlayCard | ActionType::Activate => {
            // Cost of committing this card, read while it still sits in its
            // source zone (the location distinguishes a weapon attack / played
            // card from an activated armor ability).
            let catalog = get_card_catalog();
            let cs = gs.cards[act.index];
            let cost = action_cost(cs.location, &catalog[cs.card as usize]);

            let player = if gs.active_player == 0 { &gs.p1 } else { &gs.p2 };
            let already_paid = player.resources >= cost;

            gs.pending_card = Some(PendingCard {
                index: act.index,
                typ: act.typ,
            });

            // Affordable outright: no pitching needed, commit to the stack now.
            // Otherwise leave it pending (off the stack) and pitch for it.
            if already_paid {
                commit_pending_to_stack(gs);
            } else {
                gs.phase = Phase::Pitch;
            }
        }
        // Passing ends the action phase; nothing is pending.
        ActionType::Pass => {
            gs.pending_card = None;
        }
        _ => {}
    }
}

/// Handle a pitch during the Pitch phase. The pitched card is moved from the
/// active player's hand into their pitch zone and its pitch value is banked as
/// resources. Once banked resources cover the pending card's cost, that card is
/// committed to the stack and the game advances to the Instant phase.
fn handle_pitch_phase(gs: &mut Gamestate, act: Action) {
    if act.typ != ActionType::Pitch {
        return;
    }

    let catalog = get_card_catalog();
    let pid = gs.active_player;
    let pitch_val = catalog[gs.cards[act.index].card as usize].pitch;

    // Move the pitched card out of the hand and into the pitch zone, banking the
    // resources it produces.
    let player = if pid == 0 { &mut gs.p1 } else { &mut gs.p2 };
    detach_from_current_zone(player, &mut gs.cards, act.index);
    gs.cards[act.index].location = CardLocation::pitch(pid);
    attach_to_front_of_zone(&mut gs.cards, &mut player.pitch_idx, None, None, act.index);
    player.resources += pitch_val;
    let resources = player.resources;

    // Cost still owed on the pending card; once we can cover it, commit it.
    let pending = gs.pending_card.expect("pitch phase requires a pending card");
    let pcs = gs.cards[pending.index];
    let cost = action_cost(pcs.location, &catalog[pcs.card as usize]);
    if resources >= cost {
        commit_pending_to_stack(gs);
    }
}

/// Move the pending card onto the stack, pay its cost from the active player's
/// banked resources, and advance to the Instant phase. Shared by both the
/// affordable path (straight from the Action phase) and the Pitch phase (once
/// enough has been pitched). The card is detached from its source zone — the
/// hand for a played card, the weapon/armor slot for an activation — and
/// prepended to the stack. Assumes the player can already cover the cost.
fn commit_pending_to_stack(gs: &mut Gamestate) {
    let Some(pending) = gs.pending_card else {
        return;
    };

    // Recompute the cost from the card's still-current source location, mirroring
    // the affordability check that let us get here.
    let catalog = get_card_catalog();
    let cs = gs.cards[pending.index];
    let cost = action_cost(cs.location, &catalog[cs.card as usize]);

    let player = if gs.active_player == 0 { &mut gs.p1 } else { &mut gs.p2 };
    player.resources -= cost;
    detach_from_current_zone(player, &mut gs.cards, pending.index);
    gs.cards[pending.index].location = CardLocation::Stack;
    attach_to_front_of_zone(&mut gs.cards, &mut gs.stack_idx, None, None, pending.index);

    // The card now lives on the stack, so it is no longer "pending" — clear it
    // before opening the Instant phase.
    gs.pending_card = None;
    gs.phase = Phase::Instant;
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

        // The pending card is held off the stack and still sits in the hand
        // until it is actually paid for.
        assert_eq!(gs.stack_idx, None);
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1Hand);
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
        // straight to the Instant phase, moving the card onto the stack and
        // clearing the pending slot.
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
        // Once the card hits the stack it is no longer pending.
        assert_eq!(gs.pending_card, None);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::Stack);
        assert_eq!(gs.stack_idx, Some(cb_idx as u8));
    }

    #[test]
    fn test_pitch_commits_played_card_to_stack() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Play Muscle Mutt (cost 3) with no banked resources: it becomes pending
        // and we drop into the Pitch phase, with nothing on the stack yet.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, index: mm_idx});
        assert_eq!(gs.phase, Phase::Pitch);
        assert_eq!(gs.stack_idx, None);

        // Pitch Clearing Bellow (pitch 3) — exactly covers the cost. The pending
        // card is committed to the stack, the cost is paid (3 - 3 = 0 resources
        // left), and the game advances to the Instant phase.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, index: cb_idx});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.stack_idx, Some(mm_idx as u8));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.resources, 0);
        // The pitched card now lives in the pitch zone, not the hand.
        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Pitch);
    }

    #[test]
    fn test_pitch_below_cost_keeps_card_pending() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Play Muscle Mutt (cost 3); pitching a single yellow attack action only
        // banks 2 resources, short of the cost, so the card stays pending and we
        // remain in the Pitch phase.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, index: mm_idx});

        let pc_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::PackCallY)
                .map(|(idx, _)| idx)
                .expect("Pack Call should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, index: pc_idx});

        assert_eq!(gs.phase, Phase::Pitch);
        assert_eq!(gs.stack_idx, None);
        assert_eq!(gs.p1.resources, 2);
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1Hand);
        assert_eq!(gs.pending_card.expect("still pending").index, mm_idx);

        // Pitching a second yellow attack action banks a total of 4, now enough
        // to pay the cost of 3 and commit the card.
        let ro_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::RagingOnslaughtY)
                .map(|(idx, _)| idx)
                .expect("Raging Onslaught should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, index: ro_idx});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.stack_idx, Some(mm_idx as u8));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.resources, 1);
    }

    #[test]
    fn test_pitch_commits_activated_weapon_to_stack() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Activate Bone Basher (cost 2): pending, off the stack, Pitch phase.
        let weapon_idx = gs.p1.weapon_idx.unwrap() as usize;
        step(&mut gs, Action{ typ: ActionType::Activate, index: weapon_idx});
        assert_eq!(gs.phase, Phase::Pitch);
        assert_eq!(gs.stack_idx, None);
        assert_eq!(gs.p1.weapon_idx, Some(weapon_idx as u8));

        // Pitch Clearing Bellow (pitch 3) to cover the cost of 2. The weapon is
        // committed to the stack, its slot vacated, and 1 resource is left over.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, index: cb_idx});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.stack_idx, Some(weapon_idx as u8));
        assert_eq!(gs.cards[weapon_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.weapon_idx, None);
        assert_eq!(gs.p1.resources, 1);
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

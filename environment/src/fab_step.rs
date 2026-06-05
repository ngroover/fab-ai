use crate::action::{Action,ActionType};
use crate::game_state::{Gamestate,Phase,Player,PendingCard,CardIdx,CardLocation,CardVisibleState,CardState,PLAYER_CARDS,TOTAL_CARDS};
use crate::cards::{CardData,CardType};
use crate::classic_battles::get_card_catalog;

pub fn step(gs: &mut Gamestate, act: Action) {
    match gs.phase {
        Phase::ChooseFirst => handle_choose_first(gs, act),
        Phase::Action => handle_action_phase(gs, act),
        Phase::Pitch => handle_pitch_phase(gs, act),
        Phase::Instant => handle_instant_phase(gs, act),
        Phase::Defend => handle_defend_phase(gs, act),
        _ => {}
    }
}

fn handle_choose_first(gs: &mut Gamestate, act: Action) {
    // only need to switch the player if they choose go second
    if act.typ == ActionType::ChooseSecond {
        // xor to flip the player
        gs.active_player = gs.active_player ^ 1;
    }
    // The player who goes first owns this turn. `turn_player` tracks them
    // independently of `active_player`, which will ping-pong as priority passes
    // during the Instant phase.
    gs.turn_player = gs.active_player;
    // begin turn logic
    gs.phase = Phase::Action;
    // The cards live in the shared `gs.cards` array; draw each player's opening
    // hand by passing that array alongside the player and its id.
    draw_to_intellect(&mut gs.p1, &mut gs.cards);
    draw_to_intellect(&mut gs.p2, &mut gs.cards);
}

fn handle_action_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        // Playing a card, or activating an ability (an armor piece or a weapon
        // swing) both commit a card that must then be paid for (see
        // `commit_card_to_pending`).
        ActionType::PlayCard | ActionType::Activate => {
            commit_card_to_pending(gs, act);
        }
        // Passing ends the action phase; nothing is pending.
        ActionType::Pass => {
            gs.pending_card = None;
        }
        _ => {}
    }
}

/// Handle an action during the Instant phase. Each player in turn may play
/// instants — committed exactly like an action-phase play (`commit_card_to_pending`)
/// — for as long as they keep the priority. Passing hands priority to the other
/// player; once both have passed in succession, the top of the stack resolves
/// (see `handle_instant_pass`).
fn handle_instant_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        // Only instants are legal here (see `legal_instant_phase`); they commit
        // through the same pending/pitch flow as an action-phase play.
        ActionType::PlayCard => {
            commit_card_to_pending(gs, act);
        }
        ActionType::Pass => {
            handle_instant_pass(gs);
        }
        _ => {}
    }
}

/// Commit a card the active player has chosen to play, activate, or attack with.
/// The card is recorded as `pending_card` (still sitting in its source zone, not
/// yet on the stack). If banked resources already cover its cost it is committed
/// straight to the stack (advancing to the Instant phase); otherwise it stays
/// pending and we drop into the Pitch phase to pitch for the rest. Shared by the
/// Action and Instant phases.
fn commit_card_to_pending(gs: &mut Gamestate, act: Action) {
    // Cost of committing this card: an Activate (an armor ability or a weapon
    // swing) pays its ability's cost; a played card pays its own catalog cost.
    let catalog = get_card_catalog();
    let card_idx = act.card.expect("play/activate action requires a card");
    let cs = gs.cards[card_idx.get()];
    let cost = action_cost(act.typ, &catalog[cs.card as usize]);

    let player = if gs.active_player == 0 { &gs.p1 } else { &gs.p2 };
    let already_paid = player.resources >= cost;

    // Bank the phase and active player to restore once the Instant phase ends,
    // but only for a fresh play from the Action phase — a card committed in
    // response during the Instant phase must not clobber what the original
    // Action-phase play stored. An attack action or weapon swing heads for the
    // Defend phase with the non-turn player (the defender) active; any other
    // played card returns to the Action phase with the turn player active.
    // Recorded here, before any drop into the Pitch phase masks the origin.
    if gs.phase == Phase::Action {
        if commits_as_attack(act.typ, &catalog[cs.card as usize]) {
            gs.return_after_instant = Phase::Defend;
            gs.player_after_instant = gs.turn_player ^ 1;
        } else {
            gs.return_after_instant = Phase::Action;
            gs.player_after_instant = gs.turn_player;
        }
    }

    gs.pending_card = Some(PendingCard {
        index: card_idx,
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

/// Handle a pass during the Instant phase, tracked by `gs.passes`. Each pass
/// hands priority to the other player and bumps the consecutive-pass count; the
/// count is reset to 0 whenever a card is played onto the stack (see
/// `commit_pending_to_stack`), so it only reaches 2 when both players pass in
/// succession with no card played between. On that second pass the top card of
/// the stack resolves (see `resolve_top_of_stack`) and the count resets.
fn handle_instant_pass(gs: &mut Gamestate) {
    gs.passes += 1;
    if gs.passes >= 2 {
        // Both players have now passed in succession: resolve the top of the
        // stack and reset the pass count. The resolution itself decides where
        // priority and the phase go next, since that depends on the card type.
        gs.passes = 0;
        resolve_top_of_stack(gs);
    } else {
        // First pass: hand priority to the other player, who may now respond.
        gs.active_player ^= 1;
    }
}

/// Handle an action during the Defend phase. The active player is the defender
/// (flipped to the opponent of the attacker in `resolve_top_of_stack`). They may
/// commit blockers one at a time — each chosen card moves out of their hand onto
/// the next free link of their own combat chain, and the phase stays on Defend so
/// further blockers can be declared. Passing finishes declaring blockers and
/// advances to the Reaction phase.
fn handle_defend_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        ActionType::Defend => commit_blocker(gs, act.card_index()),
        ActionType::Pass => gs.phase = Phase::Reaction,
        _ => {}
    }
}

/// Move the defender's chosen blocker out of their hand and onto the combat
/// chain. All blockers declared against the current attack join a *single* chain
/// link (link 0 of the defender's chain), chained together through
/// `next_card`/`prev_card` just like the hand, deck, and pitch zones — so the one
/// link can hold any number of blocking cards. The defender is the active player;
/// the attacker's card sits on the *attacker's* chain, a separate array.
fn commit_blocker(gs: &mut Gamestate, idx: usize) {
    let pid = gs.active_player;
    let player = if pid == 0 { &mut gs.p1 } else { &mut gs.p2 };

    detach_from_current_zone(player, &mut gs.cards, idx);
    gs.cards[idx].location = CardLocation::combat_chain(pid);
    attach_to_front_of_zone(&mut gs.cards, &mut player.chain_link[0], None, None, idx);
}

/// Resolve the card at the top of the stack (its most recently added card),
/// detaching it from the stack and routing it by the action that committed it
/// (carried on the `PendingCard`). The owner is implied by which half of the
/// shared `cards` array the slot falls in. A no-op when the stack is empty.
///
/// - A **played attack action card**, or a **weapon swing** (the weapon card
///   itself), moves onto its owner's combat chain at link 0, the opponent
///   becomes the active player to defend, and we enter the Defend phase.
/// - Anything **else** resolves to its owner's graveyard. Priority returns to
///   the turn player, who resumes the Action phase once the stack is empty, or
///   keeps responding in the Instant phase while cards remain.
fn resolve_top_of_stack(gs: &mut Gamestate) {
    let Some(pending) = gs.pop_stack() else {
        return;
    };
    let top = pending.index.get();
    let owner = if top < PLAYER_CARDS { 0 } else { 1 };

    let catalog = get_card_catalog();
    let data = &catalog[gs.cards[top].card as usize];

    // A card joins its owner's combat chain when it is attacking: a played
    // attack action card, or a weapon being swung (the weapon itself joins the
    // chain). Everything else resolves to the graveyard.
    if commits_as_attack(pending.typ, data) {
        // The attacking card leaves the stack for link 0 of its owner's combat
        // chain. Leaving the Instant phase, we restore the phase and active
        // player banked when the card was committed (Defend, with the non-turn
        // player active to declare blocks).
        gs.cards[top].location = CardLocation::combat_chain(owner);
        let attacker = if owner == 0 { &mut gs.p1 } else { &mut gs.p2 };
        attacker.chain_link[0] = Some(CardIdx::new(top));
        gs.active_player = gs.player_after_instant;
        gs.phase = gs.return_after_instant;
    } else {
        gs.cards[top].location = CardLocation::graveyard(owner);
        if gs.stack_is_empty() {
            // The stack is empty, so the Instant phase ends: restore the phase
            // and active player banked when the resolved card was committed.
            gs.active_player = gs.player_after_instant;
            gs.phase = gs.return_after_instant;
        } else {
            // Cards remain on the stack: priority returns to the turn player for
            // a fresh round of responses and we stay in the Instant phase.
            gs.active_player = gs.turn_player;
            gs.phase = Phase::Instant;
        }
    }
}

/// Whether committing `typ` on `data` puts an attack on the stack — a played
/// attack action card, or a weapon being swung. Such a card resolves onto its
/// owner's combat chain and sends the game to the Defend phase; every other
/// committed card resolves to the graveyard and returns to the Action phase.
fn commits_as_attack(typ: ActionType, data: &CardData) -> bool {
    match typ {
        ActionType::PlayCard => matches!(data.typ, CardType::AttackAction),
        // An Activate covers both armor abilities and weapon swings; only a
        // weapon's activation is an attack action that joins the combat chain,
        // so it is distinguished here by the ability's activation card type.
        ActionType::Activate => data
            .ability
            .as_ref()
            .map(|a| matches!(a.card_type(), CardType::AttackAction))
            .unwrap_or(false),
        _ => false,
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
    let card_idx = act.card_index();
    let pitch_val = catalog[gs.cards[card_idx].card as usize].pitch;

    // Move the pitched card out of the hand and into the pitch zone, banking the
    // resources it produces.
    let player = if pid == 0 { &mut gs.p1 } else { &mut gs.p2 };
    detach_from_current_zone(player, &mut gs.cards, card_idx);
    gs.cards[card_idx].location = CardLocation::pitch(pid);
    attach_to_front_of_zone(&mut gs.cards, &mut player.pitch_idx, None, None, card_idx);
    player.resources += pitch_val;
    let resources = player.resources;

    // Cost still owed on the pending card; once we can cover it, commit it.
    let pending = gs.pending_card.expect("pitch phase requires a pending card");
    let pcs = gs.cards[pending.index.get()];
    let cost = action_cost(pending.typ, &catalog[pcs.card as usize]);
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

    // Recompute the cost from the pending action's type, mirroring the
    // affordability check that let us get here.
    let catalog = get_card_catalog();
    let pending_idx = pending.index.get();
    let cs = gs.cards[pending_idx];
    let cost = action_cost(pending.typ, &catalog[cs.card as usize]);

    let player = if gs.active_player == 0 { &mut gs.p1 } else { &mut gs.p2 };
    player.resources -= cost;
    detach_from_current_zone(player, &mut gs.cards, pending_idx);
    gs.cards[pending_idx].location = CardLocation::Stack;
    gs.push_to_stack(pending);

    // The card now lives on the stack, so it is no longer "pending" — clear it
    // before opening the Instant phase. A new layer landing on the stack also
    // interrupts any pending resolution, so the consecutive-pass count resets.
    gs.pending_card = None;
    gs.passes = 0;
    gs.phase = Phase::Instant;
}

/// Resource cost of committing a card for the given action. An `Activate` (an
/// armor ability or a weapon swing) pays the card's activated ability's resource
/// cost — an Activate is always done on a card that carries an ability, so the
/// ability is expected to be present. Every other action (playing a card from
/// hand) pays the card's own catalog `cost`. Mirrors the affordability checks in
/// `legal_actions`.
fn action_cost(typ: ActionType, data: &CardData) -> u8 {
    match typ {
        ActionType::Activate => data
            .ability
            .as_ref()
            .expect("Activate action requires a card with an ability")
            .resource_cost(),
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
    head: &mut Option<CardIdx>,
    bottom: Option<&mut Option<CardIdx>>,
    count: Option<&mut u8>,
    idx: usize,
) {
    if let Some(old_head) = *head {
        cards[idx].next_card = old_head;
        cards[old_head.get()].prev_card = CardIdx::new(idx);
    } else {
        // Empty zone: the card becomes the sole node, its own tail, and the
        // zone's bottom.
        cards[idx].next_card = CardIdx::new(idx);
        if let Some(bottom) = bottom {
            *bottom = Some(CardIdx::new(idx));
        }
    }
    *head = Some(CardIdx::new(idx));
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
    head: &mut Option<CardIdx>,
    bottom: Option<&mut Option<CardIdx>>,
    count: Option<&mut u8>,
    idx: usize,
) {
    let next = cards[idx].next_card.get();
    let is_head = *head == Some(CardIdx::new(idx));
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
        *head = Some(CardIdx::new(next));
    } else {
        // Non-head node always has a valid prev_card.
        let prev = cards[idx].prev_card.get();
        if is_tail {
            // Removing the tail: prev becomes the new tail (points to itself).
            cards[prev].next_card = CardIdx::new(prev);
            if let Some(bottom) = bottom {
                *bottom = Some(CardIdx::new(prev));
            }
        } else {
            // Middle node: splice prev and next together.
            cards[prev].next_card = CardIdx::new(next);
            cards[next].prev_card = CardIdx::new(prev);
        }
    }
    if let Some(count) = count {
        *count -= 1;
    }
}

fn draw_to_intellect(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS]) {
    // Draw back up to intellect. `saturating_sub` guards the unsigned subtraction:
    // if the hand is already at or over intellect there is nothing to draw (a
    // plain `-` would underflow-panic, and `.max(0)` on a `u8` is a no-op).
    let need = player.intellect.saturating_sub(player.hand_size) as usize;
    draw_cards(player, cards, need);
}

fn draw_cards(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS], num: usize) {
    let mut drawn = 0;
    while drawn < num {
        // Re-read the top each iteration: drawing a card updates `top_deck_idx`,
        // and it becomes `None` once the (formerly last) card is drawn.
        let Some(current_idx) = player.top_deck_idx.map(|x| x.get()) else {
            break; // deck is empty
        };
        move_from_deck_to_hand(player, cards, current_idx);
        drawn += 1;
    }
}

fn move_from_deck_to_hand(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS], card_idx : usize) {
    // Pull the card off the deck (updates top/bottom pointers and deck_size),
    // then prepend it to the hand and mark it as known to its owner.
    let pid = player.pid;
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

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        assert_eq!(gs.active_player, 0);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_go_second_step() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        assert_eq!(gs.active_player, 0);

        let go_second = Action{ typ: ActionType::ChooseSecond, card: None};
        step(&mut gs, go_second);

        assert_eq!(gs.active_player, 1);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_play_card_moves_to_pitch() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);
        assert_eq!(gs.phase, Phase::Action);

        // Play Muscle Mutt (cost 3). With no banked resources the player can't
        // afford it outright, so the pending card is recorded and the phase
        // advances to Pitch to pay for it.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        let play = Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))};
        step(&mut gs, play);

        assert_eq!(gs.phase, Phase::Pitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index.get(), mm_idx);
        assert_eq!(pending.typ, ActionType::PlayCard);

        // The pending card is held off the stack and still sits in the hand
        // until it is actually paid for.
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1Hand);
    }

    #[test]
    fn test_play_affordable_card_moves_to_instant() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
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
                .find(|a| a.typ == ActionType::PlayCard && a.card == Some(CardIdx::new(cb_idx)))
                .expect("playing Clearing Bellow should be a legal action");

        step(&mut gs, play);

        assert_eq!(gs.phase, Phase::Instant);
        // Once the card hits the stack it is no longer pending.
        assert_eq!(gs.pending_card, None);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::Stack);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(cb_idx));
    }

    #[test]
    fn test_pitch_commits_played_card_to_stack() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Play Muscle Mutt (cost 3) with no banked resources: it becomes pending
        // and we drop into the Pitch phase, with nothing on the stack yet.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});
        assert_eq!(gs.phase, Phase::Pitch);
        assert_eq!(gs.stack_top(), None);

        // Pitch Clearing Bellow (pitch 3) — exactly covers the cost. The pending
        // card is committed to the stack, the cost is paid (3 - 3 = 0 resources
        // left), and the game advances to the Instant phase.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(mm_idx));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.resources, 0);
        // The pitched card now lives in the pitch zone, not the hand.
        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Pitch);
    }

    #[test]
    fn test_pitch_below_cost_keeps_card_pending() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Play Muscle Mutt (cost 3); pitching a single yellow attack action only
        // banks 2 resources, short of the cost, so the card stays pending and we
        // remain in the Pitch phase.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});

        let pc_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::PackCallY)
                .map(|(idx, _)| idx)
                .expect("Pack Call should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(pc_idx))});

        assert_eq!(gs.phase, Phase::Pitch);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.p1.resources, 2);
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1Hand);
        assert_eq!(gs.pending_card.expect("still pending").index.get(), mm_idx);

        // Pitching a second yellow attack action banks a total of 4, now enough
        // to pay the cost of 3 and commit the card.
        let ro_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::RagingOnslaughtY)
                .map(|(idx, _)| idx)
                .expect("Raging Onslaught should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(ro_idx))});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(mm_idx));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.resources, 1);
    }

    #[test]
    fn test_pitch_commits_weapon_attack_to_stack() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Swing Bone Basher (ability cost 2): pending, off the stack, Pitch phase.
        let weapon_idx = gs.p1.weapon_idx.unwrap().get();
        step(&mut gs, Action{ typ: ActionType::Activate, card: Some(CardIdx::new(weapon_idx))});
        assert_eq!(gs.phase, Phase::Pitch);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.p1.weapon_idx, Some(CardIdx::new(weapon_idx)));

        // Pitch Clearing Bellow (pitch 3) to cover the cost of 2. The weapon is
        // committed to the stack, its slot vacated, and 1 resource is left over.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(weapon_idx));
        assert_eq!(gs.cards[weapon_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.weapon_idx, None);
        assert_eq!(gs.p1.resources, 1);
    }

    #[test]
    fn test_attack_card_moves_to_pitch() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Swing the equipped weapon. Same flow: record pending, go to Pitch.
        let weapon_idx = gs.p1.weapon_idx.unwrap().get();
        let attack = Action{ typ: ActionType::Activate, card: Some(CardIdx::new(weapon_idx))};
        step(&mut gs, attack);

        assert_eq!(gs.phase, Phase::Pitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index.get(), weapon_idx);
        assert_eq!(pending.typ, ActionType::Activate);
    }

    #[test]
    fn test_play_packcall() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
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
                .find(|a| a.typ == ActionType::PlayCard && a.card == Some(CardIdx::new(packcall_idx)))
                .expect("playing Pack Call should be a legal action");

        step(&mut gs, play);

        // Pack Call is now the pending card, committed via PlayCard, and the
        // game has advanced to the Pitch phase to pay for it.
        assert_eq!(gs.phase, Phase::Pitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index.get(), packcall_idx);
        assert_eq!(pending.typ, ActionType::PlayCard);
        assert_eq!(gs.cards[pending.index.get()].card, Card::PackCallY);
    }

    /// Drive a game to the Instant phase: Rhinar (p1) goes first, plays Clearing
    /// Bellow (cost 0, so it commits straight to the stack) and we land in the
    /// Instant phase with p1 as both turn and active player. Returns the gamestate
    /// and the stack slot the committed card occupies.
    fn instant_phase_with_one_card() -> (Gamestate, usize) {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(cb_idx))});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.turn_player, 0);
        assert_eq!(gs.active_player, 0);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(cb_idx));
        (gs, cb_idx)
    }

    #[test]
    fn test_choose_first_sets_turn_player() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);
        assert_eq!(gs.active_player, 0);

        // Going second flips both the active player and, since they own the turn,
        // the turn player along with it.
        step(&mut gs, Action{ typ: ActionType::ChooseSecond, card: None});
        assert_eq!(gs.active_player, 1);
        assert_eq!(gs.turn_player, 1);
    }

    #[test]
    fn test_instant_pass_gives_priority_to_opponent() {
        let (mut gs, cb_idx) = instant_phase_with_one_card();

        // The turn player passes: priority moves to the opponent, but nothing
        // resolves yet — the card stays on the stack and we remain in Instant.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.turn_player, 0);
        assert_eq!(gs.active_player, 1);
        assert_eq!(gs.passes, 1);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(cb_idx));
        assert_eq!(gs.cards[cb_idx].location, CardLocation::Stack);
    }

    #[test]
    fn test_both_pass_resolves_top_of_stack() {
        let (mut gs, cb_idx) = instant_phase_with_one_card();

        // Turn player passes (priority to opponent), then the opponent passes:
        // both have now passed, so the top of the stack resolves. The card moves
        // to its owner's graveyard, priority returns to the turn player, and with
        // an empty stack we drop back into the Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, 0);
        assert_eq!(gs.turn_player, 0);
        assert_eq!(gs.passes, 0);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Graveyard);
    }

    #[test]
    fn test_attack_action_resolves_to_combat_chain() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Rhinar plays Muscle Mutt (an attack action, cost 3) and pitches
        // Clearing Bellow (pitch 3) to commit it to the stack, landing in the
        // Instant phase.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});
        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(mm_idx));

        // Both players pass. Because Muscle Mutt is an attack action, resolving it
        // moves it off the stack onto link 0 of Rhinar's combat chain, makes the
        // opponent (Dorinthea, p2) the active player, and enters the Defend phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, 1);
        assert_eq!(gs.turn_player, 0);
        assert_eq!(gs.passes, 0);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.p1.chain_link[0], Some(CardIdx::new(mm_idx)));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1CombatChain);
    }

    #[test]
    fn test_weapon_attack_resolves_to_combat_chain() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Swing Bone Basher (ability cost 2) and pitch Clearing Bellow (pitch 3)
        // to commit the weapon to the stack, landing in the Instant phase.
        let weapon_idx = gs.p1.weapon_idx.unwrap().get();
        step(&mut gs, Action{ typ: ActionType::Activate, card: Some(CardIdx::new(weapon_idx))});
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});
        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(weapon_idx));

        // Both players pass. Because this is a weapon swing, resolving it puts
        // the weapon card itself on link 0 of Rhinar's combat chain, makes the
        // opponent (Dorinthea, p2) the active player, and enters the Defend phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, 1);
        assert_eq!(gs.turn_player, 0);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.p1.chain_link[0], Some(CardIdx::new(weapon_idx)));
        assert_eq!(gs.cards[weapon_idx].location, CardLocation::P1CombatChain);
    }

    /// Drive a game until Dorinthea (p2) is on defense against Rhinar's Muscle
    /// Mutt. Mirrors `test_attack_action_resolves_to_combat_chain` and leaves the
    /// gamestate in the Defend phase with p2 as the active (defending) player.
    fn step_to_dorinthea_defending() -> Gamestate {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});

        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, 1);
        gs
    }

    #[test]
    fn test_defend_moves_card_to_combat_chain() {
        let mut gs = step_to_dorinthea_defending();

        // The defender commits a blocker from hand. It moves onto link 0 of the
        // defender's own combat chain (separate from the attacker's chain) and
        // leaves the hand; the phase stays on Defend so more blockers can follow.
        let db_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in Dorinthea's opening hand");
        let hand_before = gs.p2.hand_size;

        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(db_idx))});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.p2.chain_link[0], Some(CardIdx::new(db_idx)));
        assert_eq!(gs.cards[db_idx].location, CardLocation::P2CombatChain);
        assert_eq!(gs.p2.hand_size, hand_before - 1);
        // The blocker lands on the defender's chain, not the attacker's.
        assert_eq!(gs.p1.chain_link[0].map(|i| gs.cards[i.get()].card), Some(Card::MuscleMuttY));
    }

    #[test]
    fn test_defend_multiple_blockers_share_one_chain_link() {
        let mut gs = step_to_dorinthea_defending();

        // Two blockers committed in succession join the *same* chain link (link 0),
        // chained together via next_card/prev_card. The link is the head of a
        // linked list, so it can hold any number of blockers.
        let first = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(first))});

        let second = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::InTheSwingR)
                .map(|(idx, _)| idx)
                .expect("In the Swing should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(second))});

        assert_eq!(gs.phase, Phase::Defend);
        // Both cards live on the combat chain and nowhere fills a second link.
        assert_eq!(gs.cards[first].location, CardLocation::P2CombatChain);
        assert_eq!(gs.cards[second].location, CardLocation::P2CombatChain);
        assert_eq!(gs.p2.chain_link[1], None);

        // Walk link 0's linked list (head -> ... -> a node pointing at itself) and
        // confirm it contains exactly the two blockers.
        let mut on_link: Vec<usize> = Vec::new();
        let mut cur = gs.p2.chain_link[0].expect("link 0 should hold the blockers").get();
        loop {
            on_link.push(cur);
            let next = gs.cards[cur].next_card.get();
            if next == cur { break; }
            cur = next;
        }
        assert_eq!(on_link.len(), 2);
        assert!(on_link.contains(&first));
        assert!(on_link.contains(&second));
    }

    #[test]
    fn test_defend_pass_advances_to_reaction() {
        let mut gs = step_to_dorinthea_defending();

        // Passing during the Defend phase finishes declaring blockers and moves
        // the game into the Reaction phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Reaction);
    }

    #[test]
    fn test_opponent_plays_instant_in_response() {
        let (mut gs, cb_idx) = instant_phase_with_one_card();

        // Seed 42 doesn't deal Dorinthea an instant in her opening hand, so move
        // a Sigil of Solace (an Instant) from her deck into her hand to set up
        // the response. The same detach/attach helpers the engine uses keep the
        // zone bookkeeping consistent.
        let sigil_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::SigilofSolaceB)
                .map(|(idx, _)| idx)
                .or_else(|| {
                    // Not already in hand: pull one out of the deck.
                    let from_deck = (PLAYER_CARDS..TOTAL_CARDS)
                        .find(|&i| gs.cards[i].card == Card::SigilofSolaceB
                            && gs.cards[i].location == CardLocation::P2Deck)
                        .expect("Dorinthea's deck should contain Sigil of Solace");
                    detach_from_current_zone(&mut gs.p2, &mut gs.cards, from_deck);
                    gs.cards[from_deck].location = CardLocation::P2Hand;
                    attach_to_front_of_zone(
                        &mut gs.cards,
                        &mut gs.p2.hand_idx,
                        None,
                        Some(&mut gs.p2.hand_size),
                        from_deck,
                    );
                    Some(from_deck)
                })
                .unwrap();

        // Turn player passes; priority moves to the opponent (Dorinthea, p2).
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.active_player, 1);

        // The opponent responds with an instant of their own. Sigil of Solace
        // (cost 0) commits straight to the stack on top of Clearing Bellow; the
        // responder keeps priority, so they remain the active player. Adding this
        // layer resets the consecutive-pass count.
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(sigil_idx))});

        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.active_player, 1);
        assert_eq!(gs.passes, 0);
        // Sigil is now on top of the stack, above Clearing Bellow.
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(sigil_idx));
        assert_eq!(gs.cards[sigil_idx].location, CardLocation::Stack);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::Stack);

        // The opponent passes (1 pass). Because Sigil reset the count, this does
        // not resolve anything — priority simply returns to the turn player, who
        // now gets a window to respond to Sigil.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.active_player, 0);
        assert_eq!(gs.passes, 1);
        assert_eq!(gs.cards[sigil_idx].location, CardLocation::Stack);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(sigil_idx));

        // The turn player also passes (2 passes in succession): the top card
        // (Sigil) resolves to its owner's graveyard. Clearing Bellow remains on
        // the stack and priority returns to the turn player for a fresh round.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Instant);
        assert_eq!(gs.active_player, 0);
        assert_eq!(gs.passes, 0);
        assert_eq!(gs.cards[sigil_idx].location, CardLocation::P2Graveyard);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(cb_idx));
    }

    #[test]
    fn test_pass_clears_pending_card() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        let pass = Action{ typ: ActionType::Pass, card: None};
        step(&mut gs, pass);

        assert_eq!(gs.pending_card, None);
    }

    #[test]
    fn test_initial_hand() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
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

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // After the opening draw p1 has 36 cards left in the deck.
        let deck_before = gs.p1.deck_size;
        assert_eq!(deck_before, 36);
        let hand_before = gs.p1.hand_size;

        // Draw the entire deck, including the final (tail) card.
        draw_cards(&mut gs.p1, &mut gs.cards, deck_before as usize);

        // The whole deck moved to hand; nothing is left behind and the head
        // pointer is cleared.
        assert_eq!(gs.p1.deck_size, 0);
        assert_eq!(gs.p1.top_deck_idx, None);
        assert_eq!(gs.p1.bottom_deck_idx, None);
        assert_eq!(gs.p1.hand_size, hand_before + deck_before);

        // Drawing further from an empty deck is a safe no-op.
        draw_cards(&mut gs.p1, &mut gs.cards, 5);
        assert_eq!(gs.p1.deck_size, 0);
        assert_eq!(gs.p1.hand_size, hand_before + deck_before);
    }
}

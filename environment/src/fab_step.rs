use crate::action::{Action,ActionType};
use crate::game_state::{Gamestate,Phase,Player,PendingCard,PlayerIndex,CardIdx,CardLocation,CardVisibleState,CardState,PLAYER_CARDS,TOTAL_CARDS};
use crate::cards::{CardData,CardType,Keyword};
use rand::RngExt;

pub fn step(gs: &mut Gamestate, act: Action) {
    // Once the game has been won or drawn it is frozen: no further actions are
    // processed.
    if gs.is_game_over() {
        return;
    }
    match gs.phase {
        Phase::ChooseFirst => handle_choose_first(gs, act),
        Phase::Action => handle_action_phase(gs, act),
        Phase::ActionPitch | Phase::ReactionPitch => handle_pitch_phase(gs, act),
        Phase::ActionInstant => handle_action_instant_phase(gs, act),
        Phase::Defend => handle_defend_phase(gs, act),
        Phase::Reaction => handle_reaction_phase(gs, act),
        Phase::Arsenal => handle_arsenal_phase(gs, act),
        Phase::PitchOrder => handle_pitch_order_phase(gs, act),
        _ => {}
    }
}

/// Short display name for a player in log messages.
fn player_name(pid: PlayerIndex) -> &'static str {
    if pid == PlayerIndex::P1 { "P1" } else { "P2" }
}

/// The card names in `player`'s hand, joined for a log message.
fn hand_card_names(player: &Player, cards: &[CardState; TOTAL_CARDS]) -> String {
    player
        .hand_iter(cards)
        .map(|(_, cs)| format!("{:?}", cs.card))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Log that `pid` drew their opening hand. The drawn cards are hidden
/// information: the omniscient gamestate log and the drawing player's own log
/// name the cards, while the opponent's log only records how many were drawn.
fn log_opening_hand(gs: &mut Gamestate, pid: PlayerIndex) {
    if !gs.logging_enabled() {
        return;
    }
    let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    let named = format!("{} draws [{}]", player_name(pid), hand_card_names(player, &gs.cards));
    let hidden = format!("{} draws {} cards", player_name(pid), player.hand_size);
    let (p1_msg, p2_msg) = if pid == PlayerIndex::P1 {
        (named.clone(), hidden)
    } else {
        (hidden, named.clone())
    };
    gs.log_views(named, p1_msg, p2_msg);
}

fn handle_choose_first(gs: &mut Gamestate, act: Action) {
    if gs.logging_enabled() {
        // The chooser is the active player *before* any flip below.
        let choice = if act.typ == ActionType::ChooseSecond { "second" } else { "first" };
        gs.log_public(format!("{} chooses to go {}", player_name(gs.active_player), choice));
    }
    // only need to switch the player if they choose go second
    if act.typ == ActionType::ChooseSecond {
        // flip to the other player
        gs.active_player = gs.active_player.opponent();
    }
    // The player who goes first owns this turn. `turn_player` tracks them
    // independently of `active_player`, which will ping-pong as priority passes
    // during the ActionInstant phase.
    gs.turn_player = gs.active_player;
    // begin turn logic
    gs.phase = Phase::Action;
    begin_turn(gs);
    // The cards live in the shared `gs.cards` array; draw each player's opening
    // hand by passing that array alongside the player and its id.
    draw_to_intellect(&mut gs.p1, &mut gs.cards);
    log_opening_hand(gs, PlayerIndex::P1);
    draw_to_intellect(&mut gs.p2, &mut gs.cards);
    log_opening_hand(gs, PlayerIndex::P2);
}

/// Start the turn player's turn. They are granted a single action point — the
/// resource spent to play an action card or activate an action-speed ability.
/// It is consumed when an action resolves (or, for an attack, once combat damage
/// is dealt) unless the card has Go Again.
fn begin_turn(gs: &mut Gamestate) {
    // Count this turn. Reaching `MAX_TURNS` with both heroes alive ends the game
    // in a draw, which `check_game_end` detects from the bumped counter.
    gs.turn_count += 1;
    let turn_player = if gs.turn_player == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    turn_player.action_points = 1;
    gs.check_game_end();
}

fn handle_action_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        // Playing a card, or activating an ability (an armor piece or a weapon
        // swing) both commit a card that must then be paid for (see
        // `commit_card_to_pending`).
        ActionType::PlayCard | ActionType::Activate => {
            commit_card_to_pending(gs, act);
        }
        // Passing ends the action phase; nothing is pending. The combat chain
        // closes and play advances to the turn player's Arsenal phase.
        ActionType::Pass => {
            if gs.logging_enabled() {
                gs.log_public(format!("{} passes", player_name(gs.active_player)));
            }
            gs.pending_card = None;
            close_combat_chain(gs);
            gs.phase = Phase::Arsenal;
            // Intimidate banishes last only until end of turn: as the turn passes
            // into the Arsenal phase, every Intimidate-banished card returns to its
            // owner's hand (the ordinary banished zone is left untouched).
            return_intimidate_banished_to_hands(gs);
        }
        _ => {}
    }
}

/// Handle an action during the ActionInstant phase. Each player in turn may play
/// instants — committed exactly like an action-phase play (`commit_card_to_pending`)
/// — for as long as they keep the priority. Passing hands priority to the other
/// player; once both have passed in succession, the top of the stack resolves
/// (see `handle_action_instant_pass`).
fn handle_action_instant_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        // Only instants are legal here (see `legal_instant_phase`); they commit
        // through the same pending/pitch flow as an action-phase play.
        ActionType::PlayCard => {
            commit_card_to_pending(gs, act);
        }
        ActionType::Pass => {
            handle_priority_pass(gs);
        }
        _ => {}
    }
}

/// Commit a card the active player has chosen to play, activate, or attack with.
/// The card is recorded as `pending_card` (still sitting in its source zone, not
/// yet on the stack). If banked resources already cover its cost it is committed
/// straight to the stack (re-entering the live priority window); otherwise it
/// stays pending and we drop into a pitch phase to pitch for the rest — the
/// `ReactionPitch` phase when committing during the Reaction window (so play
/// returns to it once paid), or the `ActionPitch` phase otherwise. Shared by
/// the Action, ActionInstant, and Reaction phases.
fn commit_card_to_pending(gs: &mut Gamestate, act: Action) {
    // Cost of committing this card: an Activate (an armor ability or a weapon
    // swing) pays its ability's cost; a played card pays its own catalog cost.
    let card_idx = act.card.expect("play/activate action requires a card");
    let cs = gs.cards[card_idx.get()];
    let cost = action_cost(act.typ, cs.card.data());

    // Committing a card reveals it, so the announcement is public.
    if gs.logging_enabled() {
        let verb = if act.typ == ActionType::Activate { "activates" } else { "plays" };
        gs.log_public(format!("{} {} {:?}", player_name(gs.active_player), verb, cs.card));
    }

    let player = if gs.active_player == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    let already_paid = player.resources >= cost;

    gs.pending_card = Some(PendingCard {
        index: card_idx,
        typ: act.typ,
    });

    // Affordable outright: no pitching needed, commit to the stack now.
    // Otherwise leave it pending (off the stack) and pitch for it.
    if already_paid {
        commit_pending_to_stack(gs);
    } else {
        // Pitching during the Reaction window stays within that window: drop
        // into ReactionPitch so we return to Reaction once paid. Every other
        // caller (the Action and ActionInstant phases) uses ActionPitch.
        gs.phase = if gs.phase == Phase::Reaction {
            Phase::ReactionPitch
        } else {
            Phase::ActionPitch
        };
    }
}

/// Handle a pass during a priority window (the ActionInstant or Reaction phase),
/// tracked by `gs.passes`. Each pass hands priority to the other player and
/// bumps the consecutive-pass count; the count is reset to 0 whenever a card is
/// played onto the stack (see `commit_pending_to_stack`), so it only reaches 2
/// when both players pass in succession with no card played between. On that
/// second pass the top card of the stack resolves (see `resolve_top_of_stack`)
/// and the count resets. The Reaction phase opens on an empty stack, so a double
/// pass with nothing on the stack instead closes the window and advances to the
/// next itinerary phase banked on the return stack (the turn player resuming the
/// Action phase).
fn handle_priority_pass(gs: &mut Gamestate) {
    if gs.logging_enabled() {
        gs.log_public(format!("{} passes", player_name(gs.active_player)));
    }
    gs.passes += 1;
    if gs.passes >= 2 {
        // Both players have now passed in succession; reset the pass count.
        gs.passes = 0;
        if gs.stack_is_empty() {
            // Nothing left to resolve (the reaction window opened on an empty
            // stack, or every layer has already resolved): the window closes and
            // play advances to the next itinerary phase.
            close_priority_window(gs);
        } else {
            // Resolve the top of the stack. The resolution itself decides where
            // priority and the phase go next, since that depends on the card type.
            resolve_top_of_stack(gs);
        }
    } else {
        // First pass: hand priority to the other player, who may now respond.
        gs.active_player = gs.active_player.opponent();
    }
}

fn close_priority_window(gs: &mut Gamestate) {
    if gs.phase == Phase::ActionInstant {
        gs.phase = Phase::Action;
        gs.active_player = gs.turn_player;
    }
    else if ( gs.phase == Phase::Reaction ) {
        resolve_combat_damage(gs);
        // Combat damage may have reduced a hero to 0 and ended the game; if so,
        // leave the terminal phase in place rather than returning to the Action
        // phase.
        if !gs.is_game_over() {
            gs.phase = Phase::Action;
            gs.active_player = gs.turn_player
        }
    }
}

/// Handle an action during the Defend phase. The active player is the defender
/// (restored from the `Defend` return frame in `resolve_top_of_stack`). They may
/// commit blockers one at a time — each chosen card moves out of their hand onto
/// the next free link of their own combat chain, and the phase stays on Defend so
/// further blockers can be declared. Passing finishes declaring blockers and
/// advances to the next itinerary phase — the turn player's Reaction window,
/// pushed onto the return stack when the attack was committed.
fn handle_defend_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        ActionType::Defend => commit_blocker(gs, act.card_index()),
        ActionType::Pass => {
            if gs.logging_enabled() {
                gs.log_public(format!("{} passes", player_name(gs.active_player)));
            }
            gs.phase = Phase::Reaction;
            gs.active_player = gs.turn_player;
        }
        _ => {}
    }
}

/// Handle an action during the Arsenal phase. The turn player (the active
/// player here) may set one card from their hand into their empty arsenal slot
/// (`Arsenal`), or pass to keep their whole hand. Either way the turn then
/// winds down: both players' pitched cards return to the bottom of their decks
/// (see `start_pitch_order`) and the turn ends — the turn player draws back up
/// to intellect and the opponent's turn begins (see `end_turn`).
fn handle_arsenal_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        ActionType::Arsenal => {
            let pid = gs.active_player;
            let card_idx = act.card_index();

            // The arsenal is face-down: its owner (and the omniscient log) know
            // the card, the opponent only sees that a card was set aside.
            if gs.logging_enabled() {
                let named = format!("{} arsenals {:?}", player_name(pid), gs.cards[card_idx].card);
                let hidden = format!("{} arsenals a card", player_name(pid));
                let (p1_msg, p2_msg) = if pid == PlayerIndex::P1 {
                    (named.clone(), hidden)
                } else {
                    (hidden, named.clone())
                };
                gs.log_views(named, p1_msg, p2_msg);
            }

            // Move the chosen card out of the hand into the arsenal slot.
            let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
            detach_from_current_zone(player, &mut gs.cards, card_idx);
            gs.cards[card_idx].location = CardLocation::arsenal(pid);
            player.arsenal_idx = Some(CardIdx::new(card_idx));

            start_pitch_order(gs);
        }
        ActionType::Pass => {
            if gs.logging_enabled() {
                gs.log_public(format!("{} passes", player_name(gs.active_player)));
            }
            start_pitch_order(gs);
        }
        _ => {}
    }
}

/// Wind down the turn after the Arsenal phase. Both players' pitched cards
/// must return to the bottom of their decks before the turn ends — the turn
/// player's first, then the opponent's. For each player in that order: an
/// empty pitch zone is skipped, a single pitched card is bottomed
/// automatically (there is only one possible order), and two or more cards
/// enter the PitchOrder phase so that player picks the order, one
/// `BottomPitch` action at a time (see `handle_pitch_order_phase`). The turn
/// ends once both zones are empty.
fn start_pitch_order(gs: &mut Gamestate) {
    if !resolve_pitch_zone(gs, gs.turn_player) {
        return;
    }
    if !resolve_pitch_zone(gs, gs.turn_player.opponent()) {
        return;
    }
    end_turn(gs);
}

/// Clear `pid`'s pitch zone on the way out of the turn, if it can be done
/// without a choice: an empty zone needs nothing, and a single pitched card is
/// bottomed automatically. Returns `true` when the zone is now empty. With two
/// or more cards an order must be chosen, so the PitchOrder phase begins with
/// `pid` active (they own the cards being ordered) and `false` is returned.
fn resolve_pitch_zone(gs: &mut Gamestate, pid: PlayerIndex) -> bool {
    let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    let pitch_count = player.pitch_iter(&gs.cards).count();
    let pitch_head = player.pitch_idx;
    match pitch_count {
        0 => true,
        1 => {
            bottom_pitched_card(gs, pitch_head.expect("pitch zone holds a card").get());
            true
        }
        _ => {
            gs.phase = Phase::PitchOrder;
            gs.active_player = pid;
            false
        }
    }
}

/// Handle an action during the PitchOrder phase. The active player (the owner
/// of the pitch zone being cleared) picks pitched cards one at a time; each
/// chosen card is placed on the bottom of their deck. When the turn player's
/// zone empties, the opponent's zone is cleared next (see
/// `resolve_pitch_zone`); once both are empty the turn ends normally (see
/// `end_turn`).
fn handle_pitch_order_phase(gs: &mut Gamestate, act: Action) {
    if act.typ != ActionType::BottomPitch {
        return;
    }
    bottom_pitched_card(gs, act.card_index());

    let pid = gs.active_player;
    let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    if player.pitch_idx.is_some() {
        // More pitched cards to order; stay in the PitchOrder phase.
        return;
    }
    if pid == gs.turn_player {
        // The turn player is done; the opponent's pitch zone is cleared next.
        if resolve_pitch_zone(gs, pid.opponent()) {
            end_turn(gs);
        }
    } else {
        end_turn(gs);
    }
}

/// Move the card at global index `idx` out of its owner's pitch zone onto the
/// bottom of their deck. Pitched cards sit face-up, so the move is public.
fn bottom_pitched_card(gs: &mut Gamestate, idx: usize) {
    let pid = if idx < PLAYER_CARDS { PlayerIndex::P1 } else { PlayerIndex::P2 };
    if gs.logging_enabled() {
        gs.log_public(format!(
            "{} puts {:?} on the bottom of their deck",
            player_name(pid), gs.cards[idx].card
        ));
    }
    let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    detach_from_current_zone(player, &mut gs.cards, idx);
    gs.cards[idx].location = CardLocation::deck(pid);
    attach_to_bottom_of_deck(player, &mut gs.cards, idx);
}

/// Append the card at global index `idx` to the bottom of `player`'s deck. The
/// card becomes the new tail terminator (its `next_card` points at itself); an
/// empty deck makes it the top as well. Counterpart of `attach_to_front_of_zone`
/// for the one zone that grows from its tail. The caller is responsible for
/// setting the card's `location`.
fn attach_to_bottom_of_deck(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS], idx: usize) {
    if let Some(old_bottom) = player.bottom_deck_idx {
        cards[old_bottom.get()].next_card = CardIdx::new(idx);
        cards[idx].prev_card = old_bottom;
    } else {
        player.top_deck_idx = Some(CardIdx::new(idx));
    }
    cards[idx].next_card = CardIdx::new(idx);
    player.bottom_deck_idx = Some(CardIdx::new(idx));
    player.deck_size += 1;
}

/// End the turn player's turn: they draw back up to intellect, then the turn
/// and active player both flip to the opponent, whose Action phase begins.
/// Unspent resource points are lost at the end of the turn — both players',
/// since the defender can bank leftovers from reaction-window pitches too.
fn end_turn(gs: &mut Gamestate) {
    let pid = gs.turn_player;
    let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    let hand_before = player.hand_size;
    draw_to_intellect(player, &mut gs.cards);
    let drawn = player.hand_size - hand_before;
    log_end_of_turn_draw(gs, pid, drawn);

    gs.p1.resources = 0;
    gs.p2.resources = 0;

    gs.turn_player = gs.turn_player.opponent();
    gs.active_player = gs.turn_player;
    gs.phase = Phase::Action;
    begin_turn(gs);
}

/// Log that `pid` drew back up to intellect at the end of their turn. The drawn
/// cards are hidden information: the omniscient log and the drawing player's
/// own log name them, while the opponent's log only records the count. Drawn
/// cards are prepended to the hand, so the first `drawn` hand cards are the
/// ones just drawn. Drawing nothing logs nothing.
fn log_end_of_turn_draw(gs: &mut Gamestate, pid: PlayerIndex, drawn: u8) {
    if !gs.logging_enabled() || drawn == 0 {
        return;
    }
    let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    let names = player
        .hand_iter(&gs.cards)
        .take(drawn as usize)
        .map(|(_, cs)| format!("{:?}", cs.card))
        .collect::<Vec<_>>()
        .join(", ");
    let named = format!("{} draws [{}]", player_name(pid), names);
    let hidden = format!("{} draws {} cards", player_name(pid), drawn);
    let (p1_msg, p2_msg) = if pid == PlayerIndex::P1 {
        (named.clone(), hidden)
    } else {
        (hidden, named.clone())
    };
    gs.log_views(named, p1_msg, p2_msg);
}

/// Handle an action during the Reaction phase. The active player may play a
/// reaction or instant (committed through the same pending/pitch flow as any
/// other play) or activate an instant-speed ability, for as long as they keep
/// priority. Passing hands priority to the other player; once both have passed
/// in succession the top of the stack resolves, and when the stack is empty the
/// reaction step ends and play returns to the Action phase (see
/// `handle_priority_pass`).
fn handle_reaction_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        ActionType::PlayCard | ActionType::Activate => {
            commit_card_to_pending(gs, act);
        }
        ActionType::Pass => {
            handle_priority_pass(gs);
        }
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
    // Declaring a blocker reveals it, so the announcement is public.
    if gs.logging_enabled() {
        gs.log_public(format!("{} defends with {:?}", player_name(pid), gs.cards[idx].card));
    }
    let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };

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
///   itself), moves onto its owner's combat chain at link 0, and we advance to
///   the next itinerary phase — the `Defend` frame banked when the attack was
///   committed, with the defender active to declare blocks.
/// - Anything **else** resolves to its owner's graveyard. If the stack is now
///   empty the window closes and we advance to the next itinerary phase;
///   otherwise priority returns to the turn player and we keep responding in the
///   live priority window (Instant or Reaction).
fn resolve_top_of_stack(gs: &mut Gamestate) {
    let Some(pending) = gs.pop_stack() else {
        return;
    };
    let top = pending.index.get();
    let owner = if top < PLAYER_CARDS { PlayerIndex::P1 } else { PlayerIndex::P2 };

    let data = gs.cards[top].card.data();

    if gs.logging_enabled() {
        let card = gs.cards[top].card;
        let msg = if commits_as_attack(pending.typ, data) {
            format!("{} attacks with {:?}", player_name(owner), card)
        } else {
            format!("{}'s {:?} resolves", player_name(owner), card)
        };
        gs.log_public(msg);
    }

    // Intimidate triggers as the card resolves, regardless of whether it goes on
    // to the combat chain (an attack action) or to the graveyard (a non-attack
    // action): the resolving player's opponent banishes a random card from their
    // hand to their Intimidate banish zone.
    if data.keyword.contains(Keyword::Intimidate) {
        apply_intimidate(gs, owner);
    }

    // A card joins its owner's combat chain when it is attacking: a played
    // attack action card, or a weapon being swung (the weapon itself joins the
    // chain). Everything else resolves to the graveyard.
    if commits_as_attack(pending.typ, data) {
        // The attacking card leaves the stack for link 0 of its owner's combat
        // chain. The instant window is done; advance to the banked Defend frame
        // (the defender declares blocks).
        gs.cards[top].location = CardLocation::combat_chain(owner);
        let attacker = if owner == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
        // Attach through the linked-list helper so the attacker's chain link is
        // well-terminated (its `next_card` points at itself), letting the chain
        // be walked the same way as the defender's blockers when combat resolves.
        attach_to_front_of_zone(&mut gs.cards, &mut attacker.chain_link[0], None, None, top);
        gs.phase = Phase::Defend;
    } else {
        gs.cards[top].location = CardLocation::graveyard(owner);
        // A resolving non-attack action — an action card, or an activated
        // action-speed ability — spends the owner's action point unless it has
        // Go Again. Attacks are handled when combat damage resolves; instants and
        // reactions are free.
        if uses_action_point(pending.typ, data) && !data.keyword.contains(Keyword::GoAgain) {
            spend_action_point(gs, owner);
        }
        if gs.stack_is_empty() {
            close_priority_window(gs);
        } else {
            // Cards remain on the stack: priority returns to the turn player for
            // a fresh round of responses and we stay in the live priority window
            // (`gs.phase` is already that window).
            gs.active_player = gs.turn_player;
        }
    }
}

/// Resolve an Intimidate trigger: the opponent of `attacker` (the player whose
/// card just resolved) banishes one card chosen uniformly at random from their
/// hand into their Intimidate banish zone. A no-op when that opponent's hand is
/// empty. The banished card is hidden information to the attacker, so the log
/// reveals its identity only to the banishing player.
fn apply_intimidate(gs: &mut Gamestate, attacker: PlayerIndex) {
    let victim = attacker.opponent();

    // Snapshot the victim's hand slots, releasing the borrow on `gs.cards`
    // before we draw from the rng and mutate the hand below.
    let hand: Vec<usize> = {
        let player = if victim == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
        player.hand_iter(&gs.cards).map(|(idx, _)| idx).collect()
    };
    if hand.is_empty() {
        return;
    }

    let pick = hand[gs.rng.random_range(0..hand.len())];

    if gs.logging_enabled() {
        let card = gs.cards[pick].card;
        let full = format!("{} banishes {:?} to Intimidate", player_name(victim), card);
        let hidden = format!("{} banishes a card to Intimidate", player_name(victim));
        let (p1_view, p2_view) = if victim == PlayerIndex::P1 {
            (full.clone(), hidden)
        } else {
            (hidden, full.clone())
        };
        gs.log_views(full, p1_view, p2_view);
    }

    let player = if victim == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    detach_from_current_zone(player, &mut gs.cards, pick);
    gs.cards[pick].location = CardLocation::intimidate_banish(victim);
    attach_to_front_of_zone(&mut gs.cards, &mut player.intimidate_banish_idx, None, None, pick);
}

/// Return every card sitting in either player's Intimidate banish zone to its
/// owner's hand. Intimidate only banishes a card until the end of the turn:
/// unlike the ordinary banished zone (which is permanent), the Intimidate banish
/// zone empties back into its owner's hand as the turn passes into the Arsenal
/// phase. Both players' zones are drained, since either could have been
/// intimidated during the turn.
fn return_intimidate_banished_to_hands(gs: &mut Gamestate) {
    for pid in [PlayerIndex::P1, PlayerIndex::P2] {
        // Drain the zone from its head; each move advances `intimidate_banish_idx`
        // to the next card, so the loop ends once the zone is empty.
        loop {
            let player = if pid == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
            let Some(head) = player.intimidate_banish_idx else { break };
            let idx = head.get();

            if gs.logging_enabled() {
                let card = gs.cards[idx].card;
                let full = format!("{} returns {:?} from Intimidate banish to hand", player_name(pid), card);
                let hidden = format!("{} returns a card from Intimidate banish to hand", player_name(pid));
                let (p1_view, p2_view) = if pid == PlayerIndex::P1 {
                    (full.clone(), hidden)
                } else {
                    (hidden, full.clone())
                };
                gs.log_views(full, p1_view, p2_view);
            }

            let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
            detach_from_current_zone(player, &mut gs.cards, idx);
            gs.cards[idx].location = CardLocation::hand(pid);
            gs.cards[idx].visible = if pid == PlayerIndex::P1 {
                CardVisibleState::P1Knows
            } else {
                CardVisibleState::P2Knows
            };
            attach_to_front_of_zone(
                &mut gs.cards,
                &mut player.hand_idx,
                None,
                Some(&mut player.hand_size),
                idx,
            );
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

/// Whether committing `typ` on `data` spends an action point — i.e. it is an
/// action: an attack action or a non-attack action, played from hand
/// (`PlayCard`) or activated as an action-speed ability (`Activate`, e.g. a
/// weapon swing or Blossom of Spring). Instants, reactions, and instant-speed
/// abilities are free and never touch the action-point pool. The point itself is
/// only deducted when the action resolves (or, for an attack, once combat damage
/// is dealt), and Go Again skips that deduction.
pub(crate) fn uses_action_point(typ: ActionType, data: &CardData) -> bool {
    let card_type = match typ {
        ActionType::PlayCard => Some(data.typ),
        // An Activate is always done on a card carrying an ability; its activation
        // card type is the speed the ability plays at.
        ActionType::Activate => data.ability.as_ref().map(|a| a.card_type()),
        _ => None,
    };
    matches!(card_type, Some(CardType::AttackAction | CardType::Action))
}

/// Handle a pitch during a pitch phase (`ActionPitch` or `ReactionPitch`). The
/// pitched card is moved from the active player's hand into their pitch zone and
/// its pitch value is banked as resources. Once banked resources cover the
/// pending card's cost, that card is committed to the stack and the game returns
/// to the window the pitch interrupted (the ActionInstant or Reaction phase; see
/// `commit_pending_to_stack`).
fn handle_pitch_phase(gs: &mut Gamestate, act: Action) {
    if act.typ != ActionType::Pitch {
        return;
    }

    let pid = gs.active_player;
    let card_idx = act.card_index();
    let pitch_val = gs.cards[card_idx].card.data().pitch;

    // Pitched cards land face-up in the pitch zone, so the pitch is public.
    if gs.logging_enabled() {
        gs.log_public(format!("{} pitches {:?}", player_name(pid), gs.cards[card_idx].card));
    }

    // Move the pitched card out of the hand and into the pitch zone, banking the
    // resources it produces.
    let player = if pid == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    detach_from_current_zone(player, &mut gs.cards, card_idx);
    gs.cards[card_idx].location = CardLocation::pitch(pid);
    attach_to_front_of_zone(&mut gs.cards, &mut player.pitch_idx, None, None, card_idx);
    player.resources += pitch_val;
    let resources = player.resources;

    // Cost still owed on the pending card; once we can cover it, commit it.
    let pending = gs.pending_card.expect("pitch phase requires a pending card");
    let pcs = gs.cards[pending.index.get()];
    let cost = action_cost(pending.typ, pcs.card.data());
    if resources >= cost {
        commit_pending_to_stack(gs);
    }
}

/// Move the pending card onto the stack, pay its cost from the active player's
/// banked resources, and re-enter the live priority window. Shared by both the
/// affordable path (straight from the Action phase) and a pitch phase (once
/// enough has been pitched). The card is detached from its source zone — the
/// hand for a played card, the weapon/armor slot for an activation — and
/// prepended to the stack. Assumes the player can already cover the cost.
fn commit_pending_to_stack(gs: &mut Gamestate) {
    let Some(pending) = gs.pending_card else {
        return;
    };

    // Recompute the cost from the pending action's type, mirroring the
    // affordability check that let us get here.
    let pending_idx = pending.index.get();
    let cs = gs.cards[pending_idx];
    let cost = action_cost(pending.typ, cs.card.data());

    let player = if gs.active_player == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    player.resources -= cost;
    detach_from_current_zone(player, &mut gs.cards, pending_idx);
    gs.cards[pending_idx].location = CardLocation::Stack;
    gs.push_to_stack(pending);

    // The card now lives on the stack, so it is no longer "pending" — clear it.
    // A new layer landing on the stack also interrupts any pending resolution, so
    // the consecutive-pass count resets.
    gs.pending_card = None;
    gs.passes = 0;

    // We keep our phase normally unless we are coming from a phase that opens a
    // fresh priority window once the card is paid for. Committing from the
    // Action phase, or after paying in the ActionPitch phase, opens the
    // ActionInstant window; paying in the ReactionPitch phase returns to the
    // Reaction window the pitch interrupted.
    if gs.phase == Phase::Action || gs.phase == Phase::ActionPitch {
        gs.phase = Phase::ActionInstant;
    } else if gs.phase == Phase::ReactionPitch {
        gs.phase = Phase::Reaction;
    }
}

/// Resolve combat as the reaction window closes. The attacker (the turn player)
/// has its attack card on the combat chain and the defender has its blockers on
/// theirs; total attack power less total blocked defense is dealt to the
/// defender's life. A fully (or over-) blocked attack deals nothing — the damage
/// floors at 0 — and life saturates at 0 rather than underflowing.
fn resolve_combat_damage(gs: &mut Gamestate) {
    let attacker_id = gs.turn_player;
    let defender_id = attacker_id.opponent();

    let attacker = if attacker_id == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    let defender = if defender_id == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
    let power = combat_chain_total(attacker, &gs.cards, |d| d.power);
    let blocked = combat_chain_total(defender, &gs.cards, |d| d.defense);
    let damage = power.saturating_sub(blocked);

    // Whether the attacking card (link 0 of the attacker's chain) has Go Again;
    // if so the turn player keeps their action point and may act again.
    let attack_has_go_again = attacker.chain_link[0]
        .map(|idx| gs.cards[idx.get()].card.data().keyword.contains(Keyword::GoAgain))
        .unwrap_or(false);
    // The attacking card, for the damage log message.
    let attack_card = attacker.chain_link[0].map(|idx| gs.cards[idx.get()].card);

    let defender = if defender_id == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    let life_before = defender.life;
    defender.life = defender.life.saturating_sub(damage);
    let life_after = defender.life;

    // The damage and the resulting life change are one event, logged together.
    if gs.logging_enabled() {
        let source = attack_card
            .map(|c| format!("{:?}", c))
            .unwrap_or_else(|| "unknown".to_string());
        gs.log_public(format!(
            "{} takes {} damage from {} attack (life: {}->{})",
            player_name(defender_id), damage, source, life_before, life_after
        ));
    }

    // A hero reduced to 0 ends the game in the attacker's favour.
    gs.check_game_end();

    // The attack's action point is spent now that damage has been dealt, unless
    // Go Again let the turn player keep acting (see `uses_action_point`).
    if !attack_has_go_again {
        spend_action_point(gs, attacker_id);
    }
}

/// Close the combat chain as the action phase ends. Every card still sitting on
/// either player's chain leaves it: a weapon returns to its owner's weapon slot,
/// every other card goes to its owner's graveyard. Each occupied chain link is
/// the head of a linked list (all blockers against one attack share a link), so
/// the whole list is walked before the link is cleared. Cards on a player's
/// chain are owned by that player — attacks sit on the attacker's chain,
/// blockers on the defender's — so the owner is the chain's player.
fn close_combat_chain(gs: &mut Gamestate) {
    let chain_occupied = gs.p1.chain_link.iter().chain(gs.p2.chain_link.iter())
        .any(|link| link.is_some());
    if chain_occupied && gs.logging_enabled() {
        gs.log_public("combat chain closes".to_string());
    }
    for pid in [PlayerIndex::P1, PlayerIndex::P2] {
        // Borrow the player and the shared cards array as disjoint fields so the
        // chain links and the card states can be updated in the same walk.
        let (player, cards) = if pid == PlayerIndex::P1 {
            (&mut gs.p1, &mut gs.cards)
        } else {
            (&mut gs.p2, &mut gs.cards)
        };
        for link in player.chain_link.iter_mut() {
            let Some(head) = link.take() else { continue };
            let mut cur = head.get();
            loop {
                let next = cards[cur].next_card.get();
                if is_weapon(cards[cur].card.data()) {
                    // A weapon on the chain returns to the slot it swung from.
                    cards[cur].location = CardLocation::weapon(pid);
                    player.weapon_idx = Some(CardIdx::new(cur));
                } else {
                    cards[cur].location = CardLocation::graveyard(pid);
                }
                // A node whose next_card points at itself ends the list.
                if next == cur {
                    break;
                }
                cur = next;
            }
        }
    }
}

/// Whether `data` is a weapon card (one that lives in the weapon slot rather
/// than the deck). Mirrors the weapon classification used when dealing out a
/// decklist in `fab_game`.
fn is_weapon(data: &CardData) -> bool {
    matches!(data.typ, CardType::Weapon | CardType::Sword2h | CardType::Club2h)
}

/// Deduct one action point from `player`, flooring at 0 so it never underflows.
fn spend_action_point(gs: &mut Gamestate, player: PlayerIndex) {
    let p = if player == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    p.action_points = p.action_points.saturating_sub(1);
}

/// Sum a per-card stat (`power` for the attacker, `defense` for the defender)
/// over every card on `player`'s combat chain. Each occupied chain link is the
/// head of a linked list — all blockers declared against one attack share a
/// single link — walked via `next_card` until a node points at itself.
fn combat_chain_total(
    player: &Player,
    cards: &[CardState; TOTAL_CARDS],
    stat: impl Fn(&CardData) -> u8,
) -> u8 {
    let mut total: u8 = 0;
    for link in player.chain_link.iter() {
        let Some(head) = link else { continue };
        let mut cur = head.get();
        loop {
            total = total.saturating_add(stat(cards[cur].card.data()));
            let next = cards[cur].next_card.get();
            if next == cur {
                break;
            }
            cur = next;
        }
    }
    total
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
        CardLocation::P1IntimidateBanish | CardLocation::P2IntimidateBanish => {
            detach_from_linked_list(cards, &mut player.intimidate_banish_idx, None, None, idx);
        }
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
    cards[card_idx].visible = if pid == PlayerIndex::P1 {
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
        reset(&mut gs, false);

        assert_eq!(gs.active_player, PlayerIndex::P1);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_go_second_step() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        assert_eq!(gs.active_player, PlayerIndex::P1);

        let go_second = Action{ typ: ActionType::ChooseSecond, card: None};
        step(&mut gs, go_second);

        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_play_card_moves_to_pitch() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

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

        assert_eq!(gs.phase, Phase::ActionPitch);
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
        reset(&mut gs, false);

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

        assert_eq!(gs.phase, Phase::ActionInstant);
        // Once the card hits the stack it is no longer pending.
        assert_eq!(gs.pending_card, None);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::Stack);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(cb_idx));
    }

    #[test]
    fn test_pitch_commits_played_card_to_stack() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Play Muscle Mutt (cost 3) with no banked resources: it becomes pending
        // and we drop into the ActionPitch phase, with nothing on the stack yet.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});
        assert_eq!(gs.phase, Phase::ActionPitch);
        assert_eq!(gs.stack_top(), None);

        // Pitch Clearing Bellow (pitch 3) — exactly covers the cost. The pending
        // card is committed to the stack, the cost is paid (3 - 3 = 0 resources
        // left), and the game advances to the Instant phase.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(mm_idx));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.resources, 0);
        // The pitched card now lives in the pitch zone, not the hand.
        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Pitch);
    }

    #[test]
    fn test_pitch_below_cost_keeps_card_pending() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Play Muscle Mutt (cost 3); pitching a single yellow attack action only
        // banks 2 resources, short of the cost, so the card stays pending and we
        // remain in the ActionPitch phase.
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

        assert_eq!(gs.phase, Phase::ActionPitch);
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

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(mm_idx));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.resources, 1);
    }

    #[test]
    fn test_pitch_commits_weapon_attack_to_stack() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Swing Bone Basher (ability cost 2): pending, off the stack, Pitch phase.
        let weapon_idx = gs.p1.weapon_idx.unwrap().get();
        step(&mut gs, Action{ typ: ActionType::Activate, card: Some(CardIdx::new(weapon_idx))});
        assert_eq!(gs.phase, Phase::ActionPitch);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.p1.weapon_idx, Some(CardIdx::new(weapon_idx)));

        // Pitch Clearing Bellow (pitch 3) to cover the cost of 2. The weapon is
        // committed to the stack, its slot vacated, and 1 resource is left over.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(weapon_idx));
        assert_eq!(gs.cards[weapon_idx].location, CardLocation::Stack);
        assert_eq!(gs.p1.weapon_idx, None);
        assert_eq!(gs.p1.resources, 1);
    }

    #[test]
    fn test_end_turn_clears_unspent_resources() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Bank leftover resource points on both players mid-turn (as if each
        // had over-pitched paying for something), then let Rhinar pass his
        // Action and Arsenal phases to end the turn.
        gs.p1.resources = 2;
        gs.p2.resources = 1;
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // The turn has flipped to Dorinthea and both pools are empty: unspent
        // resource points are lost at the end of the turn, not carried over.
        assert_eq!(gs.turn_player, PlayerIndex::P2);
        assert_eq!(gs.p1.resources, 0);
        assert_eq!(gs.p2.resources, 0);
    }

    #[test]
    fn test_attack_card_moves_to_pitch() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Swing the equipped weapon. Same flow: record pending, go to Pitch.
        let weapon_idx = gs.p1.weapon_idx.unwrap().get();
        let attack = Action{ typ: ActionType::Activate, card: Some(CardIdx::new(weapon_idx))};
        step(&mut gs, attack);

        assert_eq!(gs.phase, Phase::ActionPitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(pending.index.get(), weapon_idx);
        assert_eq!(pending.typ, ActionType::Activate);
    }

    #[test]
    fn test_play_packcall() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

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
        // game has advanced to the ActionPitch phase to pay for it.
        assert_eq!(gs.phase, Phase::ActionPitch);
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
        reset(&mut gs, false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(cb_idx))});

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(cb_idx));
        (gs, cb_idx)
    }

    #[test]
    fn test_choose_first_sets_turn_player() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);
        assert_eq!(gs.active_player, PlayerIndex::P1);

        // Going second flips both the active player and, since they own the turn,
        // the turn player along with it.
        step(&mut gs, Action{ typ: ActionType::ChooseSecond, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P2);
    }

    #[test]
    fn test_instant_pass_gives_priority_to_opponent() {
        let (mut gs, cb_idx) = instant_phase_with_one_card();

        // The turn player passes: priority moves to the opponent, but nothing
        // resolves yet — the card stays on the stack and we remain in Instant.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.active_player, PlayerIndex::P2);
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
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.passes, 0);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Graveyard);
    }

    #[test]
    fn test_attack_action_resolves_to_combat_chain() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

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
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(mm_idx));

        // Both players pass. Because Muscle Mutt is an attack action, resolving it
        // moves it off the stack onto link 0 of Rhinar's combat chain, makes the
        // opponent (Dorinthea, p2) the active player, and enters the Defend phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.passes, 0);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.p1.chain_link[0], Some(CardIdx::new(mm_idx)));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1CombatChain);
    }

    #[test]
    fn test_weapon_attack_resolves_to_combat_chain() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

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
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(weapon_idx));

        // Both players pass. Because this is a weapon swing, resolving it puts
        // the weapon card itself on link 0 of Rhinar's combat chain, makes the
        // opponent (Dorinthea, p2) the active player, and enters the Defend phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.p1.chain_link[0], Some(CardIdx::new(weapon_idx)));
        assert_eq!(gs.cards[weapon_idx].location, CardLocation::P1CombatChain);
    }

    /// Drive a game until Dorinthea (p2) is on defense against Rhinar's Muscle
    /// Mutt. Mirrors `test_attack_action_resolves_to_combat_chain` and leaves the
    /// gamestate in the Defend phase with p2 as the active (defending) player.
    fn step_to_dorinthea_defending() -> Gamestate {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

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
        assert_eq!(gs.active_player, PlayerIndex::P2);
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
    fn test_defend_pass_advances_to_post_defend_instant() {
        let mut gs = step_to_dorinthea_defending();

        // Passing during the Defend phase finishes declaring blockers and opens
        // the post-defend Instant window. The defending player (Dorinthea, p2)
        // holds priority first per FaB rules; the stack is still empty.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, gs.turn_player);
        assert!(gs.stack_is_empty());
    }

    #[test]
    fn test_reaction_phase_double_pass_returns_to_action() {
        let mut gs = step_to_dorinthea_defending();

        // Defend pass → post-defend Instant window (defender holds priority first).
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, gs.turn_player);

        // Double pass with empty stack closes the Reaction window and returns
        // to the turn player's Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, gs.turn_player);
    }

    #[test]
    fn test_unblocked_attack_deals_full_damage_when_reaction_closes() {
        // Rhinar attacks with Muscle Mutt (power 6); Dorinthea (20 life) declares
        // no blocks. When the reaction window closes, the full 6 damage is applied
        // to her life before play returns to the Action phase.
        let mut gs = step_to_dorinthea_defending();
        assert_eq!(gs.p2.life, 20);

        // Defender declares no blocks → post-defend Instant window (defender holds
        // priority first).
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);

        // Both pass the Reaction window → combat resolves, then Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, gs.turn_player);
        // 6 power, nothing blocked → 6 damage. 20 - 6 = 14.
        assert_eq!(gs.p2.life, 14);
    }

    #[test]
    fn test_blocked_attack_deals_reduced_damage_when_reaction_closes() {
        // Rhinar attacks with Muscle Mutt (power 6); Dorinthea blocks with Driving
        // Blade (defense 3). When the reaction window closes, 6 - 3 = 3 damage is
        // applied to her life.
        let mut gs = step_to_dorinthea_defending();
        assert_eq!(gs.p2.life, 20);

        // Defender commits Driving Blade as a blocker, then passes to finish.
        let db_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(db_idx))});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Reaction);

        // Both pass the Reaction window → combat resolves, then Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Action);
        // 6 power - 3 blocked = 3 damage. 20 - 3 = 17.
        assert_eq!(gs.p2.life, 17);
    }

    #[test]
    fn test_fully_blocked_attack_deals_no_damage() {
        // Dorinthea blocks Muscle Mutt (power 6) with both Driving Blade (def 3)
        // and In the Swing (def 3): 6 total blocked, so no damage gets through and
        // her life is unchanged when the reaction window closes.
        let mut gs = step_to_dorinthea_defending();
        assert_eq!(gs.p2.life, 20);

        let db_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(db_idx))});
        let its_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::InTheSwingR)
                .map(|(idx, _)| idx)
                .expect("In the Swing should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(its_idx))});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Reaction);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Action);
        // 6 power - 6 blocked = 0 damage. Life unchanged.
        assert_eq!(gs.p2.life, 20);
    }

    #[test]
    fn test_turn_player_starts_with_one_action_point() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        // Before the first-player choice neither player has been granted their
        // action point yet.
        assert_eq!(gs.p1.action_points, 0);
        assert_eq!(gs.p2.action_points, 0);

        // Rhinar goes first; the turn player is granted exactly one action point,
        // the defender none.
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.p1.action_points, 1);
        assert_eq!(gs.p2.action_points, 0);
    }

    #[test]
    fn test_attack_spends_action_point_and_blocks_further_actions() {
        // Rhinar steps through a full Muscle Mutt attack: play it, pitch for it,
        // let it resolve onto the chain, defend, and run out the reaction window
        // so combat damage is dealt. Muscle Mutt has no Go Again, so its action
        // point is spent once damage resolves — and back in the Action phase
        // Rhinar can no longer play another attack action or action.
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.p1.action_points, 1);

        // Play Muscle Mutt (attack action) and pitch Clearing Bellow to pay for it.
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

        // The attack is on the stack but combat is not resolved yet, so the action
        // point is still in hand.
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.p1.action_points, 1);

        // Both pass: the attack resolves onto the chain → Defend phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Defend);
        // Still unspent — damage has not been calculated yet.
        assert_eq!(gs.p1.action_points, 1);

        // Dorinthea declares no blocks → both pass →
        // Reaction window; both pass → combat resolves.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // Back in the Action phase: 6 damage dealt (20 → 14) and the attack's
        // action point has now been spent.
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.p2.life, 14);
        assert_eq!(gs.p1.action_points, 0);

        // With no action points left Rhinar cannot play another attack action or
        // action (Pack Call and Raging Onslaught remain in hand, and the weapon
        // swing is an attack-action activation) — every offered action is free of
        // action-point cost, leaving only Pass.
        let actions = legal_actions(&gs);
        for a in &actions {
            if let Some(idx) = a.card {
                assert!(
                    !uses_action_point(a.typ, gs.cards[idx.get()].card.data()),
                    "with 0 action points, {:?} of {:?} must not be offered",
                    a.typ, gs.cards[idx.get()].card
                );
            }
        }
        // No attack-action / action card is offered for play.
        let play_types: Vec<CardType> = actions.iter()
                .filter(|a| a.typ == ActionType::PlayCard)
                .map(|a| gs.cards[a.card_index()].card.data().typ)
                .collect();
        assert!(!play_types.contains(&CardType::AttackAction));
        assert!(!play_types.contains(&CardType::Action));
        // Passing is still available to end the turn.
        assert!(actions.iter().any(|a| a.typ == ActionType::Pass));
    }

    #[test]
    fn test_full_walkthrough_rhinar_attack_dorinthea_takes_damage() {
        // A complete attack stepped through every phase with no helpers: Rhinar
        // attacks with Muscle Mutt (power 6), Dorinthea blocks with a single
        // Driving Blade (defense 3), and 6 - 3 = 3 damage is dealt to her life.
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        // ── ChooseFirst phase ──────────────────────────────────────────────
        assert_eq!(gs.phase, Phase::ChooseFirst);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.p1.action_points, 1);
        assert_eq!(gs.p2.life, 20);

        // ── Action phase: play Muscle Mutt ─────────────────────────────────
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in Rhinar's opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});
        assert_eq!(gs.phase, Phase::ActionPitch);
        assert_eq!(gs.pending_card.expect("pending Muscle Mutt").index.get(), mm_idx);

        // ── Pitch phase: pitch Clearing Bellow (pitch 3) to pay the cost of 3 ─
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in Rhinar's opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(mm_idx));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::Stack);

        // ── Instant phase (attack on the stack): both pass to resolve it ───
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Rhinar passes
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Dorinthea passes
        // The attack resolves onto Rhinar's combat chain and play enters Defend.
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.p1.chain_link[0], Some(CardIdx::new(mm_idx)));
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1CombatChain);
        assert!(gs.stack_is_empty());

        // ── Defend phase: Dorinthea blocks with Driving Blade (defense 3) ──
        let db_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(db_idx))});
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.p2.chain_link[0], Some(CardIdx::new(db_idx)));
        assert_eq!(gs.cards[db_idx].location, CardLocation::P2CombatChain);
        // Done blocking.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // ── Reaction phase (turn player holds priority first) ──────────────
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Rhinar passes
        assert_eq!(gs.active_player, PlayerIndex::P2);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Dorinthea passes → combat

        // ── Back to the Action phase: damage has been dealt ────────────────
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        // 6 power - 3 blocked = 3 damage. 20 - 3 = 17.
        assert_eq!(gs.p2.life, 17);
        // Muscle Mutt has no Go Again, so its action point is now spent.
        assert_eq!(gs.p1.action_points, 0);
    }

    #[test]
    fn test_full_walkthrough_dorinthea_overblocks_no_damage() {
        // The same attack, stepped through every phase, but Dorinthea over-blocks:
        // she commits three defense-3 cards (Driving Blade, In the Swing, Second
        // Swing) for 9 total defense against Muscle Mutt's 6 power, so no damage
        // gets through and her life is unchanged.
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        // ── ChooseFirst phase ──────────────────────────────────────────────
        assert_eq!(gs.phase, Phase::ChooseFirst);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.p2.life, 20);

        // ── Action phase: play Muscle Mutt ─────────────────────────────────
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in Rhinar's opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});
        assert_eq!(gs.phase, Phase::ActionPitch);

        // ── Pitch phase: pay for it with Clearing Bellow ───────────────────
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in Rhinar's opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(mm_idx));

        // ── Instant phase: both pass to resolve the attack onto the chain ──
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Rhinar
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Dorinthea
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1CombatChain);

        // ── Defend phase: Dorinthea commits three defense-3 blockers ───────
        let db_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(db_idx))});
        assert_eq!(gs.phase, Phase::Defend);
        let its_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::InTheSwingR)
                .map(|(idx, _)| idx)
                .expect("In the Swing should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(its_idx))});
        assert_eq!(gs.phase, Phase::Defend);
        let ss_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::SecondSwingR)
                .map(|(idx, _)| idx)
                .expect("Second Swing should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(ss_idx))});
        assert_eq!(gs.phase, Phase::Defend);
        // All three blockers now sit on Dorinthea's combat chain.
        assert_eq!(gs.cards[db_idx].location, CardLocation::P2CombatChain);
        assert_eq!(gs.cards[its_idx].location, CardLocation::P2CombatChain);
        assert_eq!(gs.cards[ss_idx].location, CardLocation::P2CombatChain);
        // Done blocking.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // ── Reaction phase: both pass → combat resolves ───────────────────
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Rhinar
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Dorinthea → combat

        // ── Back to the Action phase: the over-block soaked all the damage ─
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        // 6 power - 9 blocked floors at 0 damage; life unchanged.
        assert_eq!(gs.p2.life, 20);
        // The attack's action point is still spent even though it dealt no damage.
        assert_eq!(gs.p1.action_points, 0);
    }

    #[test]
    fn test_reaction_played_resolves_and_returns_to_action() {
        let mut gs = step_to_dorinthea_defending();

        // Defend pass → post-defend Instant window (defender p2 holds priority).
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, gs.turn_player);

        // Relabel a card in p1's hand to Sigil of Solace (an Instant, cost 0) and
        // play it as a reaction. It commits straight to the stack and we remain in
        // the Reaction phase (not the Instant phase) with the responder keeping
        // priority — the live priority window is preserved.
        let sigil_idx = gs.p1.hand_iter(&gs.cards)
                .map(|(idx, _)| idx)
                .next()
                .expect("p1 should hold a card in the reaction phase");
        gs.cards[sigil_idx].card = Card::SigilofSolaceB;

        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(sigil_idx))});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.cards[sigil_idx].location, CardLocation::Stack);

        // Both players pass: the reaction resolves to its owner's graveyard, the
        // stack empties, and play returns to the turn player's Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.cards[sigil_idx].location, CardLocation::P1Graveyard);
        assert!(gs.stack_is_empty());
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, gs.turn_player);
    }

    #[test]
    fn test_reaction_pitch_returns_to_reaction() {
        // A reaction played during the Reaction window that can't be paid for
        // outright drops into the ReactionPitch phase; once enough is pitched
        // the reaction commits to the stack and play returns to the Reaction
        // window it interrupted — not the ActionInstant window. Toughen Up, a
        // defense reaction that costs 2 (and so must be pitched for), exercises
        // this path.
        let mut gs = step_to_dorinthea_defending();

        // Defender declares no blocks; the post-defend Reaction window opens with
        // the turn player (the attacker, p1) holding priority first.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, gs.turn_player);

        // Turn player passes; priority moves to the defender (p2), who reacts.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P2);

        // Relabel two cards in the defender's hand: one to Toughen Up (the
        // reaction to play, cost 2) and one to Clearing Bellow (pitch 3) to pay
        // for it. Seed 42 doesn't guarantee this exact pairing, so set it up
        // explicitly — only the card stats matter here, the zones are untouched.
        let hand: Vec<usize> = gs.p2.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
        let tu_idx = hand[0];
        let pitch_idx = hand[1];
        gs.cards[tu_idx].card = Card::ToughenUpB;
        gs.cards[pitch_idx].card = Card::ClearingBellowB;

        // Play Toughen Up. It costs 2 and the defender has no banked resources,
        // so instead of committing to the stack we drop into the ReactionPitch
        // phase with Toughen Up held pending in hand and nothing on the stack.
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(tu_idx))});
        assert_eq!(gs.phase, Phase::ReactionPitch);
        assert_eq!(gs.pending_card.expect("Toughen Up pending").index.get(), tu_idx);
        assert!(gs.stack_is_empty());

        // Pitch Clearing Bellow (pitch 3) to cover the cost of 2. That commits
        // Toughen Up to the stack and returns to the Reaction window — not the
        // ActionInstant window — with the responder (p2) keeping priority.
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(pitch_idx))});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(tu_idx));
        assert_eq!(gs.cards[tu_idx].location, CardLocation::Stack);
        assert_eq!(gs.cards[pitch_idx].location, CardLocation::P2Pitch);
    }

    #[test]
    fn test_attack_instant_defend_reaction() {
        // Drive a whole attack interaction and confirm the phase-return stack
        // walks it back in order: the instant window resolves the attack onto the
        // chain (-> Defend), declaring blocks advances to the turn player's
        // Reaction window, and emptying that window returns to the Action phase.
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        let tp = gs.turn_player;

        // Rhinar plays Muscle Mutt (an attack action) and pitches Clearing Bellow
        // to pay for it, opening the Instant window with the attack on the stack.
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
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, tp);

        // Both pass: the attack resolves to the chain and we advance to Defend
        // with the defender (the non-turn player) active.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, tp.opponent());

        // The defender passes (declares no further blocks): opens the post-defend
        // Instant window with the defender holding priority first.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, tp);

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, tp.opponent());

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, tp);
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
        assert_eq!(gs.active_player, PlayerIndex::P2);

        // The opponent responds with an instant of their own. Sigil of Solace
        // (cost 0) commits straight to the stack on top of Clearing Bellow; the
        // responder keeps priority, so they remain the active player. Adding this
        // layer resets the consecutive-pass count.
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(sigil_idx))});

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.passes, 0);
        // Sigil is now on top of the stack, above Clearing Bellow.
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(sigil_idx));
        assert_eq!(gs.cards[sigil_idx].location, CardLocation::Stack);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::Stack);

        // The opponent passes (1 pass). Because Sigil reset the count, this does
        // not resolve anything — priority simply returns to the turn player, who
        // now gets a window to respond to Sigil.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.passes, 1);
        assert_eq!(gs.cards[sigil_idx].location, CardLocation::Stack);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(sigil_idx));

        // The turn player also passes (2 passes in succession): the top card
        // (Sigil) resolves to its owner's graveyard. Clearing Bellow remains on
        // the stack and priority returns to the turn player for a fresh round.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.passes, 0);
        assert_eq!(gs.cards[sigil_idx].location, CardLocation::P2Graveyard);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(cb_idx));
    }

    #[test]
    fn test_pass_clears_pending_card() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        let pass = Action{ typ: ActionType::Pass, card: None};
        step(&mut gs, pass);

        assert_eq!(gs.pending_card, None);
    }

    // ── Arsenal phase / closing the combat chain ───────────────────────────

    #[test]
    fn test_action_phase_pass_advances_to_arsenal() {
        // The turn player passing during the Action phase ends it: play advances
        // to the Arsenal phase. With nothing ever attacked there is no combat
        // chain to close.
        let mut gs = fresh_game();
        assert_eq!(gs.phase, Phase::Action);

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert!(gs.p1.chain_link.iter().all(|l| l.is_none()));
        assert!(gs.p2.chain_link.iter().all(|l| l.is_none()));
    }

    #[test]
    fn test_action_phase_pass_closes_combat_chain_to_graveyards() {
        // Run a full Muscle Mutt attack blocked by Driving Blade, returning to
        // the Action phase with both cards still sitting on the combat chain.
        let mut gs = step_to_dorinthea_defending();
        let mm_idx = gs.p1.chain_link[0].expect("attack on p1's chain").get();
        let db_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(db_idx))});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1CombatChain);
        assert_eq!(gs.cards[db_idx].location, CardLocation::P2CombatChain);

        // The turn player passes: the chain breaks. Each card on it goes to its
        // owner's graveyard — the attack to the attacker's, the blocker to the
        // defender's — every link clears, and play enters the Arsenal phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1Graveyard);
        assert_eq!(gs.cards[db_idx].location, CardLocation::P2Graveyard);
        assert!(gs.p1.chain_link.iter().all(|l| l.is_none()));
        assert!(gs.p2.chain_link.iter().all(|l| l.is_none()));
    }

    #[test]
    fn test_action_phase_pass_closes_chain_multiple_blockers() {
        // Two blockers share chain link 0 as a linked list; closing the chain
        // must walk the whole list, not just the head.
        let mut gs = step_to_dorinthea_defending();
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
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.cards[first].location, CardLocation::P2Graveyard);
        assert_eq!(gs.cards[second].location, CardLocation::P2Graveyard);
        assert!(gs.p2.chain_link.iter().all(|l| l.is_none()));
    }

    #[test]
    fn test_action_phase_pass_returns_weapon_to_weapon_slot() {
        // Swing Bone Basher through a full combat so the weapon card is left on
        // the combat chain when play returns to the Action phase.
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        let weapon_idx = gs.p1.weapon_idx.unwrap().get();
        step(&mut gs, Action{ typ: ActionType::Activate, card: Some(CardIdx::new(weapon_idx))});
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Defend);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.cards[weapon_idx].location, CardLocation::P1CombatChain);
        assert_eq!(gs.p1.weapon_idx, None);

        // The turn player passes: the chain breaks, and the weapon goes back to
        // the weapon slot it swung from rather than the graveyard.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.cards[weapon_idx].location, CardLocation::P1Weapon);
        assert_eq!(gs.p1.weapon_idx, Some(CardIdx::new(weapon_idx)));
        assert!(gs.p1.chain_link.iter().all(|l| l.is_none()));
    }

    #[test]
    fn test_arsenal_moves_card_and_starts_opponent_turn() {
        let mut gs = fresh_game();
        assert_eq!(gs.turn_count, 1);

        // Rhinar passes his action phase and sets Muscle Mutt into his arsenal.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Arsenal, card: Some(CardIdx::new(mm_idx))});

        // The card left the hand for the arsenal slot.
        assert_eq!(gs.cards[mm_idx].location, CardLocation::P1Arsenal);
        assert_eq!(gs.p1.arsenal_idx, Some(CardIdx::new(mm_idx)));

        // Arsenaling dropped the hand to 3; the end-of-turn draw refills it to
        // intellect (4), taking one card off the deck.
        assert_eq!(gs.p1.hand_size, 4);
        assert_eq!(gs.p1.deck_size, 35);

        // The opponent's turn begins: both the turn and active player flip, play
        // is back in the Action phase, and the new turn player has their action
        // point.
        assert_eq!(gs.turn_player, PlayerIndex::P2);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.p2.action_points, 1);
        assert_eq!(gs.turn_count, 2);
    }

    #[test]
    fn test_arsenal_pass_skips_arsenal_and_starts_opponent_turn() {
        let mut gs = fresh_game();

        // Rhinar passes his action phase, then passes again to skip arsenaling.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // Nothing was arsenaled, and the hand was already at intellect so
        // nothing was drawn.
        assert_eq!(gs.p1.arsenal_idx, None);
        assert_eq!(gs.p1.hand_size, 4);
        assert_eq!(gs.p1.deck_size, 36);

        // The opponent's turn still begins.
        assert_eq!(gs.turn_player, PlayerIndex::P2);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.turn_count, 2);
    }

    // ── PitchOrder phase ───────────────────────────────────────────────────

    #[test]
    fn test_single_pitched_card_bottoms_automatically_at_end_of_turn() {
        // Rhinar's Muscle Mutt attack pitched a single card (Clearing Bellow).
        // Run the turn out: with exactly one card in the pitch zone there is no
        // order to choose, so it goes to the bottom of the deck automatically
        // and the turn ends without entering the PitchOrder phase.
        let mut gs = step_to_dorinthea_defending();
        let cb_idx = gs.p1.pitch_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should sit in the pitch zone");
        assert_eq!(gs.p1.deck_size, 36);

        // Defender declares no blocks → Reaction window → combat resolves →
        // Action phase; the turn player passes through Arsenal to end the turn.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // The turn ended normally — no PitchOrder phase for a single card.
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.turn_player, PlayerIndex::P2);

        // Clearing Bellow left the pitch zone for the bottom of the deck. The
        // deck went 36 → 37, then the end-of-turn draw (hand 2 → 4) left 35.
        assert_eq!(gs.p1.pitch_idx, None);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Deck);
        assert_eq!(gs.p1.bottom_deck_idx, Some(CardIdx::new(cb_idx)));
        assert_eq!(gs.p1.deck_size, 35);
        assert_eq!(gs.p1.hand_size, 4);
    }

    /// Drive Rhinar through a Muscle Mutt attack paid for with *two* pitched
    /// cards (Pack Call and Raging Onslaught, pitch 2 each, against the cost of
    /// 3), then run the turn out to the Arsenal pass. Ends with the game in the
    /// PitchOrder phase and returns the gamestate plus the two pitched slots.
    fn step_to_pitch_order() -> (Gamestate, usize, usize) {
        let mut gs = fresh_game();

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
        let ro_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::RagingOnslaughtY)
                .map(|(idx, _)| idx)
                .expect("Raging Onslaught should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(ro_idx))});
        assert_eq!(gs.phase, Phase::ActionInstant);

        // Resolve the attack onto the chain, decline blocks, run out the
        // reaction window, and pass the Action and Arsenal phases.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Defend);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // Two cards sit in the pitch zone, so instead of ending the turn we
        // enter the PitchOrder phase with the turn player still active.
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.active_player, PlayerIndex::P1);

        (gs, pc_idx, ro_idx)
    }

    #[test]
    fn test_two_pitched_cards_enter_pitch_order_phase() {
        let (gs, pc_idx, ro_idx) = step_to_pitch_order();

        // Both pitched cards are still in the pitch zone, and each is offered
        // as a BottomPitch action — nothing else, no pass.
        assert_eq!(gs.cards[pc_idx].location, CardLocation::P1Pitch);
        assert_eq!(gs.cards[ro_idx].location, CardLocation::P1Pitch);
        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().all(|a| a.typ == ActionType::BottomPitch));
        let offered: Vec<usize> = actions.iter().map(|a| a.card_index()).collect();
        assert!(offered.contains(&pc_idx));
        assert!(offered.contains(&ro_idx));
    }

    #[test]
    fn test_bottom_pitch_orders_cards_and_ends_turn() {
        let (mut gs, pc_idx, ro_idx) = step_to_pitch_order();
        assert_eq!(gs.p1.deck_size, 36);

        // Bottom Pack Call first: it leaves the pitch zone for the bottom of
        // the deck, and with Raging Onslaught still pitched the phase stays on
        // PitchOrder with the turn not yet over.
        step(&mut gs, Action{ typ: ActionType::BottomPitch, card: Some(CardIdx::new(pc_idx))});
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.cards[pc_idx].location, CardLocation::P1Deck);
        assert_eq!(gs.p1.bottom_deck_idx, Some(CardIdx::new(pc_idx)));
        assert_eq!(gs.p1.deck_size, 37);

        // Only the remaining pitched card is offered now.
        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].typ, ActionType::BottomPitch);
        assert_eq!(actions[0].card_index(), ro_idx);

        // Bottom Raging Onslaught: the pitch zone empties and the turn ends
        // normally — Raging Onslaught is the new bottom card, sitting below
        // Pack Call, and the opponent's turn begins.
        step(&mut gs, Action{ typ: ActionType::BottomPitch, card: Some(CardIdx::new(ro_idx))});
        assert_eq!(gs.p1.pitch_idx, None);
        assert_eq!(gs.cards[ro_idx].location, CardLocation::P1Deck);
        assert_eq!(gs.p1.bottom_deck_idx, Some(CardIdx::new(ro_idx)));
        assert_eq!(gs.cards[pc_idx].next_card, CardIdx::new(ro_idx));

        // End-of-turn bookkeeping ran as usual: the hand (1 card after playing
        // and pitching three) drew back to intellect, 38 - 3 = 35 cards remain
        // in the deck, and the turn flipped to Dorinthea.
        assert_eq!(gs.p1.hand_size, 4);
        assert_eq!(gs.p1.deck_size, 35);
        assert_eq!(gs.turn_player, PlayerIndex::P2);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.turn_count, 2);
    }

    /// From the Reaction phase with the attacker (p1) holding priority, make
    /// the defender (Dorinthea, p2) pitch two cards: her hand is relabeled so
    /// she plays Toughen Up (a defense reaction, cost 2) paid for with two
    /// pitch-1 reds, then both players pass the window out so combat resolves
    /// and play returns to the Action phase.
    fn defender_pitches_two_in_reaction(gs: &mut Gamestate) -> (usize, usize) {
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, gs.turn_player);

        // Attacker passes; priority moves to the defender.
        step(gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P2);

        // Relabel three of the defender's hand cards: the reaction to play and
        // two pitch-1 reds to pay its cost of 2 with two separate pitches.
        let hand: Vec<usize> = gs.p2.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
        assert!(hand.len() >= 3, "expected Dorinthea to hold three cards in reaction");
        gs.cards[hand[0]].card = Card::ToughenUpB;
        gs.cards[hand[1]].card = Card::SharpenSteelR;
        gs.cards[hand[2]].card = Card::SharpenSteelR;

        step(gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(hand[0]))});
        assert_eq!(gs.phase, Phase::ReactionPitch);
        step(gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(hand[1]))});
        assert_eq!(gs.phase, Phase::ReactionPitch);
        step(gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(hand[2]))});
        assert_eq!(gs.phase, Phase::Reaction);

        // Both pass: Toughen Up resolves to the graveyard, the window closes,
        // combat damage resolves, and play returns to the Action phase.
        step(gs, Action{ typ: ActionType::Pass, card: None});
        step(gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);

        (hand[1], hand[2])
    }

    #[test]
    fn test_defender_pitch_orders_after_turn_player_auto_bottoms() {
        // Rhinar pitched a single card (Clearing Bellow) for his attack;
        // Dorinthea pitches two defending. At the end of the turn Rhinar's
        // card bottoms automatically, then the PitchOrder phase opens for the
        // *defender* to order hers — the turn player goes first.
        let mut gs = step_to_dorinthea_defending();
        let cb_idx = gs.p1.pitch_iter(&gs.cards)
                .map(|(idx, _)| idx)
                .next()
                .expect("Clearing Bellow should sit in p1's pitch zone");

        step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // declare no blocks
        let (d1_idx, d2_idx) = defender_pitches_two_in_reaction(&mut gs);

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // The turn player's single card bottomed automatically; the defender's
        // two cards put the game in the PitchOrder phase with *her* active,
        // while the turn (and turn player) is still Rhinar's.
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.p1.pitch_idx, None);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Deck);
        assert_eq!(gs.p1.bottom_deck_idx, Some(CardIdx::new(cb_idx)));

        // Only the defender's pitched cards are offered.
        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().all(|a| a.typ == ActionType::BottomPitch));
        let offered: Vec<usize> = actions.iter().map(|a| a.card_index()).collect();
        assert!(offered.contains(&d1_idx));
        assert!(offered.contains(&d2_idx));

        // She orders both to the bottom of her own deck; the turn then ends.
        step(&mut gs, Action{ typ: ActionType::BottomPitch, card: Some(CardIdx::new(d1_idx))});
        assert_eq!(gs.phase, Phase::PitchOrder);
        step(&mut gs, Action{ typ: ActionType::BottomPitch, card: Some(CardIdx::new(d2_idx))});

        assert_eq!(gs.p2.pitch_idx, None);
        assert_eq!(gs.cards[d1_idx].location, CardLocation::P2Deck);
        assert_eq!(gs.cards[d2_idx].location, CardLocation::P2Deck);
        assert_eq!(gs.p2.bottom_deck_idx, Some(CardIdx::new(d2_idx)));
        assert_eq!(gs.cards[d1_idx].next_card, CardIdx::new(d2_idx));
        assert_eq!(gs.turn_player, PlayerIndex::P2);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_both_players_order_pitch_turn_player_first() {
        // Rhinar pitches two cards (Pack Call and Raging Onslaught) for Muscle
        // Mutt; Dorinthea pitches two defending. Both must order their pitch at
        // the end of the turn: Rhinar (the turn player) first, then the phase
        // stays on PitchOrder while the active player flips to Dorinthea.
        let mut gs = fresh_game();

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
        let ro_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::RagingOnslaughtY)
                .map(|(idx, _)| idx)
                .expect("Raging Onslaught should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(ro_idx))});

        // Resolve the attack onto the chain; the defender declares no blocks.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Defend);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        let (d1_idx, d2_idx) = defender_pitches_two_in_reaction(&mut gs);

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // The turn player orders first.
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        let offered: Vec<usize> = legal_actions(&gs).iter().map(|a| a.card_index()).collect();
        assert!(offered.contains(&pc_idx));
        assert!(offered.contains(&ro_idx));

        step(&mut gs, Action{ typ: ActionType::BottomPitch, card: Some(CardIdx::new(pc_idx))});
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        step(&mut gs, Action{ typ: ActionType::BottomPitch, card: Some(CardIdx::new(ro_idx))});

        // Rhinar's zone is empty but Dorinthea's is not: the phase stays on
        // PitchOrder, the active player flips to her, and the turn is not over.
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.p1.pitch_idx, None);
        assert_eq!(gs.p1.bottom_deck_idx, Some(CardIdx::new(ro_idx)));
        let offered: Vec<usize> = legal_actions(&gs).iter().map(|a| a.card_index()).collect();
        assert!(offered.contains(&d1_idx));
        assert!(offered.contains(&d2_idx));

        // Dorinthea orders hers; only then does the turn end.
        step(&mut gs, Action{ typ: ActionType::BottomPitch, card: Some(CardIdx::new(d2_idx))});
        assert_eq!(gs.phase, Phase::PitchOrder);
        step(&mut gs, Action{ typ: ActionType::BottomPitch, card: Some(CardIdx::new(d1_idx))});

        assert_eq!(gs.p2.pitch_idx, None);
        assert_eq!(gs.p2.bottom_deck_idx, Some(CardIdx::new(d1_idx)));
        assert_eq!(gs.cards[d2_idx].next_card, CardIdx::new(d1_idx));
        assert_eq!(gs.turn_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.turn_count, 2);
    }

    #[test]
    fn test_pitch_order_ignores_non_bottom_pitch_actions() {
        let (mut gs, pc_idx, ro_idx) = step_to_pitch_order();

        // A stray Pass (or any other action type) is a no-op in the PitchOrder
        // phase: the phase, the pitch zone, and the turn are all untouched.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.cards[pc_idx].location, CardLocation::P1Pitch);
        assert_eq!(gs.cards[ro_idx].location, CardLocation::P1Pitch);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
    }

    #[test]
    fn test_arsenal_logs_hide_card_from_opponent() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, true);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Arsenal, card: Some(CardIdx::new(mm_idx))});

        // The arsenal is face-down and the end-of-turn draw is hidden: the
        // omniscient log and the owner's log name the cards, the opponent's log
        // only sees that a card was arsenaled and one was drawn.
        let last_two = |log: &Option<Vec<String>>| {
            let log = log.as_ref().unwrap();
            log[log.len() - 2..].to_vec()
        };
        assert_eq!(last_two(&gs.log)[0], "P1 arsenals MuscleMuttY");
        assert!(last_two(&gs.log)[1].starts_with("P1 draws ["));
        assert_eq!(last_two(&gs.p1.log)[0], "P1 arsenals MuscleMuttY");
        assert!(last_two(&gs.p1.log)[1].starts_with("P1 draws ["));
        assert_eq!(last_two(&gs.p2.log), vec![
            "P1 arsenals a card".to_string(),
            "P1 draws 1 cards".to_string(),
        ]);
    }

    #[test]
    fn test_close_combat_chain_logs_once() {
        // With logging on, closing a populated chain logs a single public event
        // after the pass itself.
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, true);
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
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);

        let len = gs.log.as_ref().unwrap().len();
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        let log = gs.log.as_ref().unwrap();
        assert_eq!(log.len(), len + 2);
        assert_eq!(log[len], "P1 passes");
        assert_eq!(log[len + 1], "combat chain closes");

        // An empty chain closes silently: only the pass is logged.
        gs.phase = Phase::Action;
        let len = gs.log.as_ref().unwrap().len();
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.log.as_ref().unwrap().len(), len + 1);
    }

    #[test]
    fn test_initial_hand() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        assert_eq!(gs.p1.hand_size, 4);
        assert_eq!(gs.p1.deck_size, 36);
        assert_eq!(gs.p2.hand_size, 4);
        assert_eq!(gs.p2.deck_size, 36);

        let hand = get_card_states_from_location(&gs, PlayerIndex::P1, CardLocation::P1Hand);

        assert_eq!(hand.len(), 4);

        assert_eq!(hand[0].card, Card::MuscleMuttY);
        assert_eq!(hand[1].card, Card::PackCallY);
        assert_eq!(hand[2].card, Card::RagingOnslaughtY);
        assert_eq!(hand[3].card, Card::ClearingBellowB);

        let hand2 = get_card_states_from_location(&gs, PlayerIndex::P2, CardLocation::P2Hand);

        assert_eq!(hand2.len(), 4);

        assert_eq!(hand2[0].card, Card::InTheSwingR);
        assert_eq!(hand2[1].card, Card::SecondSwingR);
        assert_eq!(hand2[2].card, Card::SharpenSteelR);
        assert_eq!(hand2[3].card, Card::DrivingBladeY);
    }

    #[test]
    fn test_draw_cards_includes_last_card() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);

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

    // ── Win / draw phases ──────────────────────────────────────────────────

    use crate::game_state::MAX_TURNS;

    /// A fresh, reset game just past the first-player choice — both heroes at 20
    /// life, the turn counter at 1, in the Action phase. A convenient starting
    /// point for poking the win/draw conditions.
    fn fresh_game() -> Gamestate {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        gs
    }

    #[test]
    fn test_is_terminal_classifies_phases() {
        assert!(Phase::Player1Win.is_terminal());
        assert!(Phase::Player2Win.is_terminal());
        assert!(Phase::Draw.is_terminal());
        // Every non-terminal phase reports false.
        for p in [
            Phase::Start, Phase::ChooseFirst, Phase::Action, Phase::ActionPitch,
            Phase::ActionInstant, Phase::Defend, Phase::Reaction, Phase::ReactionPitch,
            Phase::Arsenal, Phase::PitchOrder,
        ] {
            assert!(!p.is_terminal(), "{:?} should not be terminal", p);
        }
    }

    #[test]
    fn test_check_game_end_no_op_while_both_alive() {
        let mut gs = fresh_game();
        // Both heroes alive and well under the turn limit: nothing changes.
        assert!(!gs.check_game_end());
        assert!(!gs.is_game_over());
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_check_game_end_player1_wins_when_p2_dies() {
        let mut gs = fresh_game();
        gs.p2.life = 0;

        assert!(gs.check_game_end());
        assert!(gs.is_game_over());
        assert_eq!(gs.phase, Phase::Player1Win);
    }

    #[test]
    fn test_check_game_end_player2_wins_when_p1_dies() {
        let mut gs = fresh_game();
        gs.p1.life = 0;

        assert!(gs.check_game_end());
        assert!(gs.is_game_over());
        assert_eq!(gs.phase, Phase::Player2Win);
    }

    #[test]
    fn test_check_game_end_draw_at_turn_limit() {
        let mut gs = fresh_game();
        // Reaching the turn cap with both heroes alive is a draw.
        gs.turn_count = MAX_TURNS;

        assert!(gs.check_game_end());
        assert!(gs.is_game_over());
        assert_eq!(gs.phase, Phase::Draw);
    }

    #[test]
    fn test_check_game_end_is_idempotent() {
        let mut gs = fresh_game();
        gs.p2.life = 0;
        assert!(gs.check_game_end());
        assert_eq!(gs.phase, Phase::Player1Win);

        // A win already recorded is never overwritten — even if the turn counter
        // later crosses the draw threshold, the original result stands.
        gs.turn_count = MAX_TURNS;
        assert!(gs.check_game_end());
        assert_eq!(gs.phase, Phase::Player1Win);
    }

    #[test]
    fn test_death_takes_priority_over_draw() {
        let mut gs = fresh_game();
        // Both the turn cap and a dead hero in the same call: a decided game beats
        // a draw, so player 1 is declared the winner.
        gs.turn_count = MAX_TURNS;
        gs.p2.life = 0;

        assert!(gs.check_game_end());
        assert_eq!(gs.phase, Phase::Player1Win);
    }

    #[test]
    fn test_begin_turn_increments_turn_count() {
        let mut gs = fresh_game();
        // The first-player choice ran one `begin_turn`, so the counter is at 1.
        assert_eq!(gs.turn_count, 1);

        begin_turn(&mut gs);
        assert_eq!(gs.turn_count, 2);
        assert!(!gs.is_game_over());
    }

    #[test]
    fn test_begin_turn_draws_at_turn_limit() {
        let mut gs = fresh_game();
        // One more turn would tip the counter to the cap; beginning it ends the
        // game in a draw.
        gs.turn_count = MAX_TURNS - 1;
        begin_turn(&mut gs);

        assert_eq!(gs.turn_count, MAX_TURNS);
        assert_eq!(gs.phase, Phase::Draw);
        assert!(gs.is_game_over());
    }

    #[test]
    fn test_step_is_frozen_once_game_is_over() {
        let mut gs = fresh_game();
        gs.p2.life = 0;
        gs.check_game_end();
        assert_eq!(gs.phase, Phase::Player1Win);

        // Any further action is a no-op: the phase, the turn counter, and life
        // totals are all untouched.
        let turns_before = gs.turn_count;
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.phase, Phase::Player1Win);
        assert_eq!(gs.turn_count, turns_before);
        assert_eq!(gs.p1.life, 20);
        assert_eq!(gs.p2.life, 0);
    }

    #[test]
    fn test_terminal_phase_has_no_legal_actions() {
        let mut gs = fresh_game();
        for terminal in [Phase::Player1Win, Phase::Player2Win, Phase::Draw] {
            gs.phase = terminal;
            assert!(legal_actions(&gs).is_empty(), "{:?} should offer no actions", terminal);
        }
    }

    #[test]
    fn test_lethal_combat_ends_game_with_player1_win() {
        // Drive Rhinar's Muscle Mutt (power 6) attack against a Dorinthea whose
        // life has been whittled down to 6. With no block, the 6 damage is
        // exactly lethal: combat resolution should end the game as a player-1 win
        // rather than returning to the Action phase.
        let mut gs = step_to_dorinthea_defending();
        gs.p2.life = 6;

        // Defender declares no blocks → Reaction window.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);

        // Both pass → combat resolves → 6 damage drops Dorinthea to 0.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.p2.life, 0);
        assert_eq!(gs.phase, Phase::Player1Win);
        assert!(gs.is_game_over());
    }

    #[test]
    fn test_blocked_lethal_attack_does_not_end_game() {
        // The same low-life Dorinthea (6 life) blocks Muscle Mutt with Driving
        // Blade (defense 3): only 3 damage gets through, leaving her at 3 life, so
        // the game continues into the Action phase as usual.
        let mut gs = step_to_dorinthea_defending();
        gs.p2.life = 6;

        let db_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in Dorinthea's opening hand");
        step(&mut gs, Action{ typ: ActionType::Defend, card: Some(CardIdx::new(db_idx))});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.p2.life, 3);
        assert_eq!(gs.phase, Phase::Action);
        assert!(!gs.is_game_over());
    }

    #[test]
    fn test_overkill_damage_still_ends_game() {
        // A hero on 2 life facing 6 unblocked power: life saturates at 0 (no
        // underflow) and the game still ends as a player-1 win.
        let mut gs = step_to_dorinthea_defending();
        gs.p2.life = 2;

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.p2.life, 0);
        assert_eq!(gs.phase, Phase::Player1Win);
    }

    #[test]
    fn test_reset_clears_turn_count() {
        let mut gs = fresh_game();
        gs.turn_count = 17;
        reset(&mut gs, false);
        assert_eq!(gs.turn_count, 0);
    }

    // ── Logging ────────────────────────────────────────────────────────────

    #[test]
    fn test_logging_disabled_leaves_logs_none() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert!(gs.log.is_none());
        assert!(gs.p1.log.is_none());
        assert!(gs.p2.log.is_none());
    }

    #[test]
    fn test_logging_events_as_they_happen() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, true);

        // The first-player choice logs three events: the choice itself, then
        // each player's opening-hand draw the moment it happens.
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.log.as_ref().unwrap().len(), 3);
        assert_eq!(gs.log.as_ref().unwrap()[0], "P1 chooses to go first");

        // Playing and pitching each log one public event, identical in all
        // three logs.
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

        for log in [&gs.log, &gs.p1.log, &gs.p2.log] {
            let log = log.as_ref().expect("logging is enabled");
            assert_eq!(log.len(), 5);
            assert_eq!(log[3], "P1 plays MuscleMuttY");
            assert_eq!(log[4], "P1 pitches ClearingBellowB");
        }
    }

    #[test]
    fn test_logging_pass_and_stack_resolution_in_one_step() {
        // Drive Muscle Mutt onto the stack with logging on.
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, true);
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
        assert_eq!(gs.phase, Phase::ActionInstant);

        // First pass logs only the pass.
        let len = gs.log.as_ref().unwrap().len();
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.log.as_ref().unwrap().len(), len + 1);
        assert_eq!(gs.log.as_ref().unwrap()[len], "P1 passes");

        // The second pass logs two events in the same step: the pass, then the
        // attack resolving off the stack.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        let log = gs.log.as_ref().unwrap();
        assert_eq!(log.len(), len + 3);
        assert_eq!(log[len + 1], "P2 passes");
        assert_eq!(log[len + 2], "P1 attacks with MuscleMuttY");
    }

    #[test]
    fn test_logging_hides_opponent_draws() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, true);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Entries 1 and 2 are the opening-hand draws (entry 0 is the choice).
        // Seed 42 opening hands (see test_initial_hand).
        let full = gs.log.as_ref().unwrap();
        let p1_view = gs.p1.log.as_ref().unwrap();
        let p2_view = gs.p2.log.as_ref().unwrap();

        // The omniscient log names cards from both hands.
        assert!(full[1].contains("MuscleMuttY"), "full log should name p1's cards: {}", full[1]);
        assert!(full[2].contains("DrivingBladeY"), "full log should name p2's cards: {}", full[2]);

        // Each player sees their own cards but only a count for the opponent's.
        assert!(p1_view[1].contains("MuscleMuttY"));
        assert_eq!(p1_view[2], "P2 draws 4 cards");
        assert!(p2_view[2].contains("DrivingBladeY"));
        assert_eq!(p2_view[1], "P1 draws 4 cards");
    }

    #[test]
    fn test_logging_damage_message_includes_life_change() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, true);
        // Re-drive step_to_dorinthea_defending with logging on, then run out the
        // reaction window so Muscle Mutt's 6 damage lands.
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
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        let entries_before = gs.log.as_ref().unwrap().len();
        // The final pass closes the reaction window and deals 6 damage: the
        // step logs the pass, then a single damage message carrying the life
        // change inline.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.p2.life, 14);

        for log in [&gs.log, &gs.p1.log, &gs.p2.log] {
            let log = log.as_ref().expect("logging is enabled");
            assert_eq!(log.len(), entries_before + 2);
            assert_eq!(log[log.len() - 2], "P2 passes");
            assert_eq!(
                log[log.len() - 1],
                "P2 takes 6 damage from MuscleMuttY attack (life: 20->14)"
            );
        }
    }

    #[test]
    fn test_pack_hunt_intimidate_banishes_opponent_card() {
        // Pack Hunt is an attack action carrying Intimidate. When it resolves
        // (onto the attacker's combat chain) the defending player must banish a
        // random card from their hand into their Intimidate banish zone.
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Relabel two of Rhinar's (p1) hand cards: one to Pack Hunt (the attack
        // action to play, cost 2) and one to Clearing Bellow (pitch 3) to pay for
        // it. Only the card stats matter here; the zones stay untouched.
        let hand: Vec<usize> = gs.p1.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
        let ph_idx = hand[0];
        let pitch_idx = hand[1];
        gs.cards[ph_idx].card = Card::PackHuntR;
        gs.cards[pitch_idx].card = Card::ClearingBellowB;

        // The opponent (p2) starts with a full opening hand to banish from.
        let victim_hand_before = gs.p2.hand_size;
        assert!(victim_hand_before > 0);
        assert_eq!(gs.p2.intimidate_banish_idx, None);

        // Play Pack Hunt and pitch Clearing Bellow to cover its cost, opening the
        // instant window with the attack on the stack.
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(ph_idx))});
        assert_eq!(gs.phase, Phase::ActionPitch);
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(pitch_idx))});
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.stack_top().map(|p| p.index.get()), Some(ph_idx));

        // Both players pass: Pack Hunt resolves onto p1's combat chain and its
        // Intimidate fires, forcing p2 to banish one card from hand.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.cards[ph_idx].location, CardLocation::P1CombatChain);

        // Exactly one card left p2's hand for the Intimidate banish zone.
        assert_eq!(gs.p2.hand_size, victim_hand_before - 1);
        let banished_idx = gs.p2.intimidate_banish_idx.expect("a card was banished to Intimidate");
        assert_eq!(gs.cards[banished_idx.get()].location, CardLocation::P2IntimidateBanish);
        let banished = get_card_states_from_location(&gs, PlayerIndex::P2, CardLocation::P2IntimidateBanish);
        assert_eq!(banished.len(), 1);
    }

    #[test]
    fn test_clearing_bellow_intimidate_banishes_opponent_card() {
        // Clearing Bellow is a (non-attack) action carrying Intimidate. When it
        // resolves (to the graveyard) its Intimidate still fires, forcing the
        // opponent to banish a random card from hand into their Intimidate banish
        // zone.
        let (mut gs, cb_idx) = instant_phase_with_one_card();

        let victim_hand_before = gs.p2.hand_size;
        assert!(victim_hand_before > 0);
        assert_eq!(gs.p2.intimidate_banish_idx, None);

        // Both players pass: Clearing Bellow resolves to p1's graveyard and its
        // Intimidate fires.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Graveyard);

        // Exactly one card left p2's hand for the Intimidate banish zone.
        assert_eq!(gs.p2.hand_size, victim_hand_before - 1);
        let banished_idx = gs.p2.intimidate_banish_idx.expect("a card was banished to Intimidate");
        assert_eq!(gs.cards[banished_idx.get()].location, CardLocation::P2IntimidateBanish);
        let banished = get_card_states_from_location(&gs, PlayerIndex::P2, CardLocation::P2IntimidateBanish);
        assert_eq!(banished.len(), 1);
    }

    #[test]
    fn test_intimidate_banished_card_returns_to_hand_in_arsenal() {
        // A card Intimidate-banished during the turn returns to its owner's hand
        // as the turn passes into the Arsenal phase; the ordinary banished zone is
        // untouched. Clearing Bellow (a non-attack action with Intimidate and Go
        // Again) is convenient: after it resolves play returns to p1's Action
        // phase, and one more pass closes the turn into the Arsenal phase.
        let (mut gs, cb_idx) = instant_phase_with_one_card();
        let victim_hand_before = gs.p2.hand_size;

        // Both players pass: Clearing Bellow resolves and its Intimidate banishes a
        // random card from p2's hand. We are back in p1's Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.cards[cb_idx].location, CardLocation::P1Graveyard);

        let banished_idx = gs.p2.intimidate_banish_idx
            .expect("a card was banished to Intimidate")
            .get();
        assert_eq!(gs.cards[banished_idx].location, CardLocation::P2IntimidateBanish);
        assert_eq!(gs.p2.hand_size, victim_hand_before - 1);

        // p1 passes, closing the turn into the Arsenal phase. Entering it returns
        // the Intimidate-banished card to p2's hand and empties the zone.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);

        assert_eq!(gs.p2.intimidate_banish_idx, None);
        assert_eq!(gs.cards[banished_idx].location, CardLocation::P2Hand);
        assert_eq!(gs.p2.hand_size, victim_hand_before);
        assert!(get_card_states_from_location(&gs, PlayerIndex::P2, CardLocation::P2IntimidateBanish).is_empty());
        // The returned card is reachable again by walking p2's hand.
        assert!(gs.p2.hand_iter(&gs.cards).any(|(idx, _)| idx == banished_idx));
    }
}

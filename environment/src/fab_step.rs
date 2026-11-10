use crate::action::{Action,ActionType};
use crate::card_counts::{CardCounts, DeckCardId};
use crate::game_state::{CHAIN_LINKS,MAX_QUICKENS,DeckSlot,Gamestate,Phase,Player,PendingCard,PlayerIndex,Zone};
use crate::cards::{Card,CardClass,CardData,CardType,Keyword,WeaponType};
use crate::card_effects::{Ability,OnPlayEffect,OnPlayConditionType,OnPlayEffectType,AdditionalCostType,ConstantEffect,DefendEffect,MentorEffect,PlayCondition,AttackEffect,AttackEffectType,AttackKind};
use crate::fab_game::{shuffle_deck_for, GameRng};
use crate::deck_cards::TokenCard;
use rand::RngExt;

/// Apply `act`, the active player's decision, to `gs`. Any random choice the
/// action sets off (an Intimidate or random discard, a deck shuffle) is drawn
/// from `rng`, which the caller keeps beside the game state (see `GameRng`).
pub fn step(gs: &mut Gamestate, rng: &mut GameRng, act: Action) {
    // Once the game has been won or drawn it is frozen: no further actions are
    // processed.
    if gs.is_game_over() {
        return;
    }
    #[cfg(debug_assertions)]
    gs.debug_check_card_totals();
    match gs.phase {
        Phase::ChooseFirst => handle_choose_first(gs, act),
        Phase::MentorFlip => handle_mentor_flip_phase(gs, act),
        Phase::Action => handle_action_phase(gs, rng, act),
        Phase::ActionPitch | Phase::ReactionPitch | Phase::DefendPitch => handle_pitch_phase(gs, rng, act),
        Phase::ActionInstant => handle_action_instant_phase(gs, rng, act),
        Phase::Defend => handle_defend_phase(gs, act),
        Phase::DefendTriggers | Phase::Reaction => handle_reaction_phase(gs, rng, act),
        Phase::DefendPayment => handle_defend_payment_phase(gs, act),
        Phase::RevealChoice => handle_reveal_choice_phase(gs, rng, act),
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
fn hand_card_names(player: &Player) -> String {
    card_names(player, player.hand)
}

/// Every card in `counts` (one entry per copy, in id order), joined for a log
/// message.
fn card_names(player: &Player, counts: CardCounts) -> String {
    counts
        .iter()
        .flat_map(|(id, n)| std::iter::repeat_n(player.card(id), n as usize))
        .map(|card| format!("{:?}", card))
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
    let named = format!("{} draws [{}]", player_name(pid), hand_card_names(player));
    let hidden = format!("{} draws {} cards", player_name(pid), player.hand_size());
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
    draw_to_intellect(&mut gs.p1);
    log_opening_hand(gs, PlayerIndex::P1);
    draw_to_intellect(&mut gs.p2);
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
    // Any unconsumed attack-power bonus from a prior turn is cleared so it can
    // never leak into this turn's attacks.
    gs.p1.attack_power_bonus = 0;
    gs.p2.attack_power_bonus = 0;
    // Likewise clear any unconsumed conditional Go Again so it cannot leak into
    // this turn's attacks.
    gs.p1.set_attack_go_again_bonus(false);
    gs.p2.set_attack_go_again_bonus(false);
    // Likewise clear any unconsumed "next brute attack" power bonus so it cannot
    // leak into this turn's attacks.
    gs.p1.next_brute_attack_action_bonus = 0;
    gs.p2.next_brute_attack_action_bonus = 0;
    // Likewise clear any unconsumed "next attack action card" power bonus so it
    // cannot leak into this turn's attacks.
    gs.p1.next_attack_action_bonus = 0;
    gs.p2.next_attack_action_bonus = 0;
    // Likewise clear any unconsumed conditional "next brute attack" bonus so it
    // cannot leak into this turn's attacks.
    gs.p1.next_brute_attack_conditional_bonus = 0;
    gs.p2.next_brute_attack_conditional_bonus = 0;
    // Likewise clear any unconsumed "next weapon attack" bonus (En Garde).
    gs.p1.next_weapon_attack_bonus = 0;
    gs.p2.next_weapon_attack_bonus = 0;
    gs.p1.next_attack_bonus = 0;
    gs.p2.next_attack_bonus = 0;
    // Clear the per-turn "has intimidated" flag so a prior turn's Intimidate
    // can never satisfy this turn's "if you've intimidated" conditions.
    gs.p1.set_has_intimidated(false);
    gs.p2.set_has_intimidated(false);
    // "Once per turn" activations are freed for both players at the same
    // boundary as the bonuses above.
    gs.p1.abilities_used = CardCounts::EMPTY;
    gs.p2.abilities_used = CardCounts::EMPTY;
    // Dorinthea's extra Dawnblade attack lasts for the turn it was granted in.
    gs.p1.set_extra_weapon_attack(false);
    gs.p2.set_extra_weapon_attack(false);
    // "At the start of your turn, you may turn [a face-down mentor] face up":
    // when the turn player has one in their arsenal, the turn opens on that
    // decision instead of going straight to the Action phase (see
    // `handle_mentor_flip_phase`). The callers have already set the phase to
    // Action, so with no such mentor there is nothing to change.
    if face_down_mentor_in_arsenal(gs, gs.turn_player).is_some() {
        gs.phase = Phase::MentorFlip;
        gs.active_player = gs.turn_player;
    }
    gs.check_game_end();
}

/// The face-down mentor in `pid`'s arsenal, if there is one — the card the
/// `MentorFlip` phase offers to turn face up. Every mentor in the catalog
/// carries the "at the start of your turn, you may turn [it] face up" clause,
/// so this keys off the card type rather than any one card's effect. A mentor
/// already face up is never offered again: nothing turns it back down while it
/// stays in the arsenal.
pub(crate) fn face_down_mentor_in_arsenal(gs: &Gamestate, pid: PlayerIndex) -> Option<DeckCardId> {
    let player = gs.player(pid);
    let id = player.arsenal?;
    (player.card(id).data().typ == CardType::Mentor && !player.arsenal_face_up()).then_some(id)
}

/// Handle the turn player's decision in the `MentorFlip` phase: flip the
/// face-down mentor in their arsenal face up (`FlipFaceUp`), or leave it face
/// down (`Pass`). Either way the turn carries on into the Action phase.
///
/// Leaving it down hides the card from the opponent, but a face-down mentor
/// does nothing — it cannot be played, pitched, or trigger — so the choice is
/// between information and the mentor's ability. A declined mentor is offered
/// again at the start of its owner's next turn.
fn handle_mentor_flip_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        ActionType::FlipFaceUp => turn_mentor_face_up(gs, gs.active_player),
        ActionType::Pass => {
            // Private to the decider: the opponent only ever sees a face-down
            // arsenal card, and a public "leaves it face down" would tell them
            // it is a mentor — the only card that is ever offered the flip.
            if gs.logging_enabled() {
                let pid = gs.active_player;
                gs.log_private(pid, format!("{} leaves their arsenal face down", player_name(pid)));
            }
        }
        _ => return,
    }
    gs.phase = Phase::Action;
}

/// Mark `player`'s arsenal card face up. A face-up card is public — both players
/// can see what it is (see `Player::visibility`). Every place that turns a card
/// face up goes through here.
fn turn_arsenal_face_up(player: &mut Player) {
    debug_assert!(player.arsenal.is_some(), "only an arsenal card can be face up");
    player.set_arsenal_face_up(true);
}

/// Turn the mentor face down in `pid`'s arsenal face up. It
/// becomes public, and stays face up for as long as it remains in the arsenal
/// (`Player::arsenal_face_up` is cleared only as the arsenal empties).
fn turn_mentor_face_up(gs: &mut Gamestate, pid: PlayerIndex) {
    let player = gs.player_mut(pid);
    turn_arsenal_face_up(player);
    if gs.logging_enabled() {
        let player = gs.player(pid);
        let card = player.card(player.arsenal.expect("a mentor is in the arsenal"));
        gs.log_public(format!("{} turns {:?} face up in their arsenal", player_name(pid), card));
    }
}

fn handle_action_phase(gs: &mut Gamestate, rng: &mut GameRng, act: Action) {
    match act.typ {
        // Playing a card, or activating an ability (an armor piece or a weapon
        // swing) both commit a card that must then be paid for (see
        // `commit_card_to_pending`).
        ActionType::PlayCard | ActionType::Activate => {
            commit_card_to_pending(gs, rng, act);
        }
        // Breaking the combat chain clears it ahead of the next attack, which
        // then opens a fresh chain on link 0. The action phase carries on.
        ActionType::CloseChain => {
            if gs.logging_enabled() {
                gs.log_public(format!("{} breaks the combat chain", player_name(gs.active_player)));
            }
            close_combat_chain(gs);
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
fn handle_action_instant_phase(gs: &mut Gamestate, rng: &mut GameRng, act: Action) {
    match act.typ {
        // Only instants are legal here (see `legal_instant_phase`); they commit
        // through the same pending/pitch flow as an action-phase play.
        ActionType::PlayCard => {
            commit_card_to_pending(gs, rng, act);
        }
        ActionType::Pass => {
            handle_priority_pass(gs, rng);
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
fn commit_card_to_pending(gs: &mut Gamestate, rng: &mut GameRng, act: Action) {
    // Cost of committing this card: an Activate (an armor ability or a weapon
    // swing) pays its ability's cost; a played card pays its own catalog cost.
    let card_ref = act.card.expect("play/activate action requires a card");
    let owner = gs.active_player;
    let card = gs.player(owner).card(card_ref.id);
    let cost = action_cost(act.typ, card.data());

    // Committing a card reveals it, so the announcement is public.
    if gs.logging_enabled() {
        let verb = if act.typ == ActionType::Activate { "activates" } else { "plays" };
        gs.log_public(format!("{} {} {:?}", player_name(owner), verb, card));
    }

    let already_paid = gs.player(owner).resources >= cost;

    gs.pending_card = Some(PendingCard {
        owner,
        card: card_ref.id,
        from: card_ref.zone,
        typ: act.typ,
    });

    // Affordable outright: no pitching needed, commit to the stack now.
    // Otherwise leave it pending (still in its zone) and pitch for it.
    if already_paid {
        commit_pending_to_stack(gs, rng);
    } else {
        // Pitching during a reaction window stays within that window: each has
        // a pitch phase of its own so we return to the window we interrupted
        // once the cost is paid, rather than to whichever came later. Every
        // other caller (the Action and ActionInstant phases) uses ActionPitch.
        gs.phase = match gs.phase {
            Phase::Reaction => Phase::ReactionPitch,
            Phase::DefendTriggers => Phase::DefendPitch,
            _ => Phase::ActionPitch,
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
fn handle_priority_pass(gs: &mut Gamestate, rng: &mut GameRng) {
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
            resolve_top_of_stack(gs, rng);
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
    else if gs.phase == Phase::DefendTriggers {
        // The defend-trigger window is over, but combat is not: hand over to the
        // ordinary reaction window, where reactions are legal again and the
        // stack starts empty. Damage waits for that window to close.
        gs.phase = Phase::Reaction;
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
/// their side of the chain link the attack occupies, and the phase stays on
/// Defend so further blockers can be declared. Passing finishes declaring
/// blockers and advances to the next itinerary phase — the turn player's
/// Reaction window, pushed onto the return stack when the attack was committed.
fn handle_defend_phase(gs: &mut Gamestate, act: Action) {
    match act.typ {
        ActionType::Defend => commit_blocker(gs, act.card_ref().id, act.card_ref().zone),
        ActionType::Pass => {
            if gs.logging_enabled() {
                gs.log_public(format!("{} passes", player_name(gs.active_player)));
            }
            // Blockers are all declared now, so every "when you defend with"
            // trigger for this attack goes on the stack together. They resolve
            // in a window of their own; with none to resolve there is nothing
            // to open it for, so we go straight to the reaction window and an
            // ordinary block costs no extra round of priority.
            gs.phase = if push_defend_triggers(gs) {
                Phase::DefendTriggers
            } else {
                Phase::Reaction
            };
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
            let id = act.card_ref().id;

            // The arsenal is face-down: its owner (and the omniscient log) know
            // the card, the opponent only sees that a card was set aside.
            if gs.logging_enabled() {
                let named = format!("{} arsenals {:?}", player_name(pid), gs.player(pid).card(id));
                let hidden = format!("{} arsenals a card", player_name(pid));
                let (p1_msg, p2_msg) = if pid == PlayerIndex::P1 {
                    (named.clone(), hidden)
                } else {
                    (hidden, named.clone())
                };
                gs.log_views(named, p1_msg, p2_msg);
            }

            // Move the chosen card out of the hand into the arsenal slot, face
            // down. The opponent knows what it is only if the hand held known
            // copies of that one card and nothing else.
            let player = gs.player_mut(pid);
            let deducible = player.hand_is_one_known_card();
            player.remove_from_hand_face_down(id);
            put_in_zone(player, id, Zone::Arsenal);
            player.set_arsenal_known(deducible);

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
    let pitch = gs.player(pid).pitch;
    match pitch.len() {
        0 => true,
        1 => {
            bottom_pitched_card(gs, pid, pitch.nth(0).expect("pitch zone holds a card"));
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
    let pid = gs.active_player;
    bottom_pitched_card(gs, pid, act.card_ref().id);

    if !gs.player(pid).pitch.is_empty() {
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

/// Move a copy of `id` out of `pid`'s pitch zone onto the bottom of their deck.
/// Pitched cards sit face-up, so the move is public.
fn bottom_pitched_card(gs: &mut Gamestate, pid: PlayerIndex, id: DeckCardId) {
    if gs.logging_enabled() {
        gs.log_public(format!(
            "{} puts {:?} on the bottom of their deck",
            player_name(pid), gs.player(pid).card(id)
        ));
    }
    let player = gs.player_mut(pid);
    player.remove_from(Zone::Pitch, id);
    // These cards were pitched face-up, so both players already saw them; on
    // the bottom of the deck they stay known.
    player.deck.push_back(DeckSlot::known(id));
}

/// End the turn player's turn: they draw back up to intellect, then the turn
/// and active player both flip to the opponent, whose Action phase begins.
/// Unspent resource points are lost at the end of the turn — both players',
/// since the defender can bank leftovers from reaction-window pitches too.
fn end_turn(gs: &mut Gamestate) {
    let pid = gs.turn_player;
    // The cards about to be drawn, named for the log before they leave the deck.
    let drawn_names = if gs.logging_enabled() {
        let player = gs.player(pid);
        let need = player.intellect.saturating_sub(player.hand_size()) as usize;
        Some(player.deck.iter().take(need)
            .map(|slot| format!("{:?}", player.card(slot.card())))
            .collect::<Vec<_>>())
    } else {
        None
    };
    let drawn = draw_to_intellect(gs.player_mut(pid));
    if let Some(names) = drawn_names {
        log_end_of_turn_draw(gs, pid, drawn, &names);
    }

    gs.p1.resources = 0;
    gs.p2.resources = 0;

    gs.turn_player = gs.turn_player.opponent();
    gs.active_player = gs.turn_player;
    gs.phase = Phase::Action;
    begin_turn(gs);
}

/// Log that `pid` drew back up to intellect at the end of their turn. The drawn
/// cards are hidden information: the omniscient log and the drawing player's
/// own log name them (`names`, read off the top of the deck before the draw),
/// while the opponent's log only records the count. Drawing nothing logs
/// nothing.
fn log_end_of_turn_draw(gs: &mut Gamestate, pid: PlayerIndex, drawn: u8, names: &[String]) {
    if !gs.logging_enabled() || drawn == 0 {
        return;
    }
    let named = format!("{} draws [{}]", player_name(pid), names.join(", "));
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
fn handle_reaction_phase(gs: &mut Gamestate, rng: &mut GameRng, act: Action) {
    match act.typ {
        ActionType::PlayCard | ActionType::Activate => {
            commit_card_to_pending(gs, rng, act);
        }
        ActionType::Pass => {
            handle_priority_pass(gs, rng);
        }
        _ => {}
    }
}

/// Move a copy of the defender's chosen blocker `id` out of their hand and onto
/// the combat chain. All blockers declared against the current attack join a
/// *single* chain link on the defender's side — an unordered `CardCounts`, so
/// the one link can hold any number of blocking cards. The defender is the
/// active player; the attacker's card sits on the *attacker's* chain, a
/// separate array.
fn commit_blocker(gs: &mut Gamestate, id: DeckCardId, from: Zone) {
    let pid = gs.active_player;
    // Declaring a blocker reveals it, so the announcement is public.
    if gs.logging_enabled() {
        gs.log_public(format!("{} defends with {:?}", player_name(pid), gs.player(pid).card(id)));
    }
    // A blocker joins the link of the attack it is blocking — the attacker's
    // current chain link — so that link's damage is calculated against these
    // blockers alone and they play no part in a later attack this turn. Only the
    // turn player attacks, so the attacker is always the turn player.
    let link = current_chain_link(gs.player(gs.turn_player));
    // A blocker comes from the hand or, for a piece of equipment, its slot.
    let player = gs.player_mut(pid);
    take_from_zone(player, id, from);
    player.chain_link[link].add(id);
    if from == Zone::Hand {
        player.set_defended_from_hand(true);
    }
}

/// Put a `DefendTrigger` on the stack for each declared blocker carrying a
/// "when you defend with this" trigger, returning whether any were pushed.
/// Called once the defender has finished declaring blockers, so every trigger
/// for this attack goes on the stack together and each is then open to
/// responses in the `DefendTriggers` window that follows, resolving one layer
/// at a time like any other stack entry. The caller opens that window only when
/// this returns true.
///
/// Blockers without a `defend_effect` — nearly all of them — put nothing on the
/// stack, so an ordinary block still costs no extra priority rounds.
///
/// A chain link is unordered, so the triggers are pushed in card-id order, one
/// per copy, and resolve in the reverse of that. Real FaB lets their controller
/// order simultaneous triggers; the engine has no choice phase for that, and
/// none of the catalog's defend triggers depend on the order.
fn push_defend_triggers(gs: &mut Gamestate) -> bool {
    let mut pushed = false;
    let defender = gs.active_player;
    // Only the turn player attacks, so the link being defended is the current
    // link of the turn player's chain.
    let link = current_chain_link(gs.player(gs.turn_player));
    let player = gs.player(defender);
    let blockers = player.chain_link[link];
    let deck_kind = player.deck_kind;

    for (id, copies) in blockers.iter() {
        if deck_kind.card(id).data().defend_effect.is_none() {
            continue;
        }
        for _ in 0..copies {
            gs.push_to_stack(PendingCard {
                owner: defender,
                card: id,
                from: Zone::CombatChain,
                typ: ActionType::DefendTrigger,
            });
            pushed = true;
        }
    }
    pushed
}

/// Fire the "when you defend with this" trigger of `id`, a blocker declared by
/// `pid`, as that trigger resolves off the stack. Cards without a
/// `defend_effect` never get a trigger pushed in the first place.
///
/// Returns `true` when the trigger is waiting on a choice from its controller
/// (an optional payment, `Phase::DefendPayment`), in which case the caller
/// must leave the window as it is: `finish_defend_trigger` runs once the
/// choice is made.
fn apply_defend_effect(gs: &mut Gamestate, pid: PlayerIndex, id: DeckCardId) -> bool {
    match &gs.player(pid).card(id).data().defend_effect {
        Some(DefendEffect::Reveal6BottomOtherwise) => {
            reveal_top_keep_6(gs, pid);
            false
        }
        Some(effect @ DefendEffect::PayForPlus2BlockThenDestroy) => {
            // "You may pay {r}": only a choice if it can be paid, from banked
            // resources plus whatever the hand can pitch. Otherwise it is
            // declined without asking.
            let player = gs.player(pid);
            let can_pay = player.resources.saturating_add(player.hand_pitch_total())
                >= effect.resource_cost();
            if !can_pay {
                if gs.logging_enabled() {
                    gs.log_public(format!("{} cannot pay for {:?}", player_name(pid), player.card(id)));
                }
                return false;
            }
            gs.pending_card = Some(PendingCard {
                owner: pid,
                card: id,
                from: Zone::CombatChain,
                typ: ActionType::DefendTrigger,
            });
            gs.phase = Phase::DefendPayment;
            gs.active_player = pid;
            true
        }
        // `PitchToBlock` is unused and no card in the catalog carries it. It was
        // put here for Rally the Rearguard, whose "+3 block" turned out to be an
        // activated ability rather than a defend trigger — it is implemented as
        // `Ability::DiscardCardPlusBlock` (see `apply_defending_ability`), and
        // the card's `defend_effect` is `None`.
        Some(DefendEffect::PitchToBlock) | None => false,
    }
}

/// Handle the defender's choice in `Phase::DefendPayment`: `Activate` the
/// trigger's card to pay for it, or `Pass` to decline. Paying out of banked
/// resources applies the effect at once; otherwise the defender pitches for it
/// in `DefendPitch` first (see `handle_pitch_phase`).
fn handle_defend_payment_phase(gs: &mut Gamestate, act: Action) {
    let pending = gs.pending_card.expect("a defend payment is pending");
    match act.typ {
        ActionType::Activate => {
            let data = gs.player(pending.owner).card(pending.card).data();
            if gs.player(pending.owner).resources >= action_cost(pending.typ, data) {
                pay_for_defend_trigger(gs);
            } else {
                gs.phase = Phase::DefendPitch;
            }
        }
        ActionType::Pass => {
            if gs.logging_enabled() {
                gs.log_public(format!("{} declines to pay for {:?}",
                    player_name(pending.owner), gs.player(pending.owner).card(pending.card)));
            }
            gs.pending_card = None;
            gs.phase = Phase::DefendTriggers;
            finish_defend_trigger(gs);
        }
        _ => {}
    }
}

/// Pay for the pending defend trigger out of banked resources (which cover
/// it) and apply what the payment buys. Ironhide's: +2 block on the defended
/// link, and the piece is marked to be destroyed when the chain closes. Play
/// returns to the `DefendTriggers` window.
fn pay_for_defend_trigger(gs: &mut Gamestate) {
    let pending = gs.pending_card.take().expect("a defend payment is pending");
    let owner = pending.owner;
    let card = gs.player(owner).card(pending.card);
    let data = card.data();
    let cost = action_cost(pending.typ, data);
    if gs.logging_enabled() {
        gs.log_public(format!("{} pays {} for {:?}: +2 block, destroyed when the chain closes",
            player_name(owner), cost, card));
    }
    let link = current_chain_link(gs.player(gs.turn_player));
    let player = gs.player_mut(owner);
    player.resources -= cost;
    player.chain_defense_bonus[link] = player.chain_defense_bonus[link].saturating_add(2);
    if let Some(slot) = &data.slot {
        player.set_destroy_on_chain_close(slot);
    }
    gs.phase = Phase::DefendTriggers;
    finish_defend_trigger(gs);
}

/// A defend trigger has finished resolving in the `DefendTriggers` window:
/// close the window if the stack is now empty, or hand priority back to the
/// turn player for the next layer.
fn finish_defend_trigger(gs: &mut Gamestate) {
    if gs.stack_is_empty() {
        close_priority_window(gs);
    } else {
        gs.active_player = gs.turn_player;
    }
}

/// Resolve the effect of an ability activated while its card was defending, as
/// that activation resolves off the stack. `id` is `owner`'s card on the combat
/// chain; it stays there.
///
/// Rally the Rearguard's is the only such ability: "+3 block", which lands on
/// the defended link's `Player::chain_defense_bonus` and is counted when the
/// block is totalled at combat damage. Two activations (one per copy) add up;
/// the bonus is cleared as the chain closes (see `close_combat_chain`).
fn apply_defending_ability(gs: &mut Gamestate, owner: PlayerIndex, id: DeckCardId) {
    let Some(ability) = &gs.player(owner).card(id).data().ability else {
        return;
    };
    match ability {
        Ability::DiscardCardPlusBlock => {
            // The card is blocking the turn player's current attack, so it is on
            // that link.
            let link = current_chain_link(gs.player(gs.turn_player));
            let player = gs.player_mut(owner);
            player.chain_defense_bonus[link] = player.chain_defense_bonus[link].saturating_add(3);
        }
        // Every other ability in the catalog is activated from an equipment or
        // weapon slot, never while defending, so none reaches this path.
        _ => {}
    }
}

/// Pack Call's trigger: reveal the top card of `pid`'s own deck. A card with 6
/// or more power stays where it is; anything less goes to the bottom. Either way
/// the reveal is public, so the card stays known to both players — on the bottom
/// that mirrors a face-up pitched card being bottomed (see `bottom_pitch_card`).
///
/// Revealing nothing is a legal outcome: an empty deck is a no-op rather than a
/// panic, since a player can be decked and still declare blockers.
fn reveal_top_keep_6(gs: &mut Gamestate, pid: PlayerIndex) {
    let player = gs.player(pid);
    let Some(top) = player.deck.front() else {
        return;
    };
    let card = player.card(top.card());
    let keeps_top = card.data().power >= 6;

    if gs.logging_enabled() {
        gs.log_public(format!(
            "{} reveals {:?} ({} power) from the top of their deck and puts it on the {}",
            player_name(pid),
            card,
            card.data().power,
            if keeps_top { "top" } else { "bottom" },
        ));
    }

    let deck = &mut gs.player_mut(pid).deck;
    if keeps_top {
        deck.get_mut(0).expect("the deck has a top card").set_known(true);
    } else {
        // Move the top to the bottom: the deck is reordered, never resized.
        deck.pop_front();
        deck.push_back(DeckSlot::known(top.card()));
    }
}

/// Move a defense reaction that has just resolved off the stack onto the
/// defender's side of the combat chain. It joins the very link the blockers
/// declared in the defend step sit on — the attacker's current link — so its
/// block is summed with theirs against this attack alone. Unlike `commit_blocker`
/// the card comes from the stack, which `resolve_top_of_stack` has already popped
/// it from, so there is no zone to take it out of first.
fn commit_defense_reaction(gs: &mut Gamestate, owner: PlayerIndex, id: DeckCardId, from: Zone) {
    // Only the turn player attacks, so the link being defended is the current
    // link of the turn player's chain.
    let link = current_chain_link(gs.player(gs.turn_player));
    let player = gs.player_mut(owner);
    player.chain_link[link].add(id);
    // Played from hand, it is a card from hand defending this link (Reprise);
    // played from the arsenal, it is not.
    if from == Zone::Hand {
        player.set_defended_from_hand(true);
    }
}

/// Resolve the layer at the top of the stack (its most recently added one),
/// popping it and routing its card by the action that committed it (carried on
/// the `PendingCard`, with the card's owner). A no-op when the stack is empty.
///
/// - A **played attack action card**, or a **weapon swing** (the weapon card
///   itself), moves onto the next free link of its owner's combat chain, and we
///   advance to the next itinerary phase — the `Defend` frame banked when the
///   attack was committed, with the defender active to declare blocks.
/// - Anything **else** resolves to its owner's graveyard (or, for a defense
///   reaction, onto the chain). Priority then returns to the turn player; an
///   instant window whose stack is now empty closes instead, but the reaction
///   step only closes on a double pass (see `after_resolution`).
fn resolve_top_of_stack(gs: &mut Gamestate, rng: &mut GameRng) {
    let Some(pending) = gs.pop_stack() else {
        return;
    };
    let owner = pending.owner;
    let top = pending.card;
    let card = gs.player(owner).card(top);
    let data = card.data();

    // A "when you defend with" trigger: only the effect resolves. The card
    // itself was put on the combat chain when it was declared as a blocker and
    // stays there, so — unlike every branch below — nothing moves zones, no
    // action point is spent, and the card's own keywords and on-play effect are
    // not re-fired.
    // An ability activated while its card was defending (Rally the Rearguard).
    // Like a defend trigger, only the effect resolves: the card is on the combat
    // chain from when it was declared as a blocker and stays there, so nothing
    // changes zones, no action point is spent, and the card's own keywords and
    // on-play effect are not fired — it was never played, only activated.
    if pending.typ == ActionType::Activate && pending.from == Zone::CombatChain {
        if gs.logging_enabled() {
            gs.log_public(format!("{}'s {:?} ability resolves", player_name(owner), card));
        }
        apply_defending_ability(gs, owner, top);
        after_resolution(gs);
        return;
    }

    if pending.typ == ActionType::DefendTrigger {
        if gs.logging_enabled() {
            gs.log_public(format!("{}'s {:?} defend trigger resolves", player_name(owner), card));
        }
        // A trigger awaiting its controller's choice finishes once it is made.
        if !apply_defend_effect(gs, owner, top) {
            finish_defend_trigger(gs);
        }
        return;
    }

    if gs.logging_enabled() {
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
        apply_intimidate(gs, rng, owner);
    }

    // "When you play" on-play effects fire as the card resolves, before it goes
    // on to the combat chain (an attack) or the graveyard. `data` is a 'static
    // reference into the card catalog, so holding the effect across the mutable
    // `gs` calls below is fine.
    if let Some(effect) = &data.play_effect {
        apply_on_play_effect(gs, rng, owner, effect);
    }

    // A card joins its owner's combat chain when it is attacking: a played
    // attack action card, or a weapon being swung (the weapon itself joins the
    // chain). Everything else resolves to the graveyard.
    if commits_as_attack(pending.typ, data) {
        // The attacking card leaves the stack for the next free link of its
        // owner's combat chain: every attack this turn takes a link of its own,
        // and combat damage is calculated one link at a time (see
        // `resolve_combat_damage`), so a follow-up attack neither inherits the
        // power of the attack before it nor is blunted by the cards that blocked
        // that one. A turn that produces more attacks than there are links has
        // outgrown the chain, which is a bug in the engine rather than a game
        // state to paper over: fail loudly instead of quietly reusing a link and
        // mis-scoring the attack.
        let attacker = gs.player_mut(owner);
        let link = next_free_chain_link(attacker).unwrap_or_else(|| {
            panic!(
                "combat chain overflowed: {} attacks on one chain, but Player::chain_link \
                 holds only {} links",
                attacker.chain_link.len() + 1,
                attacker.chain_link.len(),
            )
        });
        attacker.chain_link[link].add(top);
        attacker.chain_links_open = link as u8 + 1;
        // A fresh link: nothing has defended it yet (Reprise).
        gs.player_mut(owner.opponent()).set_defended_from_hand(false);
        gs.phase = Phase::Defend;
    } else if commits_as_defense(pending.typ, data, owner, gs.turn_player) {
        // A defense reaction resolving in the defender's reaction window joins
        // the blockers already declared against the attack instead of heading
        // for the graveyard, so its block counts towards stopping this attack
        // (see `resolve_combat_damage`). It leaves for the graveyard with the
        // rest of the link when the chain closes.
        commit_defense_reaction(gs, owner, top, pending.from);
        after_resolution(gs);
    } else {
        // A resolved card goes to the graveyard, face up. For an armor ability
        // whose cost is destroying its card (Blossom of Spring), this is where
        // the destroyed card ends up: it left its equipment slot when the
        // activation was paid for (`commit_pending_to_stack`) and never returns.
        gs.player_mut(owner).add_to(Zone::Graveyard, top);
        // An attack reaction's effect on the attack it is played into (In the
        // Swing's and Ironsong Response's +3), if that attack is still of the
        // right kind and the effect's condition holds.
        if let Some(effect) = &data.attack_effect {
            apply_attack_effect(gs, rng, owner, effect);
        }
        // An activated ability's own effect, e.g. Blossom of Spring's "Gain {r}".
        // The resource floats like a pitched one until spent or the turn ends.
        let gained = activated_ability(pending.typ, data).map_or(0, |a| a.resources_gained());
        if gained > 0 {
            let player = gs.player_mut(owner);
            player.resources = player.resources.saturating_add(gained);
        }
        // A resolving non-attack action — an action card, or an activated
        // action-speed ability — spends the owner's action point unless it has
        // Go Again. Attacks are handled when combat damage resolves; instants and
        // reactions are free.
        if uses_action_point(pending.typ, data) && !data.keyword.contains(Keyword::GoAgain) {
            spend_action_point(gs, owner);
        }
        after_resolution(gs);
    }
}

/// Where priority goes once a layer has resolved off the stack in a priority
/// window, other than an attack resolving onto the chain.
///
/// In the reaction step it always returns to the turn player, and the step
/// closes only when both players pass in succession on an empty stack
/// (`handle_priority_pass`). So after the defender's defense reaction resolves,
/// the attacker can still answer it with an attack reaction (Ironsong
/// Response's Reprise reads the block it added), and the defender can answer
/// that in turn.
///
/// The action step's instant window still closes as its stack empties: it
/// hands back to the `Action` phase with the turn player holding priority,
/// where instants are playable anyway. With layers left, priority returns to
/// the turn player for a fresh round in the same window.
fn after_resolution(gs: &mut Gamestate) {
    if gs.stack_is_empty() && gs.phase != Phase::Reaction {
        close_priority_window(gs);
    } else {
        gs.active_player = gs.turn_player;
    }
}

/// Whether `player` may play `data` now as far as the card's own text goes: its
/// "Play only if ..." condition holds, and an attack reaction has an attack to
/// target. Checked when the card is offered (`legal_actions`), alongside its
/// speed and cost; a card with neither restriction always passes.
pub(crate) fn play_requirements_met(player: &Player, data: &CardData) -> bool {
    let condition_met = match data.play_condition {
        None => true,
        Some(PlayCondition::Played2WeaponAttacks) => player.weapon_attacks_this_turn() >= 2,
        Some(PlayCondition::PlayedWeaponAttack) => player.weapon_attacks_this_turn() >= 1,
    };
    condition_met
        && data.attack_effect.as_ref()
            .is_none_or(|effect| !effect.targeted || affected_attack(player, effect).is_some())
}

/// The attack `effect` applies to for `player`: their current attack, when it
/// is of the kind the effect names. Attack reactions are only played by the
/// attacker, into the attack on the current chain link.
fn affected_attack(player: &Player, effect: &AttackEffect) -> Option<DeckCardId> {
    let attack = player.current_attack()?;
    let data = player.card(attack).data();
    let valid = is_weapon(data) && match effect.attack {
        AttackKind::Weapon => true,
        AttackKind::Sword => matches!(data.weapon_type, Some(WeaponType::Sword2h)),
    };
    valid.then_some(attack)
}

/// Resolve an attack reaction's `effect` on the attack it is played into. It
/// does nothing if that attack is not of the kind the effect names, or if its
/// condition does not hold.
fn apply_attack_effect(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex, effect: &AttackEffect) {
    let Some(attack) = affected_attack(gs.player(owner), effect) else { return };
    if !on_play_condition_met(gs, rng, owner, &effect.condition) {
        return;
    }
    match effect.effect {
        AttackEffectType::GainPower => {
            let player = gs.player_mut(owner);
            player.attack_power_bonus = player.attack_power_bonus.saturating_add(effect.magnitude);
        }
        AttackEffectType::GainGoAgain => attack_gains_go_again(gs, owner, attack),
    }
}

/// A card chosen uniformly at random from `pid`'s hand — every copy equally
/// likely — or `None` when the hand is empty. Draws one index from the rng and
/// reads that copy off the bitfield (`CardCounts::nth`), so the random hand
/// picks made by Intimidate and the discard effects allocate nothing.
fn random_hand_card(gs: &Gamestate, rng: &mut GameRng, pid: PlayerIndex) -> Option<DeckCardId> {
    let hand = gs.player(pid).hand;
    let len = hand.len() as usize;
    if len == 0 {
        return None;
    }
    let k = rng.random_range(0..len);
    hand.nth(k)
}

/// Resolve an Intimidate trigger: the opponent of `attacker` (the player whose
/// card just resolved) banishes one card chosen uniformly at random from their
/// hand into their Intimidate banish zone. A no-op when that opponent's hand is
/// empty. The banished card is hidden information to the attacker, so the log
/// reveals its identity only to the banishing player.
fn apply_intimidate(gs: &mut Gamestate, rng: &mut GameRng, attacker: PlayerIndex) {
    let victim = attacker.opponent();

    // The Intimidate trigger has resolved, so the attacker has now intimidated
    // this turn. Record it even if the victim has no card to banish below, so
    // that "if you've intimidated this turn" conditions key off the keyword
    // triggering rather than off a card actually being banished.
    let intimidator = if attacker == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    intimidator.set_has_intimidated(true);

    let Some(pick) = random_hand_card(gs, rng, victim) else {
        return;
    };

    if gs.logging_enabled() {
        let card = gs.player(victim).card(pick);
        let full = format!("{} banishes {:?} to Intimidate", player_name(victim), card);
        let hidden = format!("{} banishes a card to Intimidate", player_name(victim));
        let (p1_view, p2_view) = if victim == PlayerIndex::P1 {
            (full.clone(), hidden)
        } else {
            (hidden, full.clone())
        };
        gs.log_views(full, p1_view, p2_view);
    }

    // Banished face down: the attacker does not see which card went.
    let player = gs.player_mut(victim);
    player.remove_from_hand_face_down(pick);
    put_in_zone(player, pick, Zone::IntimidateBanish);
}

/// Pay a "discard a card" additional cost (e.g. Alpha Rampage, Wrecker Romp):
/// `owner`, the player playing the card, discards one card chosen uniformly at
/// random from their hand into their graveyard. Paid as the card is played (once
/// it is on the stack and out of hand). A no-op when the hand is empty. Like
/// Intimidate and the on-play discards, the choice is made by the engine without
/// a dedicated choice phase; the play/pitch legal-action gates ensure a card is
/// held back in hand so this cost can always be paid.
fn apply_discard_cost(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex) {
    let Some(pick) = random_hand_card(gs, rng, owner) else {
        return;
    };
    let card = gs.player(owner).card(pick);
    let discarded_power = card.data().power;

    // The discard lands face-up in the graveyard, so it is public.
    if gs.logging_enabled() {
        gs.log_public(format!("{} discards {:?}", player_name(owner), card));
    }

    move_card(gs.player_mut(owner), pick, Zone::Hand, Zone::Graveyard);

    maybe_discard6_intimidate(gs, rng, owner, discarded_power);
}

/// Whether `data` can be revealed for a "reveal a card in your hand with cost
/// 1 or less" additional cost (Flock of the Feather Walkers).
pub(crate) fn can_reveal_for_cost(data: &CardData) -> bool {
    data.cost <= 1
}

/// The distinct cards in `owner`'s hand that could pay a "reveal a card with
/// cost 1 or less" additional cost, lowest id first. Copies count once: which
/// copy is shown makes no difference.
pub(crate) fn revealable_cards(player: &Player) -> impl Iterator<Item = DeckCardId> + '_ {
    player.hand.iter()
        .map(|(id, _)| id)
        .filter(|&id| can_reveal_for_cost(player.card(id).data()))
}

/// Reveal `id` from `owner`'s hand to pay a reveal cost. The card stays in
/// hand, now known to the opponent (`Player::hand_known`).
fn reveal_from_hand(gs: &mut Gamestate, owner: PlayerIndex, id: DeckCardId) {
    gs.player_mut(owner).mark_known_in_hand(id);
    if gs.logging_enabled() {
        gs.log_public(format!("{} reveals {:?}", player_name(owner), gs.player(owner).card(id)));
    }
}

/// Handle the choice in `Phase::RevealChoice`: reveal the chosen hand card,
/// then finish the play that paused for it, from the phase it was committed in.
fn handle_reveal_choice_phase(gs: &mut Gamestate, rng: &mut GameRng, act: Action) {
    if act.typ != ActionType::Reveal {
        return;
    }
    let pending = gs.pending_card.expect("a reveal choice belongs to a pending play");
    reveal_from_hand(gs, pending.owner, act.card_ref().id);
    gs.phase = gs.choice_return_phase;
    finish_commit(gs, rng, pending);
}

/// Create a Quicken token for `owner`, unless they already hold
/// `MAX_QUICKENS`.
fn create_quicken(gs: &mut Gamestate, owner: PlayerIndex) {
    let player = gs.player_mut(owner);
    if player.token_count(TokenCard::QuickenToken) >= MAX_QUICKENS {
        return;
    }
    player.add_token(TokenCard::QuickenToken);
    if gs.logging_enabled() {
        gs.log_public(format!("{} creates a Quicken token", player_name(owner)));
    }
}

/// `owner` has just played an attack action card or attacked with a weapon:
/// each Quicken they hold triggers, is destroyed, and gives the attack go
/// again. Every Quicken triggers on the same attack, so all are spent at once.
/// The go again is banked like Wild Ride's (`attack_go_again_bonus`) and read
/// when combat damage resolves.
fn trigger_quickens(gs: &mut Gamestate, owner: PlayerIndex, attack: DeckCardId) {
    let player = gs.player_mut(owner);
    let mut triggered = false;
    for (id, _) in player.tokens.iter() {
        let token = TokenCard::from_id(id);
        if matches!(token.card().data().constant_effect, Some(ConstantEffect::OnAttackDestroyForGoAgain)) {
            while player.token_count(token) > 0 {
                player.remove_token(token);
            }
            triggered = true;
        }
    }
    if triggered {
        if gs.logging_enabled() {
            gs.log_public(format!("{}'s Quicken is destroyed and the attack gains go again", player_name(owner)));
        }
        attack_gains_go_again(gs, owner, attack);
    }
}

/// `owner`'s attack with `attack` gains go again. Banked on the player like
/// Wild Ride's (`attack_go_again_bonus`) and read when combat damage resolves.
/// Every effect that gives an attack go again should come through here, so
/// that "when this gains go again" abilities see it.
///
/// Dorinthea's hero ability is the one such today: the first time Dawnblade
/// gains go again each turn, she may attack an additional time with it this
/// turn (`PlayerFlags::EXTRA_WEAPON_ATTACK`, which lets the swing past its once
/// per turn). "You may" needs no prompt: the extra attack is only an option,
/// taken by swinging again.
fn attack_gains_go_again(gs: &mut Gamestate, owner: PlayerIndex, attack: DeckCardId) {
    let player = gs.player_mut(owner);
    player.set_attack_go_again_bonus(true);
    let hero_extra_swing = matches!(
        player.hero.data().constant_effect,
        Some(ConstantEffect::OnDawnbladeGoAgainExtraSwing)
    );
    if hero_extra_swing && player.card(attack) == Card::Dawnblade && !player.extra_weapon_attack() {
        player.set_extra_weapon_attack(true);
        if gs.logging_enabled() {
            gs.log_public(format!("{}'s Dawnblade gained go again: it may attack an additional time this turn",
                player_name(owner)));
        }
    }
}

/// Apply Rhinar's `OnDiscard6Intimidate` constant hero ability after a card has
/// just been discarded from `owner`'s hand: when the discarded card had 6 or more
/// power and the owner's hero carries the constant effect, trigger an Intimidate
/// (`apply_intimidate`). It is independent of the played card's own keywords, so
/// a card that both carries Intimidate and discards a 6-power card (e.g. Alpha
/// Rampage) intimidates twice.
///
/// It triggers on *every* qualifying discard — there is no once-per-turn limit,
/// so two 6-power discards in a turn intimidate twice.
///
/// It is, however, action-speed: it only triggers during its controller's own
/// action phase. Only the turn player has an action phase, so that is the gate —
/// a discard made while the opponent is attacking (paying for Rally the
/// Rearguard's ability while defending is the only way today) does not
/// intimidate, however much power it had. The check is stated here rather than
/// at each call site so it holds for any future off-turn discard as well.
///
/// The discard paths that do fire it are the ones taken on your own turn: the
/// "discard a card" additional cost (e.g. Alpha Rampage, Wrecker Romp) and the
/// play-effect draw-then-discard (e.g. Bare Fangs, Wild Ride).
fn maybe_discard6_intimidate(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex, discarded_power: u8) {
    if owner != gs.turn_player {
        return;
    }
    // `hero` is `Copy` and `data()` returns a `'static` reference, so this read
    // releases its borrow of the player before `apply_intimidate` mutates `gs`.
    let hero_data = (if owner == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 }).hero.data();
    if discarded_power >= 6
        && matches!(hero_data.constant_effect, Some(ConstantEffect::OnDiscard6Intimidate))
    {
        apply_intimidate(gs, rng, owner);
    }
}

/// `ConstantEffect::OnPlay6Intimidate`, checked each time `owner` plays a card,
/// with `played_power` the played card's printed power: "whenever you play a
/// card with 6 or more power, intimidate". The play counterpart of Rhinar's
/// `OnDiscard6Intimidate` (`maybe_discard6_intimidate`).
///
/// Its only carrier is Chief Ruk'utan, whose text puts the whole ability behind
/// "While Chief Ruk'utan is face up in arsenal", so it is read off the card in
/// `owner`'s arsenal and only while that card is face up (see
/// `handle_mentor_flip_phase`). The card that triggers it is always played from
/// hand, since the mentor is occupying the arsenal. Once the intimidate is done,
/// a mentor's lesson follows on the same trigger: its `MentorEffect`, if it has
/// one (Ruk'utan's `mentor_alpha_rampage_search`).
///
/// - **Power** is the printed power, the same reading Rhinar's discard trigger
///   uses: the engine banks power bonuses on the player and applies them at
///   combat damage, so no played card has a modified power to read here.
/// - **Timing**: it fires as the card is played — when it goes on the stack,
///   after any additional cost — rather than as it resolves. In FaB the trigger
///   goes on the stack above the card and resolves first; the engine resolves
///   it inline, as it does Rhinar's discard trigger, so the intimidate still
///   lands before the played card resolves and before anyone blocks it.
/// - It is not limited to its owner's turn ("whenever you play"); no card that
///   can be played on the opponent's turn has 6 power today, so in practice it
///   only fires on its owner's turn.
fn maybe_play6_intimidate(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex, played_power: u8) {
    if played_power < 6 {
        return;
    }
    let player = gs.player(owner);
    let Some(mentor) = player.arsenal else {
        return;
    };
    let card = player.card(mentor);
    let data = card.data();
    if !player.arsenal_face_up()
        || !matches!(data.constant_effect, Some(ConstantEffect::OnPlay6Intimidate))
    {
        return;
    }

    if gs.logging_enabled() {
        gs.log_public(format!("{}'s {:?} triggers", player_name(owner), card));
    }
    apply_intimidate(gs, rng, owner);
    match data.mentor_effect {
        Some(MentorEffect::AlphaRampageSearch) => mentor_alpha_rampage_search(gs, rng, owner),
        None => {}
    }
}

/// Chief Ruk'utan's lesson (`MentorEffect::AlphaRampageSearch`), run each time
/// his constant effect triggers: "put a lesson counter on Chief Ruk'utan. Then
/// if there are 2 or more lesson counters on him, banish him, search your deck
/// for Alpha Rampage, put it face up in arsenal and shuffle." Ruk'utan is in
/// `owner`'s arsenal; the counters are the arsenal's
/// (`Player::arsenal_lesson_counters`).
fn mentor_alpha_rampage_search(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex) {
    let player = gs.player_mut(owner);
    player.arsenal_lesson_counters = player.arsenal_lesson_counters.saturating_add(1);
    if player.arsenal_lesson_counters >= 2 {
        graduate_mentor(gs, rng, owner, Card::AlphaRampageR);
    }
}

/// A mentor's lesson is complete: banish the mentor (the card in `owner`'s
/// arsenal), search `owner`'s deck for `fetch`, put it face up into the
/// arsenal the mentor just left, and shuffle the deck. Banishing happens whether
/// or not the search finds anything — `fetch` may already have been drawn — and
/// the deck is shuffled either way, as searching it entitles the player to.
///
/// The fetched card arrives face up and is public; like any arsenal card it can
/// be played this very turn, paid for out of hand.
fn graduate_mentor(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex, fetch: Card) {
    // Leaving the arsenal clears its face-up state and lesson counters: they
    // belonged to the mentor while it sat there, and do not follow it.
    let player = gs.player_mut(owner);
    let mentor = player.arsenal.expect("the mentor is in the arsenal");
    move_card(player, mentor, Zone::Arsenal, Zone::Banish);

    let fetch_id = player.deck_kind.id_of(fetch);
    let found = fetch_id.and_then(|id| player.deck.iter().position(|slot| slot.card() == id));

    if gs.logging_enabled() {
        let card = gs.player(owner).card(mentor);
        let outcome = match found {
            Some(_) => format!("puts {:?} face up in their arsenal", fetch),
            None => format!("finds no {:?}", fetch),
        };
        gs.log_public(format!(
            "{} banishes {:?}, searches their deck and {}",
            player_name(owner), card, outcome
        ));
    }

    let player = gs.player_mut(owner);
    if let Some(pos) = found {
        let slot = player.deck.remove(pos).expect("the search found this card");
        put_in_zone(player, slot.card(), Zone::Arsenal);
        turn_arsenal_face_up(player);
    }
    shuffle_deck_for(player, rng);
}

/// Resolve a card's "when you play" effect as it resolves off the stack. The
/// effect is a (condition, effect, magnitude) triple: the condition is evaluated
/// first (it may itself move cards, as `DrawDiscardHit6` does), and the effect is
/// applied only when the condition holds. `owner` is the player whose card is
/// resolving. Effects the engine does not model yet are no-ops.
fn apply_on_play_effect(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex, effect: &OnPlayEffect) {
    if !on_play_condition_met(gs, rng, owner, &effect.condition) {
        return;
    }
    apply_on_play_effect_type(gs, rng, owner, effect);
}

/// Evaluate an effect's condition for `owner` as its card resolves. Shared by
/// on-play effects and attack effects (`AttackEffect::condition`). Some
/// conditions act as they are checked: `DrawDiscardHit6` draws and discards.
fn on_play_condition_met(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex, condition: &OnPlayConditionType) -> bool {
    match condition {
        OnPlayConditionType::DrawDiscardHit6 => draw_then_discard_hit6(gs, rng, owner),
        // "If you've intimidated this turn" (e.g. Beast Mode): met when the
        // owner has resolved an Intimidate trigger earlier this turn.
        OnPlayConditionType::HasIntimidated => {
            let player = if owner == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };
            player.has_intimidated()
        }
        // "If you have less health than an opposing hero" (e.g. Wounded Bull):
        // compared at resolution, so life changes earlier in the turn count.
        OnPlayConditionType::HasLessLife => {
            let (mine, theirs) = if owner == PlayerIndex::P1 {
                (gs.p1.life, gs.p2.life)
            } else {
                (gs.p2.life, gs.p1.life)
            };
            mine < theirs
        }
        // Reprise: the defending hero — the owner's opponent, as only the
        // attacker plays attack reactions — has defended this chain link with a
        // card from their hand.
        OnPlayConditionType::Reprise => gs.player(owner.opponent()).defended_from_hand(),
        // "If you have attacked with a weapon this turn" (Second Swing).
        OnPlayConditionType::HasAttackedWithWeapon => gs.player(owner).weapon_attacks_this_turn() >= 1,
        OnPlayConditionType::Always => true,
    }
}

/// Apply an on-play effect whose condition has been met.
fn apply_on_play_effect_type(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex, effect: &OnPlayEffect) {
    match effect.effectType {
        // The resolving attack gains `magnitude` power. The card is on its way to
        // link 0 of the owner's combat chain; the bonus is banked on the player
        // and folded into the chain's power when combat damage resolves.
        OnPlayEffectType::ConditionalPower => {
            let player = if owner == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
            player.attack_power_bonus = player.attack_power_bonus.saturating_add(effect.magnitude);
        }
        // The resolving attack gains Go Again. Like the power bonus, the card is on
        // its way to link 0 of the owner's combat chain; the flag is banked on the
        // player and folded into the Go Again check when combat damage resolves.
        OnPlayEffectType::ConditionalGoAgain => {
            let player = if owner == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
            player.set_attack_go_again_bonus(true);
        }
        // The next Brute attack the owner plays this turn gains `magnitude` power
        // (e.g. Awakening Bellow). Banked on the player and applied only when a
        // brute attack resolves combat damage, so a non-brute attack played in
        // between does not consume it.
        OnPlayEffectType::NextBrutePower => {
            let player = if owner == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
            player.next_brute_attack_action_bonus = player.next_brute_attack_action_bonus.saturating_add(effect.magnitude);
        }
        // The next attack action card the owner plays this turn gains `magnitude`
        // power, whatever its class (e.g. Come to Fight). Banked like the brute
        // bonus above and applied only when an attack action card resolves combat
        // damage, so a weapon swing in between neither takes nor spends it.
        OnPlayEffectType::NextAttackActionPower => {
            let player = if owner == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
            player.next_attack_action_bonus = player.next_attack_action_bonus.saturating_add(effect.magnitude);
        }
        // The next Brute attack the owner makes this turn gains `magnitude`
        // power, but only while it is defended by fewer than two non-equipment
        // cards (e.g. Barraging Beatdown). Banked here; the blocker count is a
        // property of how the defender answers the attack, so it can only be
        // read when combat damage resolves — see `resolve_combat_damage`.
        // The next weapon attack the owner makes this turn gains `magnitude`
        // power (e.g. En Garde). Banked like the bonuses above and applied
        // only when a weapon swing resolves combat damage.
        // The next attack the owner makes this turn, of any kind, gains
        // `magnitude` power (Second Swing). Banked and spent by the next attack
        // to reach combat damage.
        OnPlayEffectType::NextAttackPower => {
            let player = gs.player_mut(owner);
            player.next_attack_bonus = player.next_attack_bonus.saturating_add(effect.magnitude);
        }
        OnPlayEffectType::NextWeaponPower => {
            let player = gs.player_mut(owner);
            player.next_weapon_attack_bonus = player.next_weapon_attack_bonus.saturating_add(effect.magnitude);
        }
        OnPlayEffectType::NextBruteConditionalPower => {
            let player = if owner == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
            player.next_brute_attack_conditional_bonus =
                player.next_brute_attack_conditional_bonus.saturating_add(effect.magnitude);
        }
        // The owner Intimidates: their opponent banishes a random card from hand
        // (e.g. Wrecking Ball, whose `DrawDiscardHit6` condition Intimidates on a
        // 6-power discard). This is separate from Rhinar's `OnDiscard6Intimidate`
        // constant ability, which already fired inside the condition's own
        // discard — so Rhinar hitting a 6 with Wrecking Ball Intimidates twice.
        OnPlayEffectType::ConditionalIntimidate => {
            apply_intimidate(gs, rng, owner);
        }
        // The owner gains `magnitude` life (e.g. Sigil of Solace's "Gain 1
        // life."). There is no maximum life total, so this only ever adds.
        OnPlayEffectType::GainLife => {
            let player = if owner == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
            let before = player.life;
            player.life = player.life.saturating_add(effect.magnitude);
            let after = player.life;
            if gs.logging_enabled() {
                gs.log_public(format!(
                    "{} gains {} life (life: {}->{})",
                    player_name(owner), effect.magnitude, before, after
                ));
            }
        }
        // "When you attack with this, create a Quicken token" (Flock of the
        // Feather Walkers). On-play effects fire as the card resolves onto the
        // chain, which is when it attacks — after any Quicken already held was
        // spent on playing it (see `trigger_quickens`).
        OnPlayEffectType::CreateQuicken => create_quicken(gs, owner),
    }
}

/// `DrawDiscardHit6`: the owner draws a card, then discards a card; the condition
/// is met when the discarded card had 6 or more power. The discard target is
/// chosen uniformly at random from the resulting hand, mirroring how the engine
/// resolves other forced selections without a dedicated choice phase (see
/// `apply_intimidate`). Returns whether the discarded card hit the power-6
/// threshold; returns `false` if the hand is empty after drawing (nothing to
/// discard, e.g. an empty deck and hand).
fn draw_then_discard_hit6(gs: &mut Gamestate, rng: &mut GameRng, owner: PlayerIndex) -> bool {
    // Draw a card.
    draw_cards(gs.player_mut(owner), 1);

    // Pick a card to discard at random.
    let Some(pick) = random_hand_card(gs, rng, owner) else {
        return false;
    };
    let card = gs.player(owner).card(pick);
    let discarded_power = card.data().power;

    // The discard lands face-up in the graveyard, so it is public.
    if gs.logging_enabled() {
        gs.log_public(format!("{} discards {:?}", player_name(owner), card));
    }

    move_card(gs.player_mut(owner), pick, Zone::Hand, Zone::Graveyard);

    // Rhinar's constant ability also triggers off this discard: a power-6 discard
    // intimidates, on top of whatever the play effect itself does.
    maybe_discard6_intimidate(gs, rng, owner, discarded_power);

    discarded_power >= 6
}

/// Return every card sitting in either player's Intimidate banish zone to its
/// owner's hand. Intimidate only banishes a card until the end of the turn:
/// unlike the ordinary banished zone (which is permanent), the Intimidate banish
/// zone empties back into its owner's hand as the turn passes into the Arsenal
/// phase. Both players' zones are drained, since either could have been
/// intimidated during the turn.
fn return_intimidate_banished_to_hands(gs: &mut Gamestate) {
    for pid in [PlayerIndex::P1, PlayerIndex::P2] {
        let banished = gs.player(pid).intimidate_banished;
        for (id, copies) in banished.iter() {
            for _ in 0..copies {
                if gs.logging_enabled() {
                    let card = gs.player(pid).card(id);
                    let full = format!("{} returns {:?} from Intimidate banish to hand", player_name(pid), card);
                    let hidden = format!("{} returns a card from Intimidate banish to hand", player_name(pid));
                    let (p1_view, p2_view) = if pid == PlayerIndex::P1 {
                        (full.clone(), hidden)
                    } else {
                        (hidden, full.clone())
                    };
                    gs.log_views(full, p1_view, p2_view);
                }
                move_card(gs.player_mut(pid), id, Zone::IntimidateBanish, Zone::Hand);
            }
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

/// Whether committing `typ` on `data` puts a defense reaction on the stack that
/// should resolve onto the combat chain: a defense reaction card played from
/// hand by the defender (`owner` is not the `turn_player`, since only the turn
/// player attacks). Such a card blocks the attack it was played against rather
/// than resolving to the graveyard. The defender check is belt-and-braces — the
/// reaction window only ever offers a defense reaction to the defender — and
/// keeps a defense reaction that somehow resolved outside combat off the chain.
fn commits_as_defense(
    typ: ActionType,
    data: &CardData,
    owner: PlayerIndex,
    turn_player: PlayerIndex,
) -> bool {
    typ == ActionType::PlayCard
        && data.typ == CardType::DefenseReaction
        && owner != turn_player
}

/// The ability being activated, when `typ` is an `Activate`; `None` for a card
/// that is played (its catalog `ability`, if any, is not in use).
fn activated_ability(typ: ActionType, data: &CardData) -> Option<&Ability> {
    if typ == ActionType::Activate { data.ability.as_ref() } else { None }
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
fn handle_pitch_phase(gs: &mut Gamestate, rng: &mut GameRng, act: Action) {
    if act.typ != ActionType::Pitch {
        return;
    }

    let pid = gs.active_player;
    let id = act.card_ref().id;
    let card = gs.player(pid).card(id);
    let pitch_val = card.data().pitch;

    // Pitched cards land face-up in the pitch zone, so the pitch is public.
    if gs.logging_enabled() {
        gs.log_public(format!("{} pitches {:?}", player_name(pid), card));
    }

    // Move the pitched card out of the hand and into the pitch zone, banking the
    // resources it produces.
    let player = gs.player_mut(pid);
    move_card(player, id, Zone::Hand, Zone::Pitch);
    player.resources += pitch_val;
    let resources = player.resources;

    // Cost still owed on the pending card; once we can cover it, commit it.
    let pending = gs.pending_card.expect("pitch phase requires a pending card");
    let pending_card = gs.player(pending.owner).card(pending.card);
    let cost = action_cost(pending.typ, pending_card.data());
    if resources >= cost {
        if pending.typ == ActionType::DefendTrigger {
            // Paying a resolving trigger's optional cost: nothing goes on the
            // stack, the effect just applies.
            pay_for_defend_trigger(gs);
        } else {
            commit_pending_to_stack(gs, rng);
        }
    }
}

/// Move the pending card onto the stack, pay its cost from the active player's
/// banked resources, and re-enter the live priority window. Shared by both the
/// affordable path (straight from the Action phase) and a pitch phase (once
/// enough has been pitched). The card is taken out of its source zone — the
/// hand or arsenal for a played card, the weapon/equipment zone for an
/// activation — and pushed onto the stack. Assumes the player can already
/// cover the cost.
fn commit_pending_to_stack(gs: &mut Gamestate, rng: &mut GameRng) {
    let Some(pending) = gs.pending_card else {
        return;
    };

    // Recompute the cost from the pending action's type, mirroring the
    // affordability check that let us get here.
    let owner = pending.owner;
    let data = gs.player(owner).card(pending.card).data();
    let cost = action_cost(pending.typ, data);

    // Activating the ability of a card that is *defending* (Rally the Rearguard)
    // does not move the card: it is on its controller's combat chain as a
    // declared blocker and must stay there, still blocking, while the ability
    // waits on the stack. Only the activation goes on the stack, as a pending
    // entry pointing back at the card — the same shape as a defend trigger,
    // whose card likewise stays put (see `resolve_top_of_stack`).
    //
    // A weapon activated from the chain is the other case: it attacked earlier
    // this chain and is attacking again (Dorinthea's extra Dawnblade attack).
    // It leaves its old link for the stack, and from there the next link, like
    // any swing; its layer is recorded as coming from the weapon zone, since
    // it is the weapon itself on the stack, as for a swing from the slot.
    let from_chain = pending.typ == ActionType::Activate && pending.from == Zone::CombatChain;
    let weapon_from_chain = from_chain && commits_as_attack(pending.typ, data);
    let activated_while_defending = from_chain && !weapon_from_chain;

    let mut pending = pending;
    let player = gs.player_mut(owner);
    player.resources -= cost;
    if weapon_from_chain {
        take_from_chain(player, pending.card);
        pending.from = Zone::Weapon;
    } else if !activated_while_defending {
        // The card itself goes on the shared stack, face up.
        take_from_zone(player, pending.card, pending.from);
    }
    gs.push_to_stack(pending);

    // Additional costs are paid as the card is played. A "discard a card" cost
    // (e.g. Alpha Rampage, Wrecker Romp) is paid now that the card is on the
    // stack and out of hand, so the discard is drawn from the rest of the hand.
    // The play/pitch legal-action gates guarantee a card was held back to satisfy
    // it. Only played cards carry this cost; activations have no additional cost.
    if matches!(data.additional_cost, Some(AdditionalCostType::DiscardCard)) {
        apply_discard_cost(gs, rng, owner);
    }
    // A "reveal a card with cost 1 or less" cost (Flock of the Feather
    // Walkers) is paid out of what is left in hand after pitching; the
    // legal-action gates kept such a card back. With one card to choose from
    // it is revealed at once; with more, the owner chooses (`RevealChoice`)
    // and the play finishes once they have (`handle_reveal_choice_phase`).
    if matches!(data.additional_cost, Some(AdditionalCostType::RevealCost1OrLess)) {
        let (first, more) = {
            let mut options = revealable_cards(gs.player(owner));
            (options.next(), options.next().is_some())
        };
        debug_assert!(first.is_some(), "no card with cost 1 or less left to reveal");
        if more {
            gs.choice_return_phase = gs.phase;
            gs.phase = Phase::RevealChoice;
            gs.active_player = owner;
            return;
        }
        if let Some(id) = first {
            reveal_from_hand(gs, owner, id);
        }
    }

    finish_commit(gs, rng, pending);
}

/// The second half of committing a card to the stack, once it is there and its
/// costs are paid: the triggers that fire on playing or attacking, the
/// bookkeeping for an activated ability, and the move into the next window.
/// Runs straight after `commit_pending_to_stack`, or after a choice the
/// payment paused for (`RevealChoice`), with `gs.phase` back on the phase the
/// card was committed from.
fn finish_commit(gs: &mut Gamestate, rng: &mut GameRng, pending: PendingCard) {
    let owner = pending.owner;
    let data = gs.player(owner).card(pending.card).data();

    // Playing an attack action card or attacking with a weapon sets off every
    // Quicken its owner holds.
    if commits_as_attack(pending.typ, data) {
        trigger_quickens(gs, owner, pending.card);
    }

    // "Whenever you play a card" triggers fire once the card has been played —
    // after its additional costs are paid. Only a played card counts: swinging a
    // weapon or activating an ability is not playing a card.
    if pending.typ == ActionType::PlayCard {
        maybe_play6_intimidate(gs, rng, owner, data.power);
    }

    // An activated ability pays its own costs here for the same reason: the
    // activation is on the stack and the cost is what put it there. A "once per
    // turn" ability is marked used at the same moment — the cost has been paid,
    // so responding to the activation cannot hand the card its use back.
    if pending.typ == ActionType::Activate {
        if let Some(ability) = &data.ability {
            if ability.once_per_turn() {
                gs.player_mut(owner).abilities_used.add(pending.card);
            }
            if ability.discards_a_card() {
                apply_discard_cost(gs, rng, owner);
            }
        }
    }

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
    } else if gs.phase == Phase::DefendPitch {
        gs.phase = Phase::DefendTriggers;
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

    let attacker = gs.player(attacker_id);
    let defender = gs.player(defender_id);
    // The chain link this attack occupies. Damage is calculated for that link
    // alone, so an earlier attack this turn — and the blockers that stopped it —
    // sit on their own link and count for nothing here.
    let link = current_chain_link(attacker);
    // The attacking card: the attacker's side of a link holds the attack alone.
    let attack_card = attacker.attack_on_link(link).map(|id| attacker.card(id));
    let attack_data = attack_card.map(|c| c.data());
    // Whether the attacking card (this link's) is a Brute *attack action card*,
    // which gates the banked "next brute attack" power bonus (e.g. Awakening
    // Bellow's +3): the card text targets brute attack action cards specifically,
    // so a non-brute attack — or a brute *weapon* swing like Bone Basher — does
    // not get it, and leaves it banked for a later brute attack action.
    let attack_is_brute = attack_data
        .map(|data| data.card_class == CardClass::Brute && data.typ == CardType::AttackAction)
        .unwrap_or(false);
    let brute_bonus = if attack_is_brute { attacker.next_brute_attack_action_bonus } else { 0 };
    // Whether the attacking card is an attack action card of any class, which
    // gates the banked "next attack action card" bonus (e.g. Come to Fight's +1).
    // A weapon swing is not an attack action card, so it leaves the bonus banked
    // for a later attack action this turn.
    let attack_is_attack_action = attack_data
        .map(|data| data.typ == CardType::AttackAction)
        .unwrap_or(false);
    let attack_action_bonus = if attack_is_attack_action { attacker.next_attack_action_bonus } else { 0 };
    // Whether the attack is a weapon swing, which gates the banked "next
    // weapon attack" bonus (En Garde's +3). An attack action card leaves it
    // banked for a later swing this turn.
    let attack_is_weapon = attack_data.map(is_weapon).unwrap_or(false);
    let weapon_bonus = if attack_is_weapon { attacker.next_weapon_attack_bonus } else { 0 };
    // The banked "next attack" bonus (Second Swing's +4) is taken by any
    // attack: a weapon swing or an attack action card.
    let next_attack_bonus = if attack_data.is_some() { attacker.next_attack_bonus } else { 0 };
    // Dawnblade: "The second time you attack with Dawnblade each turn, it gains
    // +1 power until the end of turn" — so its second attack and any after it
    // this turn. Each swing is counted in `abilities_used` as it is committed.
    let second_attack_bonus = match attacker.attack_on_link(link) {
        Some(id) if attack_data.is_some_and(|d| matches!(d.constant_effect,
            Some(ConstantEffect::SecondAttackEachTurnPlus1Power)))
            && attacker.abilities_used.count(id) >= 2 => 1,
        _ => 0,
    };
    // Whether the attacking card is a Brute *attack* of any kind, which gates
    // the banked conditional bonus (e.g. Barraging Beatdown's +3). Unlike
    // `attack_is_brute` above this covers a brute weapon swing as well as a
    // brute attack action card: "your next Brute attack" names the attack, not
    // the card type it is made with, so Bone Basher qualifies.
    let attack_is_brute_attack = attack_data
        .map(|data| {
            data.card_class == CardClass::Brute
                && matches!(data.typ, CardType::AttackAction | CardType::Weapon)
        })
        .unwrap_or(false);
    // The number of non-equipment cards defending this link. Equipment that
    // defends (Bone Vizier, Ironrot Helm, ...) is excluded by card type.
    let non_equipment_blockers = chain_link_count(defender, link, |d| {
        d.typ != CardType::Equipment
    });
    // The conditional bonus pays out only while fewer than two non-equipment
    // cards defend the attack; two or more nullify it. It is consumed either way
    // (see below), so a nullified bonus does not roll over to a later attack.
    let brute_conditional_bonus = if attack_is_brute_attack && non_equipment_blockers < 2 {
        attacker.next_brute_attack_conditional_bonus
    } else {
        0
    };
    // Total attack power is the cards on this chain link plus any banked on-play
    // bonus (e.g. Bare Fangs's conditional +2 power, Awakening Bellow's brute +3,
    // Come to Fight's +1, Barraging Beatdown's conditional +3).
    let power = chain_link_total(attacker, link, |d| d.power)
        .saturating_add(attacker.attack_power_bonus)
        .saturating_add(brute_bonus)
        .saturating_add(attack_action_bonus)
        .saturating_add(brute_conditional_bonus)
        .saturating_add(weapon_bonus)
        .saturating_add(second_attack_bonus)
        .saturating_add(next_attack_bonus);
    // The block is the blockers' printed defense plus whatever was granted to
    // them during the combat (Rally the Rearguard's +3 block).
    let blocked = chain_link_total(defender, link, |d| d.defense)
        .saturating_add(defender.chain_defense_bonus[link]);
    let damage = power.saturating_sub(blocked);

    // Whether the attacking card (link 0 of the attacker's chain) has Go Again;
    // if so the turn player keeps their action point and may act again. An on-play
    // effect may have conditionally granted Go Again (e.g. Wild Ride after
    // discarding a 6-power card), banked on the player.
    let attack_has_go_again = attacker.attack_go_again_bonus()
        || attack_data
            .map(|data| data.keyword.contains(Keyword::GoAgain))
            .unwrap_or(false);

    let defender = gs.player_mut(defender_id);
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

    // The on-play power bonus is single-use: consume it so a follow-up attack
    // (e.g. after Go Again) doesn't inherit it.
    let attacker = gs.player_mut(attacker_id);
    attacker.attack_power_bonus = 0;
    attacker.set_attack_go_again_bonus(false);
    // The brute power bonus is single-use, but only a brute attack consumes it:
    // a non-brute attack leaves it banked for a later brute attack this turn.
    if attack_is_brute {
        attacker.next_brute_attack_action_bonus = 0;
    }
    // Likewise the attack-action bonus: only an attack action card consumes it,
    // so a weapon swing leaves it banked for a later attack action this turn.
    if attack_is_attack_action {
        attacker.next_attack_action_bonus = 0;
    }
    // And the weapon bonus: spent by a weapon swing only.
    if attack_is_weapon {
        attacker.next_weapon_attack_bonus = 0;
    }
    // And the any-attack bonus: spent by whichever attack this is.
    if attack_data.is_some() {
        attacker.next_attack_bonus = 0;
    }
    // The conditional brute bonus is spent by any brute attack, whether or not
    // the blocker count let it pay out: the attack it was banked for has now
    // happened, so a nullified bonus is gone rather than saved for the next one.
    // A non-brute attack leaves it banked for a later brute attack this turn.
    if attack_is_brute_attack {
        attacker.next_brute_attack_conditional_bonus = 0;
    }
}

/// Close the combat chain, either as the action phase ends or when the turn
/// player breaks it mid-turn (`ActionType::CloseChain`). Every card still sitting on
/// either player's chain leaves it: a weapon returns to its owner's weapon slot,
/// every other card goes to its owner's graveyard, and every link — with any
/// block bonus granted on it — clears. Cards on a player's
/// chain are owned by that player — attacks sit on the attacker's chain,
/// blockers on the defender's — so the owner is the chain's player.
fn close_combat_chain(gs: &mut Gamestate) {
    if (gs.p1.chain_occupied() || gs.p2.chain_occupied()) && gs.logging_enabled() {
        gs.log_public("combat chain closes".to_string());
    }
    for pid in [PlayerIndex::P1, PlayerIndex::P2] {
        for link in 0..CHAIN_LINKS {
            let player = gs.player_mut(pid);
            let cards = std::mem::take(&mut player.chain_link[link]);
            player.chain_links_open = 0;
            // A block bonus granted during the combat (Rally the Rearguard's +3)
            // belongs to that combat, so it goes with the link.
            player.chain_defense_bonus[link] = 0;
            for (id, copies) in cards.iter() {
                for _ in 0..copies {
                    let data = gs.player(pid).card(id).data();
                    if is_weapon(data) {
                        // A weapon on the chain returns to the slot it swung from.
                        put_in_zone(gs.player_mut(pid), id, Zone::Weapon);
                    } else if data.typ == CardType::Equipment {
                        // Equipment that defended goes back to its slot, unless
                        // Blade Break, or an effect that marked it ("when the
                        // combat chain closes, destroy this"), destroys it.
                        let marked = data.slot.as_ref()
                            .is_some_and(|slot| gs.player(pid).destroy_on_chain_close(slot));
                        if data.keyword.contains(Keyword::BladeBreak) || marked {
                            destroy_equipment(gs, pid, id);
                        } else {
                            put_in_zone(gs.player_mut(pid), id, Zone::Equipment);
                        }
                    } else {
                        // The graveyard is a public zone.
                        gs.player_mut(pid).add_to(Zone::Graveyard, id);
                    }
                }
            }
        }
        // Every mark has now been acted on.
        gs.player_mut(pid).clear_destroy_on_chain_close();
        gs.player_mut(pid).set_defended_from_hand(false);
    }
}

/// Destroy `pid`'s equipment `id`, which has already left its slot (it was
/// defending on the chain): it goes to the graveyard, and its "when this is
/// destroyed" trigger, if any, fires. Like Rhinar's and Ruk'utan's triggers it
/// resolves inline rather than as a stack layer of its own.
fn destroy_equipment(gs: &mut Gamestate, pid: PlayerIndex, id: DeckCardId) {
    let card = gs.player(pid).card(id);
    if gs.logging_enabled() {
        gs.log_public(format!("{}'s {:?} is destroyed", player_name(pid), card));
    }
    gs.player_mut(pid).add_to(Zone::Graveyard, id);
    if matches!(card.data().constant_effect, Some(ConstantEffect::OnDestroyedReveal6BottomOtherwise)) {
        reveal_top_keep_6(gs, pid);
    }
}

/// Whether `data` is a weapon card (one that lives in the weapon slot rather
/// than the deck). Mirrors the weapon classification used when dealing out a
/// decklist in `fab_game`.
fn is_weapon(data: &CardData) -> bool {
    matches!(data.typ, CardType::Weapon)
}

/// Deduct one action point from `player`, flooring at 0 so it never underflows.
fn spend_action_point(gs: &mut Gamestate, player: PlayerIndex) {
    let p = if player == PlayerIndex::P1 { &mut gs.p1 } else { &mut gs.p2 };
    p.action_points = p.action_points.saturating_sub(1);
}

/// How many cards on `player`'s chain link `link` satisfy `pred`. The companion
/// of `chain_link_total`: it counts copies rather than summing a stat, for
/// rules that care about the number of cards defending an attack
/// rather than the defense they add up to (e.g. Barraging Beatdown's "less than
/// 2 non-equipment cards").
fn chain_link_count(player: &Player, link: usize, pred: impl Fn(&CardData) -> bool) -> u8 {
    player.chain_link[link]
        .iter()
        .filter(|&(id, _)| pred(player.card(id).data()))
        .map(|(_, copies)| copies)
        .sum()
}

/// Sum a printed per-card stat (`power` for the attacker, `defense` for the
/// defender) over every copy on `player`'s chain link `link`. Bonuses granted
/// during combat are kept per link (`Player::chain_defense_bonus`) and added
/// by the caller.
fn chain_link_total(player: &Player, link: usize, stat: impl Fn(&CardData) -> u8) -> u8 {
    player.chain_link[link]
        .iter()
        .fold(0u8, |total, (id, copies)| {
            total.saturating_add(stat(player.card(id).data()).saturating_mul(copies))
        })
}

/// The link the attacking player's current attack occupies: the last link
/// their attacks have opened (`Player::chain_links_open`). An empty chain
/// answers link 0 — where the next attack will land.
///
/// Only meaningful for the attacking player; a defender opens no links.
pub(crate) fn current_chain_link(attacker: &Player) -> usize {
    (attacker.chain_links_open as usize).saturating_sub(1)
}

/// The link a newly resolving attack should take: the next one after those
/// already opened. `None` means every link is taken, so there is nowhere to
/// put another attack — an engine bug the caller panics on.
fn next_free_chain_link(attacker: &Player) -> Option<usize> {
    let next = attacker.chain_links_open as usize;
    (next < CHAIN_LINKS).then_some(next)
}

/// Take `id` off whichever of `player`'s chain links holds it: a weapon that
/// attacked earlier in the chain, attacking again.
fn take_from_chain(player: &mut Player, id: DeckCardId) {
    let link = player.chain_link.iter().position(|l| l.contains(id))
        .expect("the card is not on the combat chain");
    player.chain_link[link].remove(id);
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
        // Paying for a resolving defend trigger (Ironhide's "you may pay {r}").
        ActionType::DefendTrigger => data
            .defend_effect
            .as_ref()
            .map_or(0, |effect| effect.resource_cost()),
        _ => data.cost,
    }
}

/// Take one copy of `id` out of `player`'s zone `zone`, so it can be put
/// somewhere else. An unordered zone's bitfield drops the copy; the arsenal or
/// weapon zone empties (an emptied arsenal also loses its face-up state and
/// lesson counters, which belonged to the card that sat there); the deck gives
/// up the first copy of `id` from the top. Panics when the zone does not hold
/// the card, or for the combat chain and stack, which are not left this way.
pub(crate) fn take_from_zone(player: &mut Player, id: DeckCardId, zone: Zone) {
    match zone {
        Zone::Arsenal => {
            assert_eq!(player.arsenal, Some(id), "the arsenal does not hold that card");
            player.arsenal = None;
            player.set_arsenal_face_up(false);
            player.set_arsenal_known(false);
            player.arsenal_lesson_counters = 0;
        }
        Zone::Weapon => {
            assert_eq!(player.weapon, Some(id), "the weapon zone does not hold that card");
            player.weapon = None;
        }
        Zone::Deck => {
            let pos = player.deck.iter().position(|slot| slot.card() == id)
                .expect("the deck does not hold that card");
            player.deck.remove(pos);
        }
        Zone::CombatChain | Zone::Stack => panic!("cards are not taken out of the {:?} this way", zone),
        _ => player.remove_from(zone, id),
    }
}

/// Put one copy of `id` into `player`'s zone `zone`: counted in an unordered
/// zone's bitfield, or placed in the arsenal or weapon zone, which must be
/// empty. A card put in the arsenal this way arrives face down. Cards reach the
/// deck, combat chain and stack by their own routes (drawn order matters there),
/// so those zones panic.
pub(crate) fn put_in_zone(player: &mut Player, id: DeckCardId, zone: Zone) {
    match zone {
        Zone::Arsenal => {
            assert!(player.arsenal.is_none(), "the arsenal already holds a card");
            player.arsenal = Some(id);
        }
        Zone::Weapon => {
            assert!(player.weapon.is_none(), "the weapon zone already holds a card");
            player.weapon = Some(id);
        }
        Zone::Deck | Zone::CombatChain | Zone::Stack => panic!("cards are not put in the {:?} this way", zone),
        _ => player.add_to(zone, id),
    }
}

/// Move one copy of `id` from `player`'s zone `from` to their zone `to`.
pub(crate) fn move_card(player: &mut Player, id: DeckCardId, from: Zone, to: Zone) {
    take_from_zone(player, id, from);
    put_in_zone(player, id, to);
}

/// Draw back up to intellect, returning how many cards were drawn. If the hand
/// is already at or over intellect there is nothing to draw.
fn draw_to_intellect(player: &mut Player) -> u8 {
    let need = player.intellect.saturating_sub(player.hand_size()) as usize;
    draw_cards(player, need)
}

/// Draw up to `num` cards off the top of the deck into the hand, stopping early
/// if the deck runs out. Returns how many were drawn.
fn draw_cards(player: &mut Player, num: usize) -> u8 {
    let mut drawn = 0;
    while drawn < num {
        let Some(slot) = player.deck.pop_front() else {
            break; // deck is empty
        };
        player.add_to(Zone::Hand, slot.card());
        // A card both players had seen on the deck (revealed and kept on top,
        // say) is still known once drawn.
        if slot.is_known() {
            player.mark_known_in_hand(slot.card());
        }
        drawn += 1;
    }
    drawn as u8
}

/// Per-card on-play effect unit tests live in their own file. Declared as a
/// child module here (rather than a `tests/` integration test) so they retain
/// access to `fab_step`'s private helpers via `use super::*`.
#[cfg(test)]
#[path = "fab_step_card_tests.rs"]
mod card_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists, new_rng, reset, Decklists};
    use crate::game_state::CardVisibleState;
    use crate::cards::Card;
    use crate::deck_cards::{DorintheaCard, RhinarCard};
    use crate::legal_actions::legal_actions;
    use crate::test_support::*;
    use crate::test_support::step;

    #[test]
    fn test_go_first_step() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        assert_eq!(gs.active_player, PlayerIndex::P1);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_go_second_step() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        assert_eq!(gs.active_player, PlayerIndex::P1);

        let go_second = Action{ typ: ActionType::ChooseSecond, card: None};
        step(&mut gs, go_second);

        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_play_card_moves_to_pitch() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);
        assert_eq!(gs.phase, Phase::Action);

        // Play Muscle Mutt (cost 3). With no banked resources the player can't
        // afford it outright, so the pending card is recorded and the phase
        // advances to Pitch to pay for it.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        let play = act(&gs, ActionType::PlayCard, mm);
        step(&mut gs, play);

        assert_eq!(gs.phase, Phase::ActionPitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(layer_card(&gs, &pending), mm);
        assert_eq!(pending.typ, ActionType::PlayCard);

        // The pending card is held off the stack and still sits in the hand
        // until it is actually paid for.
        assert_eq!(gs.stack_top(), None);
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::Hand));
    }

    #[test]
    fn test_play_affordable_card_moves_to_instant() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);
        assert_eq!(gs.phase, Phase::Action);

        // Clearing Bellow costs 0, so the player already has enough resources
        // (zero) to pay for it. Committing it should skip pitching and advance
        // straight to the Instant phase, moving the card onto the stack and
        // clearing the pending slot.
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");

        let actions = legal_actions(&gs);
        let play = actions.into_iter()
                .find(|a| a.typ == ActionType::PlayCard && named(&gs, a) == Some(cb))
                .expect("playing Clearing Bellow should be a legal action");

        step(&mut gs, play);

        assert_eq!(gs.phase, Phase::ActionInstant);
        // Once the card hits the stack it is no longer pending.
        assert_eq!(gs.pending_card, None);
        assert!(on_stack(&gs, cb));
        assert_eq!(stack_top_card(&gs), Some(cb));
    }

    #[test]
    fn test_pitch_commits_played_card_to_stack() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Play Muscle Mutt (cost 3) with no banked resources: it becomes pending
        // and we drop into the ActionPitch phase, with nothing on the stack yet.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        assert_eq!(gs.phase, Phase::ActionPitch);
        assert_eq!(gs.stack_top(), None);

        // Pitch Clearing Bellow (pitch 3) — exactly covers the cost. The pending
        // card is committed to the stack, the cost is paid (3 - 3 = 0 resources
        // left), and the game advances to the Instant phase.
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(stack_top_card(&gs), Some(mm));
        assert!(on_stack(&gs, mm));
        assert_eq!(gs.p1.resources, 0);
        // The pitched card now lives in the pitch zone, not the hand.
        assert!(in_zone(&gs, PlayerIndex::P1, cb, Zone::Pitch));
    }

    #[test]
    fn test_card_visibility_follows_zone() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Cards still in the deck are hidden from both players; cards drawn into a
        // hand are known only to the player holding them.
        assert!(!gs.p1.deck.is_empty(), "p1 should still have cards in the deck");
        assert_eq!(gs.p1.deck_slot_visibility(0), Some(CardVisibleState::Hidden));
        assert_eq!(gs.p1.visibility(Zone::Deck), CardVisibleState::Hidden);

        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        assert_eq!(gs.p1.visibility(Zone::Hand), CardVisibleState::P1Knows);
        assert_eq!(gs.p2.visibility(Zone::Hand), CardVisibleState::P2Knows);

        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");

        // Play Muscle Mutt and pitch Clearing Bellow to pay for it.
        step_card(&mut gs, ActionType::PlayCard, mm);
        // While the card is pending it is still in hand, so it stays known only to
        // its holder.
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::Hand));

        step_card(&mut gs, ActionType::Pitch, cb);

        // Playing the card put it on the stack and pitching revealed the pitched
        // card; both are now known to both players.
        assert!(on_stack(&gs, mm));
        assert_eq!(gs.p1.visibility(Zone::Stack), CardVisibleState::BothKnow);
        assert!(in_zone(&gs, PlayerIndex::P1, cb, Zone::Pitch));
        assert_eq!(gs.p1.visibility(Zone::Pitch), CardVisibleState::BothKnow);
    }

    #[test]
    fn test_bottomed_pitch_card_stays_visible() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Pitch a card to pay for Muscle Mutt, then play through to the end of the
        // turn so the pitched card is bottomed back into the deck.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        step_card(&mut gs, ActionType::Pitch, cb);
        assert_eq!(gs.p1.visibility(Zone::Pitch), CardVisibleState::BothKnow);

        // Bottom the pitched card directly; both players already saw it pitched, so
        // its visibility is preserved even though it is back in the deck.
        let cb_id = gs.p1.deck_id(cb);
        bottom_pitched_card(&mut gs, PlayerIndex::P1, cb_id);
        let bottom = gs.p1.deck.len() - 1;
        assert_eq!(gs.p1.deck.back().map(|s| s.card()), Some(cb_id));
        assert_eq!(gs.p1.deck_slot_visibility(bottom), Some(CardVisibleState::BothKnow));
        // A card that was never seen stays hidden.
        assert_eq!(gs.p1.deck_slot_visibility(0), Some(CardVisibleState::Hidden));
    }

    #[test]
    fn test_pitch_below_cost_keeps_card_pending() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Play Muscle Mutt (cost 3); pitching a single yellow attack action only
        // banks 2 resources, short of the cost, so the card stays pending and we
        // remain in the ActionPitch phase.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);

        let pc = expect_in(&gs, PlayerIndex::P1, Card::PackCallY, Zone::Hand, "Pack Call should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, pc);

        assert_eq!(gs.phase, Phase::ActionPitch);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(gs.p1.resources, 2);
        assert_eq!(gs.p1.pitch.len(), 1);
        assert_eq!(gs.p1.pitch.count_as(RhinarCard::PackCallY), 1);
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::Hand));
        assert_eq!(pending_card(&gs).expect("still pending"), mm);

        // Pitching a second yellow attack action banks a total of 4, now enough
        // to pay the cost of 3 and commit the card.
        let ro = expect_in(&gs, PlayerIndex::P1, Card::RagingOnslaughtY, Zone::Hand, "Raging Onslaught should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, ro);

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(stack_top_card(&gs), Some(mm));
        assert!(on_stack(&gs, mm));
        assert_eq!(gs.p1.resources, 1);
    }

    #[test]
    fn test_pitch_commits_weapon_attack_to_stack() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Swing Bone Basher (ability cost 2): pending, off the stack, Pitch phase.
        let weapon = weapon_card(&gs, PlayerIndex::P1);
        step_card(&mut gs, ActionType::Activate, weapon);
        assert_eq!(gs.phase, Phase::ActionPitch);
        assert_eq!(gs.stack_top(), None);
        assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));

        // Pitch Clearing Bellow (pitch 3) to cover the cost of 2. The weapon is
        // committed to the stack, its slot vacated, and 1 resource is left over.
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(stack_top_card(&gs), Some(weapon));
        assert!(on_stack(&gs, weapon));
        assert_eq!(gs.p1.weapon, None);
        assert_eq!(gs.p1.resources, 1);
    }

    #[test]
    fn test_end_turn_clears_unspent_resources() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

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
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Swing the equipped weapon. Same flow: record pending, go to Pitch.
        let weapon = weapon_card(&gs, PlayerIndex::P1);
        let attack = act(&gs, ActionType::Activate, weapon);
        step(&mut gs, attack);

        assert_eq!(gs.phase, Phase::ActionPitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(layer_card(&gs, &pending), weapon);
        assert_eq!(pending.typ, ActionType::Activate);
    }

    #[test]
    fn test_play_packcall() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);
        assert_eq!(gs.phase, Phase::Action);

        // Seed 42 deals Rhinar an opening hand containing Pack Call. Locate its
        // slot, then find the legal PlayCard action that targets it.
        let packcall = expect_in(&gs, PlayerIndex::P1, Card::PackCallY, Zone::Hand, "Pack Call should be in the opening hand");

        let actions = legal_actions(&gs);
        let play = actions.into_iter()
                .find(|a| a.typ == ActionType::PlayCard && named(&gs, a) == Some(packcall))
                .expect("playing Pack Call should be a legal action");

        step(&mut gs, play);

        // Pack Call is now the pending card, committed via PlayCard, and the
        // game has advanced to the ActionPitch phase to pay for it.
        assert_eq!(gs.phase, Phase::ActionPitch);
        let pending = gs.pending_card.expect("pending card should be set");
        assert_eq!(layer_card(&gs, &pending), packcall);
        assert_eq!(pending.typ, ActionType::PlayCard);
        assert_eq!(layer_card(&gs, &pending), Card::PackCallY);
    }

    /// Drive a game to the Instant phase: Rhinar (p1) goes first, plays Clearing
    /// Bellow (cost 0, so it commits straight to the stack) and we land in the
    /// Instant phase with p1 as both turn and active player. Returns the gamestate
    /// and the stack slot the committed card occupies.
    fn instant_phase_with_one_card() -> (TestGame, Card) {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, cb);

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(stack_top_card(&gs), Some(cb));
        (gs, cb)
    }

    #[test]
    fn test_choose_first_sets_turn_player() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        assert_eq!(gs.active_player, PlayerIndex::P1);

        // Going second flips both the active player and, since they own the turn,
        // the turn player along with it.
        step(&mut gs, Action{ typ: ActionType::ChooseSecond, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P2);
    }

    #[test]
    fn test_instant_pass_gives_priority_to_opponent() {
        let (mut gs, cb) = instant_phase_with_one_card();

        // The turn player passes: priority moves to the opponent, but nothing
        // resolves yet — the card stays on the stack and we remain in Instant.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.passes, 1);
        assert_eq!(stack_top_card(&gs), Some(cb));
        assert!(on_stack(&gs, cb));
    }

    #[test]
    fn test_both_pass_resolves_top_of_stack() {
        let (mut gs, cb) = instant_phase_with_one_card();

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
        assert!(in_zone(&gs, PlayerIndex::P1, cb, Zone::Graveyard));
    }

    #[test]
    fn test_attack_action_resolves_to_combat_chain() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Rhinar plays Muscle Mutt (an attack action, cost 3) and pitches
        // Clearing Bellow (pitch 3) to commit it to the stack, landing in the
        // Instant phase.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(stack_top_card(&gs), Some(mm));

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
        assert_eq!(link_cards(&gs, PlayerIndex::P1, 0), vec![mm]);
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::CombatChain));
    }

    #[test]
    fn test_weapon_attack_resolves_to_combat_chain() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Swing Bone Basher (ability cost 2) and pitch Clearing Bellow (pitch 3)
        // to commit the weapon to the stack, landing in the Instant phase.
        let weapon = weapon_card(&gs, PlayerIndex::P1);
        step_card(&mut gs, ActionType::Activate, weapon);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(stack_top_card(&gs), Some(weapon));

        // Both players pass. Because this is a weapon swing, resolving it puts
        // the weapon card itself on link 0 of Rhinar's combat chain, makes the
        // opponent (Dorinthea, p2) the active player, and enters the Defend phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.stack_top(), None);
        assert_eq!(link_cards(&gs, PlayerIndex::P1, 0), vec![weapon]);
        assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::CombatChain));
    }

    /// Drive a game until Dorinthea (p2) is on defense against Rhinar's Muscle
    /// Mutt. Mirrors `test_attack_action_resolves_to_combat_chain` and leaves the
    /// gamestate in the Defend phase with p2 as the active (defending) player.
    fn step_to_dorinthea_defending() -> TestGame {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);

        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);

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
        let db = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        let hand_before = gs.p2.hand_size();

        step_card(&mut gs, ActionType::Defend, db);

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(link_cards(&gs, PlayerIndex::P2, 0), vec![db]);
        assert!(in_zone(&gs, PlayerIndex::P2, db, Zone::CombatChain));
        assert_eq!(gs.p2.hand_size(), hand_before - 1);
        // The blocker lands on the defender's chain, not the attacker's.
        assert_eq!(link_cards(&gs, PlayerIndex::P1, 0).first().copied(), Some(Card::MuscleMuttY));
    }

    #[test]
    fn test_defend_multiple_blockers_share_one_chain_link() {
        let mut gs = step_to_dorinthea_defending();

        // Two blockers committed in succession join the *same* chain link (link 0).
        // The link is an unordered bitfield, so it can hold any number of
        // blockers.
        let first = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, first);

        let second = expect_in(&gs, PlayerIndex::P2, Card::InTheSwingR, Zone::Hand, "In the Swing should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, second);

        assert_eq!(gs.phase, Phase::Defend);
        // Both cards live on the combat chain and nowhere fills a second link.
        assert!(in_zone(&gs, PlayerIndex::P2, first, Zone::CombatChain));
        assert!(in_zone(&gs, PlayerIndex::P2, second, Zone::CombatChain));
        assert!(gs.p2.chain_link[1].is_empty());

        // Link 0 holds exactly the two blockers.
        let on_link = link_cards(&gs, PlayerIndex::P2, 0);
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
        let db = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, db);
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

        let db = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, db);
        let its = expect_in(&gs, PlayerIndex::P2, Card::InTheSwingR, Zone::Hand, "In the Swing should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, its);
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
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

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
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.p1.action_points, 1);

        // Play Muscle Mutt (attack action) and pitch Clearing Bellow to pay for it.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);

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
            if let Some(card) = named(&gs, a) {
                assert!(
                    !uses_action_point(a.typ, card.data()),
                    "with 0 action points, {:?} of {:?} must not be offered",
                    a.typ, card
                );
            }
        }
        // No attack-action / action card is offered for play.
        let play_types: Vec<CardType> = actions.iter()
                .filter(|a| a.typ == ActionType::PlayCard)
                .map(|a| card_of(&gs, a).data().typ)
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
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        // ── ChooseFirst phase ──────────────────────────────────────────────
        assert_eq!(gs.phase, Phase::ChooseFirst);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.p1.action_points, 1);
        assert_eq!(gs.p2.life, 20);

        // ── Action phase: play Muscle Mutt ─────────────────────────────────
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in Rhinar's opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        assert_eq!(gs.phase, Phase::ActionPitch);
        assert_eq!(pending_card(&gs).expect("pending Muscle Mutt"), mm);

        // ── Pitch phase: pitch Clearing Bellow (pitch 3) to pay the cost of 3 ─
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in Rhinar's opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(stack_top_card(&gs), Some(mm));
        assert!(on_stack(&gs, mm));

        // ── Instant phase (attack on the stack): both pass to resolve it ───
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Rhinar passes
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Dorinthea passes
        // The attack resolves onto Rhinar's combat chain and play enters Defend.
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(link_cards(&gs, PlayerIndex::P1, 0), vec![mm]);
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::CombatChain));
        assert!(gs.stack_is_empty());

        // ── Defend phase: Dorinthea blocks with Driving Blade (defense 3) ──
        let db = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, db);
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(link_cards(&gs, PlayerIndex::P2, 0), vec![db]);
        assert!(in_zone(&gs, PlayerIndex::P2, db, Zone::CombatChain));
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
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        // ── ChooseFirst phase ──────────────────────────────────────────────
        assert_eq!(gs.phase, Phase::ChooseFirst);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.p2.life, 20);

        // ── Action phase: play Muscle Mutt ─────────────────────────────────
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in Rhinar's opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        assert_eq!(gs.phase, Phase::ActionPitch);

        // ── Pitch phase: pay for it with Clearing Bellow ───────────────────
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in Rhinar's opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(stack_top_card(&gs), Some(mm));

        // ── Instant phase: both pass to resolve the attack onto the chain ──
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Rhinar
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});  // Dorinthea
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::CombatChain));

        // ── Defend phase: Dorinthea commits three defense-3 blockers ───────
        let db = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, db);
        assert_eq!(gs.phase, Phase::Defend);
        let its = expect_in(&gs, PlayerIndex::P2, Card::InTheSwingR, Zone::Hand, "In the Swing should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, its);
        assert_eq!(gs.phase, Phase::Defend);
        let ss = expect_in(&gs, PlayerIndex::P2, Card::SecondSwingR, Zone::Hand, "Second Swing should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, ss);
        assert_eq!(gs.phase, Phase::Defend);
        // All three blockers now sit on Dorinthea's combat chain.
        assert!(in_zone(&gs, PlayerIndex::P2, db, Zone::CombatChain));
        assert!(in_zone(&gs, PlayerIndex::P2, its, Zone::CombatChain));
        assert!(in_zone(&gs, PlayerIndex::P2, ss, Zone::CombatChain));
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

        // Defend pass → post-defend Reaction window, turn player first; his pass
        // hands priority to the defender (Dorinthea, p2).
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, gs.turn_player);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P2);

        // Relabel a card in Dorinthea's hand to Sigil of Solace (an Instant from
        // her deck, cost 0) and play it as a reaction. It commits straight to the
        // stack and we remain in the Reaction phase (not the Instant phase) with
        // the responder keeping priority — the live priority window is preserved.
        let held = *hand_cards(&gs, PlayerIndex::P2).first()
                .expect("p2 should hold a card in the reaction phase");
        relabel(&mut gs, PlayerIndex::P2, Zone::Hand, held, Card::SigilofSolaceB);
        let sigil = Card::SigilofSolaceB;

        step_card(&mut gs, ActionType::PlayCard, sigil);
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert!(on_stack(&gs, sigil));

        // Both players pass: the reaction resolves to its owner's graveyard and
        // the stack empties. The step stays open with the turn player holding
        // priority; both pass again and play returns to the Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert!(in_zone(&gs, PlayerIndex::P2, sigil, Zone::Graveyard));
        close_reaction_step(&mut gs);
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
        // reaction to play, cost 2) and one to Hit and Run (pitch 3) to pay
        // for it. Seed 42 doesn't guarantee this exact pairing, so set it up
        // explicitly — only the card stats matter here, the zones are untouched.
        let hand = hand_cards(&gs, PlayerIndex::P2);
        let tu = hand[0];
        let pitch = hand[1];
        relabel(&mut gs, PlayerIndex::P2, Zone::Hand, tu, Card::ToughenUpB);
        let tu = Card::ToughenUpB;
        relabel(&mut gs, PlayerIndex::P2, Zone::Hand, pitch, Card::HitAndRunB);
        let pitch = Card::HitAndRunB;
        assert_eq!(Card::HitAndRunB.data().pitch, 3);

        // Play Toughen Up. It costs 2 and the defender has no banked resources,
        // so instead of committing to the stack we drop into the ReactionPitch
        // phase with Toughen Up held pending in hand and nothing on the stack.
        step_card(&mut gs, ActionType::PlayCard, tu);
        assert_eq!(gs.phase, Phase::ReactionPitch);
        assert_eq!(pending_card(&gs).expect("Toughen Up pending"), tu);
        assert!(gs.stack_is_empty());

        // Pitch Hit and Run (pitch 3) to cover the cost of 2. That commits
        // Toughen Up to the stack and returns to the Reaction window — not the
        // ActionInstant window — with the responder (p2) keeping priority.
        step_card(&mut gs, ActionType::Pitch, pitch);
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(stack_top_card(&gs), Some(tu));
        assert!(on_stack(&gs, tu));
        assert!(in_zone(&gs, PlayerIndex::P2, pitch, Zone::Pitch));
    }

    #[test]
    fn test_attack_instant_defend_reaction() {
        // Drive a whole attack interaction and confirm the phase-return stack
        // walks it back in order: the instant window resolves the attack onto the
        // chain (-> Defend), declaring blocks advances to the turn player's
        // Reaction window, and emptying that window returns to the Action phase.
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        let tp = gs.turn_player;

        // Rhinar plays Muscle Mutt (an attack action) and pitches Clearing Bellow
        // to pay for it, opening the Instant window with the attack on the stack.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
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
        let (mut gs, cb) = instant_phase_with_one_card();

        // Seed 42 doesn't deal Dorinthea an instant in her opening hand, so move
        // a Sigil of Solace (an Instant) from her deck into her hand to set up
        // the response, with the same zone helpers the engine uses.
        let sigil = Card::SigilofSolaceB;
        if !in_zone(&gs, PlayerIndex::P2, sigil, Zone::Hand) {
            let id = gs.p2.deck_id(sigil);
            take_from_zone(&mut gs.p2, id, Zone::Deck);
            put_in_zone(&mut gs.p2, id, Zone::Hand);
        }

        // Turn player passes; priority moves to the opponent (Dorinthea, p2).
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P2);

        // The opponent responds with an instant of their own. Sigil of Solace
        // (cost 0) commits straight to the stack on top of Clearing Bellow; the
        // responder keeps priority, so they remain the active player. Adding this
        // layer resets the consecutive-pass count.
        step_card(&mut gs, ActionType::PlayCard, sigil);

        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.passes, 0);
        // Sigil is now on top of the stack, above Clearing Bellow.
        assert_eq!(stack_top_card(&gs), Some(sigil));
        assert!(on_stack(&gs, sigil));
        assert!(on_stack(&gs, cb));

        // The opponent passes (1 pass). Because Sigil reset the count, this does
        // not resolve anything — priority simply returns to the turn player, who
        // now gets a window to respond to Sigil.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.passes, 1);
        assert!(on_stack(&gs, sigil));
        assert_eq!(stack_top_card(&gs), Some(sigil));

        // The turn player also passes (2 passes in succession): the top card
        // (Sigil) resolves to its owner's graveyard. Clearing Bellow remains on
        // the stack and priority returns to the turn player for a fresh round.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        assert_eq!(gs.passes, 0);
        assert!(in_zone(&gs, PlayerIndex::P2, sigil, Zone::Graveyard));
        assert_eq!(stack_top_card(&gs), Some(cb));
    }

    #[test]
    fn test_pass_clears_pending_card() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

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
        assert!(gs.p1.chain_link.iter().all(|l| l.is_empty()));
        assert!(gs.p2.chain_link.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn test_action_phase_pass_closes_combat_chain_to_graveyards() {
        // Run a full Muscle Mutt attack blocked by Driving Blade, returning to
        // the Action phase with both cards still sitting on the combat chain.
        let mut gs = step_to_dorinthea_defending();
        let mm = *link_cards(&gs, PlayerIndex::P1, 0).first().expect("attack on p1's chain");
        let db = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, db);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::CombatChain));
        assert!(in_zone(&gs, PlayerIndex::P2, db, Zone::CombatChain));

        // The turn player passes: the chain breaks. Each card on it goes to its
        // owner's graveyard — the attack to the attacker's, the blocker to the
        // defender's — every link clears, and play enters the Arsenal phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Arsenal);
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::Graveyard));
        assert!(in_zone(&gs, PlayerIndex::P2, db, Zone::Graveyard));
        // Each graveyard's bitfield counts the card that landed in it.
        assert_eq!(gs.p1.graveyard.count_as(RhinarCard::MuscleMuttY), 1);
        assert_eq!(gs.p2.graveyard.count_as(DorintheaCard::DrivingBladeY), 1);
        assert!(gs.p1.chain_link.iter().all(|l| l.is_empty()));
        assert!(gs.p2.chain_link.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn test_action_phase_pass_closes_chain_multiple_blockers() {
        // Two blockers share chain link 0; closing the chain must send every
        // card on it to the graveyard, not just one.
        let mut gs = step_to_dorinthea_defending();
        let first = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, first);
        let second = expect_in(&gs, PlayerIndex::P2, Card::InTheSwingR, Zone::Hand, "In the Swing should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, second);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Arsenal);
        assert!(in_zone(&gs, PlayerIndex::P2, first, Zone::Graveyard));
        assert!(in_zone(&gs, PlayerIndex::P2, second, Zone::Graveyard));
        assert!(gs.p2.chain_link.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn test_action_phase_pass_returns_weapon_to_weapon_slot() {
        // Swing Bone Basher through a full combat so the weapon card is left on
        // the combat chain when play returns to the Action phase.
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        let weapon = weapon_card(&gs, PlayerIndex::P1);
        step_card(&mut gs, ActionType::Activate, weapon);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Defend);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::CombatChain));
        assert_eq!(gs.p1.weapon, None);

        // The turn player passes: the chain breaks, and the weapon goes back to
        // the weapon slot it swung from rather than the graveyard.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Arsenal);
        assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));
        assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));
        assert!(gs.p1.chain_link.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn test_arsenal_moves_card_and_starts_opponent_turn() {
        let mut gs = fresh_game();
        assert_eq!(gs.turn_count, 1);

        // Rhinar passes his action phase and sets Muscle Mutt into his arsenal.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::Arsenal, mm);

        // The card left the hand for the arsenal slot.
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::Arsenal));
        assert_eq!(arsenal_card(&gs, PlayerIndex::P1), Some(mm));

        // Arsenaling dropped the hand to 3; the end-of-turn draw refills it to
        // intellect (4), taking one card off the deck.
        assert_eq!(gs.p1.hand_size(), 4);
        assert_eq!(gs.p1.deck_size(), 35);

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
        assert_eq!(gs.p1.arsenal, None);
        assert_eq!(gs.p1.hand_size(), 4);
        assert_eq!(gs.p1.deck_size(), 36);

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
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Pitch, "Clearing Bellow should sit in the pitch zone");
        assert_eq!(gs.p1.deck_size(), 36);

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
        assert!(gs.p1.pitch.is_empty());
        assert!(in_zone(&gs, PlayerIndex::P1, cb, Zone::Deck));
        assert_eq!(deck_cards(&gs, PlayerIndex::P1).last().copied(), Some(cb));
        assert_eq!(gs.p1.deck_size(), 35);
        assert_eq!(gs.p1.hand_size(), 4);
    }

    /// Drive Rhinar through a Muscle Mutt attack paid for with *two* pitched
    /// cards (Pack Call and Raging Onslaught, pitch 2 each, against the cost of
    /// 3), then run the turn out to the Arsenal pass. Ends with the game in the
    /// PitchOrder phase and returns the gamestate plus the two pitched slots.
    fn step_to_pitch_order() -> (TestGame, Card, Card) {
        let mut gs = fresh_game();

        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let pc = expect_in(&gs, PlayerIndex::P1, Card::PackCallY, Zone::Hand, "Pack Call should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, pc);
        let ro = expect_in(&gs, PlayerIndex::P1, Card::RagingOnslaughtY, Zone::Hand, "Raging Onslaught should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, ro);
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

        (gs, pc, ro)
    }

    #[test]
    fn test_two_pitched_cards_enter_pitch_order_phase() {
        let (gs, pc, ro) = step_to_pitch_order();

        // Both pitched cards are still in the pitch zone, and each is offered
        // as a BottomPitch action — nothing else, no pass.
        assert!(in_zone(&gs, PlayerIndex::P1, pc, Zone::Pitch));
        assert!(in_zone(&gs, PlayerIndex::P1, ro, Zone::Pitch));
        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().all(|a| a.typ == ActionType::BottomPitch));
        let offered: Vec<Card> = actions.iter().map(|a| card_of(&gs, a)).collect();
        assert!(offered.contains(&pc));
        assert!(offered.contains(&ro));
    }

    #[test]
    fn test_bottom_pitch_orders_cards_and_ends_turn() {
        let (mut gs, pc, ro) = step_to_pitch_order();
        assert_eq!(gs.p1.deck_size(), 36);

        // Bottom Pack Call first: it leaves the pitch zone for the bottom of
        // the deck, and with Raging Onslaught still pitched the phase stays on
        // PitchOrder with the turn not yet over.
        step_card(&mut gs, ActionType::BottomPitch, pc);
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert!(in_zone(&gs, PlayerIndex::P1, pc, Zone::Deck));
        assert_eq!(deck_cards(&gs, PlayerIndex::P1).last().copied(), Some(pc));
        assert_eq!(gs.p1.deck_size(), 37);

        // Only the remaining pitched card is offered now.
        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].typ, ActionType::BottomPitch);
        assert_eq!(card_of(&gs, &actions[0]), ro);

        // Bottom Raging Onslaught: the pitch zone empties and the turn ends
        // normally — Raging Onslaught is the new bottom card, sitting below
        // Pack Call, and the opponent's turn begins.
        step_card(&mut gs, ActionType::BottomPitch, ro);
        assert!(gs.p1.pitch.is_empty());
        assert!(in_zone(&gs, PlayerIndex::P1, ro, Zone::Deck));
        // Raging Onslaught is the new bottom card, just below Pack Call.
        let deck = deck_cards(&gs, PlayerIndex::P1);
        assert_eq!(deck[deck.len() - 2..], [pc, ro]);

        // End-of-turn bookkeeping ran as usual: the hand (1 card after playing
        // and pitching three) drew back to intellect, 38 - 3 = 35 cards remain
        // in the deck, and the turn flipped to Dorinthea.
        assert_eq!(gs.p1.hand_size(), 4);
        assert_eq!(gs.p1.deck_size(), 35);
        assert_eq!(gs.turn_player, PlayerIndex::P2);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.turn_count, 2);
    }

    /// From the Reaction phase with the attacker (p1) holding priority, make
    /// the defender (Dorinthea, p2) pitch two cards: her hand is relabeled so
    /// she plays Toughen Up (a defense reaction, cost 2) paid for with two
    /// different pitch-1 reds (Sharpen Steel and Thrust), then both players pass the window out so combat resolves
    /// and play returns to the Action phase.
    fn defender_pitches_two_in_reaction(gs: &mut TestGame) -> (Card, Card) {
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, gs.turn_player);

        // Attacker passes; priority moves to the defender.
        step(gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P2);

        // Relabel three of the defender's hand cards: the reaction to play and
        // two pitch-1 reds to pay its cost of 2 with two separate pitches.
        let mut hand = hand_cards(gs, PlayerIndex::P2);
        assert!(hand.len() >= 3, "expected Dorinthea to hold three cards in reaction");
        relabel(gs, PlayerIndex::P2, Zone::Hand, hand[0], Card::ToughenUpB);
        hand[0] = Card::ToughenUpB;
        relabel(gs, PlayerIndex::P2, Zone::Hand, hand[1], Card::SharpenSteelR);
        hand[1] = Card::SharpenSteelR;
        relabel(gs, PlayerIndex::P2, Zone::Hand, hand[2], Card::ThrustR);
        hand[2] = Card::ThrustR;

        step_card(gs, ActionType::PlayCard, hand[0]);
        assert_eq!(gs.phase, Phase::ReactionPitch);
        step_card(gs, ActionType::Pitch, hand[1]);
        assert_eq!(gs.phase, Phase::ReactionPitch);
        step_card(gs, ActionType::Pitch, hand[2]);
        assert_eq!(gs.phase, Phase::Reaction);

        // Both pass: Toughen Up resolves onto the defender's side of the combat
        // chain (a defense reaction blocks with its printed defense). Both pass
        // again on the empty stack: the step closes, combat damage resolves,
        // and play returns to the Action phase.
        step(gs, Action{ typ: ActionType::Pass, card: None});
        step(gs, Action{ typ: ActionType::Pass, card: None});
        close_reaction_step(gs);
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
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Pitch,
                "Clearing Bellow should sit in p1's pitch zone");

        step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // declare no blocks
        let (d1, d2) = defender_pitches_two_in_reaction(&mut gs);

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // The turn player's single card bottomed automatically; the defender's
        // two cards put the game in the PitchOrder phase with *her* active,
        // while the turn (and turn player) is still Rhinar's.
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert!(gs.p1.pitch.is_empty());
        assert!(in_zone(&gs, PlayerIndex::P1, cb, Zone::Deck));
        assert_eq!(deck_cards(&gs, PlayerIndex::P1).last().copied(), Some(cb));

        // Only the defender's pitched cards are offered.
        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().all(|a| a.typ == ActionType::BottomPitch));
        let offered: Vec<Card> = actions.iter().map(|a| card_of(&gs, a)).collect();
        assert!(offered.contains(&d1));
        assert!(offered.contains(&d2));

        // She orders both to the bottom of her own deck; the turn then ends.
        step_card(&mut gs, ActionType::BottomPitch, d1);
        assert_eq!(gs.phase, Phase::PitchOrder);
        step_card(&mut gs, ActionType::BottomPitch, d2);

        assert!(gs.p2.pitch.is_empty());
        assert!(in_zone(&gs, PlayerIndex::P2, d1, Zone::Deck));
        assert!(in_zone(&gs, PlayerIndex::P2, d2, Zone::Deck));
        let deck = deck_cards(&gs, PlayerIndex::P2);
        assert_eq!(deck[deck.len() - 2..], [d1, d2]);
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

        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let pc = expect_in(&gs, PlayerIndex::P1, Card::PackCallY, Zone::Hand, "Pack Call should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, pc);
        let ro = expect_in(&gs, PlayerIndex::P1, Card::RagingOnslaughtY, Zone::Hand, "Raging Onslaught should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, ro);

        // Resolve the attack onto the chain; the defender declares no blocks.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Defend);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        let (d1, d2) = defender_pitches_two_in_reaction(&mut gs);

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        // The turn player orders first.
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        let offered: Vec<Card> = legal_actions(&gs).iter().map(|a| card_of(&gs, a)).collect();
        assert!(offered.contains(&pc));
        assert!(offered.contains(&ro));

        step_card(&mut gs, ActionType::BottomPitch, pc);
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.active_player, PlayerIndex::P1);
        step_card(&mut gs, ActionType::BottomPitch, ro);

        // Rhinar's zone is empty but Dorinthea's is not: the phase stays on
        // PitchOrder, the active player flips to her, and the turn is not over.
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        assert_eq!(gs.turn_player, PlayerIndex::P1);
        assert!(gs.p1.pitch.is_empty());
        assert_eq!(deck_cards(&gs, PlayerIndex::P1).last().copied(), Some(ro));
        let offered: Vec<Card> = legal_actions(&gs).iter().map(|a| card_of(&gs, a)).collect();
        assert!(offered.contains(&d1));
        assert!(offered.contains(&d2));

        // Dorinthea orders hers; only then does the turn end.
        step_card(&mut gs, ActionType::BottomPitch, d2);
        assert_eq!(gs.phase, Phase::PitchOrder);
        step_card(&mut gs, ActionType::BottomPitch, d1);

        assert!(gs.p2.pitch.is_empty());
        let deck = deck_cards(&gs, PlayerIndex::P2);
        assert_eq!(deck[deck.len() - 2..], [d2, d1]);
        assert_eq!(gs.turn_player, PlayerIndex::P2);
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.turn_count, 2);
    }

    #[test]
    fn test_identical_pitched_copies_are_one_bottom_pitch_choice() {
        // The pitch zone is unordered and copies of a card are interchangeable,
        // so two pitched copies of the same card are a single choice — but the
        // phase still asks once per copy, since both must go to the bottom.
        let (mut gs, pc, ro) = step_to_pitch_order();
        relabel(&mut gs, PlayerIndex::P1, Zone::Pitch, ro, pc);
        assert_eq!(count_in(&gs, PlayerIndex::P1, pc, Zone::Pitch), 2);

        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 1);
        assert_eq!(card_of(&gs, &actions[0]), pc);

        step_card(&mut gs, ActionType::BottomPitch, pc);
        assert_eq!(gs.phase, Phase::PitchOrder, "one copy is still pitched");
        step_card(&mut gs, ActionType::BottomPitch, pc);
        assert!(gs.p1.pitch.is_empty());
        let deck = deck_cards(&gs, PlayerIndex::P1);
        assert_eq!(gs.phase, Phase::Action);
        assert!(deck.iter().filter(|&&c| c == pc).count() >= 2);
    }

    #[test]
    fn test_pitch_order_ignores_non_bottom_pitch_actions() {
        let (mut gs, pc, ro) = step_to_pitch_order();

        // A stray Pass (or any other action type) is a no-op in the PitchOrder
        // phase: the phase, the pitch zone, and the turn are all untouched.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert!(in_zone(&gs, PlayerIndex::P1, pc, Zone::Pitch));
        assert!(in_zone(&gs, PlayerIndex::P1, ro, Zone::Pitch));
        assert_eq!(gs.turn_player, PlayerIndex::P1);
    }

    #[cfg(feature = "logging")]
    #[test]
    fn test_arsenal_logs_hide_card_from_opponent() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), true);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::Arsenal, mm);

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

    #[cfg(feature = "logging")]
    #[test]
    fn test_close_combat_chain_logs_once() {
        // With logging on, closing a populated chain logs a single public event
        // after the pass itself.
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), true);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
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
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        assert_eq!(gs.p1.hand_size(), 4);
        assert_eq!(gs.p1.deck_size(), 36);
        assert_eq!(gs.p2.hand_size(), 4);
        assert_eq!(gs.p2.deck_size(), 36);

        // The hand is unordered; `hand_cards` lists it in deck-id order.
        assert_eq!(hand_cards(&gs, PlayerIndex::P1), vec![
            Card::MuscleMuttY, Card::PackCallY, Card::RagingOnslaughtY, Card::ClearingBellowB,
        ]);
        assert_eq!(hand_cards(&gs, PlayerIndex::P2), vec![
            Card::InTheSwingR, Card::SecondSwingR, Card::SharpenSteelR, Card::DrivingBladeY,
        ]);
    }

    #[test]
    fn test_draw_cards_includes_last_card() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // After the opening draw p1 has 36 cards left in the deck.
        let deck_before = gs.p1.deck_size();
        assert_eq!(deck_before, 36);
        let hand_before = gs.p1.hand_size();

        // Draw the entire deck, including the final (tail) card.
        let drawn = draw_cards(&mut gs.p1, deck_before as usize);

        // The whole deck moved to hand; nothing is left behind.
        assert_eq!(drawn, deck_before);
        assert_eq!(gs.p1.deck_size(), 0);
        assert!(gs.p1.deck.is_empty());
        assert_eq!(gs.p1.hand_size(), hand_before + deck_before);

        // Drawing further from an empty deck is a safe no-op.
        assert_eq!(draw_cards(&mut gs.p1, 5), 0);
        assert_eq!(gs.p1.deck_size(), 0);
        assert_eq!(gs.p1.hand_size(), hand_before + deck_before);
    }

    // ── Win / draw phases ──────────────────────────────────────────────────

    use crate::game_state::MAX_TURNS;

    /// A fresh, reset game just past the first-player choice — both heroes at 20
    /// life, the turn counter at 1, in the Action phase. A convenient starting
    /// point for poking the win/draw conditions.
    fn fresh_game() -> TestGame {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
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

        let db = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in Dorinthea's opening hand");
        step_card(&mut gs, ActionType::Defend, db);
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
        let decks = Decklists::new(build_rhinar_deck(), build_dorinthea_deck());
        let mut gs = TestGame { state: gamestate_from_decklists(&decks), rng: new_rng(Some(42)) };
        reset(&mut gs.state, &decks, &mut gs.rng, false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        gs.turn_count = 17;
        reset(&mut gs.state, &decks, &mut gs.rng, false);
        assert_eq!(gs.turn_count, 0);
    }

    // ── Logging ────────────────────────────────────────────────────────────

    #[cfg(feature = "logging")]
    #[test]
    fn test_logging_disabled_leaves_logs_none() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert!(gs.log.is_none());
        assert!(gs.p1.log.is_none());
        assert!(gs.p2.log.is_none());
    }

    #[cfg(feature = "logging")]
    #[test]
    fn test_logging_events_as_they_happen() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), true);

        // The first-player choice logs three events: the choice itself, then
        // each player's opening-hand draw the moment it happens.
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.log.as_ref().unwrap().len(), 3);
        assert_eq!(gs.log.as_ref().unwrap()[0], "P1 chooses to go first");

        // Playing and pitching each log one public event, identical in all
        // three logs.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);

        for log in [&gs.log, &gs.p1.log, &gs.p2.log] {
            let log = log.as_ref().expect("logging is enabled");
            assert_eq!(log.len(), 5);
            assert_eq!(log[3], "P1 plays MuscleMuttY");
            assert_eq!(log[4], "P1 pitches ClearingBellowB");
        }
    }

    #[cfg(feature = "logging")]
    #[test]
    fn test_logging_pass_and_stack_resolution_in_one_step() {
        // Drive Muscle Mutt onto the stack with logging on.
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), true);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
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

    #[cfg(feature = "logging")]
    #[test]
    fn test_logging_hides_opponent_draws() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), true);
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

    #[cfg(feature = "logging")]
    #[test]
    fn test_logging_damage_message_includes_life_change() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), true);
        // Re-drive step_to_dorinthea_defending with logging on, then run out the
        // reaction window so Muscle Mutt's 6 damage lands.
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
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
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Relabel two of Rhinar's (p1) hand cards: one to Pack Hunt (the attack
        // action to play, cost 2) and one to Clearing Bellow (pitch 3) to pay for
        // it. Only the card stats matter here; the zones stay untouched.
        let hand = hand_cards(&gs, PlayerIndex::P1);
        let ph = hand[0];
        let pitch = hand[1];
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, ph, Card::PackHuntR);
        let ph = Card::PackHuntR;
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, pitch, Card::ClearingBellowB);

        // The opponent (p2) starts with a full opening hand to banish from.
        let pitch = Card::ClearingBellowB;
        let victim_hand_before = gs.p2.hand_size();
        assert!(victim_hand_before > 0);
        assert!(gs.p2.intimidate_banished.is_empty());

        // Play Pack Hunt and pitch Clearing Bellow to cover its cost, opening the
        // instant window with the attack on the stack.
        step_card(&mut gs, ActionType::PlayCard, ph);
        assert_eq!(gs.phase, Phase::ActionPitch);
        step_card(&mut gs, ActionType::Pitch, pitch);
        assert_eq!(gs.phase, Phase::ActionInstant);
        assert_eq!(stack_top_card(&gs), Some(ph));

        // Both players pass: Pack Hunt resolves onto p1's combat chain and its
        // Intimidate fires, forcing p2 to banish one card from hand.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert!(in_zone(&gs, PlayerIndex::P1, ph, Zone::CombatChain));

        // Exactly one card left p2's hand for the Intimidate banish zone.
        assert_eq!(gs.p2.hand_size(), victim_hand_before - 1);
        assert_eq!(gs.p2.intimidate_banished.len(), 1, "a card was banished to Intimidate");
    }

    #[test]
    fn test_clearing_bellow_intimidate_banishes_opponent_card() {
        // Clearing Bellow is a (non-attack) action carrying Intimidate. When it
        // resolves (to the graveyard) its Intimidate still fires, forcing the
        // opponent to banish a random card from hand into their Intimidate banish
        // zone.
        let (mut gs, cb) = instant_phase_with_one_card();

        let victim_hand_before = gs.p2.hand_size();
        assert!(victim_hand_before > 0);
        assert!(gs.p2.intimidate_banished.is_empty());

        // Both players pass: Clearing Bellow resolves to p1's graveyard and its
        // Intimidate fires.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert!(in_zone(&gs, PlayerIndex::P1, cb, Zone::Graveyard));

        // Exactly one card left p2's hand for the Intimidate banish zone.
        assert_eq!(gs.p2.hand_size(), victim_hand_before - 1);
        assert_eq!(gs.p2.intimidate_banished.len(), 1, "a card was banished to Intimidate");
    }

    #[test]
    fn test_intimidate_banished_card_returns_to_hand_in_arsenal() {
        // A card Intimidate-banished during the turn returns to its owner's hand
        // as the turn passes into the Arsenal phase; the ordinary banished zone is
        // untouched. Clearing Bellow (a non-attack action with Intimidate and Go
        // Again) is convenient: after it resolves play returns to p1's Action
        // phase, and one more pass closes the turn into the Arsenal phase.
        let (mut gs, cb) = instant_phase_with_one_card();
        let victim_hand_before = gs.p2.hand_size();

        // Both players pass: Clearing Bellow resolves and its Intimidate banishes a
        // random card from p2's hand. We are back in p1's Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert!(in_zone(&gs, PlayerIndex::P1, cb, Zone::Graveyard));

        let banished = *cards_in(&gs, PlayerIndex::P2, gs.p2.intimidate_banished).first()
            .expect("a card was banished to Intimidate");
        let hand_copies_before = count_in(&gs, PlayerIndex::P2, banished, Zone::Hand);
        assert_eq!(gs.p2.hand_size(), victim_hand_before - 1);
        assert_eq!(gs.p2.intimidate_banished.len(), 1);
        let banished_id = gs.p2.deck_id(banished);
        assert_eq!(gs.p2.intimidate_banished.count(banished_id), 1);

        // p1 passes, closing the turn into the Arsenal phase. Entering it returns
        // the Intimidate-banished card to p2's hand and empties the zone.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);

        assert!(gs.p2.intimidate_banished.is_empty());
        assert_eq!(count_in(&gs, PlayerIndex::P2, banished, Zone::Hand), hand_copies_before + 1);
        assert_eq!(gs.p2.hand_size(), victim_hand_before);
    }

    // ── Multiple attacks in one turn: one chain link each ──────────────────

    /// Overwrite the turn player's hand with `desired` by relabelling the cards
    /// already in it. The opening hand is four cards, so `desired` may
    /// hold at most four.
    fn relabel_hand(gs: &mut Gamestate, pid: PlayerIndex, desired: &[Card]) {
        let hand = hand_cards(gs, pid);
        assert!(hand.len() >= desired.len(), "hand too small to relabel");
        for (&from, &to) in hand.iter().zip(desired) {
            relabel(gs, pid, Zone::Hand, from, to);
        }
    }

    /// Drive one complete attack by player 1: play `attack`, pitch `pitch` to pay
    /// for it, resolve it onto the combat chain, let player 2 declare `blocks`,
    /// then run the reaction window out so combat damage resolves. `go_again`
    /// banks a conditional Go Again on the attack — the same bank Wild Ride fills
    /// on a 6-power discard — so the attacker keeps their action point and can
    /// attack again this turn.
    fn run_one_attack(gs: &mut TestGame, attack: Card, pitch: Card, blocks: &[Card], go_again: bool) {
        let find = |gs: &Gamestate, pid: PlayerIndex, want: Card| {
            expect_in(gs, pid, want, Zone::Hand, "should be in hand")
        };

        let attack = find(gs, PlayerIndex::P1, attack);
        step_card(gs, ActionType::PlayCard, attack);
        let pitch = find(gs, PlayerIndex::P1, pitch);
        step_card(gs, ActionType::Pitch, pitch);
        step(gs, Action{ typ: ActionType::Pass, card: None});   // attacker passes
        step(gs, Action{ typ: ActionType::Pass, card: None});   // defender passes
        assert_eq!(gs.phase, Phase::Defend);

        for &block in blocks {
            let block = find(gs, PlayerIndex::P2, block);
            step_card(gs, ActionType::Defend, block);
        }
        step(gs, Action{ typ: ActionType::Pass, card: None});   // done blocking

        if go_again {
            gs.p1.set_attack_go_again_bonus(true);
        }
        step(gs, Action{ typ: ActionType::Pass, card: None});   // attacker passes
        step(gs, Action{ typ: ActionType::Pass, card: None});   // defender passes -> damage
    }

    #[test]
    fn test_second_attack_does_not_inherit_the_first_attacks_power() {
        // Two unblocked power-6 attacks in one turn, the second enabled by a
        // banked Go Again. Each attack sits on its own chain link and deals its
        // own 6 damage: 20 -> 14 -> 8. Were both attacks sharing a link, the
        // second would be scored as 6 + 6 and take Dorinthea to 2.
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        relabel_hand(&mut gs, PlayerIndex::P1,
            &[Card::MuscleMuttY, Card::ClearingBellowB, Card::MuscleMuttY, Card::ClearingBellowB]);

        run_one_attack(&mut gs, Card::MuscleMuttY, Card::ClearingBellowB, &[], true);
        assert_eq!(gs.p2.life, 14, "first attack deals its own 6");
        // Go Again kept the action point, so a second attack is possible.
        assert_eq!(gs.p1.action_points, 1);

        run_one_attack(&mut gs, Card::MuscleMuttY, Card::ClearingBellowB, &[], false);
        assert_eq!(gs.p2.life, 8, "second attack deals 6, not 6 + the first attack's 6");

        // Each attack took a link of its own, and both are still on the chain
        // until it closes.
        assert!(!gs.p1.chain_link[0].is_empty());
        assert!(!gs.p1.chain_link[1].is_empty());
        assert_eq!(gs.p1.chain_link[0].len(), 1);
        assert_eq!(gs.p1.chain_link[1].len(), 1);
        assert!(gs.p1.chain_link[2].is_empty());
    }

    #[test]
    fn test_second_attack_is_not_blunted_by_the_first_attacks_blockers() {
        // The blockers that stopped one attack stay on their own chain link and
        // do nothing against the next attack that turn. Muscle Mutt (power 6)
        // blocked by Driving Blade (defense 3) deals 3; the follow-up Muscle
        // Mutt is unblocked and deals its full 6: 20 -> 17 -> 11. Were the
        // blocker still counted, the second attack would be scored as 12 power
        // against 3 defense and take Dorinthea to 8.
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        relabel_hand(&mut gs, PlayerIndex::P1,
            &[Card::MuscleMuttY, Card::ClearingBellowB, Card::MuscleMuttY, Card::ClearingBellowB]);

        run_one_attack(&mut gs, Card::MuscleMuttY, Card::ClearingBellowB,
            &[Card::DrivingBladeY], true);
        assert_eq!(gs.p2.life, 17, "6 power - 3 blocked = 3 damage");

        run_one_attack(&mut gs, Card::MuscleMuttY, Card::ClearingBellowB, &[], false);
        assert_eq!(gs.p2.life, 11, "the spent blocker does not defend the second attack");

        // The blocker sits on the link of the attack it blocked; the second
        // attack's link is empty on the defender's side, having gone unblocked.
        assert_eq!(link_cards(&gs, PlayerIndex::P2, 0).first().copied(), Some(Card::DrivingBladeY));
        assert!(gs.p2.chain_link[1].is_empty());
    }

    #[test]
    fn test_closing_the_chain_clears_every_link() {
        // Two attacks on two links: passing closes the whole chain, not just the
        // link the last attack was on, so both attacks reach the graveyard.
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        relabel_hand(&mut gs, PlayerIndex::P1,
            &[Card::MuscleMuttY, Card::ClearingBellowB, Card::MuscleMuttY, Card::ClearingBellowB]);

        run_one_attack(&mut gs, Card::MuscleMuttY, Card::ClearingBellowB, &[], true);
        run_one_attack(&mut gs, Card::MuscleMuttY, Card::ClearingBellowB, &[], false);
        let first = *link_cards(&gs, PlayerIndex::P1, 0).first().expect("first attack on link 0");
        let second = *link_cards(&gs, PlayerIndex::P1, 1).first().expect("second attack on link 1");

        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Arsenal);
        assert!(in_zone(&gs, PlayerIndex::P1, first, Zone::Graveyard));
        assert!(in_zone(&gs, PlayerIndex::P1, second, Zone::Graveyard));
        assert!(gs.p1.chain_link.iter().all(|l| l.is_empty()));
    }

    #[test]
    #[should_panic(expected = "combat chain overflowed")]
    fn test_attacking_past_the_last_chain_link_panics() {
        // A turn producing more attacks than the chain has links means the engine
        // has lost track of something, so it must fail loudly rather than reuse a
        // link and mis-score the attack. Every link is parked full here — cards
        // pulled off the top of the deck and moved onto the chain — so the next
        // attack has nowhere to go.
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        relabel_hand(&mut gs, PlayerIndex::P1,
            &[Card::MuscleMuttY, Card::ClearingBellowB, Card::MuscleMuttY, Card::ClearingBellowB]);

        for link in 0..gs.p1.chain_link.len() {
            let slot = gs.p1.deck.pop_front().expect("deck should not run dry");
            gs.p1.chain_link[link].add(slot.card());
        }
        gs.p1.chain_links_open = gs.p1.chain_link.len() as u8;

        run_one_attack(&mut gs, Card::MuscleMuttY, Card::ClearingBellowB, &[], false);
    }

    /// Seat `card` in `pid`'s empty arsenal, in place of a card from their hand.
    fn seat_in_arsenal(gs: &mut Gamestate, pid: PlayerIndex, card: Card) -> Card {
        put_in_arsenal(gs, pid, card)
    }

    #[test]
    fn test_attack_action_played_from_arsenal_resolves_to_the_chain() {
        let mut gs = fresh_game();

        // Muscle Mutt (cost 3) waits in the arsenal; Clearing Bellow (pitch 3)
        // in hand pays for it, since the arsenal card can't pitch for itself.
        let mm = seat_in_arsenal(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
        let held = *hand_cards(&gs, PlayerIndex::P1).first().expect("hand should not be empty");
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, held, Card::ClearingBellowB);
        let cb = Card::ClearingBellowB;

        step_card(&mut gs, ActionType::PlayCard, mm);
        assert_eq!(gs.phase, Phase::ActionPitch);
        // Still in the arsenal while it is only pending.
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::Arsenal));
        assert_eq!(arsenal_card(&gs, PlayerIndex::P1), Some(mm));

        step_card(&mut gs, ActionType::Pitch, cb);
        assert_eq!(gs.phase, Phase::ActionInstant);
        // Paid for: it leaves the arsenal for the stack, freeing the slot.
        assert!(on_stack(&gs, mm));
        assert_eq!(gs.p1.arsenal, None);

        // Both pass and it resolves onto the chain like any other attack,
        // sending the defender to the Defend phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert!(in_zone(&gs, PlayerIndex::P1, mm, Zone::CombatChain));
        assert_eq!(link_cards(&gs, PlayerIndex::P1, 0), vec![mm]);
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
    }

    #[test]
    fn test_arsenal_slot_freed_by_playing_can_be_refilled_the_same_turn() {
        let mut gs = fresh_game();

        // Play the 0-cost action out of the arsenal, emptying the slot.
        let cb = seat_in_arsenal(&mut gs, PlayerIndex::P1, Card::ClearingBellowB);
        step_card(&mut gs, ActionType::PlayCard, cb);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.p1.arsenal, None);
        assert!(in_zone(&gs, PlayerIndex::P1, cb, Zone::Graveyard));

        // Clearing Bellow has Go Again, so the action phase continues; passing
        // it out reaches the Arsenal phase, where the freed slot means a card
        // can be set aside again this turn.
        assert_eq!(gs.phase, Phase::Action);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        let arsenal_choices = legal_actions(&gs).iter()
            .filter(|a| a.typ == ActionType::Arsenal)
            .count();
        assert!(arsenal_choices > 0, "the freed slot should accept a card again");
    }
}

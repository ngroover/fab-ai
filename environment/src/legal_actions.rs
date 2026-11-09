use crate::game_state::{Gamestate, Phase, Player, Zone};
use crate::action::{Action, ActionType, CardRef, LegalActions};
use crate::card_counts::{CardCounts, DeckCardId};
use crate::cards::{CardData, CardType};
use crate::card_effects::AdditionalCostType;
use crate::fab_step::{can_reveal_for_cost, current_chain_link, play_requirements_met, revealable_cards, face_down_mentor_in_arsenal, uses_action_point};


pub fn legal_actions(gs: &Gamestate) -> LegalActions {
    #[cfg(debug_assertions)]
    gs.debug_check_card_totals();
    match gs.phase {
        Phase::ChooseFirst => {
            let mut actions = LegalActions::new();
            actions.push(Action{
                        typ : ActionType::ChooseFirst,
                        card: None});
            actions.push(Action{
                        typ : ActionType::ChooseSecond,
                        card: None});
            actions
        },
        Phase::MentorFlip => legal_mentor_flip_phase(gs),
        Phase::Action => legal_action_phase(gs),
        Phase::ActionPitch | Phase::ReactionPitch | Phase::DefendPitch => legal_pitch_phase(gs),
        Phase::ActionInstant => legal_instant_phase(gs),
        Phase::Defend => legal_defend_phase(gs),
        Phase::DefendTriggers => legal_defend_triggers_phase(gs),
        Phase::DefendPayment => legal_defend_payment_phase(gs),
        Phase::RevealChoice => legal_reveal_choice_phase(gs),
        Phase::Reaction => legal_reaction_phase(gs),
        Phase::Arsenal => legal_arsenal_phase(gs),
        Phase::PitchOrder => legal_pitch_order_phase(gs),
        Phase::Start => LegalActions::new(),
        // Terminal phases: the game is over, so there are no legal actions.
        Phase::Player1Win | Phase::Player2Win | Phase::Draw => LegalActions::new(),
    }
}

/// The start-of-turn mentor decision: turn the face-down mentor in the turn
/// player's arsenal face up, or pass to leave it face down. Both always offered.
fn legal_mentor_flip_phase(gs: &Gamestate) -> LegalActions {
    let mut actions = LegalActions::new();
    if let Some(id) = face_down_mentor_in_arsenal(gs, gs.active_player) {
        actions.push(Action::new(ActionType::FlipFaceUp, CardRef::new(id, Zone::Arsenal)));
    }
    actions.push(Action {
        typ: ActionType::Pass,
        card: None,
    });
    actions
}

fn legal_defend_phase(gs: &Gamestate) -> LegalActions {
    // When we enter the Defend phase the active player is flipped to the
    // defender (see resolve_top_of_stack), so the usual active-player lookup
    // gives us the player choosing blockers.
    let player = gs.player(gs.active_player);

    // Every card in hand the defender may declare as a blocker — once per
    // distinct card, since copies are the same choice.
    let mut actions: LegalActions = player.hand.iter()
        .filter(|&(id, _)| can_be_declared_as_blocker(player.card(id).data()))
        .map(|(id, _)| Action::new(ActionType::Defend, CardRef::new(id, Zone::Hand)))
        .collect();

    // Each piece of equipment still worn may defend too, one at a time. It
    // leaves its slot for the chain link and comes back when the chain closes,
    // unless a keyword destroys it (see `fab_step::close_combat_chain`).
    for (id, _) in player.equipment.iter() {
        if can_equipment_defend(player.card(id).data()) {
            actions.push(Action::new(ActionType::Defend, CardRef::new(id, Zone::Equipment)));
        }
    }

    // Blocking is optional (and the only choice once the hand is empty):
    // passing finishes declaring blockers and advances to the reaction window.
    actions.push(Action {
        typ: ActionType::Pass,
        card: None,
    });

    actions
}

/// A resolving defend trigger's optional payment (Ironhide's "you may pay
/// {r}"): pay for it by activating the card on the chain, or pass to decline.
/// The phase is only entered when the payment can be made.
fn legal_defend_payment_phase(gs: &Gamestate) -> LegalActions {
    let mut actions = LegalActions::new();
    if let Some(p) = gs.pending_card {
        actions.push(Action::new(ActionType::Activate, CardRef::new(p.card, Zone::CombatChain)));
    }
    actions.push(Action::no_card(ActionType::Pass));
    actions
}

/// Choosing the card to reveal for a reveal cost: one `Reveal` per distinct
/// eligible hand card. There is no pass — the cost must be paid.
fn legal_reveal_choice_phase(gs: &Gamestate) -> LegalActions {
    revealable_cards(gs.player(gs.active_player))
        .map(|id| Action::new(ActionType::Reveal, CardRef::new(id, Zone::Hand)))
        .collect()
}

fn legal_arsenal_phase(gs: &Gamestate) -> LegalActions {
    // The turn player has passed their action phase and may now set a card
    // aside. The active player is the turn player here, so the usual lookup
    // gives us the player choosing.
    let player = gs.player(gs.active_player);

    let mut actions = LegalActions::new();

    // The arsenal holds a single card: every distinct card in hand is a
    // choice, but only while the slot is free.
    if player.arsenal.is_none() {
        actions.extend(player.hand.iter().map(|(id, _)| {
            Action::new(ActionType::Arsenal, CardRef::new(id, Zone::Hand))
        }));
    }

    // Arsenaling is optional (and the only choice when the slot is occupied or
    // the hand is empty): passing skips it and ends the turn.
    actions.push(Action {
        typ: ActionType::Pass,
        card: None,
    });

    actions
}

fn legal_pitch_order_phase(gs: &Gamestate) -> LegalActions {
    // The active player — the owner of the pitch zone being cleared, the turn
    // player first and then the opponent — picks, card by card, the order
    // their pitched cards go to the bottom of their deck. Every distinct card
    // still in the pitch zone is a choice; there is no pass — the phase only
    // ends once the zone is empty (see `handle_pitch_order_phase`).
    gs.player(gs.active_player).pitch.iter()
        .map(|(id, _)| Action::new(ActionType::BottomPitch, CardRef::new(id, Zone::Pitch)))
        .collect()
}

fn legal_pitch_phase(gs: &Gamestate) -> LegalActions {
    let player = gs.player(gs.active_player);

    // The card being paid for is held pending in the zone it is played from —
    // the hand, or the arsenal. A pending hand card can't pitch for itself, so
    // one copy of it is set aside; a pending arsenal card isn't in hand at all,
    // leaving the whole hand free to pay for it.
    let pending = gs.pending_card;
    let available = match pending {
        Some(p) if p.from == Zone::Hand => player.hand.without(p.card),
        _ => player.hand,
    };

    // A pending card with a "discard a card" additional cost must leave a card
    // behind to discard once its cost is paid. When that's the case, pitching a
    // card is only legal if the rest of the hand can still both finish covering
    // the cost and keep a spare card to discard — so pitching the card that
    // ought to be discarded (e.g. the lone low-pitch card) is disallowed.
    // The same holds for a "reveal a card with cost 1 or less" cost, where the
    // card kept back must be one costing 1 or less.
    let held_back_cost: Option<(u8, fn(&CardData) -> bool)> = pending.and_then(|p| {
        let data = player.card(p.card).data();
        if p.typ != ActionType::PlayCard {
            return None;
        }
        data.additional_cost.as_ref().map(|c| (data.cost, cost_card_filter(c)))
    });

    // Every other card in hand is a pitch option, as long as it actually pitches
    // for resources — cards with a pitch value of 0 produce nothing and can't be
    // pitched.
    available.iter()
        .filter(|&(id, _)| player.card(id).data().pitch > 0)
        .filter(|&(id, _)| match held_back_cost {
            Some((cost, eligible)) => pitch_leaves_card_for_cost(player, available, cost, player.resources, id, eligible),
            None => true,
        })
        .map(|(id, _)| Action::new(ActionType::Pitch, CardRef::new(id, Zone::Hand)))
        .collect()
}

/// While paying for a card that carries an additional cost which holds a hand
/// card back — "discard a card", or "reveal a card with cost 1 or less" —
/// decide whether pitching a copy of `cand` can still leave such a card in hand
/// once the cost is covered. `eligible` says which cards can pay that cost.
/// `available` is the hand without the pending card. Pitching the candidate
/// banks its pitch; the rest of the hand must then be able to cover whatever
/// cost remains while keeping its lowest-pitch eligible card back.
fn pitch_leaves_card_for_cost(
    player: &Player,
    available: CardCounts,
    cost: u8,
    resources: u8,
    cand: DeckCardId,
    eligible: fn(&CardData) -> bool,
) -> bool {
    // Cost still owed after banking what this candidate pitches.
    let cand_pitch = player.card(cand).data().pitch;
    let remaining_cost = cost.saturating_sub(resources.saturating_add(cand_pitch));

    // The rest of the hand left after pitching the candidate.
    let rest = available.without(cand);

    // No eligible card left means the additional cost can't be paid, so this
    // pitch is a dead end.
    let Some(reserved) = min_pitch_where(player, rest, eligible) else {
        return false;
    };
    // Cost already fully covered: the remaining cards include the one held back.
    if remaining_cost == 0 {
        return true;
    }
    // Reserve the lowest-pitch eligible card; the rest must still cover what's
    // owed.
    player.pitch_total(rest).saturating_sub(reserved) >= remaining_cost
}

fn legal_action_phase(gs: &Gamestate) -> LegalActions {
    // The action phase offers every card playable at action speed, plus
    // activating equipment and the equipped weapon.
    let mut actions = legal_play_phase(gs, is_action_phase_playable);

    // Breaking the combat chain is offered while one is open — combat damage
    // has been dealt and play has returned here — and the turn player still has
    // an action point for another attack. Without one, breaking it gains
    // nothing: passing closes the chain anyway as the action phase ends.
    let player = gs.player(gs.turn_player);
    if combat_chain_open(gs) && player.action_points > 0 {
        actions.push(Action {
            typ: ActionType::CloseChain,
            card: None,
        });
    }
    actions
}

/// Whether any card sits on either player's combat chain.
fn combat_chain_open(gs: &Gamestate) -> bool {
    gs.p1.chain_occupied() || gs.p2.chain_occupied()
}

fn legal_instant_phase(gs: &Gamestate) -> LegalActions {
    // The instant phase shares the action phase's machinery but only ever
    // offers instants (see `is_instant_phase_playable`). Equipment and weapon
    // activations are action-speed abilities, so the `is_playable` gate inside
    // `push_equipment_activations` filters them out here.
    legal_play_phase(gs, is_instant_phase_playable)
}

fn legal_reaction_phase(gs: &Gamestate) -> LegalActions {
    // The reaction phase (the combat reaction step) shares the same machinery
    // and offers cards playable at reaction speed, which depends on the side:
    // the attacker (the turn player — only the turn player attacks) may play
    // instants and attack reactions, the defender instants and defense
    // reactions. Activated abilities of those same speeds — e.g. an
    // instant-speed equipment ability — are offered too, gated by the same
    // predicate inside `push_equipment_activations`.
    if gs.active_player == gs.turn_player {
        legal_play_phase(gs, is_attacker_reaction_playable)
    } else {
        legal_play_phase(gs, is_defender_reaction_playable)
    }
}

/// The defend-trigger window (`Phase::DefendTriggers`): the defender has
/// finished declaring blockers and their "when you defend with" triggers are on
/// the stack. Both players hold priority in turn, but the window is narrowed to
/// instants — an instant may be played and resolved above a pending trigger, a
/// reaction may not. Reactions become legal again in the `Reaction` window this
/// one hands over to once its stack empties (see `close_priority_window`).
///
/// Narrowing by phase rather than by scanning the stack means playing an instant
/// in response cannot reopen the window underneath itself: the phase holds until
/// every trigger has resolved.
fn legal_defend_triggers_phase(gs: &Gamestate) -> LegalActions {
    legal_play_phase(gs, is_instant_phase_playable)
}

/// Shared body for the action and instant phases. They differ only in which
/// card types may be played (the `is_playable` predicate); the playable-card
/// affordability, equipment activations, and pass logic are otherwise
/// identical. Equipment abilities (and the weapon swing) are gated by the same
/// `is_playable` predicate, applied to each ability's activation card type.
fn legal_play_phase(gs: &Gamestate, is_playable: fn(CardType) -> bool) -> LegalActions {
    let mut legal_actions = LegalActions::new();
    let player = gs.player(gs.active_player);

    // Total pitch available across the whole hand. Computed once here and shared
    // by both the hand-card playability and equipment-activation affordability
    // checks, since pitching pays for either.
    let total_pitch = player.hand_pitch_total();

    push_playable_cards(&mut legal_actions, player, total_pitch, is_playable);
    push_equipment_activations(&mut legal_actions, player, total_pitch, is_playable);
    push_defending_activations(&mut legal_actions, gs, is_playable);

    // Passing is always available; it ends the window without playing or
    // activating anything.
    legal_actions.push(Action {
        typ: ActionType::Pass,
        card: None,
    });

    legal_actions
}

/// Activations of an ability that may only be used while its card is defending
/// — Rally the Rearguard's "Discard a card: gains +3 block. Activate this
/// ability only while Rally the Rearguard is defending." The card is not in a
/// zone `push_equipment_activations` scans: it is on its controller's combat
/// chain, put there when it was declared as a blocker.
///
/// "Defending" is read as the two windows where a declared blocker is answering
/// a live attack: the defend-trigger window and the reaction window. Narrowing
/// by phase is what keeps the ability off a blocker whose combat is already
/// over — blockers stay on the chain until the action phase ends, so a card that
/// blocked an earlier attack this turn is still sitting there during a later
/// instant window, no longer defending anything. It is the same reason the
/// defend-trigger window narrows by phase rather than by scanning the stack.
///
/// Only the defender may activate: the turn player's chain holds their attack,
/// not blockers, so the check is that the active player is not the turn player.
fn push_defending_activations(actions: &mut LegalActions, gs: &Gamestate, is_playable: fn(CardType) -> bool) {
    // Only while an attack is live and being answered.
    if !matches!(gs.phase, Phase::DefendTriggers | Phase::Reaction) {
        return;
    }
    // Only the defender has blockers on their chain.
    if gs.active_player == gs.turn_player {
        return;
    }

    let defender = gs.player(gs.active_player);
    // Only the turn player attacks, so the link being defended is the current
    // link of the turn player's chain — the same one the blockers were attached
    // to when they were declared.
    let link = current_chain_link(gs.player(gs.turn_player));

    for (id, copies) in defender.chain_link[link].iter() {
        let Some(ability) = &defender.card(id).data().ability else {
            continue;
        };
        // This generator covers only the abilities restricted to defending; any
        // other ability on a card here is activated from its own zone, if at all.
        if !ability.only_while_defending() {
            continue;
        }
        // The ability activates at a particular speed; only offer it when that
        // speed is playable in the current window.
        if !is_playable(ability.card_type()) {
            continue;
        }
        // "Once per turn": a copy that has already been activated this turn is
        // done, whether or not that activation has resolved yet. Another copy
        // on the link may still activate.
        if ability.once_per_turn() && defender.abilities_used.count(id) >= copies {
            continue;
        }
        // A cost that discards cannot be paid from an empty hand. The card
        // itself is on the chain rather than in hand, so it can never be the
        // card it discards.
        if ability.discards_a_card() && defender.hand.is_empty() {
            continue;
        }
        // Whatever resource cost the ability carries must also be payable. No
        // defending ability costs resources today, so this is the same
        // affordability rule as `push_equipment_activations`, stated so a future
        // costed one is not offered for free.
        let total_pitch = defender.hand_pitch_total();
        if total_pitch < ability.resource_cost().saturating_sub(defender.resources) {
            continue;
        }

        actions.push(Action::new(ActionType::Activate, CardRef::new(id, Zone::CombatChain)));
    }
}

fn push_equipment_activations(actions: &mut LegalActions, player: &Player, total_pitch: u8, is_playable: fn(CardType) -> bool) {
    // Each worn piece is only an option if it carries an activated ability
    // (e.g. Blossom of Spring, Gallantry Gold); passive equipment such as Bone
    // Vizier or the Ironhide pieces has none. The weapon belongs here too: a
    // swing is just an activated ability paid for with the ability's resource
    // cost. Each is offered as an Activate; whether resolving it routes the card
    // to the combat chain (a weapon) or the graveyard (an armor ability) is
    // decided later from the card type (see `commits_as_attack`).
    let worn = player.equipment.iter().map(|(id, n)| (id, n, Zone::Equipment));
    // A weapon that attacked earlier in this chain is on the chain rather than
    // in its slot; it may attack again from there, if its once per turn allows
    // (Dorinthea's extra Dawnblade attack). It moves on to the next link.
    let weapon = player.weapon.map(|id| (id, 1, Zone::Weapon)).or_else(|| {
        player.chain_link.iter()
            .flat_map(|link| link.iter())
            .find(|&(id, _)| player.card(id).data().typ == CardType::Weapon)
            .map(|(id, _)| (id, 1, Zone::CombatChain))
    });
    for (id, copies, zone) in worn.chain(weapon) {
        let data = player.card(id).data();
        let Some(ability) = &data.ability else {
            continue;
        };

        // The ability activates at a particular speed; only offer it when
        // that speed is playable in the current phase.
        if !is_playable(ability.card_type()) {
            continue;
        }

        // "Once per turn" (a weapon swing): a card that has already been
        // activated this turn is done, even once it is back in its slot.
        // Dorinthea's hero ability lets the weapon attack one more time
        // (`extra_weapon_attack`).
        let extra = (data.typ == CardType::Weapon && player.extra_weapon_attack()) as u8;
        if ability.once_per_turn() && player.abilities_used.count(id) >= copies + extra {
            continue;
        }

        // Action-speed abilities (a weapon swing, or an action ability such
        // as Blossom of Spring) cost an action point; with none left the
        // player can't activate them. Instant-speed abilities are free.
        if uses_action_point(ActionType::Activate, data) && player.action_points == 0 {
            continue;
        }

        // The activation cost is set by the ability; only offer it when the
        // hand can pitch enough to cover what banked resources don't.
        let needed = ability.resource_cost().saturating_sub(player.resources);
        if total_pitch >= needed {
            actions.push(Action::new(ActionType::Activate, CardRef::new(id, zone)));
        }
    }
}

fn push_playable_cards(actions: &mut LegalActions, player: &Player, total_pitch: u8, is_playable: fn(CardType) -> bool) {
    // The hand is a bitfield of distinct cards, so each card is offered once
    // however many copies are held — copies are an identical choice.
    for (id, _) in player.hand.iter() {
        let data = player.card(id).data();
        if is_playable_now(player, data, is_playable)
            && can_pay_for_card(player, total_pitch, id, PlayedFrom::Hand) {
            actions.push(Action::new(ActionType::PlayCard, CardRef::new(id, Zone::Hand)));
        }
    }

    // The arsenal card is played as though it were in hand: the same speeds, the
    // same action-point cost, and paid for by pitching from hand. It is offered
    // independently of the hand — playing the copy in the arsenal empties the
    // arsenal, so it stays a distinct choice from playing a copy held in hand.
    if let Some(id) = player.arsenal {
        let data = player.card(id).data();
        if is_playable_now(player, data, is_playable)
            && can_pay_for_card(player, total_pitch, id, PlayedFrom::Arsenal) {
            actions.push(Action::new(ActionType::PlayCard, CardRef::new(id, Zone::Arsenal)));
        }
    }
}

/// Whether the card can be played at all right now, before any question of
/// paying for it: its type must be playable at all, it must be playable at the
/// current phase's speed, and — if it is an action, which costs an action
/// point — the player must have a point left. Instants and reactions are free,
/// so they stay playable with none. The card's own text must allow it too: a
/// "Play only if ..." condition, and an attack reaction's need for an attack to
/// target (`play_requirements_met`). Applies to a card in hand and to the
/// arsenal card alike.
fn is_playable_now(player: &Player, data: &CardData, is_playable: fn(CardType) -> bool) -> bool {
    can_ever_be_played(data.typ)
        && is_playable(data.typ)
        && !(uses_action_point(ActionType::PlayCard, data) && player.action_points == 0)
        && play_requirements_met(player, data)
}

/// Whether a card of this type is ever put on the stack, in any window.
///
/// A resource card — Titanium Bauble is the catalog's only one — has no cost
/// line to pay and no text to resolve. It earns its slot in the deck the two
/// other ways any card can: pitched from hand for its 3 resources, or declared
/// as a blocker for its 3 defense. Neither is a *play*, so it must never be
/// offered as one.
///
/// The four speed gates below are allowlists that already omit `Resource`, so
/// this check changes no behavior today. It is stated anyway, next to
/// `is_playable_now` where both the hand and arsenal paths pass through it, so
/// that the rule is a named rule rather than an accident of four separate
/// omissions — the same reason `can_be_declared_as_blocker` tests the card type
/// instead of leaning on every defense reaction carrying `no_block`.
fn can_ever_be_played(typ: CardType) -> bool {
    typ != CardType::Resource
}

/// The zone a card is being played from, which decides whether its own pitch
/// value is part of the pool that pays for it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlayedFrom {
    Hand,
    Arsenal,
}

/// Whether the player can cover the cost of playing a copy of `id`, given
/// `total_pitch` — the pitch available across their whole hand.
///
/// A card played from hand cannot pitch for itself, so its own pitch comes out
/// of the pool. A card played from the arsenal was never in that pool, so the
/// whole hand pays for it. Only hand cards are ever pitched, so the arsenal
/// card itself never adds to the pool either way.
fn can_pay_for_card(
    player: &Player,
    total_pitch: u8,
    id: DeckCardId,
    played_from: PlayedFrom,
) -> bool {
    let data = player.card(id).data();

    // Cost still owed after spending banked resource points.
    let needed = data.cost.saturating_sub(player.resources);

    // Pitch available to pay this card's cost. `total_pitch` summed the whole
    // hand, so subtracting a hand card's own pitch can't underflow;
    // `saturating_sub` makes that explicit rather than relying on the invariant
    // holding.
    let mut pitch_pool = match played_from {
        PlayedFrom::Hand => total_pitch.saturating_sub(data.pitch),
        PlayedFrom::Arsenal => total_pitch,
    };

    // A "discard a card" additional cost reserves one card in hand to be
    // discarded — it can't also be pitched. Set aside the lowest-pitch card of
    // those available (the cheapest to give up) for the discard, so only what
    // remains can pay the cost. With no card in hand to discard, the additional
    // cost can't be paid and the card isn't playable. For an arsenal card every
    // hand card is available to discard, since none of them is the card being
    // played.
    //
    // A "reveal a card with cost 1 or less" cost (Flock of the Feather Walkers)
    // works the same way, except only a card costing 1 or less can be set
    // aside. The revealed card stays in hand, but it must still be there once
    // the pitching is done, so it cannot be pitched either.
    if let Some(eligible) = data.additional_cost.as_ref().map(cost_card_filter) {
        let others = match played_from {
            PlayedFrom::Hand => player.hand.without(id),
            PlayedFrom::Arsenal => player.hand,
        };
        match min_pitch_where(player, others, eligible) {
            Some(reserved) => pitch_pool = pitch_pool.saturating_sub(reserved),
            None => return false,
        }
    }

    // Free to play, or the hand can pitch enough to cover it.
    pitch_pool >= needed
}

/// The pitch value of the lowest-pitch card in `counts` that passes
/// `eligible` — the card an additional cost that holds a hand card back (a
/// discard, or a reveal) sets aside, the cheapest to give up. Removing it from
/// the pitch pool is equivalent to sorting the eligible cards by pitch and
/// dropping the lowest. `None` when no card in `counts` is eligible.
fn min_pitch_where(player: &Player, counts: CardCounts, eligible: fn(&CardData) -> bool) -> Option<u8> {
    counts.iter()
        .map(|(id, _)| player.card(id).data())
        .filter(|data| eligible(data))
        .map(|data| data.pitch)
        .min()
}

/// Which hand cards can pay an additional cost that holds a card back: any
/// card for a discard, a card costing 1 or less for a reveal.
fn cost_card_filter(cost: &AdditionalCostType) -> fn(&CardData) -> bool {
    match cost {
        AdditionalCostType::DiscardCard => |_| true,
        AdditionalCostType::RevealCost1OrLess => can_reveal_for_cost,
    }
}

/// Whether a card in the defender's hand may be committed as a blocker in the
/// defend step. Two things rule it out: the `no_block` flag (a card that cannot
/// block normally, e.g. Smash with Big Tree), and being a defense reaction.
///
/// A defense reaction is never *declared* as a blocker: it is played in the
/// defend reaction step (see `legal_reaction_phase`), where it goes on the stack
/// like an instant, is paid for by pitching, and adds its block to the chain
/// link as it resolves. The card type is checked here rather than leaving it to
/// each defense reaction to also carry `no_block`, so the rule holds for any
/// defense reaction the catalog gains later.
fn can_be_declared_as_blocker(data: &CardData) -> bool {
    !data.no_block && data.typ != CardType::DefenseReaction
}

/// Whether a worn piece of equipment may be declared as a blocker. Any
/// equipment may defend, except — for now — a Battleworn piece (Gallantry
/// Gold): Battleworn's -1 defense counter is not modelled yet, so letting it
/// defend would hand out its full defense every turn, forever.
fn can_equipment_defend(data: &CardData) -> bool {
    use crate::cards::Keyword;
    !data.keyword.contains(Keyword::Battleworn)
}

fn is_action_phase_playable(typ: CardType) -> bool {
    matches!(
        typ,
        CardType::AttackAction |
        CardType::Action |
        CardType::Instant
    )
}

fn is_instant_phase_playable(typ: CardType) -> bool {
    matches!(typ, CardType::Instant)
}

fn is_attacker_reaction_playable(typ: CardType) -> bool {
    matches!(
        typ,
        CardType::Instant |
        CardType::AttackReaction
    )
}

fn is_defender_reaction_playable(typ: CardType) -> bool {
    matches!(
        typ,
        CardType::Instant |
        CardType::DefenseReaction
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Card;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::game_state::{PlayerIndex, Zone};
    use crate::test_support::*;
    use crate::test_support::step;
    use std::collections::HashSet;

    #[test]
    fn legal_actions_in_choose_first_phase() {
        let gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let actions = legal_actions(&gs);

        assert_eq!(actions.len(), 2);
        let types : Vec<ActionType> = actions.iter().map(|x| x.typ).collect();
        assert!(types.contains(&ActionType::ChooseFirst));
        assert!(types.contains(&ActionType::ChooseSecond));
    }

    #[test]
    fn legal_actions_in_action_phase_playcard() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Active player is Rhinar (p1). Every distinct, affordable attack/action
        // card in the opening hand should be offered as a PlayCard action.
        let actions = legal_actions(&gs);

        let plays: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::PlayCard)
                .collect();

        // The opening hand is the three yellow attack actions plus Clearing
        // Bellow; all are playable and affordable from the rest of the hand.
        let playable: HashSet<Card> = plays.iter()
                .map(|a| card_of(&gs, a))
                .collect();
        assert_eq!(playable, HashSet::from([
            Card::MuscleMuttY,
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));

        // Every play is sourced from the Hand.
        for a in &plays {
            assert_eq!(a.card_ref().zone, Zone::Hand);
        }
    }

    #[test]
    fn legal_actions_in_pitch_phase() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Enter the pitch phase directly to isolate its legal-action generator.
        gs.phase = Phase::ActionPitch;

        let actions = legal_actions(&gs);

        // Rhinar's seed-42 opening hand is three yellow attack actions plus a
        // blue defense reaction — all pitch for more than 0, so every card in
        // hand is offered as a Pitch action sourced from the Hand.
        for a in &actions {
            assert_eq!(a.typ, ActionType::Pitch);
            assert_eq!(a.card_ref().zone, Zone::Hand);
        }
        let pitchable: HashSet<Card> = actions.iter()
                .map(|a| card_of(&gs, a))
                .collect();
        assert_eq!(pitchable, HashSet::from([
            Card::MuscleMuttY,
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));
    }

    #[test]
    fn legal_actions_in_pitch_phase_excludes_pending_card() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Play Muscle Mutt: it becomes the pending card and we enter the Pitch
        // phase. The pending card can't pitch for itself, so it should not appear
        // among the pitch options even though it sits in the hand with a positive
        // pitch value.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        assert_eq!(gs.phase, Phase::ActionPitch);

        let actions = legal_actions(&gs);
        assert!(actions.iter().all(|a| named(&gs, a) != Some(mm)));

        // The rest of the opening hand is still offered.
        let pitchable: HashSet<Card> = actions.iter()
                .map(|a| card_of(&gs, a))
                .collect();
        assert_eq!(pitchable, HashSet::from([
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));
    }

    /// Drive the game through the early phases until Dorinthea (p2) is on
    /// defense. Rhinar (p1) goes first, plays Muscle Mutt (an attack action) and
    /// pitches Clearing Bellow to pay for it, then both players pass priority so
    /// the attack resolves onto the combat chain — which flips the active player
    /// to Dorinthea and enters the Defend phase. This mirrors the real engine
    /// flow rather than poking `gs.phase` directly.
    fn step_to_dorinthea_defending() -> TestGame {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);

        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);

        // Both players pass; the attack resolves to the chain and Dorinthea must
        // now defend.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);

        gs
    }

    /// The seat-swapped `step_to_dorinthea_defending`: Dorinthea (p1) attacks
    /// Rhinar (p2) with Flock of the Feather Walkers (cost 1), paid with a
    /// pitch-1 Sharpen Steel, leaving two cards in her hand and Rhinar to defend.
    fn step_to_rhinar_defending() -> TestGame {
        let mut gs = new_game(build_dorinthea_deck(), build_rhinar_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P1);

        let mut hand = hand_cards(&gs, PlayerIndex::P1);
        assert_eq!(hand.len(), 4);
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, hand[0], Card::FlockOfTheFeatherWalkersR);
        hand[0] = Card::FlockOfTheFeatherWalkersR;
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, hand[1], Card::SharpenSteelR);
        hand[1] = Card::SharpenSteelR;
        step_card(&mut gs, ActionType::PlayCard, hand[0]);
        step_card(&mut gs, ActionType::Pitch, hand[1]);
        // Flock's reveal cost: with more than one card to show, pick the first.
        if gs.phase == Phase::RevealChoice {
            let reveal = legal_actions(&gs)[0];
            step(&mut gs, reveal);
        }

        // Both players pass; the attack resolves to the chain and Rhinar must
        // now defend.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Defend);
        assert_eq!(gs.active_player, PlayerIndex::P2);
        gs
    }

    #[test]
    fn legal_actions_in_defend_phase() {
        let gs = step_to_dorinthea_defending();

        let actions = legal_actions(&gs);

        // Dorinthea's seed-42 opening hand is Driving Blade plus three red
        // warrior cards — none are no_block, so every card in hand is offered as
        // a Defend action sourced from her hand.
        let defends: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Defend)
                .collect();
        let from = |zone: Zone| -> HashSet<Card> {
            defends.iter()
                .filter(|a| a.card_ref().zone == zone)
                .map(|a| card_of(&gs, a))
                .collect()
        };
        assert_eq!(from(Zone::Hand), HashSet::from([
            Card::DrivingBladeY,
            Card::SharpenSteelR,
            Card::SecondSwingR,
            Card::InTheSwingR,
        ]));
        // Her worn equipment may defend too — all but Gallantry Gold, whose
        // Battleworn counter is not modelled yet.
        assert_eq!(from(Zone::Equipment), HashSet::from([
            Card::BlossomOfSpring,
            Card::IronrotHelm,
            Card::IronrotLegs,
        ]));
        assert!(defends.iter().all(|a| matches!(a.card_ref().zone, Zone::Hand | Zone::Equipment)));

        // Blocking is optional: passing is offered too, exactly once.
        let passes = actions.iter().filter(|a| a.typ == ActionType::Pass).count();
        assert_eq!(passes, 1);
        assert_eq!(actions.len(), defends.len() + 1);
    }

    #[test]
    fn legal_actions_in_defend_phase_excludes_no_block() {
        let mut gs = step_to_dorinthea_defending();

        // Turn one of Dorinthea's hand cards into Toughen Up, a defense reaction
        // — played in the reaction step, never declared as a blocker — that also
        // carries no_block. It must drop out of the defend options while the
        // rest of the hand is still offered.
        let db = expect_in(&gs, PlayerIndex::P2, Card::DrivingBladeY, Zone::Hand, "Driving Blade should be in the opening hand");
        relabel(&mut gs, PlayerIndex::P2, Zone::Hand, db, Card::ToughenUpB);
        let db = Card::ToughenUpB;
        assert!(Card::ToughenUpB.data().no_block);

        let actions = legal_actions(&gs);

        assert!(actions.iter().all(|a| named(&gs, a) != Some(db)));
        let blockable: HashSet<Card> = actions.iter()
                .filter(|a| a.typ == ActionType::Defend && a.card_ref().zone == Zone::Hand)
                .map(|a| card_of(&gs, a))
                .collect();
        assert_eq!(blockable, HashSet::from([
            Card::SharpenSteelR,
            Card::SecondSwingR,
            Card::InTheSwingR,
        ]));
    }

    /// A card with the given type and `no_block` flag and no rules text, for
    /// testing the blocker predicate on combinations the catalog doesn't
    /// currently hold.
    fn blocker_test_card(typ: CardType, no_block: bool) -> CardData {
        use crate::cards::{CardClass, Keyword};
        CardData {
            typ,
            weapon_type: None,
            cost: 0,
            pitch: 0,
            power: 0,
            defense: 3,
            color: None,
            no_block,
            slot: None,
            card_class: CardClass::Generic,
            keyword: Keyword::empty(),
            hero_life: 0,
            hero_intellect: 0,
            constant_effect: None,
            ability: None,
            defend_effect: None,
            next_attack_effect: None,
            additional_cost: None,
            target_effect: None,
            play_condition: None,
            play_effect: None,
            mentor_effect: None,
        }
    }

    #[test]
    fn can_be_declared_as_blocker_excludes_defense_reactions_and_no_block() {
        // An ordinary card with defense blocks.
        assert!(can_be_declared_as_blocker(&blocker_test_card(CardType::AttackAction, false)));
        // `no_block` rules a card out whatever its type.
        assert!(!can_be_declared_as_blocker(&blocker_test_card(CardType::AttackAction, true)));
        // A defense reaction is ruled out by its type alone: it is played in
        // the defend reaction step instead. Both defense reactions in the
        // catalog also carry no_block, so this is what enforces the rule for a
        // defense reaction that doesn't.
        assert!(!can_be_declared_as_blocker(&blocker_test_card(CardType::DefenseReaction, false)));
        assert!(!can_be_declared_as_blocker(&blocker_test_card(CardType::DefenseReaction, true)));
        // The two the catalog does hold are excluded in practice.
        assert!(!can_be_declared_as_blocker(Card::DodgeB.data()));
        assert!(!can_be_declared_as_blocker(Card::ToughenUpB.data()));
    }

    #[test]
    fn legal_actions_in_arsenal_phase_offers_whole_hand_plus_pass() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Rhinar passes his action phase without playing anything, entering the
        // Arsenal phase with his full opening hand of four cards.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);

        let actions = legal_actions(&gs);

        // Every card in hand is offered as an Arsenal choice, sourced from the
        // hand, plus exactly one Pass to skip arsenaling.
        let arsenals: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Arsenal)
                .collect();
        assert_eq!(arsenals.len(), 4);
        for a in &arsenals {
            assert_eq!(a.card_ref().zone, Zone::Hand);
        }
        let choices: HashSet<Card> = arsenals.iter()
                .map(|a| card_of(&gs, a))
                .collect();
        assert_eq!(choices, HashSet::from([
            Card::MuscleMuttY,
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));

        let passes = actions.iter().filter(|a| a.typ == ActionType::Pass).count();
        assert_eq!(passes, 1);
        assert_eq!(actions.len(), 5);
    }

    #[test]
    fn legal_actions_in_arsenal_phase_occupied_only_offers_pass() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Turn 1: Rhinar passes and arsenals a card, handing the turn to
        // Dorinthea. Turn 2: she passes and skips her arsenal, handing the turn
        // back. Turn 3: Rhinar passes into his Arsenal phase again — his slot is
        // still occupied from turn 1.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::Arsenal, mm);

        assert_eq!(gs.active_player, PlayerIndex::P2);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});

        assert_eq!(gs.active_player, PlayerIndex::P1);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Arsenal);
        assert!(gs.p1.arsenal.is_some());

        // The single arsenal slot is taken, so no Arsenal choices are offered —
        // only the Pass that ends the turn.
        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].typ, ActionType::Pass);
    }

    #[test]
    fn is_attacker_reaction_playable_allows_instants_and_attack_reactions() {
        assert!(is_attacker_reaction_playable(CardType::Instant));
        assert!(is_attacker_reaction_playable(CardType::AttackReaction));
        assert!(!is_attacker_reaction_playable(CardType::DefenseReaction));
        assert!(!is_attacker_reaction_playable(CardType::AttackAction));
        assert!(!is_attacker_reaction_playable(CardType::Action));
        assert!(!is_attacker_reaction_playable(CardType::Equipment));
        assert!(!is_attacker_reaction_playable(CardType::Weapon));
    }

    #[test]
    fn is_defender_reaction_playable_allows_instants_and_defense_reactions() {
        assert!(is_defender_reaction_playable(CardType::Instant));
        assert!(is_defender_reaction_playable(CardType::DefenseReaction));
        assert!(!is_defender_reaction_playable(CardType::AttackReaction));
        assert!(!is_defender_reaction_playable(CardType::AttackAction));
        assert!(!is_defender_reaction_playable(CardType::Action));
        assert!(!is_defender_reaction_playable(CardType::Equipment));
        assert!(!is_defender_reaction_playable(CardType::Weapon));
    }

    #[test]
    fn legal_actions_in_reaction_phase_offers_reactions_not_attack_actions() {
        // Dorinthea attacks here: the attack reactions and the instant this test
        // needs on the attacker's side are all in her deck, none in Rhinar's.
        let mut gs = step_to_rhinar_defending();

        // Finish declaring blocks: passing the Defend phase opens the post-defend
        // Instant window. Double pass closes it, advancing to the Reaction phase
        // with the turn player (Dorinthea, p1) holding priority.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P1);

        // After committing the attack Dorinthea holds two cards. She is the
        // attacker here, so relabel them to an attack reaction (In the Swing)
        // and a non-reaction (Flock of the Feather Walkers, an AttackAction) so
        // we can see the predicate filter at work: only the attack reaction is
        // offered as a PlayCard. The attack is restaged as her second Dawnblade
        // swing, which In the Swing needs to be playable at all.
        stage_second_weapon_swing(&mut gs, PlayerIndex::P1);
        let mut hand = hand_cards(&gs, PlayerIndex::P1);
        assert_eq!(hand.len(), 2, "expected Dorinthea to hold two cards in reaction");
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, hand[0], Card::InTheSwingR);
        hand[0] = Card::InTheSwingR;
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, hand[1], Card::FlockOfTheFeatherWalkersR);
        hand[1] = Card::FlockOfTheFeatherWalkersR;

        let actions = legal_actions(&gs);

        let playable: HashSet<Card> = actions.iter()
                .filter(|a| a.typ == ActionType::PlayCard)
                .map(|a| card_of(&gs, a))
                .collect();
        assert!(playable.contains(&Card::InTheSwingR));
        assert!(!playable.contains(&Card::FlockOfTheFeatherWalkersR));

        // A defense reaction (Toughen Up) is not the attacker's to play:
        // relabeled to one, the card drops out of the offered plays — even with
        // its cost of 2 already banked.
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, hand[0], Card::ToughenUpB);
        hand[0] = Card::ToughenUpB;
        gs.p1.resources = 2;
        let actions = legal_actions(&gs);
        assert!(actions.iter().all(|a| a.typ != ActionType::PlayCard
                || card_of(&gs, a) != Card::ToughenUpB));
        gs.p1.resources = 0;

        // Relabel the reaction to an Instant (Sigil of Solace) — still a reaction-
        // speed card, so it stays offered.
        relabel(&mut gs, PlayerIndex::P1, Zone::Hand, hand[0], Card::SigilofSolaceB);
        hand[0] = Card::SigilofSolaceB;
        let actions = legal_actions(&gs);
        let playable: HashSet<Card> = actions.iter()
                .filter(|a| a.typ == ActionType::PlayCard)
                .map(|a| card_of(&gs, a))
                .collect();
        assert!(playable.contains(&Card::SigilofSolaceB));

        // Pass is still offered exactly once.
        let passes = actions.iter().filter(|a| a.typ == ActionType::Pass).count();
        assert_eq!(passes, 1);
    }

    #[test]
    fn legal_actions_in_reaction_phase_defender_plays_defense_not_attack_reactions() {
        let mut gs = step_to_dorinthea_defending();

        // Defend pass → post-defend Instant window → Reaction phase with the
        // attacker (Rhinar, p1) holding priority; his pass hands priority to
        // the defender (Dorinthea, p2).
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P2);

        // Relabel two of Dorinthea's hand cards to a defense reaction (Toughen
        // Up) and an attack reaction (Thrust): as the defender she is offered the
        // defense reaction but not the attack reaction. The rest of her hand
        // pitches for Toughen Up's 2, so cost is not what decides it.
        let mut hand = hand_cards(&gs, PlayerIndex::P2);
        assert!(hand.len() >= 4, "expected Dorinthea to hold her opening hand in reaction");
        relabel(&mut gs, PlayerIndex::P2, Zone::Hand, hand[0], Card::ToughenUpB);
        hand[0] = Card::ToughenUpB;
        relabel(&mut gs, PlayerIndex::P2, Zone::Hand, hand[1], Card::ThrustR);
        hand[1] = Card::ThrustR;

        let actions = legal_actions(&gs);

        let playable: HashSet<Card> = actions.iter()
                .filter(|a| a.typ == ActionType::PlayCard)
                .map(|a| card_of(&gs, a))
                .collect();
        assert!(playable.contains(&Card::ToughenUpB));
        assert!(!playable.contains(&Card::ThrustR));
    }

    #[test]
    fn legal_actions_in_reaction_phase_double_pass_returns_to_action() {
        let mut gs = step_to_dorinthea_defending();

        // Defend pass → post-defend Instant window. Double pass closes it,
        // advancing to the Reaction phase on an empty stack.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);

        // Neither player has a reaction to add, so both pass; the reaction step
        // ends and play returns to the turn player's Action phase.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Action);
        assert_eq!(gs.active_player, gs.turn_player);
    }

    #[test]
    fn is_instant_phase_playable_only_allows_instants() {
        assert!(is_instant_phase_playable(CardType::Instant));
        assert!(!is_instant_phase_playable(CardType::AttackAction));
        assert!(!is_instant_phase_playable(CardType::Action));
        assert!(!is_instant_phase_playable(CardType::AttackReaction));
        assert!(!is_instant_phase_playable(CardType::DefenseReaction));
        assert!(!is_instant_phase_playable(CardType::Equipment));
        assert!(!is_instant_phase_playable(CardType::Weapon));
    }

    #[test]
    fn can_ever_be_played_excludes_resources() {
        // A resource card is never put on the stack: it is pitched or used as a
        // blocker instead. Titanium Bauble is the catalog's only one.
        assert!(!can_ever_be_played(CardType::Resource));
        assert_eq!(Card::TitaniumBaubleB.data().typ, CardType::Resource);

        // Every type that a speed gate does offer passes this first check, so
        // it narrows nothing else.
        assert!(can_ever_be_played(CardType::AttackAction));
        assert!(can_ever_be_played(CardType::Action));
        assert!(can_ever_be_played(CardType::Instant));
        assert!(can_ever_be_played(CardType::AttackReaction));
        assert!(can_ever_be_played(CardType::DefenseReaction));
    }

    #[test]
    fn legal_actions_in_instant_phase_only_offers_pass() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Rhinar plays Muscle Mutt (cost 3): it becomes pending and we drop into
        // the ActionPitch phase.
        let mm = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
        step_card(&mut gs, ActionType::PlayCard, mm);
        assert_eq!(gs.phase, Phase::ActionPitch);

        // Pitch Clearing Bellow (pitch 3) to cover the cost. Muscle Mutt commits
        // to the stack and the game advances to the Instant phase.
        let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
        step_card(&mut gs, ActionType::Pitch, cb);
        assert_eq!(gs.phase, Phase::ActionInstant);

        // Rhinar's remaining hand is two yellow attack actions (Pack Call, Raging
        // Onslaught) — no instants. Even though the equipped Bone Basher is still
        // affordable, weapon/equipment activations are not offered at instant
        // speed, so the only legal action is to pass.
        let actions = legal_actions(&gs);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].typ, ActionType::Pass);
    }

    #[test]
    fn legal_actions_in_action_phase_includes_pass() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Pass is always offered during the action phase, exactly once.
        let actions = legal_actions(&gs);
        let passes: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Pass)
                .collect();
        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].card, None);
    }

    #[test]
    fn legal_actions_in_action_phase_activate_equipment() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Active player is Rhinar (p1). Every equipped card with an activated
        // ability is offered as an Activate, including the weapon swing. The
        // passive equipment (Bone Vizier, Ironhide Gauntlet/Legs) is not.
        let actions = legal_actions(&gs);

        let activations: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Activate)
                .collect();

        // Blossom of Spring (chest ability) and Bone Basher (weapon swing) are
        // both Activates; the passive equipment is offered as neither.
        let activatable: HashSet<Card> = activations.iter()
                .map(|a| card_of(&gs, a))
                .collect();
        assert_eq!(
            activatable,
            HashSet::from([Card::BlossomOfSpring, Card::BoneBasher])
        );

        // Each action names the zone its card sits in: Bone Basher the weapon
        // zone, Blossom of Spring the equipment zones.
        for a in &activations {
            match card_of(&gs, a) {
                Card::BoneBasher => assert_eq!(a.card_ref().zone, Zone::Weapon),
                Card::BlossomOfSpring => assert_eq!(a.card_ref().zone, Zone::Equipment),
                other => panic!("unexpected activation for {:?}", other),
            }
        }
    }

    #[test]
    fn legal_actions_in_action_phase_activate_equipment_dorinthea() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

        // Choosing second flips the active player to Dorinthea (p2).
        let go_second = Action{ typ: ActionType::ChooseSecond, card: None};
        step(&mut gs, go_second);
        assert_eq!(gs.active_player, PlayerIndex::P2);

        let actions = legal_actions(&gs);

        let activations: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Activate)
                .collect();

        // Activations: Gallantry Gold (arms) + Blossom of Spring (chest, a
        // Generic piece in both decks) + Dawnblade (weapon swing). The passive
        // equipment (Ironrot Helm/Legs) is offered as neither.
        let activatable: HashSet<Card> = activations.iter()
                .map(|a| card_of(&gs, a))
                .collect();
        assert_eq!(
            activatable,
            HashSet::from([Card::GallantryGold, Card::BlossomOfSpring, Card::Dawnblade])
        );

        // Each action names the zone its card sits in.
        for a in &activations {
            match card_of(&gs, a) {
                Card::GallantryGold | Card::BlossomOfSpring => {
                    assert_eq!(a.card_ref().zone, Zone::Equipment)
                }
                Card::Dawnblade => assert_eq!(a.card_ref().zone, Zone::Weapon),
                other => panic!("unexpected activation for {:?}", other),
            }
        }
    }

    /// Drop cards from `pid`'s hand into the graveyard until only `n` remain.
    fn trim_hand(gs: &mut Gamestate, pid: PlayerIndex, n: usize) {
        let keep: Vec<Card> = hand_cards(gs, pid).into_iter().take(n).collect();
        set_hand(gs, pid, &keep);
    }

    /// Whether a `PlayCard` from the arsenal is offered.
    fn arsenal_play_offered(gs: &Gamestate) -> bool {
        legal_actions(gs).iter()
            .any(|a| a.typ == ActionType::PlayCard && a.card_ref().zone == Zone::Arsenal)
    }

    /// The cards offered to the active player as `PlayCard` actions.
    fn playable_cards(gs: &Gamestate) -> HashSet<Card> {
        legal_actions(gs).iter()
            .filter(|a| a.typ == ActionType::PlayCard)
            .map(|a| card_of(&gs, a))
            .collect()
    }

    #[test]
    fn legal_actions_in_action_phase_offers_the_arsenal_card() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // A 0-cost action in the arsenal is playable exactly as it would be
        // from hand: the arsenal card is played as though it were in hand.
        let arsenal = put_in_arsenal(&mut gs, PlayerIndex::P1, Card::ClearingBellowB);

        assert!(arsenal_play_offered(&gs), "the arsenal card should be offered as a play");
        assert!(in_zone(&gs, PlayerIndex::P1, arsenal, Zone::Arsenal),
            "it is still in the arsenal until it is actually played");
    }

    #[test]
    fn legal_actions_in_action_phase_excludes_an_arsenal_card_of_the_wrong_speed() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // A defense reaction is not playable at action speed, in hand or in the
        // arsenal: the same phase predicate gates both.
        put_in_arsenal(&mut gs, PlayerIndex::P1, Card::DodgeB);

        assert!(!playable_cards(&gs).contains(&Card::DodgeB));
    }

    #[test]
    fn legal_actions_in_action_phase_pays_for_the_arsenal_card_out_of_hand() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

        // Muscle Mutt costs 3 and the arsenal card cannot pitch for itself, so
        // the cost has to come out of hand. With a hand that pitches for
        // nothing — a lone Chief Ruk'utan, the deck's one pitch-0 card — it is
        // unaffordable and drops out of the options.
        put_in_arsenal(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
        assert_eq!(Card::MuscleMuttY.data().cost, 3);
        trim_hand(&mut gs, PlayerIndex::P1, 1);
        set_hand(&mut gs, PlayerIndex::P1, &[Card::ChiefRukutan]); // pitch 0
        assert_eq!(Card::ChiefRukutan.data().pitch, 0);
        assert!(!arsenal_play_offered(&gs),
            "an arsenal card the hand cannot pay for must not be offered");

        // Give the hand a single pitch-3 card and it becomes affordable — the
        // whole hand pays, since none of it is the card being played.
        set_hand(&mut gs, PlayerIndex::P1, &[Card::ClearingBellowB]); // pitch 3
        assert_eq!(Card::ClearingBellowB.data().pitch, 3);
        assert!(arsenal_play_offered(&gs));
    }

    #[test]
    fn legal_actions_in_reaction_phase_offers_the_arsenal_card_to_the_defender_only() {
        let mut gs = step_to_dorinthea_defending();
        step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
        assert_eq!(gs.phase, Phase::Reaction);

        // The attacker holds priority first: a defense reaction in *his*
        // arsenal is no more playable than one in his hand.
        assert_eq!(gs.active_player, PlayerIndex::P1);
        put_in_arsenal(&mut gs, PlayerIndex::P1, Card::DodgeB);
        assert!(!playable_cards(&gs).contains(&Card::DodgeB),
            "the attacking player must not be offered a defense reaction from arsenal");

        // Priority passes to the defender, who may play hers.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.active_player, PlayerIndex::P2);
        // Dorinthea's defense reaction is Toughen Up; the three cards left in
        // her hand pitch for its 2.
        put_in_arsenal(&mut gs, PlayerIndex::P2, Card::ToughenUpB);
        assert!(arsenal_play_offered(&gs));
    }
}

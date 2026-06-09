use crate::game_state::{Gamestate, Phase, Player, PlayerIndex, CardState, CardIdx, TOTAL_CARDS};
use crate::action::{Action, ActionType};
use crate::cards::{Card, CardType};
use crate::fab_step::uses_action_point;


pub fn legal_actions(gs: &Gamestate) -> Vec<Action> {
    match gs.phase {
        Phase::ChooseFirst => {
            let mut actions = Vec::new();
            actions.push(Action{
                        typ : ActionType::ChooseFirst,
                        card: None});
            actions.push(Action{
                        typ : ActionType::ChooseSecond,
                        card: None});
            actions
        },
        Phase::Action => legal_action_phase(gs),
        Phase::ActionPitch | Phase::ReactionPitch => legal_pitch_phase(gs),
        Phase::ActionInstant => legal_instant_phase(gs),
        Phase::Defend => legal_defend_phase(gs),
        Phase::Reaction => legal_reaction_phase(gs),
        Phase::Start => Vec::new(),
    }
}

fn legal_defend_phase(gs: &Gamestate) -> Vec<Action> {
    // When we enter the Defend phase the active player is flipped to the
    // defender (see resolve_top_of_stack), so the usual active-player lookup
    // gives us the player choosing blockers.
    let player = if gs.active_player == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };

    // Every card in hand can be committed as a blocker except those flagged
    // no_block (e.g. cards with no defense that cannot block normally).
    player.hand_iter(&gs.cards)
        .filter(|(_, cs)| !cs.card.data().no_block)
        .map(|(idx, _)| Action {
            typ: ActionType::Defend,
            card: Some(CardIdx::new(idx)),
        })
        .collect()
}

fn legal_pitch_phase(gs: &Gamestate) -> Vec<Action> {
    let player = if gs.active_player == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };

    // The card being paid for is held pending in the hand; it can't pitch for
    // itself, so exclude it from the options.
    let pending_index = gs.pending_card.map(|p| p.index.get());

    // Every other card in hand is a pitch option, as long as it actually pitches
    // for resources — cards with a pitch value of 0 produce nothing and can't be
    // pitched.
    player.hand_iter(&gs.cards)
        .filter(|(idx, _)| Some(*idx) != pending_index)
        .filter(|(_, cs)| cs.card.data().pitch > 0)
        .map(|(idx, _)| Action {
            typ: ActionType::Pitch,
            card: Some(CardIdx::new(idx)),
        })
        .collect()
}

fn legal_action_phase(gs: &Gamestate) -> Vec<Action> {
    // The action phase offers every card playable at action speed, plus
    // activating equipment and the equipped weapon.
    legal_play_phase(gs, is_action_phase_playable)
}

fn legal_instant_phase(gs: &Gamestate) -> Vec<Action> {
    // The instant phase shares the action phase's machinery but only ever
    // offers instants (see `is_instant_phase_playable`). Equipment and weapon
    // activations are action-speed abilities, so the `is_playable` gate inside
    // `get_equipment_activations` filters them out here.
    legal_play_phase(gs, is_instant_phase_playable)
}

fn legal_reaction_phase(gs: &Gamestate) -> Vec<Action> {
    // The reaction phase (the combat reaction step) shares the same machinery
    // and offers cards playable at reaction speed: instants plus attack and
    // defense reactions (see `is_reaction_phase_playable`). Activated abilities
    // of those same speeds — e.g. an instant-speed equipment ability — are
    // offered too, gated by the same predicate inside `get_equipment_activations`.
    legal_play_phase(gs, is_reaction_phase_playable)
}

/// Shared body for the action and instant phases. They differ only in which
/// card types may be played (the `is_playable` predicate); the playable-card
/// affordability, equipment activations, and pass logic are otherwise
/// identical. Equipment abilities (and the weapon swing) are gated by the same
/// `is_playable` predicate, applied to each ability's activation card type.
fn legal_play_phase(gs: &Gamestate, is_playable: fn(CardType) -> bool) -> Vec<Action> {
    let mut legal_actions = Vec::new();
    let player = if gs.active_player == PlayerIndex::P1 { &gs.p1 } else { &gs.p2 };

    // Total pitch available across the whole hand. Computed once here and shared
    // by both the hand-card playability and equipment-activation affordability
    // checks, since pitching pays for either.
    let total_pitch: u8 = player.hand_iter(&gs.cards)
        .map(|(_, cs)| cs.card.data().pitch)
        .sum();

    legal_actions.extend(get_playable_cards(player, &gs.cards, total_pitch, is_playable));
    legal_actions.extend(get_equipment_activations(player, &gs.cards, total_pitch, is_playable));

    // Passing is always available; it ends the window without playing or
    // activating anything.
    legal_actions.push(Action {
        typ: ActionType::Pass,
        card: None,
    });

    legal_actions
}

fn get_equipment_activations(player: &Player, cards: &[CardState; TOTAL_CARDS], total_pitch: u8, is_playable: fn(CardType) -> bool) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();

    // Each worn piece is only an option if it carries an activated ability
    // (e.g. Blossom of Spring, Gallantry Gold); passive equipment such as Bone
    // Vizier or the Ironhide pieces has none. The weapon belongs here too: a
    // swing is just an activated ability paid for with the ability's resource
    // cost. Each is offered as an Activate; whether resolving it routes the card
    // to the combat chain (a weapon) or the graveyard (an armor ability) is
    // decided later from the card type (see `commits_as_attack`).
    let activatable_slots = [
        player.head_idx,
        player.chest_idx,
        player.arms_idx,
        player.legs_idx,
        player.weapon_idx,
    ];
    for slot in activatable_slots {
        if let Some(idx) = slot {
            let idx = idx.get();
            let Some(ability) = &cards[idx].card.data().ability else {
                continue;
            };

            // The ability activates at a particular speed; only offer it when
            // that speed is playable in the current phase.
            if !is_playable(ability.card_type()) {
                continue;
            }

            // Action-speed abilities (a weapon swing, or an action ability such
            // as Blossom of Spring) cost an action point; with none left the
            // player can't activate them. Instant-speed abilities are free.
            if uses_action_point(ActionType::Activate, cards[idx].card.data())
                && player.action_points == 0 {
                continue;
            }

            // The activation cost is set by the ability; only offer it when the
            // hand can pitch enough to cover what banked resources don't.
            let needed = ability.resource_cost().saturating_sub(player.resources);
            if total_pitch >= needed {
                actions.push(Action {
                    typ: ActionType::Activate,
                    card: Some(CardIdx::new(idx)),
                });
            }
        }
    }

    actions
}

fn get_playable_cards(player: &Player, cards: &[CardState; TOTAL_CARDS], total_pitch: u8, is_playable: fn(CardType) -> bool) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();

    let mut seen: Vec<Card> = Vec::new();
    for (idx, cardstate) in player.hand_iter(cards) {
        let card = cardstate.card;
        let data = card.data();

        // Only cards playable in the current phase
        if !is_playable(data.typ) {
            continue;
        }

        // Action cards (attack actions and non-attack actions) cost an action
        // point to play; with none left they drop out of the options. Instants
        // are free, so they remain playable.
        if uses_action_point(ActionType::PlayCard, data) && player.action_points == 0 {
            continue;
        }

        // Duplicate copies are an identical choice — emit only the first.
        if seen.contains(&card) {
            continue;
        }
        seen.push(card);

        // Cost still owed after spending banked resource points.
        let needed = data.cost.saturating_sub(player.resources);

        // Free to play, or the remaining hand can pitch enough to cover it.
        // `total_pitch` summed the whole hand including this card, so subtracting
        // this card's own pitch can't underflow; `saturating_sub` makes that
        // explicit rather than relying on the invariant holding.
        let other_pitch = total_pitch.saturating_sub(data.pitch);
        if other_pitch >= needed {
            actions.push(Action {
                typ: ActionType::PlayCard,
                card: Some(CardIdx::new(idx)),
            });
        }
    }

    actions
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

fn is_reaction_phase_playable(typ: CardType) -> bool {
    matches!(
        typ,
        CardType::Instant |
        CardType::AttackReaction |
        CardType::DefenseReaction
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset};
    use crate::fab_step::step;
    use crate::game_state::CardLocation;
    use std::collections::HashSet;

    #[test]
    fn legal_actions_in_choose_first_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let actions = legal_actions(&gs);

        assert_eq!(actions.len(), 2);
        let types : Vec<ActionType> = actions.iter().map(|x| x.typ).collect();
        assert!(types.contains(&ActionType::ChooseFirst));
        assert!(types.contains(&ActionType::ChooseSecond));
    }

    #[test]
    fn legal_actions_in_action_phase_playcard() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

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
                .map(|a| gs.cards[a.card_index()].card)
                .collect();
        assert_eq!(playable, HashSet::from([
            Card::MuscleMuttY,
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));

        // Every play is sourced from the Hand.
        for a in &plays {
            assert_eq!(gs.cards[a.card_index()].location, CardLocation::P1Hand);
        }
    }

    #[test]
    fn legal_actions_in_pitch_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

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
            assert_eq!(gs.cards[a.card_index()].location, CardLocation::P1Hand);
        }
        let pitchable: HashSet<Card> = actions.iter()
                .map(|a| gs.cards[a.card_index()].card)
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
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Play Muscle Mutt: it becomes the pending card and we enter the Pitch
        // phase. The pending card can't pitch for itself, so it should not appear
        // among the pitch options even though it sits in the hand with a positive
        // pitch value.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});
        assert_eq!(gs.phase, Phase::ActionPitch);

        let actions = legal_actions(&gs);
        assert!(actions.iter().all(|a| a.card != Some(CardIdx::new(mm_idx))));

        // The rest of the opening hand is still offered.
        let pitchable: HashSet<Card> = actions.iter()
                .map(|a| gs.cards[a.card_index()].card)
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

        // Both players pass; the attack resolves to the chain and Dorinthea must
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
        for a in &actions {
            assert_eq!(a.typ, ActionType::Defend);
            assert_eq!(gs.cards[a.card_index()].location, CardLocation::P2Hand);
        }
        let blockable: HashSet<Card> = actions.iter()
                .map(|a| gs.cards[a.card_index()].card)
                .collect();
        assert_eq!(blockable, HashSet::from([
            Card::DrivingBladeY,
            Card::SharpenSteelR,
            Card::SecondSwingR,
            Card::InTheSwingR,
        ]));
    }

    #[test]
    fn legal_actions_in_defend_phase_excludes_no_block() {
        let mut gs = step_to_dorinthea_defending();

        // Turn one of Dorinthea's hand cards into Dodge, a defense reaction that
        // cannot block normally (no_block). It must drop out of the defend
        // options while the rest of the hand is still offered.
        let db_idx = gs.p2.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::DrivingBladeY)
                .map(|(idx, _)| idx)
                .expect("Driving Blade should be in the opening hand");
        gs.cards[db_idx].card = Card::DodgeB;

        let actions = legal_actions(&gs);

        assert!(actions.iter().all(|a| a.card != Some(CardIdx::new(db_idx))));
        let blockable: HashSet<Card> = actions.iter()
                .map(|a| gs.cards[a.card_index()].card)
                .collect();
        assert_eq!(blockable, HashSet::from([
            Card::SharpenSteelR,
            Card::SecondSwingR,
            Card::InTheSwingR,
        ]));
    }

    #[test]
    fn is_reaction_phase_playable_allows_instants_and_reactions() {
        assert!(is_reaction_phase_playable(CardType::Instant));
        assert!(is_reaction_phase_playable(CardType::AttackReaction));
        assert!(is_reaction_phase_playable(CardType::DefenseReaction));
        assert!(!is_reaction_phase_playable(CardType::AttackAction));
        assert!(!is_reaction_phase_playable(CardType::Action));
        assert!(!is_reaction_phase_playable(CardType::Equipment));
        assert!(!is_reaction_phase_playable(CardType::Weapon));
    }

    #[test]
    fn legal_actions_in_reaction_phase_offers_reactions_not_attack_actions() {
        let mut gs = step_to_dorinthea_defending();

        // Finish declaring blocks: passing the Defend phase opens the post-defend
        // Instant window. Double pass closes it, advancing to the Reaction phase
        // with the turn player (Rhinar, p1) holding priority.
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        assert_eq!(gs.phase, Phase::Reaction);
        assert_eq!(gs.active_player, PlayerIndex::P1);

        // After committing the attack Rhinar holds two cards. Relabel them to a
        // reaction-speed card (Dodge, a DefenseReaction, cost 0) and a non-
        // reaction (Pack Call, an AttackAction) so we can see the predicate
        // filter at work: only the reaction is offered as a PlayCard.
        let hand: Vec<usize> = gs.p1.hand_iter(&gs.cards).map(|(idx, _)| idx).collect();
        assert_eq!(hand.len(), 2, "expected Rhinar to hold two cards in reaction");
        gs.cards[hand[0]].card = Card::DodgeB;
        gs.cards[hand[1]].card = Card::PackCallY;

        let actions = legal_actions(&gs);

        let playable: HashSet<Card> = actions.iter()
                .filter(|a| a.typ == ActionType::PlayCard)
                .map(|a| gs.cards[a.card_index()].card)
                .collect();
        assert!(playable.contains(&Card::DodgeB));
        assert!(!playable.contains(&Card::PackCallY));

        // Relabel the reaction to an Instant (Sigil of Solace) — still a reaction-
        // speed card, so it stays offered.
        gs.cards[hand[0]].card = Card::SigilofSolaceB;
        let actions = legal_actions(&gs);
        let playable: HashSet<Card> = actions.iter()
                .filter(|a| a.typ == ActionType::PlayCard)
                .map(|a| gs.cards[a.card_index()].card)
                .collect();
        assert!(playable.contains(&Card::SigilofSolaceB));

        // Pass is still offered exactly once.
        let passes = actions.iter().filter(|a| a.typ == ActionType::Pass).count();
        assert_eq!(passes, 1);
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
    fn legal_actions_in_instant_phase_only_offers_pass() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, card: None};
        step(&mut gs, go_first);

        // Rhinar plays Muscle Mutt (cost 3): it becomes pending and we drop into
        // the ActionPitch phase.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, card: Some(CardIdx::new(mm_idx))});
        assert_eq!(gs.phase, Phase::ActionPitch);

        // Pitch Clearing Bellow (pitch 3) to cover the cost. Muscle Mutt commits
        // to the stack and the game advances to the Instant phase.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, card: Some(CardIdx::new(cb_idx))});
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
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

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
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

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
                .map(|a| gs.cards[a.card_index()].card)
                .collect();
        assert_eq!(
            activatable,
            HashSet::from([Card::BlossomOfSpring, Card::BoneBasher])
        );

        // Each card sits in its expected zone, derived from its slot rather than
        // carried on the action: Bone Basher as the weapon, Blossom of Spring as
        // chest equipment.
        for a in &activations {
            let cs = gs.cards[a.card_index()];
            match cs.card {
                Card::BoneBasher => assert_eq!(cs.location, CardLocation::P1Weapon),
                Card::BlossomOfSpring => assert_eq!(cs.location, CardLocation::P1Chest),
                other => panic!("unexpected activation for {:?}", other),
            }
        }
    }

    #[test]
    fn legal_actions_in_action_phase_activate_equipment_dorinthea() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

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
                .map(|a| gs.cards[a.card_index()].card)
                .collect();
        assert_eq!(
            activatable,
            HashSet::from([Card::GallantryGold, Card::BlossomOfSpring, Card::Dawnblade])
        );

        // Each card sits in its expected slot, derived from its CardState rather
        // than carried on the action.
        for a in &activations {
            let cs = gs.cards[a.card_index()];
            match cs.card {
                Card::GallantryGold => assert_eq!(cs.location, CardLocation::P2Arms),
                Card::BlossomOfSpring => assert_eq!(cs.location, CardLocation::P2Chest),
                Card::Dawnblade => assert_eq!(cs.location, CardLocation::P2Weapon),
                other => panic!("unexpected activation for {:?}", other),
            }
        }
    }

}

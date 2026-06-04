use crate::game_state::{Gamestate, Phase, Player, CardState, TOTAL_CARDS};
use crate::action::{Action, ActionType};
use crate::cards::{Card, CardType};
use crate::classic_battles::get_card_catalog;


pub fn legal_actions(gs: &Gamestate) -> Vec<Action> {
    match gs.phase {
        Phase::ChooseFirst => {
            let mut actions = Vec::new();
            actions.push(Action{
                        typ : ActionType::ChooseFirst,
                        index: 0});
            actions.push(Action{
                        typ : ActionType::ChooseSecond,
                        index: 0});
            actions
        },
        Phase::Action => legal_action_phase(gs),
        Phase::Pitch => legal_pitch_phase(gs),
        Phase::Instant => legal_instant_phase(gs),
        Phase::Defend => legal_defend_phase(gs),
        Phase::Start => Vec::new()
    }
}

fn legal_defend_phase(gs: &Gamestate) -> Vec<Action> {
    let catalog = get_card_catalog();
    // When we enter the Defend phase the active player is flipped to the
    // defender (see resolve_top_of_stack), so the usual active-player lookup
    // gives us the player choosing blockers.
    let player = if gs.active_player == 0 { &gs.p1 } else { &gs.p2 };

    // Every card in hand can be committed as a blocker except those flagged
    // no_block (e.g. cards with no defense that cannot block normally).
    player.hand_iter(&gs.cards)
        .filter(|(_, cs)| !catalog[cs.card as usize].no_block)
        .map(|(idx, _)| Action {
            typ: ActionType::Defend,
            index: idx,
        })
        .collect()
}

fn legal_pitch_phase(gs: &Gamestate) -> Vec<Action> {
    let catalog = get_card_catalog();
    let player = if gs.active_player == 0 { &gs.p1 } else { &gs.p2 };

    // The card being paid for is held pending in the hand; it can't pitch for
    // itself, so exclude it from the options.
    let pending_index = gs.pending_card.map(|p| p.index);

    // Every other card in hand is a pitch option, as long as it actually pitches
    // for resources — cards with a pitch value of 0 produce nothing and can't be
    // pitched.
    player.hand_iter(&gs.cards)
        .filter(|(idx, _)| Some(*idx) != pending_index)
        .filter(|(_, cs)| catalog[cs.card as usize].pitch > 0)
        .map(|(idx, _)| Action {
            typ: ActionType::Pitch,
            index: idx,
        })
        .collect()
}

fn legal_action_phase(gs: &Gamestate) -> Vec<Action> {
    // The action phase offers every card playable at action speed, plus
    // activating equipment and the equipped weapon.
    legal_play_phase(gs, is_action_phase_playable, true)
}

fn legal_instant_phase(gs: &Gamestate) -> Vec<Action> {
    // The instant phase shares the action phase's machinery but only ever
    // offers instants (see `is_instant_phase_playable`). Equipment and weapon
    // activations are action-speed, so they are not offered here.
    legal_play_phase(gs, is_instant_phase_playable, false)
}

/// Shared body for the action and instant phases. They differ only in which
/// card types may be played (the `is_playable` predicate) and whether
/// equipment/weapon activations are offered (`allow_equipment`); the
/// playable-card affordability and pass logic is otherwise identical.
fn legal_play_phase(gs: &Gamestate, is_playable: fn(CardType) -> bool, allow_equipment: bool) -> Vec<Action> {
    let catalog = get_card_catalog();
    let mut legal_actions = Vec::new();
    let player = if gs.active_player == 0 { &gs.p1 } else { &gs.p2 };

    // Total pitch available across the whole hand. Computed once here and shared
    // by both the hand-card playability and equipment-activation affordability
    // checks, since pitching pays for either.
    let total_pitch: u8 = player.hand_iter(&gs.cards)
        .map(|(_, cs)| catalog[cs.card as usize].pitch)
        .sum();

    legal_actions.extend(get_playable_cards(player, &gs.cards, total_pitch, is_playable));
    if allow_equipment {
        legal_actions.extend(get_equipment_activations(player, &gs.cards, total_pitch));
    }

    // Passing is always available; it ends the window without playing or
    // activating anything.
    legal_actions.push(Action {
        typ: ActionType::Pass,
        index: 0,
    });

    legal_actions
}

fn get_equipment_activations(player: &Player, cards: &[CardState; TOTAL_CARDS], total_pitch: u8) -> Vec<Action> {
    let catalog = get_card_catalog();
    let mut actions: Vec<Action> = Vec::new();

    // Worn armor pieces are only an option if they carry an activated ability
    // (e.g. Blossom of Spring, Gallantry Gold). Passive equipment such as
    // Bone Vizier or the Ironhide pieces has none.
    let armor_slots = [
        player.head_idx,
        player.chest_idx,
        player.arms_idx,
        player.legs_idx,
    ];
    for slot in armor_slots {
        if let Some(idx) = slot {
            let idx = idx as usize;
            let Some(ability) = &catalog[cards[idx].card as usize].ability else {
                continue;
            };

            // The activation cost is set by the ability; only offer it when the
            // hand can pitch enough to cover what banked resources don't.
            let needed = ability.resource_cost().saturating_sub(player.resources);
            if total_pitch >= needed {
                actions.push(Action {
                    typ: ActionType::Activate,
                    index: idx,
                });
            }
        }
    }

    // A weapon swing is its own action (not an equipment activation): it costs
    // the weapon's own catalog resource cost, unlike an armor ability which
    // costs the ability's resource cost.
    if let Some(idx) = player.weapon_idx {
        let idx = idx as usize;
        let needed = catalog[cards[idx].card as usize]
            .cost
            .saturating_sub(player.resources);
        if total_pitch >= needed {
            actions.push(Action {
                typ: ActionType::Attack,
                index: idx,
            });
        }
    }

    actions
}

fn get_playable_cards(player: &Player, cards: &[CardState; TOTAL_CARDS], total_pitch: u8, is_playable: fn(CardType) -> bool) -> Vec<Action> {
    let catalog = get_card_catalog();
    let mut actions: Vec<Action> = Vec::new();

    let mut seen: Vec<Card> = Vec::new();
    for (idx, cardstate) in player.hand_iter(cards) {
        let card = cardstate.card;
        let data = &catalog[card as usize];

        // Only cards playable in the current phase
        if !is_playable(data.typ) {
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
        let other_pitch = total_pitch - data.pitch;
        if other_pitch >= needed {
            actions.push(Action {
                typ: ActionType::PlayCard,
                index: idx,
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

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
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
                .map(|a| gs.cards[a.index].card)
                .collect();
        assert_eq!(playable, HashSet::from([
            Card::MuscleMuttY,
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));

        // Every play is sourced from the Hand.
        for a in &plays {
            assert_eq!(gs.cards[a.index].location, CardLocation::P1Hand);
        }
    }

    #[test]
    fn legal_actions_in_pitch_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Enter the pitch phase directly to isolate its legal-action generator.
        gs.phase = Phase::Pitch;

        let actions = legal_actions(&gs);

        // Rhinar's seed-42 opening hand is three yellow attack actions plus a
        // blue defense reaction — all pitch for more than 0, so every card in
        // hand is offered as a Pitch action sourced from the Hand.
        for a in &actions {
            assert_eq!(a.typ, ActionType::Pitch);
            assert_eq!(gs.cards[a.index].location, CardLocation::P1Hand);
        }
        let pitchable: HashSet<Card> = actions.iter()
                .map(|a| gs.cards[a.index].card)
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

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Play Muscle Mutt: it becomes the pending card and we enter the Pitch
        // phase. The pending card can't pitch for itself, so it should not appear
        // among the pitch options even though it sits in the hand with a positive
        // pitch value.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, index: mm_idx});
        assert_eq!(gs.phase, Phase::Pitch);

        let actions = legal_actions(&gs);
        assert!(actions.iter().all(|a| a.index != mm_idx));

        // The rest of the opening hand is still offered.
        let pitchable: HashSet<Card> = actions.iter()
                .map(|a| gs.cards[a.index].card)
                .collect();
        assert_eq!(pitchable, HashSet::from([
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));
    }

    #[test]
    fn legal_actions_in_defend_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Enter the Defend phase directly to isolate its legal-action generator.
        // The active player (Rhinar, p1) stands in as the defender here.
        gs.phase = Phase::Defend;

        let actions = legal_actions(&gs);

        // Rhinar's seed-42 opening hand is three yellow attack actions plus a
        // blue defense reaction — none are no_block, so every card in hand is
        // offered as a Defend action sourced from the Hand.
        for a in &actions {
            assert_eq!(a.typ, ActionType::Defend);
            assert_eq!(gs.cards[a.index].location, CardLocation::P1Hand);
        }
        let blockable: HashSet<Card> = actions.iter()
                .map(|a| gs.cards[a.index].card)
                .collect();
        assert_eq!(blockable, HashSet::from([
            Card::MuscleMuttY,
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));
    }

    #[test]
    fn legal_actions_in_defend_phase_excludes_no_block() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);
        gs.phase = Phase::Defend;

        // Turn one hand card into a no_block card (Bare Fangs is a 6/0 attack that
        // cannot block normally). It must drop out of the defend options while the
        // rest of the hand is still offered.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        gs.cards[mm_idx].card = Card::BareFangsR;

        let actions = legal_actions(&gs);

        assert!(actions.iter().all(|a| a.index != mm_idx));
        let blockable: HashSet<Card> = actions.iter()
                .map(|a| gs.cards[a.index].card)
                .collect();
        assert_eq!(blockable, HashSet::from([
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));
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

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Rhinar plays Muscle Mutt (cost 3): it becomes pending and we drop into
        // the Pitch phase.
        let mm_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::MuscleMuttY)
                .map(|(idx, _)| idx)
                .expect("Muscle Mutt should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::PlayCard, index: mm_idx});
        assert_eq!(gs.phase, Phase::Pitch);

        // Pitch Clearing Bellow (pitch 3) to cover the cost. Muscle Mutt commits
        // to the stack and the game advances to the Instant phase.
        let cb_idx = gs.p1.hand_iter(&gs.cards)
                .find(|(_, cs)| cs.card == Card::ClearingBellowB)
                .map(|(idx, _)| idx)
                .expect("Clearing Bellow should be in the opening hand");
        step(&mut gs, Action{ typ: ActionType::Pitch, index: cb_idx});
        assert_eq!(gs.phase, Phase::Instant);

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

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Pass is always offered during the action phase, exactly once.
        let actions = legal_actions(&gs);
        let passes: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Pass)
                .collect();
        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].index, 0);
    }

    #[test]
    fn legal_actions_in_action_phase_activate_equipment() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        // Active player is Rhinar (p1). Only equipment with an activated
        // ability should be offered as an Activate, plus the equipped weapon
        // offered as its own Attack.
        let actions = legal_actions(&gs);

        let activations: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Activate)
                .collect();
        let attacks: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Attack)
                .collect();

        // Blossom of Spring (activated chest equipment) is the only Activate;
        // Bone Basher (weapon) is an Attack. The passive equipment (Bone Vizier,
        // Ironhide Gauntlet/Legs) is neither.
        let activatable: HashSet<Card> = activations.iter()
                .map(|a| gs.cards[a.index].card)
                .collect();
        let attackable: HashSet<Card> = attacks.iter()
                .map(|a| gs.cards[a.index].card)
                .collect();
        assert_eq!(activatable, HashSet::from([Card::BlossomOfSpring]));
        assert_eq!(attackable, HashSet::from([Card::BoneBasher]));

        // Each card sits in its expected zone: Bone Basher as the weapon,
        // Blossom of Spring as chest equipment. (Location is derived from the
        // card's slot, not carried on the action.)
        assert_eq!(gs.cards[attacks[0].index].location, CardLocation::P1Weapon);
        assert_eq!(gs.cards[activations[0].index].location, CardLocation::P1Chest);
    }

    #[test]
    fn legal_actions_in_action_phase_activate_equipment_dorinthea() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        // Choosing second flips the active player to Dorinthea (p2).
        let go_second = Action{ typ: ActionType::ChooseSecond, index : 0};
        step(&mut gs, go_second);
        assert_eq!(gs.active_player, 1);

        let actions = legal_actions(&gs);

        let activations: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Activate)
                .collect();
        let attacks: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Attack)
                .collect();

        // Activations: Gallantry Gold (arms) + Blossom of Spring (chest, a
        // Generic piece in both decks). Dawnblade (weapon) is an Attack instead.
        // The passive equipment (Ironrot Helm/Legs) is offered as neither.
        let activatable: HashSet<Card> = activations.iter()
                .map(|a| gs.cards[a.index].card)
                .collect();
        let attackable: HashSet<Card> = attacks.iter()
                .map(|a| gs.cards[a.index].card)
                .collect();
        assert_eq!(
            activatable,
            HashSet::from([Card::GallantryGold, Card::BlossomOfSpring])
        );
        assert_eq!(attackable, HashSet::from([Card::Dawnblade]));

        // Each card sits in its expected slot, derived from its CardState rather
        // than carried on the action.
        assert_eq!(gs.cards[attacks[0].index].location, CardLocation::P2Weapon);
        for a in &activations {
            let cs = gs.cards[a.index];
            match cs.card {
                Card::GallantryGold => assert_eq!(cs.location, CardLocation::P2Arms),
                Card::BlossomOfSpring => assert_eq!(cs.location, CardLocation::P2Chest),
                other => panic!("unexpected activation for {:?}", other),
            }
        }
    }

}

use crate::game_state::{Gamestate, Phase, Player, CardLocation};
use crate::action::{Action, ActionType};
use crate::cards::{Card, CardType};
use crate::classic_battles::get_card_catalog;


pub fn legal_actions(gs: &Gamestate) -> Vec<Action> {
    match gs.phase {
        Phase::ChooseFirst => {
            let mut actions = Vec::new();
            actions.push(Action{
                        typ : ActionType::ChooseFirst,
                        index: 0,
                        location: None});
            actions.push(Action{
                        typ : ActionType::ChooseSecond,
                        index: 0,
                        location: None});
            actions
        },
        Phase::Action => legal_action_phase(gs),
        Phase::Start | Phase::Pitch => Vec::new()
    }
}

fn legal_action_phase(gs: &Gamestate) -> Vec<Action> {
    let catalog = get_card_catalog();
    let mut legal_actions = Vec::new();
    let player = if gs.active_player == 0 { &gs.p1 } else { &gs.p2 };

    // Total pitch available across the whole hand. Computed once here and shared
    // by both the hand-card playability and equipment-activation affordability
    // checks, since pitching pays for either.
    let total_pitch: u8 = player.hand_iter()
        .map(|(_, cs)| catalog[cs.card as usize].pitch)
        .sum();

    legal_actions.extend(get_playable_cards(player, total_pitch));
    legal_actions.extend(get_equipment_activations(player, total_pitch));
    legal_actions
}

fn get_equipment_activations(player: &Player, total_pitch: u8) -> Vec<Action> {
    let catalog = get_card_catalog();
    let mut actions: Vec<Action> = Vec::new();

    // Worn armor pieces are only an option if they carry an activated ability
    // (e.g. Blossom of Spring, Gallantry Gold). Passive equipment such as
    // Bone Vizier or the Ironhide pieces has none.
    let armor_slots = [
        (player.head_idx, CardLocation::Head),
        (player.chest_idx, CardLocation::Chest),
        (player.arms_idx, CardLocation::Arms),
        (player.legs_idx, CardLocation::Legs),
    ];
    for (slot, location) in armor_slots {
        if let Some(idx) = slot {
            let idx = idx as usize;
            let Some(ability) = &catalog[player.cards[idx].card as usize].ability else {
                continue;
            };

            // The activation cost is set by the ability; only offer it when the
            // hand can pitch enough to cover what banked resources don't.
            let needed = ability.resource_cost().saturating_sub(player.resources);
            if total_pitch >= needed {
                actions.push(Action {
                    typ: ActionType::Activate,
                    index: idx,
                    location: Some(location),
                });
            }
        }
    }

    // Activating the equipped weapon makes a weapon attack, which costs the
    // weapon's resource cost.
    if let Some(idx) = player.weapon_idx {
        let idx = idx as usize;
        let needed = catalog[player.cards[idx].card as usize]
            .cost
            .saturating_sub(player.resources);
        if total_pitch >= needed {
            actions.push(Action {
                typ: ActionType::Activate,
                index: idx,
                location: Some(CardLocation::Weapon),
            });
        }
    }

    actions
}

fn get_playable_cards(player: &Player, total_pitch: u8) -> Vec<Action> {
    let catalog = get_card_catalog();
    let mut actions: Vec<Action> = Vec::new();

    let mut seen: Vec<Card> = Vec::new();
    for (idx, cardstate) in player.hand_iter() {
        let card = cardstate.card;
        let data = &catalog[card as usize];

        // Only playable cards
        if !is_action_phase_playable(data.typ) {
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
                location: Some(CardLocation::Hand),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset};
    use crate::fab_step::step;
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

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0, location: None};
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
                .map(|a| gs.p1.cards[a.index].card)
                .collect();
        assert_eq!(playable, HashSet::from([
            Card::MuscleMuttY,
            Card::PackCallY,
            Card::RagingOnslaughtY,
            Card::ClearingBellowB,
        ]));

        // Every play is sourced from the Hand.
        for a in &plays {
            assert_eq!(a.location, Some(CardLocation::Hand));
        }
    }

    #[test]
    fn legal_actions_in_action_phase_activate_equipment() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0, location: None};
        step(&mut gs, go_first);

        // Active player is Rhinar (p1). Only equipment with an activated
        // ability should be offered, plus the equipped weapon attack.
        let actions = legal_actions(&gs);

        let activations: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Activate)
                .collect();

        // Blossom of Spring (activated chest equipment) + Bone Basher (weapon).
        // The passive equipment (Bone Vizier, Ironhide Gauntlet/Legs) is not.
        let activatable: HashSet<Card> = activations.iter()
                .map(|a| gs.p1.cards[a.index].card)
                .collect();
        assert_eq!(activatable, HashSet::from([Card::BlossomOfSpring, Card::BoneBasher]));

        // The weapon activation is tagged with the Weapon location, and
        // Blossom of Spring (chest equipment) with the Chest location.
        for a in &activations {
            match gs.p1.cards[a.index].card {
                Card::BoneBasher => assert_eq!(a.location, Some(CardLocation::Weapon)),
                Card::BlossomOfSpring => assert_eq!(a.location, Some(CardLocation::Chest)),
                other => panic!("unexpected activation for {:?}", other),
            }
        }
    }

    #[test]
    fn legal_actions_in_action_phase_activate_equipment_dorinthea() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        // Choosing second flips the active player to Dorinthea (p2).
        let go_second = Action{ typ: ActionType::ChooseSecond, index : 0, location: None};
        step(&mut gs, go_second);
        assert_eq!(gs.active_player, 1);

        let actions = legal_actions(&gs);

        let activations: Vec<&Action> = actions.iter()
                .filter(|a| a.typ == ActionType::Activate)
                .collect();

        // Gallantry Gold (arms) + Blossom of Spring (chest, a Generic piece in
        // both decks) + Dawnblade (weapon). The passive equipment (Ironrot
        // Helm/Legs) is not offered.
        let activatable: HashSet<Card> = activations.iter()
                .map(|a| gs.p2.cards[a.index].card)
                .collect();
        assert_eq!(
            activatable,
            HashSet::from([Card::GallantryGold, Card::BlossomOfSpring, Card::Dawnblade])
        );

        // Each activation is tagged with its slot's location.
        for a in &activations {
            match gs.p2.cards[a.index].card {
                Card::Dawnblade => assert_eq!(a.location, Some(CardLocation::Weapon)),
                Card::GallantryGold => assert_eq!(a.location, Some(CardLocation::Arms)),
                Card::BlossomOfSpring => assert_eq!(a.location, Some(CardLocation::Chest)),
                other => panic!("unexpected activation for {:?}", other),
            }
        }
    }

}

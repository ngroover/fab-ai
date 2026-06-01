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
        Phase::Start => Vec::new()
    }
}

fn legal_action_phase(gs: &Gamestate) -> Vec<Action> {
    let mut legal_actions = Vec::new();
    if gs.active_player == 0 {
        legal_actions.extend(get_playable_cards(&gs.p1));
        legal_actions.extend(get_equipment_activations(&gs.p1));
    }
    else {
        legal_actions.extend(get_playable_cards(&gs.p2));
        legal_actions.extend(get_equipment_activations(&gs.p2));
    }
    legal_actions
}

fn get_equipment_activations(player: &Player) -> Vec<Action> {
    Vec::new()
}

fn get_playable_cards(player: &Player) -> Vec<Action> {
    let catalog = get_card_catalog();
    let mut actions: Vec<Action> = Vec::new();

    let total_pitch: u8 = player.hand_iter()
        .map(|(_, cs)| catalog[cs.card as usize].pitch)
        .sum();

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

        let catalog = get_card_catalog();

        let actions = legal_actions(&gs);

        let cards_in_hand : HashSet<Card> = gs.p1.hand_iter().
                map( |(_,x)| x.card ).collect();
        let cards_to_play : HashSet<Card> = actions.iter().
                map(|x| gs.p1.cards[x.index].card).collect();
    }

    #[test]
    fn legal_actions_in_action_phase_activate_equipment() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0, location: None};
        step(&mut gs, go_first);

        let catalog = get_card_catalog();

        let actions = legal_actions(&gs);

        let cards_in_hand : HashSet<Card> = gs.p1.hand_iter().
                map( |(_,x)| x.card ).collect();
        let cards_to_play : HashSet<Card> = actions.iter().
                map(|x| gs.p1.cards[x.index].card).collect();
    }

}

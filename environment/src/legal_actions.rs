use crate::game_state::{Gamestate, Phase,Player};
use crate::action::{Action,ActionType};
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
    if gs.active_player == 0 {
        get_playable_cards(&gs.p1)
    }
    else {
        get_playable_cards(&gs.p2)
    }
}

fn get_playable_cards(player: &Player) -> Vec<Action> {
    let catalog = get_card_catalog();
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset};
    use crate::fab_step::step;

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
    fn legal_actions_in_action_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0, location: None};
        step(&mut gs, go_first);

        let actions = legal_actions(&gs);

        assert_eq!(actions.len(), 7);
        assert_eq!(actions[0].typ, ActionType::PlayCard);
        assert_eq!(actions[1].typ, ActionType::PlayCard);
        assert_eq!(actions[2].typ, ActionType::PlayCard);
        assert_eq!(actions[3].typ, ActionType::PlayCard);
    }
}

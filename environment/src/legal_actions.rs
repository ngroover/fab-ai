use crate::game_state::{Gamestate, Phase};
use crate::action::Action;

pub fn legal_actions(gs: &Gamestate) -> Vec<Action> {
    let mut actions = Vec::new();
    if gs.phase == Phase::ChooseFirst {
        actions.push(Action::ChooseFirst);
        actions.push(Action::ChooseSecond);
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset};

    #[test]
    fn legal_actions_in_choose_first_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), None);
        reset(&mut gs);

        let actions = legal_actions(&gs);

        assert_eq!(actions.len(), 2);
        assert!(actions.contains(&Action::ChooseFirst));
        assert!(actions.contains(&Action::ChooseSecond));
    }
}

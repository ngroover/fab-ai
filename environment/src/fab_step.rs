use crate::action::{Action,ActionType};
use crate::game_state::{Gamestate,Phase,Player,CardLocation,CardVisibleState};
use crate::classic_battles::get_card_catalog;

pub fn step(gs: &mut Gamestate, act: Action) {
    if gs.phase == Phase::ChooseFirst {
        // only need to switch the player if they choose go second
        if act.typ == ActionType::ChooseSecond {
            // xor to flip the player
            gs.active_player = gs.active_player ^ 1;
        }
        // begin turn logic
        gs.phase = Phase::Action;
        for player in [&mut gs.p1, &mut gs.p2] {
            draw_to_intellect(player);
        }
    }
}

fn draw_to_intellect(player: &mut Player) {
    let need = (player.intellect - player.hand_size).max(0) as usize;
    draw_cards(player, need);
}

fn draw_cards(player: &mut Player, num: usize) {
    println!("drawing {} cards", num);
    let catalog = get_card_catalog();
    if let Some(mut current_idx) = player.top_deck_idx.map(|x| x as usize) {
        let mut drawn = 0;
        loop {
            println!("current_idx is {}", current_idx);
            let mycard = player.cards[current_idx].card;
            println!("card is {} {:?}", current_idx, mycard);

            let next = player.cards[current_idx].next_card as usize;
            println!("next is {}", next);
            if next == current_idx ||
                drawn == num {
                break;
            }
            move_from_deck_to_hand(player, current_idx);

            current_idx = next;
            drawn += 1;
        }
    }
}

fn move_from_deck_to_hand(player: &mut Player, card_idx : usize) {
    let next_card_on_deck = player.cards[card_idx].next_card;
    if next_card_on_deck == card_idx as u8 {
        player.top_deck_idx = None;
    }
    else {
        player.top_deck_idx = Some(next_card_on_deck);
        player.cards[next_card_on_deck as usize].prev_card = next_card_on_deck
    }
    if let Some(hand_idx) = player.hand_idx {
        player.cards[hand_idx as usize].next_card = card_idx as u8;
        player.cards[hand_idx as usize].prev_card = hand_idx as u8;
    }
    else {
        player.hand_idx = Some(card_idx as u8);
    }

    player.cards[card_idx].location = CardLocation::Hand;
    player.cards[card_idx].visible = CardVisibleState::SelfKnows;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset};

    #[test]
    fn test_go_first_step() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        assert_eq!(gs.active_player, 0);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0};
        step(&mut gs, go_first);

        assert_eq!(gs.active_player, 0);
        assert_eq!(gs.phase, Phase::Action);
    }

    #[test]
    fn test_go_second_step() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        assert_eq!(gs.active_player, 0);

        let go_second = Action{ typ: ActionType::ChooseSecond, index : 0};
        step(&mut gs, go_second);

        assert_eq!(gs.active_player, 1);
        assert_eq!(gs.phase, Phase::Action);
    }
}

mod action;
mod cards;
mod card_effects;
mod classic_battles;
mod decks;
mod fab_game;
mod game_state;
mod legal_actions;
mod fab_step;

use decks::{build_dorinthea_deck, build_rhinar_deck};
use fab_game::{gamestate_from_decklists,reset};
use rand::rngs::SmallRng;
use legal_actions::legal_actions;
use action::{Action,ActionType};
use fab_step::step;

fn main() {
    let mut game = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    reset(&mut game);
    println!(
        "P1 (Rhinar)    — life: {}, intellect: {}",
        game.p1.life, game.p1.intellect
    );
    println!(
        "P2 (Dorinthea) — life: {}, intellect: {}",
        game.p2.life, game.p2.intellect
    );
    println!("Active player: {}", game.active_player);
    step(&mut game, Action{ typ: ActionType::ChooseFirst, card: None});

}

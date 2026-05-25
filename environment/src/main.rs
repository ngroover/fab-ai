mod action;
mod cards;
mod card_effects;
mod classic_battles;
mod decks;
mod fab_game;
mod game_state;
mod legal_actions;

use decks::{build_dorinthea_deck, build_rhinar_deck};
use fab_game::gamestate_from_decklists;
use rand::rngs::SmallRng;
use legal_actions::legal_actions;

fn main() {
    let game = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), None);
    println!(
        "P1 (Rhinar)    — life: {}, intellect: {}",
        game.p1.life, game.p1.intellect
    );
    println!(
        "P2 (Dorinthea) — life: {}, intellect: {}",
        game.p2.life, game.p2.intellect
    );
    println!("Active player: {}", game.active_player);
    let la = legal_actions(&game);
}

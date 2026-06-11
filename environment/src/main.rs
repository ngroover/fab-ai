mod action;
mod agents;
mod cards;
mod card_effects;
mod classic_battles;
mod decks;
mod fab_game;
mod game_state;
mod legal_actions;
mod fab_step;
mod simulator;

use std::time::{SystemTime, UNIX_EPOCH};

use agents::RandomAgent;
use decks::{build_dorinthea_deck, build_rhinar_deck};
use fab_game::gamestate_from_decklists;
use game_state::Phase;
use simulator::Simulator;

fn main() {
    let num_games: usize = match std::env::args().nth(1) {
        Some(arg) => match arg.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("usage: environment [num_games]");
                std::process::exit(1);
            }
        },
        None => 1,
    };

    // Seed from the clock so every execution plays different games.
    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;

    let game = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(time_seed));
    let mut sim = Simulator::new(
        game,
        RandomAgent::new(Some(time_seed.wrapping_add(1))),
        RandomAgent::new(Some(time_seed.wrapping_add(2))),
    );

    let mut rhinar_wins = 0usize;
    let mut dorinthea_wins = 0usize;
    let mut draws = 0usize;

    for _ in 0..num_games {
        sim.run(false);
        match sim.game.phase {
            Phase::Player1Win => rhinar_wins += 1,
            Phase::Player2Win => dorinthea_wins += 1,
            Phase::Draw => draws += 1,
            _ => panic!("game did not finish"),
        }
    }

    println!("Games played:    {}", num_games);
    println!("Rhinar wins:     {}", rhinar_wins);
    println!("Dorinthea wins:  {}", dorinthea_wins);
    println!("Draws:           {}", draws);
}

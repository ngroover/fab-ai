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

use agents::RandomAgent;
use decks::{build_dorinthea_deck, build_rhinar_deck};
use fab_game::gamestate_from_decklists;
use game_state::Phase;
use simulator::Simulator;

fn main() {
    // Seeded throughout so a run is reproducible; pass None for a fresh game.
    let game = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
    let mut sim = Simulator::new(game, RandomAgent::new(Some(1)), RandomAgent::new(Some(2)));

    let steps = sim.run(true);

    if let Some(log) = &sim.game.log {
        println!("Game log:");
        for entry in log {
            println!("  {}", entry);
        }
    }

    let result = match sim.game.phase {
        Phase::Player1Win => "P1 (Rhinar) wins",
        Phase::Player2Win => "P2 (Dorinthea) wins",
        Phase::Draw => "draw",
        _ => "game did not finish",
    };
    println!();
    println!(
        "Result: {} after {} turns ({} agent decisions)",
        result, sim.game.turn_count, steps
    );
}

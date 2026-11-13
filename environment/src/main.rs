mod action;
mod action_encoding;
mod arena;
mod ring_buffer;
mod agents;
mod card_counts;
mod cards;
mod card_effects;
mod classic_battles;
mod cli;
mod deck_cards;
mod determinize;
mod decks;
mod display;
mod fab_game;
mod game_state;
mod human_agent;
mod ismcts;
mod legal_actions;
mod network;
mod observation;
mod puct;
mod self_play;
mod fab_step;
mod simulator;
#[cfg(test)]
mod test_support;

use std::time::{SystemTime, UNIX_EPOCH};

use agents::RandomAgent;
use cli::Seat;
use ismcts::IsmctsAgent;
use puct::{PuctAgent, RolloutEvaluator};
use decks::{build_dorinthea_deck, build_rhinar_deck};
use fab_game::FabGame;
use game_state::Phase;
use simulator::Simulator;

fn main() {
    // Seed from the clock so every execution plays different games.
    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;

    let args: Vec<String> = std::env::args().skip(1).collect();
    // `environment play`: an interactive game in the terminal.
    if args.first().map(String::as_str) == Some("play") {
        cli::play(&args[1..], time_seed);
        return;
    }

    // `environment match`: a match between two agents, heroes swapped.
    if args.first().map(String::as_str) == Some("match") {
        arena::main(&args[1..], time_seed);
        return;
    }

    // `environment selfplay`: play self-play games and record them.
    if args.first().map(String::as_str) == Some("selfplay") {
        self_play::main(&args[1..], time_seed);
        return;
    }

    // `environment [num_games] [--p1 AGENT] [--p2 AGENT] [--iterations N]`:
    // play games between two agents and count the results.
    let usage = || -> ! {
        eprintln!("usage: environment [num_games] [--p1 AGENT] [--p2 AGENT] [--iterations N] [--network FILE]");
        eprintln!("       AGENT is random, ismcts, puct (the search with random playouts) or net (the search with --network)");
        eprintln!("       environment play [--seed N] [--reveal] [--iterations N]");
        eprintln!("       environment match --a AGENT --b AGENT [--games N] [--iterations N] [--threads N] [--seed N]");
        eprintln!("       environment selfplay --out FILE [--games N] [--iterations N] [--threads N] [--seed N] [--network FILE]");
        std::process::exit(1);
    };
    let mut num_games = 1usize;
    let mut agent_names = ["random".to_string(), "random".to_string()];
    let mut iterations = cli::DEFAULT_ISMCTS_ITERATIONS;
    let mut network: Option<std::sync::Arc<network::Network>> = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--p1" | "--p2" => match rest.next() {
                Some(name) => agent_names[(arg == "--p2") as usize] = name.clone(),
                None => usage(),
            },
            "--iterations" => match rest.next().and_then(|s| s.parse().ok()) {
                Some(n) => iterations = n,
                None => usage(),
            },
            "--network" => match rest.next() {
                Some(path) => network = Some(network::load_or_exit(path)),
                None => usage(),
            },
            n => match n.parse() {
                Ok(n) => num_games = n,
                Err(_) => usage(),
            },
        }
    }
    let seat = |player: usize| {
        let seed = Some(time_seed.wrapping_add(1 + player as u64));
        match agent_names[player].as_str() {
            "random" => Seat::Random(RandomAgent::new(seed)),
            "ismcts" => Seat::Ismcts(IsmctsAgent::new(iterations, seed)),
            // No network yet: uniform priors, and a random playout for a value.
            "puct" => Seat::Puct(PuctAgent::new(RolloutEvaluator::new(seed), iterations, seed)),
            "net" => match &network {
                Some(network) => Seat::Net(PuctAgent::new(network::evaluator_or_exit(network), iterations, seed)),
                None => {
                    eprintln!("the net agent needs --network FILE");
                    usage()
                }
            },
            _ => usage(),
        }
    };

    let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(time_seed));
    let mut sim = Simulator::new(game, seat(0), seat(1));

    let mut rhinar_wins = 0usize;
    let mut dorinthea_wins = 0usize;
    let mut draws = 0usize;

    for _ in 0..num_games {
        sim.run(false);
        match sim.game.state.phase {
            Phase::Player1Win => rhinar_wins += 1,
            Phase::Player2Win => dorinthea_wins += 1,
            Phase::Draw => draws += 1,
            _ => panic!("game did not finish"),
        }
    }

    println!("Games played:    {}", num_games);
    println!("Rhinar wins:     {}  (P1, {})", rhinar_wins, agent_names[0]);
    println!("Dorinthea wins:  {}  (P2, {})", dorinthea_wins, agent_names[1]);
    println!("Draws:           {}", draws);
}

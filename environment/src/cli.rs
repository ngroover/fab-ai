//! The interactive play mode (`environment play`): numbered terminal menus to
//! pick each player's hero and agent, then a game played in the terminal.

use std::io::{Stdout, Write};

use crate::action::Action;
use crate::agents::{Agent, RandomAgent};
use crate::cards::Card;
use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
use crate::display::{card_name, player_name};
use crate::fab_game::FabGame;
use crate::game_state::{Phase, PlayerIndex};
use crate::human_agent::{game_log, menu, HumanAgent, StdinLines};
use crate::ismcts::IsmctsAgent;
use crate::simulator::Simulator;

/// The agent in one seat, chosen at run time.
pub enum Seat {
    Human(HumanAgent<StdinLines, Stdout>),
    Random(RandomAgent),
    Ismcts(IsmctsAgent),
}

/// How many search iterations an ISMCTS seat runs per decision unless told
/// otherwise (`--iterations`).
pub const DEFAULT_ISMCTS_ITERATIONS: u32 = 2000;

impl Agent<FabGame> for Seat {
    fn select_action(&mut self, game: &FabGame, legal_actions: &[Action]) -> Action {
        match self {
            Seat::Human(agent) => agent.select_action(game, legal_actions),
            Seat::Random(agent) => agent.select_action(game, legal_actions),
            Seat::Ismcts(agent) => agent.select_action(game, legal_actions),
        }
    }
}

/// Show a setup menu on the terminal; quits if the input ends.
fn ask(title: &str, options: &[&str]) -> usize {
    menu(&mut StdinLines, &mut std::io::stdout(), title, options).unwrap_or_else(|| {
        println!("\nInput closed - quitting.");
        std::process::exit(0);
    })
}

fn choose_seat(pid: PlayerIndex, seed: u64, reveal: bool, iterations: u32) -> ([Card; 46], Seat) {
    let name = player_name(pid);
    let deck = match ask(&format!("{}: choose a hero", name), &["Rhinar", "Dorinthea"]) {
        0 => build_rhinar_deck(),
        _ => build_dorinthea_deck(),
    };
    let seat = match ask(&format!("{}: choose an agent", name), &["Human (terminal input)", "Random", "ISMCTS (tree search)"]) {
        0 => Seat::Human(HumanAgent::new(StdinLines, std::io::stdout(), reveal)),
        1 => Seat::Random(RandomAgent::new(Some(seed.wrapping_add(1 + pid.index() as u64)))),
        _ => Seat::Ismcts(IsmctsAgent::new(iterations, Some(seed.wrapping_add(1 + pid.index() as u64)))),
    };
    (deck, seat)
}

/// Run the play mode. `args` are the arguments after `play`:
/// `--seed N` for a reproducible game, `--reveal` to show human players the
/// hidden information too (the opponent's hand, the omniscient log),
/// `--iterations N` for how hard an ISMCTS seat searches each decision.
pub fn play(args: &[String], default_seed: u64) {
    let mut seed = default_seed;
    let mut reveal = false;
    let mut iterations = DEFAULT_ISMCTS_ITERATIONS;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--reveal" => reveal = true,
            "--seed" => match args.next().and_then(|s| s.parse().ok()) {
                Some(s) => seed = s,
                None => usage_error("--seed needs a number"),
            },
            "--iterations" => match args.next().and_then(|s| s.parse().ok()) {
                Some(n) => iterations = n,
                None => usage_error("--iterations needs a number"),
            },
            other => usage_error(&format!("unknown option {}", other)),
        }
    }

    println!("Flesh and Blood - Blitz");
    let (p1_deck, p1_seat) = choose_seat(PlayerIndex::P1, seed, reveal, iterations);
    let (p2_deck, p2_seat) = choose_seat(PlayerIndex::P2, seed, reveal, iterations);
    println!("\nStarting game (seed {}).", seed);

    let game = FabGame::new(p1_deck, p2_deck, Some(seed));
    let mut sim = Simulator::new(game, p1_seat, p2_seat);
    sim.run(true);

    // Show how the game ended: each human sees what happened since their last
    // decision; a game with no human prints its whole log.
    let Simulator { game, p1_agent, p2_agent } = &mut sim;
    let gs = &game.state;
    println!();
    let mut any_human = false;
    for (pid, seat) in [(PlayerIndex::P1, p1_agent), (PlayerIndex::P2, p2_agent)] {
        if let Seat::Human(agent) = seat {
            if any_human {
                println!("  ({}'s view of the ending)", player_name(pid));
            }
            agent.show_new_events(gs, pid);
            any_human = true;
        }
    }
    if !any_human {
        match game_log(gs, None) {
            Some(log) => log.iter().for_each(|entry| println!("  | {}", entry)),
            None => println!("(No game log: this build has logging compiled out; rebuild without `--no-default-features`.)"),
        }
    }

    let hero = |pid: PlayerIndex| card_name(gs.player(pid).hero);
    let result = match gs.phase {
        Phase::Player1Win => format!("P1 ({}) wins!", hero(PlayerIndex::P1)),
        Phase::Player2Win => format!("P2 ({}) wins!", hero(PlayerIndex::P2)),
        Phase::Draw => "The game is a draw.".to_string(),
        _ => "The game stopped before finishing.".to_string(),
    };
    println!("\n{} Final life: P1 {} - P2 {} after {} turns.", result, gs.p1.life, gs.p2.life, gs.turn_count);
    let _ = std::io::stdout().flush();
}

fn usage_error(msg: &str) -> ! {
    eprintln!("{}\nusage: environment play [--seed N] [--reveal] [--iterations N]", msg);
    std::process::exit(1);
}

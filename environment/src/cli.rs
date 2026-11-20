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
    Puct(crate::puct::PuctAgent<crate::puct::RolloutEvaluator>),
    Net(crate::puct::PuctAgent<crate::network::NetworkEvaluator>),
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
            Seat::Puct(agent) => agent.select_action(game, legal_actions),
            Seat::Net(agent) => agent.select_action(game, legal_actions),
        }
    }
}

impl Seat {
    /// A short name for the agent in this seat, for the game log.
    fn label(&self, network_path: Option<&str>) -> String {
        match self {
            Seat::Human(_) => "human".into(),
            Seat::Random(_) => "random".into(),
            Seat::Ismcts(_) => "ismcts".into(),
            Seat::Puct(_) => "puct".into(),
            Seat::Net(_) => format!("net:{}", network_path.unwrap_or("?")),
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

fn choose_seat(
    pid: PlayerIndex,
    seed: u64,
    reveal: bool,
    iterations: u32,
    network: Option<&std::sync::Arc<crate::network::Network>>,
) -> ([Card; 46], Seat) {
    let name = player_name(pid);
    let deck = match ask(&format!("{}: choose a hero", name), &["Rhinar", "Dorinthea"]) {
        0 => build_rhinar_deck(),
        _ => build_dorinthea_deck(),
    };
    // The network is only on the menu when one was given (`--network`).
    let mut agents = vec!["Human (terminal input)", "Random", "ISMCTS (tree search)"];
    if network.is_some() {
        agents.push("Network (tree search with the trained network)");
    }
    let agent_seed = Some(seed.wrapping_add(1 + pid.index() as u64));
    let seat = match (ask(&format!("{}: choose an agent", name), &agents), network) {
        (0, _) => Seat::Human(HumanAgent::new(StdinLines, std::io::stdout(), reveal)),
        (1, _) => Seat::Random(RandomAgent::new(agent_seed)),
        (3, Some(network)) => Seat::Net(crate::puct::PuctAgent::new(
            crate::network::evaluator_or_exit(network), iterations, agent_seed)),
        _ => Seat::Ismcts(IsmctsAgent::new(iterations, agent_seed)),
    };
    (deck, seat)
}

/// Run the play mode. `args` are the arguments after `play`:
/// `--seed N` for a reproducible game, `--reveal` to show human players the
/// hidden information too (the opponent's hand, the omniscient log),
/// `--iterations N` for how hard an ISMCTS or network seat searches each
/// decision, `--network FILE` to offer the trained network in `FILE` as an
/// agent. The game is logged (see `match_log`) to `default_log_path` unless
/// `--log FILE` names another file or `--no-log` turns logging off.
pub fn play(args: &[String], default_seed: u64) {
    let mut seed = default_seed;
    let mut reveal = false;
    let mut iterations = DEFAULT_ISMCTS_ITERATIONS;
    let mut network = None;
    let mut network_path: Option<String> = None;
    // `None`: the default file; `Some(None)`: no log; `Some(Some(path))`: `--log`.
    let mut log_choice: Option<Option<String>> = None;
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
            "--network" => match args.next() {
                Some(path) => {
                    let loaded = crate::network::load_or_exit(path);
                    // Fail on a network of the wrong sizes before the menus.
                    crate::network::evaluator_or_exit(&loaded);
                    network = Some(loaded);
                    network_path = Some(path.clone());
                }
                None => usage_error("--network needs a file"),
            },
            "--log" => match args.next() {
                Some(path) => {
                    if !cfg!(feature = "logging") {
                        usage_error("--log needs the game log, which this build compiled out; \
                            rebuild without `--no-default-features`");
                    }
                    log_choice = Some(Some(path.clone()));
                }
                None => usage_error("--log needs a file"),
            },
            "--no-log" => log_choice = Some(None),
            other => usage_error(&format!("unknown option {}", other)),
        }
    }

    // Open the log before the menus: a `--log` file that cannot be written
    // stops here, while the default one only warns and plays on unlogged.
    let log_file = match log_choice {
        Some(None) => None,
        Some(Some(path)) => match std::fs::File::create(&path) {
            Ok(file) => Some((path, file)),
            Err(error) => usage_error(&format!("cannot write the game log {path}: {error}")),
        },
        // A build without the game log plays unlogged unless asked to log.
        None if !cfg!(feature = "logging") => None,
        None => {
            let path = default_log_path(seed, default_seed);
            let opened = std::path::Path::new(&path)
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::File::create(&path));
            match opened {
                Ok(file) => Some((path, file)),
                Err(error) => {
                    eprintln!("cannot write the game log {path}: {error}; playing without a log");
                    None
                }
            }
        }
    };

    println!("Flesh and Blood - Blitz");
    if let Some((path, _)) = &log_file {
        println!("Logging this game to {path} (--no-log to turn off).");
    }
    let (p1_deck, p1_seat) = choose_seat(PlayerIndex::P1, seed, reveal, iterations, network.as_ref());
    let (p2_deck, p2_seat) = choose_seat(PlayerIndex::P2, seed, reveal, iterations, network.as_ref());
    println!("\nStarting game (seed {}).", seed);

    let game = FabGame::new(p1_deck, p2_deck, Some(seed));
    let mut sim = Simulator::new(game, p1_seat, p2_seat);
    match log_file {
        Some((path, file)) => {
            let labels = [sim.p1_agent.label(network_path.as_deref()), sim.p2_agent.label(network_path.as_deref())];
            let header = format!("Play-mode game (seed {seed}, {iterations} iterations per search)");
            let Simulator { game, p1_agent, p2_agent } = &mut sim;
            // Written as the game goes, so quitting part way keeps the log so far.
            let mut out = std::io::LineWriter::new(file);
            // With a network, it reviews each human decision in the log (not
            // on screen): its own agent, searching as hard as a network seat.
            let any_human = matches!(p1_agent, Seat::Human(_)) || matches!(p2_agent, Seat::Human(_));
            let mut advisor = network.as_ref().filter(|_| any_human).map(|network| {
                crate::puct::PuctAgent::new(
                    crate::network::evaluator_or_exit(network), iterations, Some(seed ^ 0xAD71_5E5F_00D5_EED5))
            });
            crate::match_log::play_logged(
                game, [p1_agent, p2_agent], [labels[0].as_str(), labels[1].as_str()], &header, &mut out,
                advisor.as_mut());
            match out.flush() {
                Ok(()) => println!("\nGame log written to {path}."),
                Err(error) => eprintln!("\ncannot write the game log {path}: {error}"),
            }
        }
        None => {
            sim.run(true);
        }
    }

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

/// Where a play-mode game is logged by default:
/// `logs/play_YYYYMMDD_HHMMSS_seedN.log`, the time in UTC. `clock_nanos` is
/// the time the run started, in nanoseconds since the Unix epoch.
fn default_log_path(seed: u64, clock_nanos: u64) -> String {
    let secs = clock_nanos / 1_000_000_000;
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let time = secs % 86_400;
    format!("logs/play_{year:04}{month:02}{day:02}_{:02}{:02}{:02}_seed{seed}.log",
        time / 3600, time / 60 % 60, time % 60)
}

/// The calendar date `days` days after 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (era * 400 + yoe + (month <= 2) as i64, month, day)
}

fn usage_error(msg: &str) -> ! {
    eprintln!("{}\nusage: environment play [--seed N] [--reveal] [--iterations N] [--network FILE] [--log FILE | --no-log]", msg);
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_log_is_named_by_utc_time_and_seed() {
        assert_eq!(default_log_path(7, 0), "logs/play_19700101_000000_seed7.log");
        // 2026-11-26 03:30:12 UTC.
        assert_eq!(default_log_path(42, 1_795_663_812_123_456_789), "logs/play_20261126_033012_seed42.log");
        // A leap day.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }
}

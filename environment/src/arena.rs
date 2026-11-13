//! Matches between two agents, for measuring which plays better
//! (`environment match`).
//!
//! The two heroes are far from evenly matched, so a fair comparison gives
//! each agent both: agent A plays Rhinar in the even-numbered games and
//! Dorinthea in the odd ones, and its score is its wins over all of them.
//! The agents play to win — no root noise, no sampled moves — and each game
//! is seeded from its number, so a match's result depends on its seed and
//! not on how many threads played it.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::agents::RandomAgent;
use crate::cli::Seat;
use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
use crate::fab_game::FabGame;
use crate::game_state::Phase;
use crate::ismcts::IsmctsAgent;
use crate::network::{evaluator_or_exit, Network, NetworkEvaluator};
use crate::puct::{PuctAgent, RolloutEvaluator};
use crate::simulator::Simulator;

/// Which agent to seat, as named on the command line.
#[derive(Clone)]
pub enum AgentSpec {
    /// `random`: a uniformly random legal action.
    Random,
    /// `ismcts`: information-set MCTS with random playouts.
    Ismcts,
    /// `puct`: the PUCT search with random playouts for values.
    Puct,
    /// `net:FILE`: the PUCT search with the network in `FILE`.
    Net(Arc<Network>),
}

impl AgentSpec {
    /// Read an agent's name. A `net:FILE` loads its network here, so a bad
    /// file fails before any game is played.
    pub fn parse(name: &str) -> Result<AgentSpec, String> {
        match name {
            "random" => Ok(AgentSpec::Random),
            "ismcts" => Ok(AgentSpec::Ismcts),
            "puct" => Ok(AgentSpec::Puct),
            _ => match name.strip_prefix("net:") {
                Some(path) => {
                    let network = Network::load(path)
                        .map_err(|error| format!("cannot load the network {path}: {error}"))?;
                    let network = Arc::new(network);
                    NetworkEvaluator::new(network.clone())
                        .map_err(|error| format!("cannot use the network {path}: {error}"))?;
                    Ok(AgentSpec::Net(network))
                }
                None => Err(format!("unknown agent {name:?}: expected random, ismcts, puct or net:FILE")),
            },
        }
    }

    /// An agent of this kind for one game.
    fn seat(&self, iterations: u32, seed: u64) -> Seat {
        let seed = Some(seed);
        match self {
            AgentSpec::Random => Seat::Random(RandomAgent::new(seed)),
            AgentSpec::Ismcts => Seat::Ismcts(IsmctsAgent::new(iterations, seed)),
            AgentSpec::Puct => Seat::Puct(PuctAgent::new(RolloutEvaluator::new(seed), iterations, seed)),
            AgentSpec::Net(network) => Seat::Net(PuctAgent::new(evaluator_or_exit(network), iterations, seed)),
        }
    }
}

/// The score of a match, from agent A's side.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MatchResult {
    /// Games A played as each hero: `[Rhinar, Dorinthea]`.
    pub a_games: [u32; 2],
    /// Games A won as each hero.
    pub a_wins: [u32; 2],
    /// Games B won as each hero — against A as the other one.
    pub b_wins: [u32; 2],
    pub draws: u32,
}

impl MatchResult {
    pub fn games(&self) -> u32 {
        self.a_games.iter().sum()
    }

    /// A's score as a share of the games, a draw counting half a win.
    pub fn a_score(&self) -> f64 {
        let wins: u32 = self.a_wins.iter().sum();
        (wins as f64 + 0.5 * self.draws as f64) / self.games().max(1) as f64
    }

    fn add(&mut self, other: &MatchResult) {
        for hero in 0..2 {
            self.a_games[hero] += other.a_games[hero];
            self.a_wins[hero] += other.a_wins[hero];
            self.b_wins[hero] += other.b_wins[hero];
        }
        self.draws += other.draws;
    }
}

/// Play game `index` of a match and return its one-game result. A plays
/// Rhinar (seat 0) in even games and Dorinthea (seat 1) in odd ones.
fn play_game(a: &AgentSpec, b: &AgentSpec, iterations: u32, seed: u64, index: u32) -> MatchResult {
    let game_seed = seed.wrapping_add(index as u64);
    let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(game_seed));
    let a_seat = a.seat(iterations, game_seed ^ 0x9E37_79B9_7F4A_7C15);
    let b_seat = b.seat(iterations, game_seed ^ 0xC2B2_AE3D_27D4_EB4F);
    let a_hero = (index % 2) as usize;
    let phase = if a_hero == 0 {
        let mut sim = Simulator::new(game, a_seat, b_seat);
        sim.run(false);
        sim.game.state.phase
    } else {
        let mut sim = Simulator::new(game, b_seat, a_seat);
        sim.run(false);
        sim.game.state.phase
    };

    let mut result = MatchResult::default();
    result.a_games[a_hero] = 1;
    let winner = match phase {
        Phase::Player1Win => Some(0),
        Phase::Player2Win => Some(1),
        _ => None,
    };
    match winner {
        Some(hero) if hero == a_hero => result.a_wins[hero] = 1,
        Some(hero) => result.b_wins[hero] = 1,
        None => result.draws = 1,
    }
    result
}

/// Play `games` games between `a` and `b` on `threads` threads, both
/// searching `iterations` iterations per decision, and return the score.
pub fn play_match(
    a: &AgentSpec,
    b: &AgentSpec,
    games: u32,
    iterations: u32,
    threads: usize,
    seed: u64,
) -> MatchResult {
    let next_game = AtomicU32::new(0);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.max(1))
            .map(|_| {
                scope.spawn(|| {
                    let mut result = MatchResult::default();
                    loop {
                        let index = next_game.fetch_add(1, Ordering::Relaxed);
                        if index >= games {
                            break result;
                        }
                        result.add(&play_game(a, b, iterations, seed, index));
                    }
                })
            })
            .collect();
        let mut total = MatchResult::default();
        for worker in workers {
            total.add(&worker.join().expect("a match thread panicked"));
        }
        total
    })
}

/// `environment match --a AGENT --b AGENT [--games N] [--iterations N]
/// [--threads N] [--seed N]`: play a match and print the score. The last line
/// printed, starting `RESULT`, is the score as `key=value` pairs for scripts.
pub fn main(args: &[String], time_seed: u64) {
    let usage = || -> ! {
        eprintln!("usage: environment match --a AGENT --b AGENT [--games N] [--iterations N] \
            [--threads N] [--seed N]");
        eprintln!("       AGENT is random, ismcts, puct or net:FILE; each plays both heroes equally often");
        std::process::exit(1);
    };
    let mut names: [Option<&str>; 2] = [None, None];
    let mut games = 200u32;
    let mut iterations = 400u32;
    let mut threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut seed = time_seed;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let Some(value) = rest.next() else { usage() };
        let parsed = match arg.as_str() {
            "--a" => {
                names[0] = Some(value);
                true
            }
            "--b" => {
                names[1] = Some(value);
                true
            }
            "--games" => value.parse().map(|n| games = n).is_ok(),
            "--iterations" => value.parse().map(|n| iterations = n).is_ok(),
            "--threads" => value.parse().map(|n| threads = n).is_ok(),
            "--seed" => value.parse().map(|n| seed = n).is_ok(),
            _ => false,
        };
        if !parsed {
            usage();
        }
    }
    let (Some(a_name), Some(b_name)) = (names[0], names[1]) else { usage() };
    let spec = |name: &str| {
        AgentSpec::parse(name).unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        })
    };
    let (a, b) = (spec(a_name), spec(b_name));

    let start = Instant::now();
    let r = play_match(&a, &b, games, iterations, threads, seed);
    let seconds = start.elapsed().as_secs_f64();
    let (a_total, b_total): (u32, u32) = (r.a_wins.iter().sum(), r.b_wins.iter().sum());
    println!("Games played:  {}  ({iterations} iterations, seed {seed}, {threads} threads, {seconds:.1}s)", r.games());
    println!("A ({a_name}) wins:  {a_total}  as Rhinar {}/{}, as Dorinthea {}/{}",
        r.a_wins[0], r.a_games[0], r.a_wins[1], r.a_games[1]);
    println!("B ({b_name}) wins:  {b_total}  as Rhinar {}/{}, as Dorinthea {}/{}",
        r.b_wins[0], r.a_games[1], r.b_wins[1], r.a_games[0]);
    println!("Draws:         {}", r.draws);
    println!("A's score:     {:.1}%", 100.0 * r.a_score());
    println!("RESULT games={} a_wins={a_total} b_wins={b_total} draws={} a_score={:.4} \
        a_rhinar_wins={} a_rhinar_games={} a_dorinthea_wins={} a_dorinthea_games={}",
        r.games(), r.draws, r.a_score(), r.a_wins[0], r.a_games[0], r.a_wins[1], r.a_games[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_names_are_parsed() {
        assert!(matches!(AgentSpec::parse("random"), Ok(AgentSpec::Random)));
        assert!(matches!(AgentSpec::parse("ismcts"), Ok(AgentSpec::Ismcts)));
        assert!(matches!(AgentSpec::parse("puct"), Ok(AgentSpec::Puct)));
        assert!(AgentSpec::parse("net").is_err());
        let missing = AgentSpec::parse("net:/no/such/file.bin").err().expect("a missing file fails");
        assert!(missing.contains("cannot load"));
    }

    /// Every game is counted once, each agent gets each hero half the time,
    /// and the result does not depend on the number of threads.
    #[test]
    fn a_match_accounts_for_every_game() {
        let (a, b) = (AgentSpec::Random, AgentSpec::Random);
        let result = play_match(&a, &b, 30, 1, 3, 7);
        assert_eq!(result.a_games, [15, 15]);
        let decided: u32 = result.a_wins.iter().chain(&result.b_wins).sum();
        assert_eq!(decided + result.draws, 30);
        // B's wins as a hero come out of the games A played the other one.
        assert!(result.a_wins[0] + result.b_wins[1] <= 15);
        assert!(result.a_wins[1] + result.b_wins[0] <= 15);
        assert_eq!(play_match(&a, &b, 30, 1, 1, 7), result);
        assert_ne!(play_match(&a, &b, 30, 1, 3, 8), result);
    }

    /// The score tells a strong agent from a weak one whichever is "A", and
    /// a network agent takes its seat.
    #[test]
    fn a_search_outscores_random_play_from_either_side() {
        let search = AgentSpec::Puct;
        let forward = play_match(&search, &AgentSpec::Random, 8, 40, 4, 3);
        let backward = play_match(&AgentSpec::Random, &search, 8, 40, 4, 3);
        assert!(forward.a_score() >= 0.75, "{forward:?}");
        assert!(backward.a_score() <= 0.25, "{backward:?}");

        let net = AgentSpec::Net(crate::network::tests::random_network(1));
        assert_eq!(play_match(&net, &AgentSpec::Random, 4, 10, 2, 3).games(), 4);
    }

    #[test]
    fn a_draw_counts_half() {
        let result = MatchResult { a_games: [2, 2], a_wins: [1, 0], b_wins: [1, 0], draws: 2 };
        assert_eq!(result.games(), 4);
        assert_eq!(result.a_score(), 0.5);
    }
}

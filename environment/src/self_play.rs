//! Self-play: the PUCT search plays both sides of whole games and every
//! decision becomes a training record for the policy/value network.
//!
//! A record holds what the network will be shown and what it should answer:
//! the mover's observation, the share of the search's visits each legal
//! action got (the policy target) and, filled in once the game is over, how
//! the game ended for the mover (the value target). Only real decisions are
//! recorded; `FabGame` plays forced ones itself.
//!
//! So that games differ and explore, the search runs with root noise
//! (`PuctAgent::with_root_noise`), and for the first `sampled_moves`
//! decisions of a game the move is drawn in proportion to the visits rather
//! than being the most visited.
//!
//! # File format
//!
//! Everything is little-endian. The file starts with a 16-byte header:
//!
//! ```text
//! [u8; 8]  magic "FABSP001"
//! u32      OBS_SIZE     length of an observation vector
//! u32      ACTION_SIZE  length of a policy vector
//! ```
//!
//! and is followed by records, back to back, a game's records together and in
//! the order they were played:
//!
//! ```text
//! u32  game         index of the game
//! u16  ply          index of the decision within the game
//! u8   player       the mover's seat, 0 or 1
//! u8   legal        number of legal actions, L
//! f32  outcome      the game's result for the mover: 1 win, -1 loss, 0 draw
//! f32  root_value   the search's own estimate of that, from -1 to 1
//! u16  nonzero      number of non-zero observation entries, N
//! [u16; N]  their indices into the observation vector, ascending
//! [f32; N]  their values
//! [u16; L]  the legal actions' indices into the policy vector
//! [u32; L]  the visits each got
//! ```
//!
//! The observation is stored sparsely because nearly all of it is zero. The
//! legal-action mask is the `L` indices, and the policy target is the visits
//! divided by their sum.

use std::io::{self, BufWriter, Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

use crate::action_encoding::{action_index, ACTION_SIZE};
use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
use crate::fab_game::FabGame;
use crate::game_state::Phase;
use crate::observation::{encode_into, OBS_SIZE};
use crate::puct::{outcome_for, Evaluator, PuctAgent, RolloutEvaluator, RootStat};
use crate::simulator::Game;

const MAGIC: [u8; 8] = *b"FABSP001";

/// One decision of a self-play game. See the module documentation for what
/// each field means.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub game: u32,
    pub ply: u16,
    pub player: u8,
    pub outcome: f32,
    pub root_value: f32,
    /// The non-zero observation entries: `(index, value)`, ascending.
    pub observation: Vec<(u16, f32)>,
    /// Each legal action's policy index and the visits it got.
    pub visits: Vec<(u16, u32)>,
}

/// How self-play games are played.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Search iterations per decision.
    pub iterations: u32,
    /// How many of a game's first decisions (both players' together) pick a
    /// move in proportion to its visits; later ones take the most visited.
    pub sampled_moves: u32,
    /// Dirichlet concentration of the root noise.
    pub noise_alpha: f32,
    /// Share of each root prior that is noise; 0 turns the noise off.
    pub noise_weight: f32,
}

impl Default for Config {
    fn default() -> Self {
        // AlphaZero's noise weight; its alpha is about 10 over the typical
        // number of legal moves, which here is a handful. Neither is tuned.
        Config { iterations: 400, sampled_moves: 20, noise_alpha: 1.0, noise_weight: 0.25 }
    }
}

impl Config {
    /// A search agent set up as this configuration says.
    pub fn agent<E: Evaluator>(&self, evaluator: E, seed: u64) -> PuctAgent<E> {
        let agent = PuctAgent::new(evaluator, self.iterations, Some(seed));
        if self.noise_weight > 0.0 {
            agent.with_root_noise(self.noise_alpha, self.noise_weight)
        } else {
            agent
        }
    }
}

/// Play one self-play game with `agent` searching for both players: reset
/// `game`, play it to the end and return a record for every decision, with
/// the game's result filled in. `rng` draws the sampled moves.
pub fn play_game<E: Evaluator>(
    game: &mut FabGame,
    agent: &mut PuctAgent<E>,
    rng: &mut SmallRng,
    config: &Config,
    game_index: u32,
) -> Vec<Record> {
    game.reset(false);
    let mut records: Vec<Record> = Vec::new();
    let mut observation = [0.0f32; OBS_SIZE];
    while !game.is_over() {
        let stats = agent.search(game);
        let total: u32 = stats.iter().map(|s| s.visits).sum();
        assert!(total > 0, "self-play needs at least two search iterations per decision");

        let mover = game.state.active_player;
        let player = game.state.player(mover);
        encode_into(&game.state, mover, &mut observation);
        records.push(Record {
            game: game_index,
            ply: records.len() as u16,
            player: mover.get(),
            outcome: 0.0,
            root_value: stats.iter().map(|s| s.visits as f32 * s.value).sum::<f32>() / total as f32,
            observation: observation.iter().enumerate()
                .filter(|&(_, &x)| x != 0.0)
                .map(|(i, &x)| (i as u16, x))
                .collect(),
            visits: stats.iter().map(|s| (action_index(player, s.action) as u16, s.visits)).collect(),
        });

        let sampled = (records.len() as u32) <= config.sampled_moves;
        game.step(choose(&stats, total, sampled, rng).action);
    }
    for record in &mut records {
        record.outcome = outcome_for(game.state.phase, record.player as usize);
    }
    records
}

/// The root action to play: drawn in proportion to the visits when
/// `sampled`, the most visited otherwise.
fn choose<'a>(stats: &'a [RootStat], total: u32, sampled: bool, rng: &mut SmallRng) -> &'a RootStat {
    if sampled {
        let mut pick = rng.random_range(0..total);
        for stat in stats {
            if pick < stat.visits {
                return stat;
            }
            pick -= stat.visits;
        }
    }
    stats.iter().max_by_key(|s| s.visits).expect("a decision has legal actions")
}

pub fn write_header(w: &mut impl Write) -> io::Result<()> {
    w.write_all(&MAGIC)?;
    w.write_all(&(OBS_SIZE as u32).to_le_bytes())?;
    w.write_all(&(ACTION_SIZE as u32).to_le_bytes())
}

pub fn write_record(w: &mut impl Write, record: &Record) -> io::Result<()> {
    w.write_all(&record.game.to_le_bytes())?;
    w.write_all(&record.ply.to_le_bytes())?;
    w.write_all(&[record.player, record.visits.len() as u8])?;
    w.write_all(&record.outcome.to_le_bytes())?;
    w.write_all(&record.root_value.to_le_bytes())?;
    w.write_all(&(record.observation.len() as u16).to_le_bytes())?;
    for (index, _) in &record.observation {
        w.write_all(&index.to_le_bytes())?;
    }
    for (_, value) in &record.observation {
        w.write_all(&value.to_le_bytes())?;
    }
    for (index, _) in &record.visits {
        w.write_all(&index.to_le_bytes())?;
    }
    for (_, visits) in &record.visits {
        w.write_all(&visits.to_le_bytes())?;
    }
    Ok(())
}

/// Read a whole self-play file back. Fails if the header is not this
/// build's: another format, or other observation and policy sizes.
#[allow(dead_code)] // the format's reference reader; exercised by the tests
pub fn read_records(r: &mut impl Read) -> io::Result<Vec<Record>> {
    fn array<const N: usize>(bytes: &mut &[u8]) -> io::Result<[u8; N]> {
        let (head, rest) = bytes.split_at_checked(N)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "a record is cut short"))?;
        *bytes = rest;
        Ok(head.try_into().expect("split at N"))
    }
    let u16_at = |b: &mut &[u8]| array(b).map(u16::from_le_bytes);
    let u32_at = |b: &mut &[u8]| array(b).map(u32::from_le_bytes);
    let f32_at = |b: &mut &[u8]| array(b).map(f32::from_le_bytes);

    let mut bytes = Vec::new();
    r.read_to_end(&mut bytes)?;
    let mut b = &bytes[..];
    let header = (array::<8>(&mut b)?, u32_at(&mut b)?, u32_at(&mut b)?);
    if header != (MAGIC, OBS_SIZE as u32, ACTION_SIZE as u32) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a self-play file of this build"));
    }

    let mut records = Vec::new();
    while !b.is_empty() {
        let game = u32_at(&mut b)?;
        let ply = u16_at(&mut b)?;
        let [player, legal] = array::<2>(&mut b)?;
        let outcome = f32_at(&mut b)?;
        let root_value = f32_at(&mut b)?;
        let nonzero = u16_at(&mut b)? as usize;
        let indices = (0..nonzero).map(|_| u16_at(&mut b)).collect::<io::Result<Vec<_>>>()?;
        let values = (0..nonzero).map(|_| f32_at(&mut b)).collect::<io::Result<Vec<_>>>()?;
        let actions = (0..legal).map(|_| u16_at(&mut b)).collect::<io::Result<Vec<_>>>()?;
        let visits = (0..legal).map(|_| u32_at(&mut b)).collect::<io::Result<Vec<_>>>()?;
        records.push(Record {
            game,
            ply,
            player,
            outcome,
            root_value,
            observation: indices.into_iter().zip(values).collect(),
            visits: actions.into_iter().zip(visits).collect(),
        });
    }
    Ok(records)
}

/// What a self-play run produced.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub games: u32,
    pub records: u64,
    pub p1_wins: u32,
    pub p2_wins: u32,
    pub draws: u32,
}

/// Play `games` self-play games of Rhinar (seat 0) against Dorinthea (seat 1)
/// on `threads` threads and write their records to `out`, header first.
///
/// Game `i` is seeded from `seed + i` — its deal, its search and its sampled
/// moves — and the games are written in order, so a run's output depends only
/// on `seed`, `games` and `config`, not on how many threads played it.
/// `make_evaluator` builds the evaluator for a game from that game's seed.
pub fn run<E: Evaluator>(
    out: &mut impl Write,
    games: u32,
    threads: usize,
    seed: u64,
    config: &Config,
    make_evaluator: impl Fn(u64) -> E + Sync,
) -> io::Result<Summary> {
    write_header(out)?;
    let next_game = AtomicU32::new(0);
    let (finished, results) = mpsc::channel();
    let mut summary = Summary::default();

    std::thread::scope(|scope| -> io::Result<()> {
        for _ in 0..threads.max(1) {
            let finished = finished.clone();
            let (next_game, make_evaluator) = (&next_game, &make_evaluator);
            scope.spawn(move || loop {
                let index = next_game.fetch_add(1, Ordering::Relaxed);
                if index >= games {
                    break;
                }
                // Three streams per game, kept apart: the deal, the search
                // and the sampled moves.
                let game_seed = seed.wrapping_add(index as u64);
                let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(game_seed));
                let evaluator = make_evaluator(game_seed ^ 0x9E37_79B9_7F4A_7C15);
                let mut agent = config.agent(evaluator, game_seed ^ 0xC2B2_AE3D_27D4_EB4F);
                let mut rng = SmallRng::seed_from_u64(game_seed ^ 0x1656_67B1_9E37_79F9);
                let records = play_game(&mut game, &mut agent, &mut rng, config, index);
                // The writer hanging up means it failed; nothing left to do.
                if finished.send((index, records, game.state.phase)).is_err() {
                    break;
                }
            });
        }
        drop(finished);

        // Games finish in any order; hold the early ones back.
        let mut waiting = std::collections::BTreeMap::new();
        for (index, records, phase) in results {
            waiting.insert(index, (records, phase));
            while let Some((records, phase)) = waiting.remove(&summary.games) {
                for record in &records {
                    write_record(out, record)?;
                }
                summary.games += 1;
                summary.records += records.len() as u64;
                match phase {
                    Phase::Player1Win => summary.p1_wins += 1,
                    Phase::Player2Win => summary.p2_wins += 1,
                    _ => summary.draws += 1,
                }
            }
        }
        Ok(())
    })?;
    out.flush()?;
    Ok(summary)
}

/// `environment selfplay --out FILE [--games N] [--iterations N] [--threads N]
/// [--seed N] [--sampled-moves N] [--network FILE]`: play self-play games and
/// write their records to `FILE`. The search judges positions with the
/// network in `--network`, or with random playouts when there is none.
pub fn main(args: &[String], time_seed: u64) {
    let usage = || -> ! {
        eprintln!("usage: environment selfplay --out FILE [--games N] [--iterations N] \
            [--threads N] [--seed N] [--sampled-moves N] [--network FILE]");
        std::process::exit(1);
    };
    let mut config = Config::default();
    let mut games = 100u32;
    let mut threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut seed = time_seed;
    let mut path: Option<&str> = None;
    let mut network = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let Some(value) = rest.next() else { usage() };
        let parsed = match arg.as_str() {
            "--out" => {
                path = Some(value);
                true
            }
            "--network" => {
                network = Some(crate::network::load_or_exit(value));
                true
            }
            "--games" => value.parse().map(|n| games = n).is_ok(),
            "--iterations" => value.parse().map(|n| config.iterations = n).is_ok(),
            "--threads" => value.parse().map(|n| threads = n).is_ok(),
            "--seed" => value.parse().map(|n| seed = n).is_ok(),
            "--sampled-moves" => value.parse().map(|n| config.sampled_moves = n).is_ok(),
            _ => false,
        };
        if !parsed {
            usage();
        }
    }
    let Some(path) = path else { usage() };
    if config.iterations < 2 || threads == 0 {
        usage();
    }

    let start = Instant::now();
    // Fail on a network of the wrong sizes before playing anything.
    if let Some(network) = &network {
        crate::network::evaluator_or_exit(network);
    }
    let result = std::fs::File::create(path).and_then(|file| {
        let mut out = BufWriter::new(file);
        match &network {
            Some(network) => run(&mut out, games, threads, seed, &config, |_| {
                crate::network::evaluator_or_exit(network)
            }),
            None => run(&mut out, games, threads, seed, &config, |seed| RolloutEvaluator::new(Some(seed))),
        }
    });
    let summary = match result {
        Ok(summary) => summary,
        Err(error) => {
            eprintln!("self-play failed writing {path}: {error}");
            std::process::exit(1);
        }
    };
    let seconds = start.elapsed().as_secs_f64();
    println!("Games played:    {}  ({} iterations, seed {seed}, {threads} threads, {})",
        summary.games, config.iterations,
        if network.is_some() { "network" } else { "random playouts" });
    println!("Rhinar wins:     {}", summary.p1_wins);
    println!("Dorinthea wins:  {}", summary.p2_wins);
    println!("Draws:           {}", summary.draws);
    println!("Records:         {}  ({:.1} per game)",
        summary.records, summary.records as f64 / summary.games.max(1) as f64);
    println!("Time:            {seconds:.1}s  ({:.2}s per game)", seconds / summary.games.max(1) as f64);
    println!("Written to:      {path}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_encoding::action_at;
    use crate::game_state::PlayerIndex;
    use crate::observation::observation;

    fn quick() -> Config {
        Config { iterations: 12, sampled_moves: 10, ..Config::default() }
    }

    fn run_to_bytes(games: u32, threads: usize, seed: u64, config: &Config) -> (Vec<u8>, Summary) {
        let mut bytes = Vec::new();
        let summary = run(&mut bytes, games, threads, seed, config, |s| RolloutEvaluator::new(Some(s))).unwrap();
        (bytes, summary)
    }

    /// Replaying a game's recorded moves is not possible — the record holds
    /// the visits, not the move — so instead check every record against the
    /// game as it is played: the observation is the mover's, the actions are
    /// exactly the legal ones, and the visits are the search's.
    #[test]
    fn records_describe_the_decisions_of_the_game() {
        let config = quick();
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(5));
        let mut agent = config.agent(RolloutEvaluator::new(Some(6)), 7);
        let mut rng = SmallRng::seed_from_u64(8);
        let records = play_game(&mut game, &mut agent, &mut rng, &config, 3);
        assert!(game.is_over());
        assert!(records.len() > 20);

        // Play the same game again, step by step, beside the records.
        let mut replay = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(5));
        let mut agent = config.agent(RolloutEvaluator::new(Some(6)), 7);
        let mut rng = SmallRng::seed_from_u64(8);
        replay.reset(false);
        for (ply, record) in records.iter().enumerate() {
            assert_eq!((record.game, record.ply as usize), (3, ply));
            let mover = replay.state.active_player;
            assert_eq!(record.player, mover.get());
            assert_eq!(record.outcome, outcome_for(game.state.phase, mover.index()));
            assert!((-1.0..=1.0).contains(&record.root_value));

            let mut dense = vec![0.0; OBS_SIZE];
            for &(index, value) in &record.observation {
                assert!(value != 0.0);
                dense[index as usize] = value;
            }
            assert_eq!(dense, observation(&replay.state, mover));
            assert!(record.observation.windows(2).all(|pair| pair[0].0 < pair[1].0));

            let legal = replay.legal_actions();
            let player = replay.state.player(mover);
            let mut recorded: Vec<_> = record.visits.iter()
                .map(|&(index, _)| action_at(player, index as usize).unwrap())
                .collect();
            assert_eq!(recorded.len(), legal.len());
            recorded.retain(|action| legal.contains(action));
            assert_eq!(recorded.len(), legal.len());
            // The first iteration of a search evaluates the root.
            assert_eq!(record.visits.iter().map(|v| v.1).sum::<u32>(), config.iterations - 1);

            let stats = agent.search(&replay);
            let total = stats.iter().map(|s| s.visits).sum();
            let sampled = (ply as u32) < config.sampled_moves;
            replay.step(choose(&stats, total, sampled, &mut rng).action);
        }
        assert!(replay.is_over());
        // Both players' decisions are there, with opposite results.
        let outcomes = |seat: PlayerIndex| -> Vec<f32> {
            records.iter().filter(|r| r.player == seat.get()).map(|r| r.outcome).collect()
        };
        let (p1, p2) = (outcomes(PlayerIndex::P1), outcomes(PlayerIndex::P2));
        assert!(!p1.is_empty() && !p2.is_empty());
        assert!(p1.iter().all(|&o| o == p1[0]) && p2.iter().all(|&o| o == -p1[0]));
    }

    #[test]
    fn a_file_reads_back_as_it_was_written() {
        let (bytes, summary) = run_to_bytes(3, 2, 21, &quick());
        let records = read_records(&mut &bytes[..]).unwrap();
        assert_eq!(summary.games, 3);
        assert_eq!(summary.records, records.len() as u64);
        assert_eq!(summary.p1_wins + summary.p2_wins + summary.draws, 3);
        // Games in order, each one's decisions in order.
        let order: Vec<(u32, u16)> = records.iter().map(|r| (r.game, r.ply)).collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(records.iter().filter(|r| r.ply == 0).count(), 3);

        // Written again, the records make the same bytes.
        let mut again = Vec::new();
        write_header(&mut again).unwrap();
        for record in &records {
            write_record(&mut again, record).unwrap();
        }
        assert_eq!(again, bytes);
    }

    #[test]
    fn a_damaged_file_is_refused() {
        let (bytes, _) = run_to_bytes(1, 1, 21, &quick());
        assert!(read_records(&mut &bytes[..bytes.len() - 3]).is_err());
        let mut other = bytes.clone();
        other[8] ^= 1; // another observation size
        assert!(read_records(&mut &other[..]).is_err());
        assert!(read_records(&mut &b"FABSP00"[..]).is_err());
    }

    /// The output depends on the seed, not on the number of threads.
    #[test]
    fn a_run_is_the_same_on_any_number_of_threads() {
        let config = quick();
        let (one, summary) = run_to_bytes(6, 1, 33, &config);
        let (four, same) = run_to_bytes(6, 4, 33, &config);
        assert!(one == four);
        assert_eq!(summary, same);
        let (other, _) = run_to_bytes(6, 4, 34, &config);
        assert!(one != other);
    }

    /// Early moves are drawn in proportion to visits, so they vary from draw
    /// to draw and cover every visited action; later ones are the favourite.
    #[test]
    fn early_moves_are_sampled_and_late_ones_greedy() {
        let action = crate::action::Action::no_card(crate::action::ActionType::Pass);
        let stat = |visits| RootStat { action, visits, value: 0.0, prior: 0.0 };
        let stats = [stat(10), stat(0), stat(30)];
        let mut rng = SmallRng::seed_from_u64(2);
        let mut picked = [0u32; 3];
        for _ in 0..4000 {
            let chosen = choose(&stats, 40, true, &mut rng);
            picked[stats.iter().position(|s| std::ptr::eq(s, chosen)).unwrap()] += 1;
        }
        assert_eq!(picked[1], 0);
        assert!((900..1100).contains(&picked[0]), "{picked:?}");
        for _ in 0..50 {
            assert!(std::ptr::eq(choose(&stats, 40, false, &mut rng), &stats[2]));
        }
    }
}

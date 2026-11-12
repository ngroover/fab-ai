//! The policy/value network: loading weights trained by `train_network.py`
//! and running them as the PUCT search's `Evaluator`.
//!
//! The network is a plain MLP — the observation through a stack of ReLU
//! layers, then a linear policy head with one logit per action slot and a
//! linear value head squashed by tanh — and this is its forward pass, written
//! out by hand: the search evaluates one position at a time, thousands of
//! times per move, and at this size a few loops beat any framework call.
//!
//! Two things keep an evaluation cheap:
//!
//! - **The first layer is sparse.** An observation is nearly all zeros, so
//!   the first layer adds up only the weight columns of its non-zero entries.
//!   Hidden layers do the same with the activations a ReLU zeroed.
//! - **The policy head is computed for the legal actions only.** The search
//!   never asks about the other slots, which are most of them.
//!
//! # Weights file
//!
//! Written by `train_network.py`, which documents it too. Little-endian:
//!
//! ```text
//! [u8; 8]  magic "FABNN001"
//! u32      observation size
//! u32      action size
//! u32      number of hidden layers, H
//! [u32; H] their widths
//! then for each of the H hidden layers, the policy head and the value head:
//! [f32; out * in]  weight, row-major: weight[o][i] is at o * in + i
//! [f32; out]       bias
//! ```

use std::io::{self, Read};
use std::sync::Arc;

use crate::action::Action;
use crate::action_encoding::{action_index, ACTION_SIZE};
use crate::fab_game::FabGame;
use crate::observation::{encode_into, OBS_SIZE};
use crate::puct::Evaluator;

const MAGIC: [u8; 8] = *b"FABNN001";

/// A hidden layer. Its weight is kept input-major — `columns[i * outputs..]`
/// is everything input `i` feeds — so that the layer is a sum of the columns
/// of its non-zero inputs, each a contiguous run.
struct Hidden {
    outputs: usize,
    columns: Vec<f32>,
    bias: Vec<f32>,
}

/// A loaded network. Immutable, so one copy serves every thread; the scratch
/// space an evaluation needs lives in the `NetworkEvaluator`.
pub struct Network {
    obs_size: usize,
    action_size: usize,
    hidden: Vec<Hidden>,
    /// The policy head's weight, row-major: one row of `width` per action.
    policy_weight: Vec<f32>,
    policy_bias: Vec<f32>,
    value_weight: Vec<f32>,
    value_bias: f32,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// What is left to read of a weights file.
struct Cursor<'a> {
    rest: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let (head, tail) = self.rest.split_at_checked(n)
            .ok_or_else(|| invalid("the weights file is cut short"))?;
        self.rest = tail;
        Ok(head)
    }

    fn number(&mut self) -> io::Result<usize> {
        self.take(4).map(|b| u32::from_le_bytes(b.try_into().expect("four bytes")) as usize)
    }

    /// `rows * columns` floats.
    fn floats(&mut self, rows: usize, columns: usize) -> io::Result<Vec<f32>> {
        let bytes = rows.checked_mul(columns).and_then(|n| n.checked_mul(4))
            .ok_or_else(|| invalid("the weights file's sizes overflow"))?;
        Ok(self.take(bytes)?
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().expect("four bytes")))
            .collect())
    }
}

impl Network {
    /// Read a weights file. Fails on anything but a whole, well-formed one.
    pub fn read(r: &mut impl Read) -> io::Result<Network> {
        let mut bytes = Vec::new();
        r.read_to_end(&mut bytes)?;
        let mut file = Cursor { rest: &bytes };

        if file.take(8)? != MAGIC {
            return Err(invalid("not a network weights file"));
        }
        let obs_size = file.number()?;
        let action_size = file.number()?;
        let layers = file.number()?;
        if obs_size == 0 || action_size == 0 || layers == 0 || layers > 64 {
            return Err(invalid("the weights file's header makes no sense"));
        }
        let widths = (0..layers).map(|_| file.number()).collect::<io::Result<Vec<_>>>()?;
        if widths.contains(&0) {
            return Err(invalid("the weights file has an empty layer"));
        }

        let mut hidden = Vec::with_capacity(layers);
        let mut inputs = obs_size;
        for &outputs in &widths {
            let rows = file.floats(outputs, inputs)?;
            // The file is row-major (output by output); turn it input-major.
            let mut columns = vec![0.0; rows.len()];
            for o in 0..outputs {
                for i in 0..inputs {
                    columns[i * outputs + o] = rows[o * inputs + i];
                }
            }
            hidden.push(Hidden { outputs, columns, bias: file.floats(outputs, 1)? });
            inputs = outputs;
        }
        let policy_weight = file.floats(action_size, inputs)?;
        let policy_bias = file.floats(action_size, 1)?;
        let value_weight = file.floats(inputs, 1)?;
        let value_bias = file.floats(1, 1)?[0];
        if !file.rest.is_empty() {
            return Err(invalid("the weights file is longer than its header calls for"));
        }
        Ok(Network { obs_size, action_size, hidden, policy_weight, policy_bias, value_weight, value_bias })
    }

    /// Read the weights file at `path`.
    pub fn load(path: &str) -> io::Result<Network> {
        Network::read(&mut std::fs::File::open(path)?)
    }

    /// Length of the observation vector the network takes.
    pub fn obs_size(&self) -> usize {
        self.obs_size
    }

    /// Length of the policy vector the network answers with.
    pub fn action_size(&self) -> usize {
        self.action_size
    }

    /// Run the hidden layers on `obs`, leaving the last one's activations in
    /// `scratch.0`. `scratch` is two buffers the layers pass back and forth.
    fn trunk<'a>(&self, obs: &[f32], scratch: &'a mut (Vec<f32>, Vec<f32>)) -> &'a [f32] {
        assert_eq!(obs.len(), self.obs_size, "the observation is not the network's size");
        let (current, next) = scratch;
        current.clear();
        current.extend_from_slice(obs);
        for layer in &self.hidden {
            next.clear();
            next.extend_from_slice(&layer.bias);
            for (i, &x) in current.iter().enumerate() {
                // Zero inputs — most of an observation, and whatever a ReLU
                // cut off — add nothing.
                if x != 0.0 {
                    let column = &layer.columns[i * layer.outputs..(i + 1) * layer.outputs];
                    for (sum, &w) in next.iter_mut().zip(column) {
                        *sum += x * w;
                    }
                }
            }
            for sum in next.iter_mut() {
                *sum = sum.max(0.0);
            }
            std::mem::swap(current, next);
        }
        current
    }

    /// The policy logit of the action slot `action`, given the trunk's output.
    fn logit(&self, features: &[f32], action: usize) -> f32 {
        let width = features.len();
        let row = &self.policy_weight[action * width..(action + 1) * width];
        self.policy_bias[action] + dot(row, features)
    }

    /// The value, from -1 to 1, given the trunk's output.
    fn value(&self, features: &[f32]) -> f32 {
        (self.value_bias + dot(&self.value_weight, features)).tanh()
    }

    /// The whole forward pass: a logit for every action slot, and the value.
    /// What `train_network.py`'s network computes; the search uses
    /// `NetworkEvaluator`, which only works out the legal slots.
    #[allow(dead_code)] // the reference forward pass; exercised by the tests
    pub fn forward(&self, obs: &[f32]) -> (Vec<f32>, f32) {
        let mut scratch = (Vec::new(), Vec::new());
        let features = self.trunk(obs, &mut scratch);
        let logits = (0..self.action_size).map(|action| self.logit(features, action)).collect();
        (logits, self.value(features))
    }
}

/// The dot product of two equal-length slices. Summed in eight lanes, which
/// the compiler turns into vector code; a single running sum it may not
/// reorder, and so cannot.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let mut lanes = [0.0f32; 8];
    let (a_chunks, b_chunks) = (a.chunks_exact(8), b.chunks_exact(8));
    let tail: f32 = a_chunks.remainder().iter().zip(b_chunks.remainder()).map(|(x, y)| x * y).sum();
    for (x, y) in a_chunks.zip(b_chunks) {
        for lane in 0..8 {
            lanes[lane] += x[lane] * y[lane];
        }
    }
    lanes.iter().sum::<f32>() + tail
}

/// A network as the search's evaluator: the player to act's observation in,
/// priors for their legal actions and the position's value out.
pub struct NetworkEvaluator {
    network: Arc<Network>,
    observation: Box<[f32; OBS_SIZE]>,
    scratch: (Vec<f32>, Vec<f32>),
}

impl NetworkEvaluator {
    /// An evaluator running `network`. Fails if the network was trained for
    /// other observation or policy sizes than this build's — an older
    /// encoding, say.
    pub fn new(network: Arc<Network>) -> io::Result<NetworkEvaluator> {
        if (network.obs_size(), network.action_size()) != (OBS_SIZE, ACTION_SIZE) {
            return Err(invalid(format!(
                "the network takes {} observations and gives {} actions; this build has {OBS_SIZE} and {ACTION_SIZE}",
                network.obs_size(), network.action_size(),
            )));
        }
        Ok(NetworkEvaluator {
            network,
            observation: Box::new([0.0; OBS_SIZE]),
            scratch: (Vec::new(), Vec::new()),
        })
    }
}

impl Evaluator for NetworkEvaluator {
    /// The priors are the softmax of the legal actions' logits — the
    /// distribution the trainer fitted to the search's visit shares.
    fn evaluate(&mut self, game: &FabGame, legal: &[Action], priors: &mut [f32]) -> f32 {
        let mover = game.state.active_player;
        // Only what the mover may know goes in, though `game` holds it all.
        encode_into(&game.state, mover, &mut self.observation);
        let features = self.network.trunk(&self.observation[..], &mut self.scratch);

        let player = game.state.player(mover);
        let mut highest = f32::NEG_INFINITY;
        for (prior, &action) in priors.iter_mut().zip(legal) {
            *prior = self.network.logit(features, action_index(player, action));
            highest = highest.max(*prior);
        }
        // The search normalizes; subtracting the highest logit keeps the
        // exponentials from overflowing.
        for prior in priors.iter_mut() {
            *prior = (*prior - highest).exp();
        }
        self.network.value(features)
    }
}

/// Load the weights file at `path` for a command-line run, or say why not
/// and exit.
pub fn load_or_exit(path: &str) -> Arc<Network> {
    match Network::load(path) {
        Ok(network) => Arc::new(network),
        Err(error) => {
            eprintln!("cannot load the network {path}: {error}");
            std::process::exit(1);
        }
    }
}

/// An evaluator for `network` for a command-line run, or say why not and exit.
pub fn evaluator_or_exit(network: &Arc<Network>) -> NetworkEvaluator {
    match NetworkEvaluator::new(network.clone()) {
        Ok(evaluator) => evaluator,
        Err(error) => {
            eprintln!("cannot use the network: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::determinize::shuffle_hidden_information;
    use crate::fab_game::new_rng;
    use crate::game_state::Phase;
    use crate::puct::PuctAgent;
    use crate::simulator::{Game, Simulator};

    /// A weights file whose `k`th number is `value(k)`.
    pub fn weights_file(obs: usize, hidden: &[usize], actions: usize, value: impl Fn(usize) -> f32) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        for n in [obs, actions, hidden.len()].iter().chain(hidden) {
            bytes.extend((*n as u32).to_le_bytes());
        }
        // Each hidden layer's weight and bias, then the two heads', which
        // both read the last hidden layer.
        let last = *hidden.last().expect("a hidden layer");
        let mut total = actions * last + actions + last + 1;
        let mut inputs = obs;
        for &outputs in hidden {
            total += outputs * inputs + outputs;
            inputs = outputs;
        }
        for k in 0..total {
            bytes.extend(value(k).to_le_bytes());
        }
        bytes
    }

    /// A network of this build's sizes with small pseudo-random weights.
    pub fn random_network(seed: u64) -> Arc<Network> {
        let value = |k: usize| {
            // A cheap hash to a number in -0.1..0.1.
            let h = (k as u64 ^ seed).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(29)
                .wrapping_mul(0xBF58_476D_1CE4_E5B9);
            ((h >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 0.2
        };
        let bytes = weights_file(OBS_SIZE, &[32, 24], ACTION_SIZE, value);
        Arc::new(Network::read(&mut &bytes[..]).unwrap())
    }

    /// The formula both this test and `test_train_network.py` build their
    /// reference network from.
    fn reference_weight(k: usize) -> f32 {
        (0.5 * (0.37 * k as f64 + 0.1).sin()) as f32
    }

    /// The forward pass agrees with PyTorch's: the same weights and input in
    /// `test_train_network.py` give these same numbers there.
    #[test]
    fn forward_matches_the_pytorch_network() {
        let bytes = weights_file(7, &[5, 4], 6, reference_weight);
        let network = Network::read(&mut &bytes[..]).unwrap();
        assert_eq!((network.obs_size(), network.action_size()), (7, 6));
        let input: Vec<f32> = (0..7)
            .map(|i| if i % 3 == 0 { 0.0 } else { (0.91 * i as f64).cos() as f32 })
            .collect();

        let (logits, value) = network.forward(&input);
        let expected = [0.4166978, 0.6056518, 0.5340008, 0.261982, 0.1099094, 0.0992357];
        for (got, want) in logits.iter().zip(expected) {
            assert!((got - want).abs() < 1e-5, "logits {logits:?}");
        }
        assert!((value - -0.5396721).abs() < 1e-5, "value {value}");
    }

    #[test]
    fn dot_handles_every_length() {
        for len in 0..40 {
            let a: Vec<f32> = (0..len).map(|i| i as f32 * 0.5 - 3.0).collect();
            let b: Vec<f32> = (0..len).map(|i| 1.0 - i as f32 * 0.25).collect();
            let plain: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
            assert!((dot(&a, &b) - plain).abs() < 1e-3, "length {len}");
        }
    }

    #[test]
    fn malformed_files_are_refused() {
        let good = weights_file(7, &[5, 4], 6, reference_weight);
        assert!(Network::read(&mut &good[..]).is_ok());

        let cut = &good[..good.len() - 1];
        let mut long = good.clone();
        long.push(0);
        let mut magic = good.clone();
        magic[0] = b'X';
        let mut no_layers = good.clone();
        no_layers[16..20].copy_from_slice(&0u32.to_le_bytes());
        let mut huge = good.clone();
        huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        for bad in [cut, &long[..], &magic[..], &no_layers[..], &huge[..], &good[..10]] {
            assert!(Network::read(&mut &bad[..]).is_err());
        }

        // A network of other sizes loads, but is not for this build's games.
        let small = Arc::new(Network::read(&mut &good[..]).unwrap());
        assert!(NetworkEvaluator::new(small).is_err());
    }

    fn game_at_a_decision(seed: u64) -> FabGame {
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
        game.reset(false);
        let legal = game.legal_actions();
        game.step(legal[0]);
        game
    }

    /// The evaluator's answer is the full forward pass on the mover's
    /// observation, cut down to the legal actions.
    #[test]
    fn evaluation_is_the_forward_pass_over_the_legal_actions() {
        let network = random_network(1);
        let mut evaluator = NetworkEvaluator::new(network.clone()).unwrap();
        let mut differing_priors = false;
        for seed in 0..5 {
            let game = game_at_a_decision(seed);
            let legal = game.legal_actions();
            let mut priors = vec![0.0; legal.len()];
            let value = evaluator.evaluate(&game, &legal, &mut priors);

            let mover = game.state.active_player;
            let obs = crate::observation::observation(&game.state, mover);
            let (logits, full_value) = network.forward(&obs);
            assert!((value - full_value).abs() < 1e-6);
            assert!((-1.0..=1.0).contains(&value));

            let player = game.state.player(mover);
            let total: f32 = priors.iter().sum();
            let exps: Vec<f32> = legal.iter().map(|&a| logits[action_index(player, a)].exp()).collect();
            let exp_total: f32 = exps.iter().sum();
            for (prior, exp) in priors.iter().zip(&exps) {
                assert!(*prior > 0.0 && *prior <= 1.0);
                assert!((prior / total - exp / exp_total).abs() < 1e-5);
            }
            differing_priors |= priors.iter().any(|p| (p - priors[0]).abs() > 1e-4);
        }
        assert!(differing_priors);
    }

    /// The network sees only the mover's observation: re-dealing what the
    /// mover cannot see changes nothing about its answer.
    #[test]
    fn evaluation_ignores_hidden_information() {
        let mut evaluator = NetworkEvaluator::new(random_network(2)).unwrap();
        let mut redeal = new_rng(Some(5));
        let game = game_at_a_decision(3);
        let legal = game.legal_actions();
        let mut priors = vec![0.0; legal.len()];
        let value = evaluator.evaluate(&game, &legal, &mut priors);
        for _ in 0..10 {
            let mut sample = game.clone();
            shuffle_hidden_information(&mut sample.state, game.state.active_player, &mut redeal);
            assert!(sample.state.p1.hand != game.state.p1.hand || sample.state.p2.hand != game.state.p2.hand
                || sample.state.p1.deck != game.state.p1.deck);
            let mut again = vec![0.0; legal.len()];
            assert_eq!(evaluator.evaluate(&sample, &legal, &mut again), value);
            assert_eq!(again, priors);
        }
    }

    /// A network — untrained here — drives the search through whole games
    /// from both seats, and the same seeds play the same game.
    #[test]
    fn a_network_plays_whole_games() {
        let play = |seed: u64| {
            let agent = |s| PuctAgent::new(NetworkEvaluator::new(random_network(4)).unwrap(), 30, Some(s));
            let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
            let mut sim = Simulator::new(game, agent(seed + 1), agent(seed + 2));
            sim.run(false);
            (sim.game.state.phase, sim.game.state.turn_count, sim.game.state.p1.life, sim.game.state.p2.life)
        };
        for seed in 0..3 {
            let result = play(seed);
            assert!(matches!(result.0, Phase::Player1Win | Phase::Player2Win | Phase::Draw));
            assert_eq!(play(seed), result);
        }
    }
}

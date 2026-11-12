//! A PUCT search over information sets: the AlphaZero tree search, adapted to
//! hidden information the way `ismcts` adapts plain MCTS.
//!
//! Like `IsmctsAgent`, every iteration re-deals the cards the searching player
//! cannot see and walks one tree through that sample, choosing only among the
//! children legal in it. What changes is how a position is judged and how a
//! child is chosen:
//!
//! - **Evaluation instead of playouts.** A new position is handed to an
//!   `Evaluator` — in the end a policy/value network — which answers with a
//!   prior for each legal action and a value for the player to act. The value
//!   is backed up the path straight away; no game is played out (unless the
//!   evaluator does so itself, as `RolloutEvaluator` does).
//! - **PUCT selection.** A child is scored `Q + c · P · √available / (1 +
//!   visits)`: its mean value for the player choosing, plus an exploration
//!   bonus that starts at its prior and fades as it is visited. As in ISMCTS,
//!   `available` — how often the child was legal when its parent was visited —
//!   stands in for the parent's visit count.
//!
//! A sample can offer actions at a node that earlier samples did not (the
//! opponent holds different cards). Such a node is evaluated again in the new
//! sample to give the new actions priors, and the iteration ends there, as it
//! does at a new node; the children it already had keep the priors they got.
//!
//! Values run from -1 (a loss) to 1 (a win), with 0 a draw.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::action::{Action, ActionType, MAX_LEGAL_ACTIONS};
use crate::action_encoding::{action_index, ACTION_SIZE};
use crate::agents::Agent;
use crate::determinize::shuffle_hidden_information;
use crate::fab_game::FabGame;
use crate::game_state::{Phase, PlayerIndex};
use crate::simulator::Game;

/// Judges a position for the search: a policy/value network, or a stand-in.
pub trait Evaluator {
    /// Evaluate `game`, a live position, for the player to act. Write a prior
    /// for each of `legal` (their legal actions) into `priors`, in the same
    /// order — any non-negative weights; the search normalizes them — and
    /// return the position's value for that player, from -1 to 1.
    ///
    /// `game` is a sample the search dealt, so the evaluator may read all of
    /// it, but a network should look only at the mover's observation of it
    /// (`observation::encode_into`).
    fn evaluate(&mut self, game: &FabGame, legal: &[Action], priors: &mut [f32]) -> f32;
}

/// The outcome of a finished game for `player`: 1 a win, -1 a loss, 0 a draw.
pub fn outcome_for(phase: Phase, player: usize) -> f32 {
    match (phase, player) {
        (Phase::Player1Win, 0) | (Phase::Player2Win, 1) => 1.0,
        (Phase::Player1Win, _) | (Phase::Player2Win, _) => -1.0,
        (Phase::Draw, _) => 0.0,
        _ => panic!("the game is not over in {phase:?}"),
    }
}

/// An evaluator that needs no network: every legal action gets the same
/// prior, and the value is the result of one uniformly random playout. With it
/// the search is ISMCTS with PUCT's selection rule, which makes the search
/// usable — and testable — before there is a network to plug in.
pub struct RolloutEvaluator {
    rng: SmallRng,
}

impl RolloutEvaluator {
    pub fn new(seed: Option<u64>) -> Self {
        let rng = match seed {
            Some(s) => SmallRng::seed_from_u64(s),
            None => rand::make_rng(),
        };
        RolloutEvaluator { rng }
    }
}

impl Evaluator for RolloutEvaluator {
    fn evaluate(&mut self, game: &FabGame, _legal: &[Action], priors: &mut [f32]) -> f32 {
        priors.fill(1.0);
        let player = game.player_to_act();
        let mut game = game.clone();
        while !game.is_over() {
            let legal = game.legal_actions();
            let pick = ((self.rng.next_u32() as u64 * legal.len() as u64) >> 32) as usize;
            game.step(legal[pick]);
        }
        outcome_for(game.state.phase, player)
    }
}

/// "No node": the end of a child list.
const NONE: u32 = u32::MAX;

/// One node of the search tree: the position reached by taking `action` from
/// the parent. Children are a linked list, as in `ismcts`.
struct Node {
    /// The action that leads here from the parent.
    action: Action,
    /// The prior the evaluator gave `action`, among the actions legal when
    /// this node was added.
    prior: f32,
    /// How many iterations went through this node.
    visits: u32,
    /// How many times this node was legal when its parent chose a child.
    available: u32,
    /// The values backed up through this node, added up, each from the point
    /// of view of the player who chose `action`.
    value: f32,
    first_child: u32,
    next_sibling: u32,
}

/// What the search found out about one root action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootStat {
    pub action: Action,
    /// How many iterations chose the action.
    pub visits: u32,
    /// The mean value of those iterations for the searching player (0 if
    /// there were none).
    pub value: f32,
    /// The evaluator's prior for the action.
    pub prior: f32,
}

pub struct PuctAgent<E: Evaluator> {
    pub evaluator: E,
    rng: SmallRng,
    iterations: u32,
    exploration: f32,
    /// Dirichlet noise mixed into the root priors: `(alpha, weight)`.
    root_noise: Option<(f32, f32)>,
    nodes: Vec<Node>,
    /// The nodes the current iteration walked through, each with the player
    /// who chose it.
    path: Vec<(u32, usize)>,
}

impl<E: Evaluator> PuctAgent<E> {
    /// The weight `c` of the exploration bonus. Values span 2 (-1 to 1), so
    /// this sits around where AlphaZero's does.
    pub const DEFAULT_EXPLORATION: f32 = 1.5;

    /// An agent running `iterations` iterations per decision. `seed` seeds the
    /// re-deals; the evaluator has whatever randomness it was built with.
    pub fn new(evaluator: E, iterations: u32, seed: Option<u64>) -> Self {
        let rng = match seed {
            Some(s) => SmallRng::seed_from_u64(s),
            None => rand::make_rng(),
        };
        PuctAgent {
            evaluator,
            rng,
            iterations,
            exploration: Self::DEFAULT_EXPLORATION,
            root_noise: None,
            nodes: Vec::new(),
            path: Vec::new(),
        }
    }

    #[allow(dead_code)] // a tuning knob; exercised by the tests
    pub fn with_exploration(mut self, exploration: f32) -> Self {
        self.exploration = exploration;
        self
    }

    /// Mix Dirichlet noise into the root's priors, as AlphaZero does in
    /// self-play so that games differ and no root action goes unexamined: each
    /// prior becomes `(1 - weight) · prior + weight · noise`, the noise drawn
    /// afresh for every search from a Dirichlet distribution with
    /// concentration `alpha`. Off by default, which is right for playing to
    /// win.
    pub fn with_root_noise(mut self, alpha: f32, weight: f32) -> Self {
        assert!(alpha > 0.0 && (0.0..=1.0).contains(&weight));
        self.root_noise = Some((alpha, weight));
        self
    }

    /// Mix the root noise, if any, into the priors of the root's children.
    fn add_root_noise(&mut self) {
        let Some((alpha, weight)) = self.root_noise else {
            return;
        };
        let mut noise = [0.0f32; MAX_LEGAL_ACTIONS];
        let mut children = 0;
        let mut child = self.nodes[0].first_child;
        while child != NONE {
            noise[children] = gamma(&mut self.rng, alpha);
            children += 1;
            child = self.nodes[child as usize].next_sibling;
        }
        // Gamma draws divided by their sum are a Dirichlet draw.
        let total: f32 = noise[..children].iter().sum();
        if total <= 0.0 {
            return;
        }
        let mut child = self.nodes[0].first_child;
        for share in &noise[..children] {
            let node = &mut self.nodes[child as usize];
            node.prior = (1.0 - weight) * node.prior + weight * share / total;
            child = node.next_sibling;
        }
    }

    /// The child of `parent` reached by `action`, if the tree has it.
    fn child(&self, parent: u32, action: Action) -> Option<u32> {
        let mut child = self.nodes[parent as usize].first_child;
        while child != NONE {
            let node = &self.nodes[child as usize];
            if node.action == action {
                return Some(child);
            }
            child = node.next_sibling;
        }
        None
    }

    fn add_child(&mut self, parent: u32, action: Action, prior: f32) {
        let child = self.nodes.len() as u32;
        self.nodes.push(Node {
            action,
            prior,
            visits: 0,
            available: 0,
            value: 0.0,
            first_child: NONE,
            next_sibling: self.nodes[parent as usize].first_child,
        });
        self.nodes[parent as usize].first_child = child;
    }

    /// Evaluate `game` at `node` and add a child for each legal action the
    /// node does not have yet, with its share of the priors. Returns the
    /// position's value for the player to act.
    fn expand(&mut self, node: u32, game: &FabGame, legal: &[Action]) -> f32 {
        let mut priors = [0.0f32; MAX_LEGAL_ACTIONS];
        let priors = &mut priors[..legal.len()];
        let value = self.evaluator.evaluate(game, legal, priors);
        debug_assert!((-1.0..=1.0).contains(&value), "evaluator returned the value {value}");
        debug_assert!(priors.iter().all(|p| *p >= 0.0), "evaluator returned a negative prior");
        let total: f32 = priors.iter().sum();
        for (&action, &prior) in legal.iter().zip(priors.iter()) {
            if self.child(node, action).is_none() {
                // An evaluator that rules everything out gets uniform priors.
                let prior = if total > 0.0 { prior / total } else { 1.0 / legal.len() as f32 };
                self.add_child(node, action, prior);
            }
        }
        value
    }

    /// A game `observer` cannot tell from `root_game`: the hidden cards
    /// re-dealt, and an rng of its own — the real game's would tell the search
    /// how the real shuffles and random discards are going to fall.
    fn sample(&mut self, root_game: &FabGame, observer: PlayerIndex) -> FabGame {
        let mut game = root_game.clone();
        game.rng = SmallRng::seed_from_u64(self.rng.next_u64());
        shuffle_hidden_information(&mut game.state, observer, &mut self.rng);
        game
    }

    /// One iteration: deal a sample of the hidden cards, walk down the tree
    /// through it to a position that needs evaluating, and back the value up.
    fn iterate(&mut self, root_game: &FabGame, observer: PlayerIndex) {
        let mut game = self.sample(root_game, observer);

        self.path.clear();
        let mut node = 0u32;
        // The value to back up, and the player it is for.
        let (value, value_player) = loop {
            if game.is_over() {
                break (outcome_for(game.state.phase, 0), 0);
            }
            let legal = game.legal_actions();
            let mover = game.player_to_act();

            // A new node, or one this sample offers new actions at: evaluate.
            let mut children = [NONE; MAX_LEGAL_ACTIONS];
            let mut missing = false;
            for (slot, &action) in children.iter_mut().zip(legal.iter()) {
                match self.child(node, action) {
                    Some(child) => *slot = child,
                    None => missing = true,
                }
            }
            if missing {
                break (self.expand(node, &game, &legal), mover);
            }

            // Otherwise follow the best child among those legal here. Their
            // priors are shared out again among themselves, since the node
            // may have other children that this sample does not offer.
            let children = &children[..legal.len()];
            let total: f32 = children.iter().map(|&c| self.nodes[c as usize].prior).sum();
            let mut best = (NONE, f32::NEG_INFINITY);
            for &child in children {
                let n = &mut self.nodes[child as usize];
                n.available += 1;
                let mean = if n.visits == 0 { 0.0 } else { n.value / n.visits as f32 };
                let prior = if total > 0.0 { n.prior / total } else { 1.0 / legal.len() as f32 };
                let score = mean
                    + self.exploration * prior * (n.available as f32).sqrt() / (1 + n.visits) as f32;
                if score > best.1 {
                    best = (child, score);
                }
            }
            let child = best.0;
            game.step(self.nodes[child as usize].action);
            self.path.push((child, mover));
            node = child;
        };

        // Backpropagation: each node is scored for the player who chose it.
        self.nodes[0].visits += 1;
        for &(visited, mover) in &self.path {
            let node = &mut self.nodes[visited as usize];
            node.visits += 1;
            node.value += if mover == value_player { value } else { -value };
        }
    }

    /// Run the search from `game` for the player to act and return what it
    /// found for each root action.
    pub fn search(&mut self, game: &FabGame) -> Vec<RootStat> {
        let observer = PlayerIndex::new(game.player_to_act() as u8);
        // Samples are cloned from a copy with the logs off, so a logged game
        // does not pay to copy and extend its log on every iteration.
        #[cfg_attr(not(feature = "logging"), allow(unused_mut))]
        let mut root_game = game.clone();
        #[cfg(feature = "logging")]
        {
            root_game.state.log = None;
            root_game.state.p1.log = None;
            root_game.state.p2.log = None;
        }

        self.nodes.clear();
        self.nodes.push(Node {
            action: Action::no_card(ActionType::Pass),
            prior: 1.0,
            visits: 0,
            available: 0,
            value: 0.0,
            first_child: NONE,
            next_sibling: NONE,
        });
        for i in 0..self.iterations {
            self.iterate(&root_game, observer);
            // The first iteration gave the root its children and priors.
            if i == 0 {
                self.add_root_noise();
            }
        }

        let mut stats = Vec::new();
        let mut child = self.nodes[0].first_child;
        while child != NONE {
            let node = &self.nodes[child as usize];
            stats.push(RootStat {
                action: node.action,
                visits: node.visits,
                value: node.value / node.visits.max(1) as f32,
                prior: node.prior,
            });
            child = node.next_sibling;
        }
        stats
    }
}

/// A draw from the Gamma distribution with shape `alpha` and scale 1
/// (Marsaglia and Tsang's method), for Dirichlet noise.
fn gamma(rng: &mut SmallRng, alpha: f32) -> f32 {
    use rand::RngExt;
    if alpha < 1.0 {
        // Gamma(alpha) = Gamma(alpha + 1) · U^(1 / alpha).
        let u: f32 = rng.random();
        return gamma(rng, alpha + 1.0) * u.max(f32::MIN_POSITIVE).powf(1.0 / alpha);
    }
    let d = alpha - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        // A standard normal draw, by Box and Muller's transform.
        let (u1, u2): (f32, f32) = (rng.random(), rng.random());
        let x = (-2.0 * u1.max(f32::MIN_POSITIVE).ln()).sqrt() * (std::f32::consts::TAU * u2).cos();
        let v = (1.0 + c * x).powi(3);
        let u: f32 = rng.random();
        if v > 0.0 && u.max(f32::MIN_POSITIVE).ln() < 0.5 * x * x + d - d * v + d * v.ln() {
            return d * v;
        }
    }
}

/// The policy a search's visit counts stand for, as a training target: each
/// root action's share of the visits, at its `action_encoding` index, for the
/// player to act in `game`. All zeros if the search made no visits.
#[allow(dead_code)] // a dense form of what self-play records; exercised by the tests
pub fn policy_target(game: &FabGame, stats: &[RootStat], out: &mut [f32; ACTION_SIZE]) {
    out.fill(0.0);
    let player = game.state.player(game.state.active_player);
    let total: u32 = stats.iter().map(|s| s.visits).sum();
    if total == 0 {
        return;
    }
    for stat in stats {
        out[action_index(player, stat.action)] = stat.visits as f32 / total as f32;
    }
}

impl<E: Evaluator> Agent<FabGame> for PuctAgent<E> {
    /// The most-visited root action.
    fn select_action(&mut self, game: &FabGame, legal_actions: &[Action]) -> Action {
        if legal_actions.len() == 1 {
            return legal_actions[0];
        }
        self.search(game)
            .into_iter()
            // Re-dealing never changes what the searching player may do, so
            // every root action is legal; the filter only guards that.
            .filter(|stat| legal_actions.contains(&stat.action))
            .max_by_key(|stat| stat.visits)
            .map(|stat| stat.action)
            .unwrap_or(legal_actions[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::RandomAgent;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::simulator::Simulator;

    fn game_at_a_decision(seed: u64) -> FabGame {
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
        game.reset(false);
        // Past choosing who goes first, into a turn with a hand to play.
        let legal = game.legal_actions();
        game.step(legal[0]);
        game
    }

    fn rollout_agent(iterations: u32, seed: u64) -> PuctAgent<RolloutEvaluator> {
        PuctAgent::new(RolloutEvaluator::new(Some(seed ^ 0xE7A1)), iterations, Some(seed))
    }

    /// An evaluator with fixed opinions: `favourite` gets all but a sliver of
    /// the prior wherever it is legal, and every position is worth `value` to
    /// the player to act. Counts how often it is asked.
    struct Fixed {
        favourite: Option<Action>,
        value: f32,
        calls: u32,
    }

    impl Evaluator for Fixed {
        fn evaluate(&mut self, _game: &FabGame, legal: &[Action], priors: &mut [f32]) -> f32 {
            self.calls += 1;
            for (prior, action) in priors.iter_mut().zip(legal) {
                *prior = if Some(*action) == self.favourite { 100.0 } else { 1.0 };
            }
            self.value
        }
    }

    /// The search gives every root action a child with a normalized prior,
    /// spends each iteration after the first on one of them, and leaves the
    /// real game untouched.
    #[test]
    fn search_accounts_for_every_iteration() {
        let game = game_at_a_decision(11);
        let legal = game.legal_actions();
        assert!(legal.len() >= 2);
        let before = (game.state.p1.hand, game.state.p2.hand, game.state.p1.deck, game.state.p2.deck);

        let mut agent = rollout_agent(300, 1);
        let stats = agent.search(&game);

        assert_eq!(stats.len(), legal.len());
        for stat in &stats {
            assert!(legal.contains(&stat.action));
            assert!((-1.0..=1.0).contains(&stat.value));
        }
        assert!((stats.iter().map(|s| s.prior).sum::<f32>() - 1.0).abs() < 1e-5);
        // The first iteration evaluates the root and visits no child.
        assert_eq!(stats.iter().map(|s| s.visits).sum::<u32>(), 299);
        let after = (game.state.p1.hand, game.state.p2.hand, game.state.p1.deck, game.state.p2.deck);
        assert!(before == after);
    }

    /// With every position worth the same, the visits follow the priors: the
    /// action the evaluator favours gets most of them.
    #[test]
    fn visits_follow_the_priors() {
        let game = game_at_a_decision(11);
        let legal = game.legal_actions();
        for &favourite in legal.iter() {
            let evaluator = Fixed { favourite: Some(favourite), value: 0.0, calls: 0 };
            let mut agent = PuctAgent::new(evaluator, 200, Some(3));
            let stats = agent.search(&game);
            let top = stats.iter().max_by_key(|s| s.visits).unwrap();
            assert_eq!(top.action, favourite);
            assert!(top.visits > 100, "the favourite got only {} of 199 visits", top.visits);
            assert!(top.prior > 0.8);
            assert_eq!(agent.select_action(&game, &legal), favourite);
        }
    }

    /// Every iteration evaluates at most one position — none when it runs
    /// into the end of the game.
    #[test]
    fn each_iteration_evaluates_at_most_once() {
        let game = game_at_a_decision(11);
        let evaluator = Fixed { favourite: None, value: 0.0, calls: 0 };
        let mut agent = PuctAgent::new(evaluator, 500, Some(3));
        agent.search(&game);
        assert!(agent.evaluator.calls <= 500);
        assert!(agent.evaluator.calls > 250);
        assert_eq!(agent.nodes[0].visits, 500);
    }

    /// A value is credited to the player it is for and charged to the other.
    /// Steer a two-iteration search down each root action in turn — one
    /// iteration evaluates the root, the other the position after the action
    /// — with an evaluator that tells whoever is to act that they are
    /// winning: the action looks good exactly when the searching player acts
    /// again after it.
    #[test]
    fn values_are_backed_up_for_the_player_who_moved() {
        let game = game_at_a_decision(11);
        let me = game.player_to_act();
        let observer = PlayerIndex::new(me as u8);
        let mut next_movers = std::collections::BTreeSet::new();
        for seed in 0..8 {
            for &action in game.legal_actions().iter() {
                let evaluator = Fixed { favourite: Some(action), value: 1.0, calls: 0 };
                let stats = PuctAgent::new(evaluator, 2, Some(seed)).search(&game);
                let stat = stats.iter().find(|s| s.action == action).unwrap();
                assert_eq!(stat.visits, 1);

                // Who acts next depends on the cards the sample dealt the
                // opponent (can they block?), so replay the second
                // iteration's sample with an agent seeded the same.
                let mut twin = PuctAgent::new(Fixed { favourite: None, value: 0.0, calls: 0 }, 2, Some(seed));
                twin.sample(&game, observer);
                let mut next = twin.sample(&game, observer);
                next.step(action);
                assert!(!next.is_over());
                let expected = if next.player_to_act() == me { 1.0 } else { -1.0 };
                assert_eq!(stat.value, expected, "seed {seed}: {action:?}");
                next_movers.insert(next.player_to_act());
            }
        }
        // Both cases came up: actions that kept the decision and that passed it.
        assert_eq!(next_movers.len(), 2);
    }

    /// Gamma draws have the mean and variance of the distribution — both
    /// equal to the shape — on either side of the shape-1 split.
    #[test]
    fn gamma_draws_have_the_right_mean_and_variance() {
        let mut rng = SmallRng::seed_from_u64(1);
        for alpha in [0.3f32, 1.0, 2.5] {
            let draws: Vec<f32> = (0..40_000).map(|_| gamma(&mut rng, alpha)).collect();
            assert!(draws.iter().all(|x| x.is_finite() && *x >= 0.0));
            let mean = draws.iter().sum::<f32>() / draws.len() as f32;
            let variance = draws.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / draws.len() as f32;
            assert!((mean - alpha).abs() < 0.05 * alpha.max(1.0), "alpha {alpha}: mean {mean}");
            assert!((variance - alpha).abs() < 0.1 * alpha.max(1.0), "alpha {alpha}: variance {variance}");
        }
    }

    /// Root noise leaves the priors a distribution, changes them, and differs
    /// from one search to the next; without it the priors are the evaluator's.
    #[test]
    fn root_noise_perturbs_the_root_priors() {
        let game = game_at_a_decision(11);
        let priors = |agent: &mut PuctAgent<RolloutEvaluator>| -> Vec<f32> {
            agent.search(&game).iter().map(|s| s.prior).collect()
        };
        let uniform = 1.0 / game.legal_actions().len() as f32;

        let mut plain = rollout_agent(20, 1);
        assert!(priors(&mut plain).iter().all(|p| (p - uniform).abs() < 1e-6));

        let mut noisy = rollout_agent(20, 1).with_root_noise(1.0, 0.25);
        let first = priors(&mut noisy);
        let second = priors(&mut noisy);
        assert!((first.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert!(first.iter().any(|p| (p - uniform).abs() > 1e-3));
        // A quarter of each prior is noise; the rest is still the evaluator's.
        assert!(first.iter().all(|&p| p >= 0.75 * uniform - 1e-6));
        assert_ne!(first, second);
    }

    #[test]
    fn policy_target_is_the_share_of_visits() {
        let game = game_at_a_decision(11);
        let mut agent = rollout_agent(200, 5);
        let stats = agent.search(&game);
        let mut target = [9.0; ACTION_SIZE];
        policy_target(&game, &stats, &mut target);
        assert!((target.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert_eq!(target.iter().filter(|&&p| p > 0.0).count(),
            stats.iter().filter(|s| s.visits > 0).count());
        let player = game.state.player(game.state.active_player);
        for stat in &stats {
            assert_eq!(target[action_index(player, stat.action)], stat.visits as f32 / 199.0);
        }
        policy_target(&game, &[], &mut target);
        assert!(target.iter().all(|&p| p == 0.0));
    }

    /// The same seeds make the same search.
    #[test]
    fn a_seeded_agent_is_reproducible() {
        let game = game_at_a_decision(11);
        let search = |seed| rollout_agent(150, seed).search(&game);
        assert_eq!(search(4), search(4));
    }

    /// The search does not draw from the real game's rng: the same position
    /// with a different rng gets the same search.
    #[test]
    fn the_search_ignores_the_real_games_rng() {
        let game = game_at_a_decision(11);
        let mut other = game.clone();
        other.rng = crate::fab_game::new_rng(Some(78));
        assert_eq!(rollout_agent(150, 4).search(&game), rollout_agent(150, 4).search(&other));
    }

    /// With playouts for values, plays whole games against a random opponent
    /// from both seats and wins most of them.
    #[test]
    fn beats_a_random_agent() {
        let mut wins = 0;
        let games = 6;
        for seed in 0..games {
            let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
            let puct = rollout_agent(50, seed + 100);
            let random = RandomAgent::new(Some(seed + 200));
            let puct_is_p1 = seed % 2 == 0;
            let phase = if puct_is_p1 {
                let mut sim = Simulator::new(game, puct, random);
                sim.run(false);
                sim.game.state.phase
            } else {
                let mut sim = Simulator::new(game, random, puct);
                sim.run(false);
                sim.game.state.phase
            };
            let won = if puct_is_p1 { Phase::Player1Win } else { Phase::Player2Win };
            wins += (phase == won) as u32;
        }
        assert!(wins >= 5, "PUCT won only {wins} of {games} games against random play");
    }
}

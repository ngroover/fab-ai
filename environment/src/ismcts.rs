//! An Information Set Monte Carlo Tree Search agent (single-observer ISMCTS,
//! Cowling, Powley & Whitehouse 2012).
//!
//! Ordinary MCTS searches from the true game state, which here would mean
//! reading the opponent's hand and both decks. ISMCTS instead builds one tree
//! over the searching player's *information set*: every iteration re-deals
//! the cards that player cannot see (`shuffle_hidden_information`) and walks
//! the tree through that sample. Because each sample offers different actions
//! at the same tree node — the opponent holds different cards — a node only
//! chooses among the children legal in the current sample, and UCB's
//! exploration term counts how often a child was *available*, not how often
//! its parent was visited.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::action::{Action, MAX_LEGAL_ACTIONS};
use crate::agents::Agent;
use crate::determinize::shuffle_hidden_information;
use crate::fab_game::FabGame;
use crate::game_state::{Phase, PlayerIndex};
use crate::simulator::Game;

/// "No node": the end of a child list.
const NONE: u32 = u32::MAX;

/// One node of the search tree: the position reached by taking `action` from
/// the parent. Children are a linked list (`first_child`, then each child's
/// `next_sibling`), so growing the tree allocates nothing per node.
struct Node {
    /// The action that leads here from the parent.
    action: Action,
    /// How many iterations went through this node.
    visits: u32,
    /// How many times this node was legal when its parent was visited.
    available: u32,
    /// Total reward of those iterations, for the player who chose `action`.
    reward: f32,
    first_child: u32,
    next_sibling: u32,
}

/// An agent that picks its action by ISMCTS with uniformly random playouts.
pub struct IsmctsAgent {
    rng: SmallRng,
    /// Search iterations (determinizations) per decision.
    iterations: u32,
    /// UCB1 exploration constant.
    exploration: f32,
    /// The tree of the decision in progress, root first. Kept between
    /// decisions only to reuse its allocation.
    nodes: Vec<Node>,
    /// The nodes the current iteration went through, each with the player who
    /// chose its action.
    path: Vec<(u32, usize)>,
}

impl IsmctsAgent {
    /// The exploration constant used unless `with_exploration` says otherwise:
    /// the value the ISMCTS paper found to work well with rewards in `[0, 1]`.
    pub const DEFAULT_EXPLORATION: f32 = 0.7;

    /// An agent that runs `iterations` search iterations per decision. Pass
    /// `Some(seed)` for a reproducible agent, or `None` for a random seed.
    pub fn new(iterations: u32, seed: Option<u64>) -> Self {
        let rng: SmallRng = match seed {
            Some(s) => SmallRng::seed_from_u64(s),
            None => rand::make_rng(),
        };
        IsmctsAgent {
            rng,
            iterations,
            exploration: Self::DEFAULT_EXPLORATION,
            nodes: Vec::new(),
            path: Vec::new(),
        }
    }

    /// Set the UCB1 exploration constant: higher tries rarely-taken actions
    /// more, lower concentrates on the ones that have done well so far.
    #[allow(dead_code)] // a tuning knob; exercised by the tests
    pub fn with_exploration(mut self, exploration: f32) -> Self {
        self.exploration = exploration;
        self
    }

    /// A uniformly random index below `len`.
    fn random_index(&mut self, len: usize) -> usize {
        // One 32-bit draw scaled into range, as `RandomAgent` does.
        ((self.rng.next_u32() as u64 * len as u64) >> 32) as usize
    }

    /// The child of `parent` reached by `action`, if it has been expanded.
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

    /// Add a child of `parent` for `action`.
    fn add_child(&mut self, parent: u32, action: Action) -> u32 {
        let child = self.nodes.len() as u32;
        self.nodes.push(Node {
            action,
            visits: 0,
            available: 1,
            reward: 0.0,
            first_child: NONE,
            next_sibling: self.nodes[parent as usize].first_child,
        });
        self.nodes[parent as usize].first_child = child;
        child
    }

    /// UCB1 over availability: the child's mean reward plus an exploration
    /// bonus that grows the more often it was available without being taken.
    fn ucb(&self, child: u32) -> f32 {
        let node = &self.nodes[child as usize];
        let visits = node.visits as f32;
        node.reward / visits + self.exploration * ((node.available as f32).ln() / visits).sqrt()
    }

    /// One search iteration from `root_game`, seen by `observer`: sample a
    /// state, walk the tree through it to a new node, play out at random, and
    /// credit the result to every node on the way.
    fn iterate(&mut self, root_game: &FabGame, observer: PlayerIndex) {
        let mut game = root_game.clone();
        // The sample gets an rng of its own: the real game's would tell the
        // search how the real shuffles and random discards are going to fall.
        game.rng = SmallRng::seed_from_u64(self.rng.next_u64());
        shuffle_hidden_information(&mut game.state, observer, &mut self.rng);

        // Selection and expansion: follow the best available child until a
        // node has an action it has not tried in this sample, and add it.
        self.path.clear();
        let mut node = 0u32;
        while !game.is_over() {
            let legal = game.legal_actions();
            let mover = game.player_to_act();
            let mut untried = [legal[0]; MAX_LEGAL_ACTIONS];
            let mut untried_len = 0;
            let mut best: Option<(u32, f32)> = None;
            for &action in legal.iter() {
                match self.child(node, action) {
                    Some(child) => {
                        self.nodes[child as usize].available += 1;
                        let score = self.ucb(child);
                        if best.is_none_or(|(_, top)| score > top) {
                            best = Some((child, score));
                        }
                    }
                    None => {
                        untried[untried_len] = action;
                        untried_len += 1;
                    }
                }
            }
            if untried_len > 0 {
                let action = untried[self.random_index(untried_len)];
                let child = self.add_child(node, action);
                game.step(action);
                self.path.push((child, mover));
                break;
            }
            let (child, _) = best.expect("a live game offers a legal action");
            game.step(self.nodes[child as usize].action);
            self.path.push((child, mover));
            node = child;
        }

        // Playout: uniformly random actions to the end of the game.
        while !game.is_over() {
            let legal = game.legal_actions();
            let action = legal[self.random_index(legal.len())];
            game.step(action);
        }

        // Backpropagation: each node is scored for the player who chose it.
        let reward_for = |player: usize| match (game.state.phase, player) {
            (Phase::Player1Win, 0) | (Phase::Player2Win, 1) => 1.0,
            (Phase::Draw, _) => 0.5,
            _ => 0.0,
        };
        for &(visited, mover) in &self.path {
            let node = &mut self.nodes[visited as usize];
            node.visits += 1;
            node.reward += reward_for(mover);
        }
    }

    /// Search from `game` and return how often each root action was taken:
    /// `(action, visits, mean reward)`, in no particular order.
    fn search(&mut self, game: &FabGame) -> Vec<(Action, u32, f32)> {
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
            action: Action::no_card(crate::action::ActionType::Pass),
            visits: 0,
            available: 0,
            reward: 0.0,
            first_child: NONE,
            next_sibling: NONE,
        });
        for _ in 0..self.iterations {
            self.iterate(&root_game, observer);
        }

        let mut stats = Vec::new();
        let mut child = self.nodes[0].first_child;
        while child != NONE {
            let node = &self.nodes[child as usize];
            stats.push((node.action, node.visits, node.reward / node.visits.max(1) as f32));
            child = node.next_sibling;
        }
        stats
    }
}

impl Agent<FabGame> for IsmctsAgent {
    /// Search, then play the most visited action — a steadier pick than the
    /// highest mean reward, which a lucky few playouts can inflate.
    fn select_action(&mut self, game: &FabGame, legal_actions: &[Action]) -> Action {
        if legal_actions.len() == 1 {
            return legal_actions[0];
        }
        self.search(game)
            .into_iter()
            // Re-dealing never changes what the searching player may do, so
            // every root action is legal; the filter only guards that.
            .filter(|(action, _, _)| legal_actions.contains(action))
            .max_by_key(|&(_, visits, _)| visits)
            .map(|(action, _, _)| action)
            .unwrap_or(legal_actions[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::RandomAgent;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::simulator::Simulator;

    /// A game played to its first real decision.
    fn game_at_a_decision(seed: u64) -> FabGame {
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
        game.reset(false);
        // Past choosing who goes first, into a turn with a hand to play.
        let legal = game.legal_actions();
        game.step(legal[0]);
        game
    }

    /// The search spends every iteration on a root action, tries each legal
    /// action, and leaves the real game untouched.
    #[test]
    fn search_visits_every_legal_action() {
        let game = game_at_a_decision(11);
        let legal = game.legal_actions();
        assert!(legal.len() >= 2);
        let before = (game.state.p1.hand, game.state.p2.hand, game.state.p1.deck, game.state.p2.deck);

        let mut agent = IsmctsAgent::new(200, Some(1));
        let stats = agent.search(&game);

        assert_eq!(stats.len(), legal.len());
        for (action, visits, mean) in &stats {
            assert!(legal.contains(action));
            assert!(*visits >= 1);
            assert!((0.0..=1.0).contains(mean));
        }
        assert_eq!(stats.iter().map(|s| s.1).sum::<u32>(), 200);
        let after = (game.state.p1.hand, game.state.p2.hand, game.state.p1.deck, game.state.p2.deck);
        assert!(before == after);
    }

    /// The same seed makes the same choices.
    #[test]
    fn a_seeded_agent_is_reproducible() {
        let game = game_at_a_decision(11);
        let legal = game.legal_actions();
        let pick = |seed| IsmctsAgent::new(150, Some(seed)).select_action(&game, &legal);
        assert_eq!(pick(4), pick(4));
    }

    /// The search does not draw from the real game's rng, which would tell it
    /// how the real shuffles and random discards are going to fall: the same
    /// position with a different rng gets the same search.
    #[test]
    fn the_search_ignores_the_real_games_rng() {
        let game = game_at_a_decision(11);
        let mut other = game.clone();
        other.rng = crate::fab_game::new_rng(Some(78));

        let visits = |g: &FabGame| {
            let stats = IsmctsAgent::new(150, Some(4)).search(g);
            stats.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>()
        };
        assert_eq!(visits(&game), visits(&other));
    }

    /// Plays whole games against a random opponent from both seats and wins
    /// most of them.
    #[test]
    fn beats_a_random_agent() {
        let mut wins = 0;
        let games = 6;
        for seed in 0..games {
            let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
            let ismcts = IsmctsAgent::new(50, Some(seed + 100));
            let random = RandomAgent::new(Some(seed + 200));
            let ismcts_is_p1 = seed % 2 == 0;
            let phase = if ismcts_is_p1 {
                let mut sim = Simulator::new(game, ismcts, random);
                sim.run(false);
                sim.game.state.phase
            } else {
                let mut sim = Simulator::new(game, random, ismcts);
                sim.run(false);
                sim.game.state.phase
            };
            let won = if ismcts_is_p1 { Phase::Player1Win } else { Phase::Player2Win };
            wins += (phase == won) as u32;
        }
        assert!(wins >= 5, "ISMCTS won only {wins} of {games} games against random play");
    }
}

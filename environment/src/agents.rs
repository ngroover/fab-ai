use crate::simulator::Game;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::SmallRng;

/// A decision policy for one player: given the game and the legal actions,
/// choose the action to take. Generic over the game so an agent written
/// against the `Game` trait (like `RandomAgent`) works with any game, while a
/// game-specific agent can implement it for just one. `&mut self` lets agents
/// carry state between decisions (an rng here; search trees or networks later).
pub trait Agent<G: Game> {
    /// Pick one of `legal_actions` to play. `legal_actions` is never empty:
    /// the simulator only asks while the game is live.
    fn select_action(&mut self, game: &G, legal_actions: &[G::Action]) -> G::Action;
}

/// The baseline agent: picks uniformly at random among the legal actions,
/// ignoring the game itself.
pub struct RandomAgent {
    rng: SmallRng,
}

impl RandomAgent {
    /// Pass `Some(seed)` for a reproducible agent, or `None` for a random seed.
    pub fn new(seed: Option<u64>) -> Self {
        let rng: SmallRng = match seed {
            Some(s) => SmallRng::seed_from_u64(s),
            None => rand::make_rng(),
        };
        RandomAgent { rng }
    }
}

impl<G: Game> Agent<G> for RandomAgent {
    fn select_action(&mut self, _game: &G, legal_actions: &[G::Action]) -> G::Action {
        // Scale one 32-bit draw into `0..len` with a multiply and shift instead
        // of `random_range`, whose general-purpose path dominated the cost of a
        // random playout. The bias is at most len / 2^32 — negligible for the
        // handful of legal actions a decision offers.
        let len = legal_actions.len() as u64;
        legal_actions[((self.rng.next_u32() as u64 * len) >> 32) as usize]
    }
}

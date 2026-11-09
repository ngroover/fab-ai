use crate::agents::Agent;

/// A two-player, alternating-decision game the simulator can drive. Anything
/// that can report its legal actions, apply one with a step function, and say
/// when it is finished can be simulated — the FaB `FabGame` implements this
/// (see `fab_game`), but another game with its own state and action types can
/// be dropped in unchanged.
pub trait Game {
    /// The action type agents choose between. `Copy` so a chosen action can be
    /// handed to `step` straight out of the legal-actions list.
    type Action: Copy;

    /// Reset to a fresh, playable game. `logging` opts into whatever game log
    /// the implementation keeps; implementations without one may ignore it.
    fn reset(&mut self, logging: bool);

    /// The list `legal_actions` returns. Anything that reads as a slice of
    /// actions works — a `Vec`, or a fixed-capacity inline list that avoids
    /// allocating on every decision (the FaB engine's `LegalActions`).
    type ActionList: std::ops::Deref<Target = [Self::Action]>;

    /// The actions the player to act may legally take right now. Empty only in
    /// a terminal state.
    fn legal_actions(&self) -> Self::ActionList;

    /// Apply one action, advancing the game.
    fn step(&mut self, action: Self::Action);

    /// True once the game has reached a terminal state.
    fn is_over(&self) -> bool;

    /// Which player (0 or 1) chooses the next action.
    fn player_to_act(&self) -> usize;
}

/// Runs a game to completion by asking two agents for decisions. Player 0's
/// actions are chosen by `p1_agent`, player 1's by `p2_agent`. Generic over
/// the game and both agents, so any `Game` implementation can be paired with
/// any mix of `Agent` policies (random vs random, random vs MCTS, ...).
pub struct Simulator<G: Game, A1: Agent<G>, A2: Agent<G>> {
    pub game: G,
    pub p1_agent: A1,
    pub p2_agent: A2,
}

impl<G: Game, A1: Agent<G>, A2: Agent<G>> Simulator<G, A1, A2> {
    pub fn new(game: G, p1_agent: A1, p2_agent: A2) -> Self {
        Simulator { game, p1_agent, p2_agent }
    }

    /// Reset the game and step it until it reaches a terminal state: each
    /// iteration the player to act's agent picks one legal action and the game
    /// steps on it. Returns the number of agent decisions taken. The finished
    /// game (its result, and its log when `logging` was on) stays readable on
    /// `self.game`, and `run` may be called again to play a fresh game.
    pub fn run(&mut self, logging: bool) -> usize {
        self.game.reset(logging);
        let mut steps = 0;
        while !self.game.is_over() {
            let legal = self.game.legal_actions();
            // A non-terminal state with no legal actions would never advance;
            // bail out rather than spin (the caller can see the game is not
            // over). Legal-action generators should not let this happen.
            if legal.is_empty() {
                break;
            }
            let action = if self.game.player_to_act() == 0 {
                self.p1_agent.select_action(&self.game, &legal)
            } else {
                self.p2_agent.select_action(&self.game, &legal)
            };
            self.game.step(action);
            steps += 1;
        }
        steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::RandomAgent;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::FabGame;

    #[test]
    fn run_plays_a_random_game_to_a_terminal_state() {
        let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        let mut sim = Simulator::new(game, RandomAgent::new(Some(1)), RandomAgent::new(Some(2)));

        let steps = sim.run(false);

        assert!(sim.game.is_over());
        assert!(steps > 0);
    }

    #[test]
    fn run_resets_so_a_finished_simulator_can_run_again() {
        let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        let mut sim = Simulator::new(game, RandomAgent::new(Some(1)), RandomAgent::new(Some(2)));

        sim.run(false);
        let steps = sim.run(false);

        assert!(sim.game.is_over());
        assert!(steps > 0);
    }

    #[cfg(feature = "logging")]
    #[test]
    fn run_with_logging_records_a_game_log() {
        let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        let mut sim = Simulator::new(game, RandomAgent::new(Some(1)), RandomAgent::new(Some(2)));

        sim.run(true);

        let log = sim.game.state.log.as_ref().expect("logging was enabled");
        assert!(!log.is_empty());
    }

    /// Seed 69221 reaches a reaction window where Dorinthea stacks four attack
    /// reactions and Rhinar answers with two Dodges — six layers at once, which
    /// overflowed the stack back when it held only five.
    #[test]
    fn random_game_with_a_six_layer_stack_finishes() {
        let seed = 69221;
        let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
        let mut sim = Simulator::new(game, RandomAgent::new(Some(seed + 1)), RandomAgent::new(Some(seed + 2)));
        sim.run(false);
        assert!(sim.game.is_over());
    }

    /// Random agents explore far more of the action space than scripted tests,
    /// so sweep many seeds to catch states where the engine deadlocks (a
    /// non-terminal state with no legal actions) or panics mid-game.
    #[test]
    fn random_games_terminate_across_many_seeds() {
        for seed in 0..500u64 {
            let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
            let mut sim = Simulator::new(game, RandomAgent::new(Some(seed + 1)), RandomAgent::new(Some(seed + 2)));
            sim.run(false);
            assert!(sim.game.is_over(), "seed {} deadlocked (no legal actions before terminal state)", seed);
        }
    }

    /// Play the random game for `seed` (the same seeding the other sweeps use)
    /// to completion and return the deepest the stack got.
    fn max_stack_depth(seed: u64) -> usize {
        let game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
        let mut sim = Simulator::new(game, RandomAgent::new(Some(seed + 1)), RandomAgent::new(Some(seed + 2)));
        sim.game.reset(false);
        let mut deepest = 0;
        while !sim.game.is_over() {
            let legal = sim.game.legal_actions();
            assert!(!legal.is_empty(), "seed {} deadlocked", seed);
            let action = if sim.game.player_to_act() == 0 {
                sim.p1_agent.select_action(&sim.game, &legal)
            } else {
                sim.p2_agent.select_action(&sim.game, &legal)
            };
            sim.game.step(action);
            deepest = deepest.max(sim.game.state.stack.len());
        }
        deepest
    }

    /// Seed 1995699 is the first of 20 million random games to reach seven
    /// layers at once — the deepest the sweep found, and so what `STACK_SIZE`
    /// is sized to. It must finish without overflowing the stack. (The seed
    /// moves whenever a rules change alters random play; re-run
    /// `stack_depth_histogram` with 20,000,000 games to find the new one.)
    #[test]
    fn random_game_with_a_seven_layer_stack_finishes() {
        use crate::game_state::STACK_SIZE;
        let deepest = max_stack_depth(1995699);
        assert_eq!(deepest, 7);
        assert!(deepest <= STACK_SIZE);
    }

    /// Random play across many seeds never overflows the stack (that would
    /// panic in `push_to_stack`, failing the sweep outright).
    #[test]
    fn random_games_stay_within_the_stack_size() {
        use crate::game_state::STACK_SIZE;
        let deepest = (0..2_000u64).map(max_stack_depth).max().unwrap();
        assert!(deepest <= STACK_SIZE, "random play reached {} stack layers (STACK_SIZE {})", deepest, STACK_SIZE);
    }

    /// A much larger sweep that prints how deep the stack gets, for checking
    /// `STACK_SIZE` headroom. Slow, so ignored by default; run with
    /// `cargo test --release stack_depth_histogram -- --ignored --nocapture`
    /// (`STACK_SWEEP_GAMES` sets the game count, default 1,000,000; 20,000,000
    /// reaches the seven-layer game `STACK_SIZE` is sized to).
    #[test]
    #[ignore]
    fn stack_depth_histogram() {
        use crate::game_state::STACK_SIZE;
        let games: u64 = std::env::var("STACK_SWEEP_GAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(1_000_000);
        let mut hist = [0u64; STACK_SIZE + 1];
        let mut deepest_seed = 0;
        for seed in 0..games {
            let d = max_stack_depth(seed);
            if d > hist.iter().rposition(|&c| c > 0).unwrap_or(0) {
                deepest_seed = seed;
            }
            hist[d] += 1;
        }
        println!("max stack depth per game over {} random games (STACK_SIZE {}):", games, STACK_SIZE);
        for (depth, count) in hist.iter().enumerate().filter(|(_, c)| **c > 0) {
            println!("  {:>2} layers: {}", depth, count);
        }
        println!("first game to reach the deepest: seed {}", deepest_seed);
    }
}

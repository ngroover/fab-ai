use crate::agents::Agent;

/// A two-player, alternating-decision game the simulator can drive. Anything
/// that can report its legal actions, apply one with a step function, and say
/// when it is finished can be simulated — the FaB `Gamestate` implements this
/// (see `fab_game`), but another game with its own state and action types can
/// be dropped in unchanged.
pub trait Game {
    /// The action type agents choose between. `Copy` so a chosen action can be
    /// handed to `step` straight out of the legal-actions list.
    type Action: Copy;

    /// Reset to a fresh, playable game. `logging` opts into whatever game log
    /// the implementation keeps; implementations without one may ignore it.
    fn reset(&mut self, logging: bool);

    /// The actions the player to act may legally take right now. Empty only in
    /// a terminal state.
    fn legal_actions(&self) -> Vec<Self::Action>;

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
    use crate::fab_game::gamestate_from_decklists;

    #[test]
    fn run_plays_a_random_game_to_a_terminal_state() {
        let game = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        let mut sim = Simulator::new(game, RandomAgent::new(Some(1)), RandomAgent::new(Some(2)));

        let steps = sim.run(false);

        assert!(sim.game.is_over());
        assert!(steps > 0);
    }

    #[test]
    fn run_resets_so_a_finished_simulator_can_run_again() {
        let game = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        let mut sim = Simulator::new(game, RandomAgent::new(Some(1)), RandomAgent::new(Some(2)));

        sim.run(false);
        let steps = sim.run(false);

        assert!(sim.game.is_over());
        assert!(steps > 0);
    }

    #[test]
    fn run_with_logging_records_a_game_log() {
        let game = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        let mut sim = Simulator::new(game, RandomAgent::new(Some(1)), RandomAgent::new(Some(2)));

        sim.run(true);

        let log = sim.game.log.as_ref().expect("logging was enabled");
        assert!(!log.is_empty());
    }

    /// Random agents explore far more of the action space than scripted tests,
    /// so sweep many seeds to catch states where the engine deadlocks (a
    /// non-terminal state with no legal actions) or panics mid-game.
    #[test]
    fn random_games_terminate_across_many_seeds() {
        for seed in 0..500u64 {
            let game = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
            let mut sim = Simulator::new(game, RandomAgent::new(Some(seed + 1)), RandomAgent::new(Some(seed + 2)));
            sim.run(false);
            assert!(sim.game.is_over(), "seed {} deadlocked (no legal actions before terminal state)", seed);
        }
    }
}

//! Tutor mode (`environment play --tutor`): a human plays, and at every
//! decision the trained network searches the same position. When the human's
//! choice differs from the network's, the human is told, the mistake is
//! counted, and the game goes on with the network's move instead. The tally is
//! reported at the end of the game.

use std::io::Write;

use crate::action::Action;
use crate::agents::Agent;
use crate::display::describe_action;
use crate::fab_game::FabGame;

/// A human seat checked by a teacher agent. `H` is the human, `T` the teacher
/// (the network search in a real game), and `O` where feedback is written.
pub struct Tutor<H: Agent<FabGame>, T: Agent<FabGame>, O: Write> {
    pub human: H,
    teacher: T,
    out: O,
    /// Decisions the human was asked to make.
    pub decisions: u32,
    /// Decisions where the human's choice differed from the teacher's.
    pub mistakes: u32,
}

impl<H: Agent<FabGame>, T: Agent<FabGame>, O: Write> Tutor<H, T, O> {
    pub fn new(human: H, teacher: T, out: O) -> Self {
        Tutor { human, teacher, out, decisions: 0, mistakes: 0 }
    }

    /// The end-of-game report: mistakes out of decisions, and the share of
    /// decisions that matched the teacher.
    pub fn score_line(&self) -> String {
        let correct = self.decisions - self.mistakes;
        let percent = if self.decisions == 0 { 100.0 } else { 100.0 * correct as f64 / self.decisions as f64 };
        format!(
            "Tutor score: {} mistake{} in {} decision{} - {}/{} correct ({:.0}%).",
            self.mistakes,
            if self.mistakes == 1 { "" } else { "s" },
            self.decisions,
            if self.decisions == 1 { "" } else { "s" },
            correct,
            self.decisions,
            percent,
        )
    }
}

impl<H: Agent<FabGame>, T: Agent<FabGame>, O: Write> Agent<FabGame> for Tutor<H, T, O> {
    fn select_action(&mut self, game: &FabGame, legal_actions: &[Action]) -> Action {
        let chosen = self.human.select_action(game, legal_actions);
        if legal_actions.len() == 1 {
            // No real choice to grade (the game normally plays these itself).
            return chosen;
        }
        let _ = writeln!(self.out, "  (tutor is checking your move...)");
        let _ = self.out.flush();
        let best = self.teacher.select_action(game, legal_actions);
        self.decisions += 1;
        let gs = &game.state;
        if chosen == best {
            let _ = writeln!(self.out, "  Tutor: correct - the network agrees with {}.", describe_action(gs, &chosen));
            return chosen;
        }
        self.mistakes += 1;
        let _ = writeln!(self.out, "  Tutor: WRONG MOVE (mistake #{}).", self.mistakes);
        let _ = writeln!(self.out, "    You chose:         {}", describe_action(gs, &chosen));
        let _ = writeln!(self.out, "    The network plays: {}", describe_action(gs, &best));
        let _ = writeln!(self.out, "    The game continues with the network's move.");
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::RandomAgent;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::simulator::{Game, Simulator};

    /// Always picks the legal action at a fixed position (clamped).
    struct Pick(usize);
    impl Agent<FabGame> for Pick {
        fn select_action(&mut self, _game: &FabGame, legal: &[Action]) -> Action {
            legal[self.0.min(legal.len() - 1)]
        }
    }

    fn opening() -> (FabGame, Vec<Action>) {
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        game.reset(false);
        let legal = game.legal_actions();
        assert!(legal.len() > 1);
        (game, legal.to_vec())
    }

    #[test]
    fn a_matching_choice_is_kept_and_not_a_mistake() {
        let (game, legal) = opening();
        let mut out = Vec::new();
        let mut tutor = Tutor::new(Pick(0), Pick(0), &mut out);
        assert_eq!(tutor.select_action(&game, &legal), legal[0]);
        assert_eq!((tutor.decisions, tutor.mistakes), (1, 0));
        assert!(String::from_utf8(out).unwrap().contains("correct"));
    }

    #[test]
    fn a_wrong_choice_is_counted_and_replaced_by_the_teachers() {
        let (game, legal) = opening();
        let mut out = Vec::new();
        let mut tutor = Tutor::new(Pick(0), Pick(1), &mut out);
        assert_eq!(tutor.select_action(&game, &legal), legal[1]);
        assert_eq!((tutor.decisions, tutor.mistakes), (1, 1));
        assert!(tutor.score_line().contains("1 mistake in 1 decision"));
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("WRONG MOVE"), "{}", text);
    }

    #[test]
    fn a_tutored_game_finishes_and_tallies_every_decision() {
        let game = FabGame::new(build_dorinthea_deck(), build_rhinar_deck(), Some(3));
        let tutor = Tutor::new(RandomAgent::new(Some(1)), RandomAgent::new(Some(2)), std::io::sink());
        let mut sim = Simulator::new(game, tutor, RandomAgent::new(Some(4)));
        sim.run(false);
        assert!(sim.game.is_over());
        assert!(sim.p1_agent.decisions > 0);
        assert!(sim.p1_agent.mistakes <= sim.p1_agent.decisions);
    }
}

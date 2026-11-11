//! A human player at the terminal: at every decision it offers a numbered
//! menu to print the game state, print the game log, or take one of the legal
//! actions.

use std::io::Write;

use crate::action::Action;
use crate::agents::Agent;
use crate::display::{card_name, describe_action, phase_name, player_name, render_state};
use crate::fab_game::FabGame;
use crate::game_state::{Gamestate, PlayerIndex};

/// Where menu answers come from, a line at a time. `None` means the input has
/// ended. Stdin for a real game; a queue of canned lines in tests.
pub trait LineSource {
    fn next_line(&mut self) -> Option<String>;
}

/// Standard input. Each read takes the shared stdin lock only briefly, so two
/// human players can both read from the same terminal.
pub struct StdinLines;

impl LineSource for StdinLines {
    fn next_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line),
        }
    }
}

impl LineSource for std::collections::VecDeque<String> {
    fn next_line(&mut self) -> Option<String> {
        self.pop_front()
    }
}

/// Ask for a number in `1..=count` until one is given, returning it zero-based.
/// `None` when the input ends first.
pub fn prompt_choice(input: &mut impl LineSource, out: &mut impl Write, count: usize) -> Option<usize> {
    loop {
        let _ = write!(out, "Choice [1-{}]: ", count);
        let _ = out.flush();
        let line = input.next_line()?;
        match line.trim().parse::<usize>() {
            Ok(n) if (1..=count).contains(&n) => return Some(n - 1),
            _ => {
                let _ = writeln!(out, "Please enter a number from 1 to {}.", count);
            }
        }
    }
}

/// Print `title` and `options` as a numbered menu and return the index picked.
pub fn menu(input: &mut impl LineSource, out: &mut impl Write, title: &str, options: &[&str]) -> Option<usize> {
    let _ = writeln!(out, "\n{}", title);
    for (i, option) in options.iter().enumerate() {
        let _ = writeln!(out, "  {}) {}", i + 1, option);
    }
    prompt_choice(input, out, options.len())
}

/// The game log as `viewer` may read it (`None` for the omniscient log), or
/// `None` when the game keeps no log.
#[cfg(feature = "logging")]
pub fn game_log(gs: &Gamestate, viewer: Option<PlayerIndex>) -> Option<&[String]> {
    match viewer {
        Some(pid) => gs.player(pid).log.as_deref(),
        None => gs.log.as_deref(),
    }
}

/// Always `None`: the crate was built without the `logging` feature.
#[cfg(not(feature = "logging"))]
pub fn game_log(_gs: &Gamestate, _viewer: Option<PlayerIndex>) -> Option<&[String]> {
    None
}

const NO_LOG: &str = "No game log: this build has logging off. Rebuild with \
`--features logging` (or use `cargo play`).";

pub struct HumanAgent<I: LineSource, O: Write> {
    input: I,
    out: O,
    /// Show hidden information (the opponent's hand, the omniscient log).
    reveal: bool,
    /// How many log entries this player has already been shown.
    log_seen: usize,
}

impl<I: LineSource, O: Write> HumanAgent<I, O> {
    pub fn new(input: I, out: O, reveal: bool) -> Self {
        HumanAgent { input, out, reveal, log_seen: 0 }
    }

    fn viewer(&self, pid: PlayerIndex) -> Option<PlayerIndex> {
        if self.reveal { None } else { Some(pid) }
    }

    /// Print the log entries added since this player last looked, so they see
    /// what happened between their decisions. `pid` is the seat they play.
    pub fn show_new_events(&mut self, gs: &Gamestate, pid: PlayerIndex) {
        let Some(log) = game_log(gs, self.viewer(pid)) else { return };
        // A shorter log than last time means a new game has started.
        let start = if self.log_seen > log.len() { 0 } else { self.log_seen };
        for entry in &log[start..] {
            let _ = writeln!(self.out, "  | {}", entry);
        }
        self.log_seen = log.len();
    }

    fn print_log(&mut self, gs: &Gamestate, pid: PlayerIndex) {
        match game_log(gs, self.viewer(pid)) {
            None => {
                let _ = writeln!(self.out, "{}", NO_LOG);
            }
            Some(log) => {
                let _ = writeln!(self.out, "================= GAME LOG =================");
                for (i, entry) in log.iter().enumerate() {
                    let _ = writeln!(self.out, "{:>4}  {}", i + 1, entry);
                }
                let _ = writeln!(self.out, "============================================");
            }
        }
    }
}

impl<I: LineSource, O: Write> Agent<FabGame> for HumanAgent<I, O> {
    fn select_action(&mut self, game: &FabGame, legal_actions: &[Action]) -> Action {
        let gs = &game.state;
        let pid = gs.active_player;
        let _ = writeln!(self.out);
        self.show_new_events(gs, pid);
        loop {
            let _ = writeln!(
                self.out,
                "\n{} ({}), your move - turn {}, phase: {}, life {} vs {}",
                player_name(pid),
                card_name(gs.player(pid).hero),
                gs.turn_count,
                phase_name(gs.phase),
                gs.player(pid).life,
                gs.player(pid.opponent()).life,
            );
            let _ = writeln!(self.out, "  1) Print game state");
            let _ = writeln!(self.out, "  2) Print game log");
            for (i, action) in legal_actions.iter().enumerate() {
                let _ = writeln!(self.out, "  {}) {}", i + 3, describe_action(gs, action));
            }
            let Some(choice) = prompt_choice(&mut self.input, &mut self.out, legal_actions.len() + 2) else {
                let _ = writeln!(self.out, "\nInput closed - quitting.");
                std::process::exit(0);
            };
            match choice {
                0 => {
                    let _ = writeln!(self.out, "{}", render_state(gs, self.viewer(pid)));
                }
                1 => self.print_log(gs, pid),
                n => return legal_actions[n - 2],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionType;
    use crate::agents::RandomAgent;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::simulator::{Game, Simulator};
    use std::collections::VecDeque;

    fn lines(text: &[&str]) -> VecDeque<String> {
        text.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn prompt_choice_rejects_bad_input_until_a_valid_number() {
        let mut out = Vec::new();
        let choice = prompt_choice(&mut lines(&["", "abc", "0", "4", " 2 "]), &mut out, 3);
        assert_eq!(choice, Some(1));
        assert_eq!(prompt_choice(&mut lines(&[]), &mut out, 3), None);
    }

    #[test]
    fn the_menu_prints_state_and_log_before_taking_an_action() {
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        game.reset(true);
        let legal = game.legal_actions();
        let mut out = Vec::new();
        // Print the state, print the log, then take the second action.
        let mut agent = HumanAgent::new(lines(&["1", "2", "4"]), &mut out, false);
        let action = agent.select_action(&game, &legal);
        assert_eq!(action.typ, ActionType::ChooseSecond);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("GAME STATE"), "{}", text);
        assert!(text.contains("3) Go first"), "{}", text);
        assert!(text.contains("4) Go second"), "{}", text);
        #[cfg(feature = "logging")]
        assert!(text.contains("GAME LOG"), "{}", text);
    }

    /// A human who always picks the first legal action can finish a game.
    #[test]
    fn a_scripted_human_plays_a_whole_game() {
        struct AlwaysThree;
        impl LineSource for AlwaysThree {
            fn next_line(&mut self) -> Option<String> {
                Some("3".to_string())
            }
        }
        for seed in 0..20 {
            let game = FabGame::new(build_dorinthea_deck(), build_rhinar_deck(), Some(seed));
            let human = HumanAgent::new(AlwaysThree, std::io::sink(), false);
            let mut sim = Simulator::new(game, human, RandomAgent::new(Some(seed + 1)));
            sim.run(true);
            assert!(sim.game.is_over());
        }
    }
}

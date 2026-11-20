//! Game logs for `environment match --log DIR` and `environment play --log
//! FILE`: one text file per game with
//! the whole game as it happened (every hidden card shown), each agent
//! decision written in at the point it was made, and then each player's own
//! view of the game.
//!
//! A decision by a searching agent also lists what the search found for every
//! option; a network agent's adds the network's own value and policy for the
//! position, before any search.

use std::io::Write;
use std::path::PathBuf;

use crate::action::{Action, MAX_LEGAL_ACTIONS};
use crate::agents::Agent;
use crate::cli::Seat;
use crate::display::{card_name, describe_action, phase_name, player_name};
use crate::fab_game::FabGame;
use crate::game_state::{Phase, PlayerIndex};
use crate::human_agent::game_log;
use crate::network::NetworkEvaluator;
use crate::puct::{Evaluator, PuctAgent, RootStat};
use crate::simulator::Game;

/// Where a match writes its game logs, and the agents' names for them.
pub struct MatchLog {
    pub dir: PathBuf,
    /// The names of agents A and B, as given on the command line.
    pub names: [String; 2],
}

impl MatchLog {
    /// The file game `index` is written to.
    pub fn path(&self, index: u32) -> PathBuf {
        self.dir.join(format!("game_{index:04}.log"))
    }
}

/// What an agent knew when it chose: the network's raw opinion of the position
/// (network agents only) and the search's statistics (searching agents only).
/// For a human's decision reviewed by an advisor, the network's opinion and
/// search, and the action its search would have played.
#[derive(Default)]
struct Insight {
    /// The network's value for the player to act, and its policy over the
    /// legal actions (in the order of the legal-action list), summing to 1.
    network: Option<(f32, Vec<f32>)>,
    search: Vec<RootStat>,
    /// The action the advising network would have played instead.
    advice: Option<Action>,
}

/// The action a network agent plays in `game`, with the network's own value
/// and policy for the position and the search's statistics.
fn network_decision(agent: &mut PuctAgent<NetworkEvaluator>, game: &FabGame, legal: &[Action]) -> (Action, Insight) {
    // The network's judgement of the real position. It reads only the
    // mover's observation, so this tells it nothing the mover may not
    // know, and it changes nothing the search does.
    let mut priors = [0.0f32; MAX_LEGAL_ACTIONS];
    let priors = &mut priors[..legal.len()];
    let value = agent.evaluator.evaluate(game, legal, priors);
    let total: f32 = priors.iter().sum();
    let policy = priors.iter().map(|p| if total > 0.0 { p / total } else { 0.0 }).collect();
    let (action, search) = agent.choose(game, legal);
    (action, Insight { network: Some((value, policy)), search, advice: None })
}

/// Ask `seat` for its action in `game`, and what it based the choice on. A
/// human's decision is reviewed by `advisor`, if there is one, once the human
/// has chosen; the review goes only to the log.
fn decide(
    seat: &mut Seat,
    advisor: Option<&mut PuctAgent<NetworkEvaluator>>,
    game: &FabGame,
    legal: &[Action],
) -> (Action, Insight) {
    match (seat, advisor) {
        (Seat::Net(agent), _) => network_decision(agent, game, legal),
        (Seat::Puct(agent), _) => {
            let (action, search) = agent.choose(game, legal);
            (action, Insight { search, ..Insight::default() })
        }
        (seat @ Seat::Human(_), Some(advisor)) => {
            let action = seat.select_action(game, legal);
            let (advice, mut insight) = network_decision(advisor, game, legal);
            insight.advice = Some(advice);
            (action, insight)
        }
        (other, _) => (other.select_action(game, legal), Insight::default()),
    }
}

/// The lines describing a decision's insight, each indented.
fn write_insight(out: &mut dyn Write, game: &FabGame, legal: &[Action], chosen: Action, insight: &Insight) {
    let gs = &game.state;
    let mover = player_name(gs.active_player);
    if let Some(advice) = insight.advice {
        if advice == chosen {
            let _ = writeln!(out, "       the network would play the same");
        } else {
            let _ = writeln!(out, "       the network would play: {}", describe_action(gs, &advice));
        }
    }
    if let Some((value, _)) = &insight.network {
        let _ = writeln!(out, "       network value {value:+.3} for {mover} (expected score {:.1}%)",
            50.0 * (value + 1.0));
    }
    if insight.network.is_none() && insight.search.is_empty() {
        return;
    }
    let total_visits: u32 = insight.search.iter().map(|s| s.visits).sum();
    // Every legal action, most-visited first (network policy first when
    // there was no search to rank them).
    let mut rows: Vec<(usize, Option<&RootStat>)> = legal
        .iter()
        .enumerate()
        .map(|(i, action)| (i, insight.search.iter().find(|s| s.action == *action)))
        .collect();
    let policy = insight.network.as_ref().map(|(_, p)| p);
    rows.sort_by(|(i, a), (j, b)| {
        let visits = |s: &Option<&RootStat>| s.map_or(0, |s| s.visits);
        visits(b).cmp(&visits(a)).then_with(|| match policy {
            Some(p) => p[*j].total_cmp(&p[*i]),
            None => std::cmp::Ordering::Equal,
        })
    });

    let mut header = String::from("         ");
    if policy.is_some() {
        header.push_str("  policy");
    }
    if !insight.search.is_empty() {
        header.push_str(&format!("  visits (of {total_visits:>5})  search value"));
    }
    let _ = writeln!(out, "{header}  action");
    for (i, stat) in rows {
        let action = legal[i];
        let mark = if action == chosen { "*" } else { " " };
        let mut line = format!("       {mark} ");
        if let Some(p) = policy {
            line.push_str(&format!("  {:>5.1}%", 100.0 * p[i]));
        }
        if !insight.search.is_empty() {
            let (visits, value) = stat.map_or((0, 0.0), |s| (s.visits, s.value));
            let share = 100.0 * visits as f32 / total_visits.max(1) as f32;
            line.push_str(&format!("  {visits:>6} ({share:>5.1}%)       {value:>+7.3}"));
        }
        let _ = writeln!(out, "{line}  {}", describe_action(gs, &action));
    }
}

/// Copy the omniscient log entries from `*shown` on into `out`.
fn write_new_entries(out: &mut dyn Write, game: &FabGame, shown: &mut usize) {
    if let Some(log) = game_log(&game.state, None) {
        for entry in &log[(*shown).min(log.len())..] {
            let _ = writeln!(out, "  | {entry}");
        }
        *shown = log.len();
    }
}

/// Play `game` from the start with `seats[0]` as P1 and `seats[1]` as P2,
/// writing its log to `out` as it goes, and return its final phase. `labels`
/// name each seat's agent; `header` opens the log. Written decision by
/// decision, so a game cut short (a human quitting) keeps its log so far.
/// Errors writing are ignored; the caller checks the sink afterwards.
///
/// With `advisor`, every human decision is also judged by that network agent:
/// the log gets the network's value and policy for the position, its search,
/// and what it would have played. None of it is shown to the human.
pub fn play_logged(
    game: &mut FabGame,
    seats: [&mut Seat; 2],
    labels: [&str; 2],
    header: &str,
    out: &mut dyn Write,
    mut advisor: Option<&mut PuctAgent<NetworkEvaluator>>,
) -> Phase {
    let [p1_seat, p2_seat] = seats;
    let _ = writeln!(out, "{header}");
    game.reset(true);
    let hero = |game: &FabGame, pid: PlayerIndex| card_name(game.state.player(pid).hero);
    for (pid, label) in [(PlayerIndex::P1, labels[0]), (PlayerIndex::P2, labels[1])] {
        let _ = writeln!(out, "{}: {} played by {label}", player_name(pid), hero(game, pid));
    }
    let _ = writeln!(out, "\n== The whole game (all hidden information shown), with each decision ==");

    let mut shown = 0;
    let mut decisions = 0;
    while !game.is_over() {
        let legal = game.legal_actions();
        if legal.is_empty() {
            break;
        }
        write_new_entries(&mut *out, game, &mut shown);
        let mover = game.player_to_act();
        let seat: &mut Seat = if mover == 0 { &mut *p1_seat } else { &mut *p2_seat };
        let (action, insight) = decide(seat, advisor.as_deref_mut(), game, &legal);
        decisions += 1;
        let gs = &game.state;
        let _ = writeln!(out, "  >> decision {decisions}: {} ({}) - turn {}, {}, life P1 {} / P2 {}, {} options",
            player_name(gs.active_player), labels[mover], gs.turn_count, phase_name(gs.phase),
            gs.p1.life, gs.p2.life, legal.len());
        let _ = writeln!(out, "       chose: {}", describe_action(gs, &action));
        write_insight(&mut *out, game, &legal, action, &insight);
        let _ = out.flush();
        game.step(action);
    }
    write_new_entries(&mut *out, game, &mut shown);

    for pid in [PlayerIndex::P1, PlayerIndex::P2] {
        let _ = writeln!(out, "\n== {}'s view ({}, {}) ==", player_name(pid), hero(game, pid), labels[pid.index()]);
        if let Some(log) = game_log(&game.state, Some(pid)) {
            for entry in log {
                let _ = writeln!(out, "  | {entry}");
            }
        }
    }

    let gs = &game.state;
    let result = match gs.phase {
        Phase::Player1Win => format!("P1 ({}, {}) wins", hero(game, PlayerIndex::P1), labels[0]),
        Phase::Player2Win => format!("P2 ({}, {}) wins", hero(game, PlayerIndex::P2), labels[1]),
        Phase::Draw => "Draw".to_string(),
        _ => "The game stopped before finishing".to_string(),
    };
    let _ = writeln!(out, "\n== Result ==\n{result}. Final life: P1 {} - P2 {} after {} turns, {decisions} decisions.",
        gs.p1.life, gs.p2.life, gs.turn_count);
    let _ = out.flush();
    gs.phase
}

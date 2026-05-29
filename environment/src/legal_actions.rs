use crate::game_state::{Gamestate, Phase, Player, CardLocation};
use crate::action::{Action, ActionType};
use crate::cards::{Card, CardType};
use crate::classic_battles::get_card_catalog;

pub fn legal_actions(gs: &Gamestate) -> Vec<Action> {
    match gs.phase {
        Phase::ChooseFirst => {
            let mut actions = Vec::new();
            actions.push(Action{
                        typ : ActionType::ChooseFirst,
                        index: 0,
                        location: None});
            actions.push(Action{
                        typ : ActionType::ChooseSecond,
                        index: 0,
                        location: None});
            actions
        },
        Phase::Action => legal_action_phase(gs),
        Phase::Start => Vec::new()
    }
}

fn legal_action_phase(gs: &Gamestate) -> Vec<Action> {
    if gs.active_player == 0 {
        get_playable_cards(&gs.p1)
    }
    else {
        get_playable_cards(&gs.p2)
    }
}

/// Legal `PlayCard` actions for the cards in `player`'s hand during the action
/// phase.
///
/// Mirrors the hand-card section of `actions.py::legal_attack_actions`: a card
/// is playable when the player has an action point and either already has
/// enough resource points for its cost, or the *rest* of the hand can pitch
/// enough to cover the shortfall. Duplicate copies of the same card collapse to
/// a single action since picking either copy is the same choice.
///
/// Scope: hand cards only. Arsenal, weapon, equipment and the always-legal
/// `Pass` are handled elsewhere, and additional discard/reveal costs are not
/// considered yet.
fn get_playable_cards(player: &Player) -> Vec<Action> {
    let catalog = get_card_catalog();
    let mut actions: Vec<Action> = Vec::new();

    // Playing a card from hand costs an action point; with none left there is
    // nothing to play.
    if player.action_points < 1 {
        return actions;
    }

    // Collect each hand card's slot index (needed to address the card in an
    // Action) alongside its card identity, by scanning for cards located in the
    // hand zone.
    let hand: Vec<(usize, Card)> = player
        .cards
        .iter()
        .enumerate()
        .filter(|(_, cs)| cs.location == CardLocation::Hand)
        .map(|(idx, cs)| (idx, cs.card))
        .collect();

    // Total pitch available across the whole hand. A card's own pitch is
    // subtracted below so we only count what the *rest* of the hand can pay.
    let total_pitch: u8 = hand
        .iter()
        .map(|(_, card)| catalog[*card as usize].pitch)
        .sum();

    let mut seen: Vec<Card> = Vec::new();
    for (idx, card) in &hand {
        let data = &catalog[*card as usize];

        // Reactions are only legal in the reaction step; mentors and resources
        // are never freely played from hand during the action phase.
        if !is_action_phase_playable(data.typ) {
            continue;
        }

        // Duplicate copies are an identical choice — emit only the first.
        if seen.contains(card) {
            continue;
        }
        seen.push(*card);

        // Cost still owed after spending banked resource points.
        let needed = data.cost.saturating_sub(player.resources);

        // Free to play, or the remaining hand can pitch enough to cover it.
        let other_pitch = total_pitch - data.pitch;
        if needed == 0 || other_pitch >= needed {
            actions.push(Action {
                typ: ActionType::PlayCard,
                index: *idx,
                location: Some(CardLocation::Hand),
            });
        }
    }

    actions
}

/// Whether a card of the given type can be freely played from hand during the
/// action phase. Excludes reactions (played in the reaction step) and
/// mentors/resources (never freely playable).
fn is_action_phase_playable(typ: CardType) -> bool {
    !matches!(
        typ,
        CardType::AttackReaction
            | CardType::DefenseReaction
            | CardType::Mentor
            | CardType::Resource
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset};
    use crate::fab_step::step;

    #[test]
    fn legal_actions_in_choose_first_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let actions = legal_actions(&gs);

        assert_eq!(actions.len(), 2);
        let types : Vec<ActionType> = actions.iter().map(|x| x.typ).collect();
        assert!(types.contains(&ActionType::ChooseFirst));
        assert!(types.contains(&ActionType::ChooseSecond));
    }

    #[test]
    fn legal_actions_in_action_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action{ typ: ActionType::ChooseFirst, index : 0, location: None};
        step(&mut gs, go_first);

        let actions = legal_actions(&gs);

        assert_eq!(actions.len(), 7);
        assert_eq!(actions[0].typ, ActionType::PlayCard);
        assert_eq!(actions[1].typ, ActionType::PlayCard);
        assert_eq!(actions[2].typ, ActionType::PlayCard);
        assert_eq!(actions[3].typ, ActionType::PlayCard);
    }

    /// Hand-only playability: with an action point and a drawn hand, every
    /// distinct, free-to-pitch attack/action card yields a `PlayCard` action
    /// pointing at its slot in the `Hand`.
    #[test]
    fn playable_hand_cards_during_action_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs);

        let go_first = Action { typ: ActionType::ChooseFirst, index: 0, location: None };
        step(&mut gs, go_first);

        // Grant the active player an action point so hand cards are reachable.
        // (Turn-start resource/action-point setup is implemented elsewhere.)
        gs.p1.action_points = 1;

        let actions = get_playable_cards(&gs.p1);

        assert!(!actions.is_empty());
        for a in &actions {
            assert_eq!(a.typ, ActionType::PlayCard);
            assert_eq!(a.location, Some(CardLocation::Hand));
            assert_eq!(gs.p1.cards[a.index].location, CardLocation::Hand);
        }

        // No action points → no playable hand cards.
        gs.p1.action_points = 0;
        assert!(get_playable_cards(&gs.p1).is_empty());
    }
}

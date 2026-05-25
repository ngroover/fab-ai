use crate::cards::Card;
use crate::classic_battles::get_card_catalog;
use crate::game_state::{CardLocation, CardState, CardVisibleState, Gamestate, Player};

/// Build a `Gamestate` from two decklists.
///
/// Decklist layout (46 entries each):
///   [0]      hero card
///   [1..=5]  weapon + equipment
///   [6..=45] the 40 playable deck cards
pub fn gamestate_from_decklists(p1_deck: [Card; 46], p2_deck: [Card; 46]) -> Gamestate {
    Gamestate {
        p1: player_from_decklist(p1_deck),
        p2: player_from_decklist(p2_deck),
        active_player: 0,
    }
}

fn player_from_decklist(deck: [Card; 46]) -> Player {
    let catalog = get_card_catalog();
    let hero = deck[0];
    let hero_data = &catalog[hero as usize];

    // Slots 1..=45 (weapon, equipment, playable cards) fill Player.cards.
    let cards = std::array::from_fn(|i| CardState {
        visible: CardVisibleState::HIDDEN,
        location: CardLocation::DECK,
        card: deck[i + 1],
        next_card: 0,
        prev_card: 0,
    });

    Player {
        life: hero_data.hero_life,
        intellect: hero_data.hero_intellect,
        hero,
        cards,
        resources: 0,
        action_points: 0,
    }
}

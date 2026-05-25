use crate::action::Action;
use crate::cards::{Card, CardType};
use crate::classic_battles::get_card_catalog;
use crate::game_state::{CardLocation, CardState, CardVisibleState, Gamestate, Player, Phase};
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;

/// Build a `Gamestate` from two decklists.
/// Pass `Some(seed)` for a reproducible game, or `None` for a random seed.
pub fn gamestate_from_decklists(p1_deck: [Card; 46], p2_deck: [Card; 46], seed: Option<u64>) -> Gamestate {
    let mut rng: SmallRng = match seed {
        Some(s) => SmallRng::seed_from_u64(s),
        None => rand::make_rng(),
    };
    let active_player = rng.random_range(0u8..2);
    Gamestate {
        p1: player_from_decklist(p1_deck),
        p2: player_from_decklist(p2_deck),
        active_player,
        phase: Phase::Start,
        rng,
    }
}

fn player_from_decklist(deck: [Card; 46]) -> Player {
    let catalog = get_card_catalog();
    let mut hero_opt: Option<Card> = None;
    let mut life = 0u8;
    let mut intellect = 0u8;
    let mut card_states: Vec<CardState> = Vec::with_capacity(45);

    for card in deck {
        let data = &catalog[card as usize];
        match data.typ {
            CardType::Hero => {
                hero_opt = Some(card);
                life = data.hero_life;
                intellect = data.hero_intellect;
            }
            CardType::Weapon | CardType::Sword2h | CardType::Club2h => {
                card_states.push(CardState {
                    visible: CardVisibleState::Hidden,
                    location: CardLocation::Weapon,
                    card,
                    next_card: 0,
                    prev_card: 0,
                });
            }
            CardType::Equipment => {
                card_states.push(CardState {
                    visible: CardVisibleState::Hidden,
                    location: CardLocation::EquipmentZone,
                    card,
                    next_card: 0,
                    prev_card: 0,
                });
            }
            _ => {
                card_states.push(CardState {
                    visible: CardVisibleState::Hidden,
                    location: CardLocation::Deck,
                    card,
                    next_card: 0,
                    prev_card: 0,
                });
            }
        }
    }

    let hero = hero_opt.expect("no hero in decklist");
    let cards: [CardState; 45] = card_states
        .try_into()
        .unwrap_or_else(|_| panic!("expected exactly 45 non-hero cards"));

    Player {
        life,
        intellect,
        hero,
        cards,
        resources: 0,
        action_points: 0,
    }
}


pub fn reset(gs: &mut Gamestate) {
    gs.phase = Phase::ChooseFirst;
}

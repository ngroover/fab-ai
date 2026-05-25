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

    place_cards(gs);
    //shuffle_decks(gs);
}

pub fn shuffle_decks(gs: &mut Gamestate) {
}

pub fn place_cards(gs: &mut Gamestate) {
    let catalog = get_card_catalog();
    for player in [&mut gs.p1, &mut gs.p2] {
        for card in player.cards.iter_mut() {
            let data = &catalog[card.card as usize];
            match data.typ {
                CardType::Equipment => {
                    card.location = CardLocation::EquipmentZone;
                    card.visible = CardVisibleState::BothKnow;
                }
                CardType::Club2h | CardType::Sword2h => {
                    card.location = CardLocation::Weapon;
                    card.visible = CardVisibleState::BothKnow;
                }
                CardType::Hero => {
                    // do nothing
                }
                _ => {
                    // everything else
                }
            }
        }
    }
}

// helper function to get the CardStates
pub fn get_card_states_from_card(p: &Player, card: Card) -> Vec<CardState> {
    let mut vec = Vec::<CardState>::new();
    for cardstate in p.cards.iter() {
        if cardstate.card == card {
            vec.push(*cardstate);
        }
    }
    vec
}

// helper function to get the CardStates
pub fn get_card_states_from_location(p: &Player, location: CardLocation) -> Vec<CardState> {
    let mut vec = Vec::<CardState>::new();
    for cardstate in p.cards.iter() {
        if cardstate.location == location {
            vec.push(*cardstate);
        }
    }
    vec
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset};

    #[test]
    fn test_reset_cards() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), None);
        reset(&mut gs);

        // check bonebasher on p1
        let bb = get_card_states_from_card(&gs.p1, Card::BoneBasher);

        assert_eq!(bb.len(), 1);
        assert_eq!(bb[0].location, CardLocation::Weapon);
        assert_eq!(bb[0].visible, CardVisibleState::BothKnow);

        // check dawnblade on p2
        let db = get_card_states_from_card(&gs.p2, Card::Dawnblade);

        assert_eq!(db.len(), 1);
        assert_eq!(db[0].location, CardLocation::Weapon);
        assert_eq!(db[0].visible, CardVisibleState::BothKnow);

        // check one piece of equipment from rhinar
        let ih = get_card_states_from_card(&gs.p1, Card::IronhideLegs);

        assert_eq!(ih.len(), 1);
        assert_eq!(ih[0].location, CardLocation::EquipmentZone);
        assert_eq!(ih[0].visible, CardVisibleState::BothKnow);

        // check one piece of equipment from dorinthea
        let ih = get_card_states_from_card(&gs.p2, Card::IronrotLegs);

        assert_eq!(ih.len(), 1);
        assert_eq!(ih[0].location, CardLocation::EquipmentZone);
        assert_eq!(ih[0].visible, CardVisibleState::BothKnow);

        // check deck from rhinar 
        let rhinardeck = get_card_states_from_location(&gs.p1, CardLocation::Deck);

        assert_eq!(rhinardeck.len(), 40);

        // check deck from dorinthea
        let dorintheadeck = get_card_states_from_location(&gs.p2, CardLocation::Deck);

        assert_eq!(dorintheadeck.len(), 40);
    }
}

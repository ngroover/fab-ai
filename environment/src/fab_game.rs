use crate::action::Action;
use crate::cards::{Card, CardType};
use crate::classic_battles::get_card_catalog;
use crate::game_state::{CardLocation, CardState, CardVisibleState, Gamestate, Player, Phase};
use rand::RngExt;
use rand::rngs::SmallRng;

/// Build a `Gamestate` from two decklists.
pub fn gamestate_from_decklists(p1_deck: [Card; 46], p2_deck: [Card; 46]) -> Gamestate {
    let mut rng: SmallRng = rand::make_rng();
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

    place_equipment(gs);
}

pub fn place_equipment(gs: &mut Gamestate) {
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
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{gamestate_from_decklists,reset};

    #[test]
    fn legal_actions_in_choose_first_phase() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck());
        reset(&mut gs);

        assert_eq!(gs.phase as u8, Phase::ChooseFirst as u8);

        let catalog = get_card_catalog();
        for player in [&gs.p1, &gs.p2] {
            let mut equipment_count = 0;
            let mut weapon_count = 0;
            for card in player.cards.iter() {
                match catalog[card.card as usize].typ {
                    CardType::Equipment => {
                        assert_eq!(card.location as u8, CardLocation::EquipmentZone as u8);
                        assert_eq!(card.visible as u8, CardVisibleState::BothKnow as u8);
                        equipment_count += 1;
                    }
                    CardType::Club2h | CardType::Sword2h => {
                        assert_eq!(card.location as u8, CardLocation::Weapon as u8);
                        assert_eq!(card.visible as u8, CardVisibleState::BothKnow as u8);
                        weapon_count += 1;
                    }
                    _ => {}
                }
            }
            assert!(equipment_count > 0, "expected equipment to be placed in the equipment zone");
            assert!(weapon_count > 0, "expected a two-handed weapon to be placed in the weapon zone");
        }
    }
}

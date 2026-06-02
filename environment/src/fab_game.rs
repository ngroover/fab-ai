use crate::action::Action;
use crate::cards::{Card, CardType, EquipmentSlot};
use crate::classic_battles::get_card_catalog;
use crate::game_state::{CardLocation, CardState, CardVisibleState, Gamestate, Player, Phase};
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

/// The `CardLocation` a piece of worn equipment occupies, derived from its
/// armor slot. Only called for `CardType::Equipment`, which always carries one
/// of the four armor slots; weapons are placed in `Weapon` via their card type,
/// so anything else falls back to `Weapon`.
fn equipment_zone(slot: &Option<EquipmentSlot>) -> CardLocation {
    match slot {
        Some(EquipmentSlot::Head) => CardLocation::Head,
        Some(EquipmentSlot::Chest) => CardLocation::Chest,
        Some(EquipmentSlot::Arms) => CardLocation::Arms,
        Some(EquipmentSlot::Legs) => CardLocation::Legs,
        Some(EquipmentSlot::Weapon) | None => CardLocation::Weapon,
    }
}

/// Build a `Gamestate` from two decklists.
/// Pass `Some(seed)` for a reproducible game, or `None` for a random seed.
pub fn gamestate_from_decklists(p1_deck: [Card; 46], p2_deck: [Card; 46], seed: Option<u64>) -> Gamestate {
    let mut rng: SmallRng = match seed {
        Some(s) => SmallRng::seed_from_u64(s),
        None => rand::make_rng(),
    };
    Gamestate {
        p1: player_from_decklist(p1_deck),
        p2: player_from_decklist(p2_deck),
        active_player: 0,
        phase: Phase::Start,
        rng,
        pending_card: None,
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
                    location: equipment_zone(&data.slot),
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
        top_deck_idx: None,
        bottom_deck_idx: None,
        pitch_idx: None,
        arsenal_idx: None,
        hand_idx: None,
        banish_idx : None,
        weapon_idx : None,
        head_idx : None,
        chest_idx : None,
        arms_idx : None,
        legs_idx : None,
        hand_size : 0,
        deck_size : 0,
    }
}


pub fn reset(gs: &mut Gamestate) {
    gs.phase = Phase::ChooseFirst;

    place_cards(gs);
    shuffle_decks(gs);
    set_life_and_intellect(gs);

    gs.active_player = gs.rng.random_range(0u8..2);
}


pub fn set_life_and_intellect(gs: &mut Gamestate) {
    let catalog = get_card_catalog();
    for player in [&mut gs.p1, &mut gs.p2] {
        player.life = catalog[player.hero as usize].hero_life;
        player.intellect = catalog[player.hero as usize].hero_intellect;
    }
}

pub fn shuffle_decks(gs: &mut Gamestate) {
    for player in [&mut gs.p1, &mut gs.p2] {
        let mut cards_in_deck : Vec<usize> = player.cards.iter().
                            enumerate().
                            filter(|(_,p)| p.location == CardLocation::Deck).
                            map(|(i,_)| i).
                            collect();
        cards_in_deck.shuffle(&mut gs.rng);

        if cards_in_deck.len() > 0 {
            player.top_deck_idx = cards_in_deck.first().copied().map(|x| x as u8);
            player.bottom_deck_idx = cards_in_deck.last().copied().map(|x| x as u8);
            for (i,c) in cards_in_deck.iter().copied().enumerate() {
                let prev = if i > 0 { cards_in_deck[i-1] } else { c } ;
                let next = if i < cards_in_deck.len()-1 { cards_in_deck[i+1] } else { c };
                player.cards[c].prev_card = prev as u8;
                player.cards[c].next_card = next as u8;
            }
        }
    }
}

pub fn place_cards(gs: &mut Gamestate) {
    let catalog = get_card_catalog();
    for player in [&mut gs.p1, &mut gs.p2] {
        player.hand_size = 0;
        player.deck_size = 0;
        // Reset the worn-equipment / weapon slots; they are repopulated below.
        player.weapon_idx = None;
        player.head_idx = None;
        player.chest_idx = None;
        player.arms_idx = None;
        player.legs_idx = None;
        // Index iteration so we can record each card's slot position alongside
        // mutating its CardState.
        for idx in 0..player.cards.len() {
            let data = &catalog[player.cards[idx].card as usize];
            match data.typ {
                CardType::Equipment => {
                    player.cards[idx].location = equipment_zone(&data.slot);
                    player.cards[idx].visible = CardVisibleState::BothKnow;
                    match data.slot {
                        Some(EquipmentSlot::Head) => player.head_idx = Some(idx as u8),
                        Some(EquipmentSlot::Chest) => player.chest_idx = Some(idx as u8),
                        Some(EquipmentSlot::Arms) => player.arms_idx = Some(idx as u8),
                        Some(EquipmentSlot::Legs) => player.legs_idx = Some(idx as u8),
                        _ => {}
                    }
                }
                CardType::Weapon | CardType::Club2h | CardType::Sword2h => {
                    player.cards[idx].location = CardLocation::Weapon;
                    player.cards[idx].visible = CardVisibleState::BothKnow;
                    player.weapon_idx = Some(idx as u8);
                }
                CardType::Hero => {
                    // do nothing
                }
                _ => {
                    player.cards[idx].location = CardLocation::Deck;
                    player.cards[idx].visible = CardVisibleState::Hidden;
                    player.deck_size += 1;
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
        assert_eq!(ih[0].location, CardLocation::Legs);
        assert_eq!(ih[0].visible, CardVisibleState::BothKnow);

        // check one piece of equipment from dorinthea
        let ih = get_card_states_from_card(&gs.p2, Card::IronrotLegs);

        assert_eq!(ih.len(), 1);
        assert_eq!(ih[0].location, CardLocation::Legs);
        assert_eq!(ih[0].visible, CardVisibleState::BothKnow);

        // weapon / equipment slot indices point at the right cards
        assert_eq!(gs.p1.cards[gs.p1.weapon_idx.unwrap() as usize].card, Card::BoneBasher);
        assert_eq!(gs.p1.cards[gs.p1.legs_idx.unwrap() as usize].card, Card::IronhideLegs);
        assert_eq!(gs.p2.cards[gs.p2.weapon_idx.unwrap() as usize].card, Card::Dawnblade);
        assert_eq!(gs.p2.cards[gs.p2.arms_idx.unwrap() as usize].card, Card::GallantryGold);

        // check deck from rhinar
        let rhinardeck = get_card_states_from_location(&gs.p1, CardLocation::Deck);

        assert_eq!(rhinardeck.len(), 40);
        assert_eq!(gs.p1.deck_size, 40);

        // check deck from dorinthea
        let dorintheadeck = get_card_states_from_location(&gs.p2, CardLocation::Deck);

        assert_eq!(dorintheadeck.len(), 40);
        assert_eq!(gs.p2.deck_size, 40);
    }
}

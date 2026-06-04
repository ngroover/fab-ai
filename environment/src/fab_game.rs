use crate::action::Action;
use crate::cards::{Card, CardType, EquipmentSlot};
use crate::classic_battles::get_card_catalog;
use crate::game_state::{
    CardLocation, CardState, CardVisibleState, Gamestate, Player, Phase, PLAYER_CARDS, TOTAL_CARDS,
};
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

/// The `CardLocation` a piece of worn equipment occupies, derived from its
/// armor slot and the owning player `pid`. Only called for
/// `CardType::Equipment`, which always carries one of the four armor slots;
/// weapons are placed in `Weapon` via their card type, so anything else falls
/// back to that player's weapon zone.
fn equipment_zone(slot: &Option<EquipmentSlot>, pid: u8) -> CardLocation {
    match slot {
        Some(EquipmentSlot::Head) => CardLocation::head(pid),
        Some(EquipmentSlot::Chest) => CardLocation::chest(pid),
        Some(EquipmentSlot::Arms) => CardLocation::arms(pid),
        Some(EquipmentSlot::Legs) => CardLocation::legs(pid),
        Some(EquipmentSlot::Weapon) | None => CardLocation::weapon(pid),
    }
}

/// The base offset into `Gamestate::cards` for player `pid`'s cards.
fn player_base(pid: u8) -> usize {
    pid as usize * PLAYER_CARDS
}

/// Build a `Gamestate` from two decklists.
/// Pass `Some(seed)` for a reproducible game, or `None` for a random seed.
pub fn gamestate_from_decklists(p1_deck: [Card; 46], p2_deck: [Card; 46], seed: Option<u64>) -> Gamestate {
    let rng: SmallRng = match seed {
        Some(s) => SmallRng::seed_from_u64(s),
        None => rand::make_rng(),
    };

    let (p1, p1_cards) = player_from_decklist(p1_deck, 0);
    let (p2, p2_cards) = player_from_decklist(p2_deck, 1);

    // Concatenate each player's 45 card states into the single shared array:
    // player 0 first, then player 1.
    let mut all: Vec<CardState> = Vec::with_capacity(TOTAL_CARDS);
    all.extend_from_slice(&p1_cards);
    all.extend_from_slice(&p2_cards);
    let cards: [CardState; TOTAL_CARDS] = all
        .try_into()
        .unwrap_or_else(|_| panic!("expected exactly {} cards", TOTAL_CARDS));

    Gamestate {
        p1,
        p2,
        cards,
        active_player: 0,
        phase: Phase::Start,
        rng,
        stack_idx: None,
        pending_card: None,
    }
}

/// Build a player's metadata and its `PLAYER_CARDS` card states from a decklist.
/// `pid` is the player this deck belongs to; it determines the per-player
/// `CardLocation` each card starts in. The returned card states are later copied
/// into the shared `Gamestate::cards` array (and reset by `place_cards`).
fn player_from_decklist(deck: [Card; 46], pid: u8) -> (Player, [CardState; PLAYER_CARDS]) {
    let catalog = get_card_catalog();
    let mut hero_opt: Option<Card> = None;
    let mut life = 0u8;
    let mut intellect = 0u8;
    let mut card_states: Vec<CardState> = Vec::with_capacity(PLAYER_CARDS);

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
                    location: CardLocation::weapon(pid),
                    card,
                    next_card: 0,
                    prev_card: 0,
                });
            }
            CardType::Equipment => {
                card_states.push(CardState {
                    visible: CardVisibleState::Hidden,
                    location: equipment_zone(&data.slot, pid),
                    card,
                    next_card: 0,
                    prev_card: 0,
                });
            }
            _ => {
                card_states.push(CardState {
                    visible: CardVisibleState::Hidden,
                    location: CardLocation::deck(pid),
                    card,
                    next_card: 0,
                    prev_card: 0,
                });
            }
        }
    }

    let hero = hero_opt.expect("no hero in decklist");
    let cards: [CardState; PLAYER_CARDS] = card_states
        .try_into()
        .unwrap_or_else(|_| panic!("expected exactly {} non-hero cards", PLAYER_CARDS));

    let player = Player {
        pid,
        life,
        intellect,
        hero,
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
        chain_link : [None; 5],
        hand_size : 0,
        deck_size : 0,
    };

    (player, cards)
}


pub fn reset(gs: &mut Gamestate) {
    gs.phase = Phase::ChooseFirst;
    gs.stack_idx = None;

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
    // p1, p2, and the shared cards array are disjoint fields, so each call can
    // borrow the relevant player, the cards array, and the rng together.
    shuffle_deck_for(&mut gs.p1, &mut gs.cards, &mut gs.rng);
    shuffle_deck_for(&mut gs.p2, &mut gs.cards, &mut gs.rng);
}

fn shuffle_deck_for(
    player: &mut Player,
    cards: &mut [CardState; TOTAL_CARDS],
    rng: &mut SmallRng,
) {
    let base = player_base(player.pid);
    let deck_loc = CardLocation::deck(player.pid);
    let mut cards_in_deck: Vec<usize> = (base..base + PLAYER_CARDS)
        .filter(|&i| cards[i].location == deck_loc)
        .collect();
    cards_in_deck.shuffle(rng);

    if !cards_in_deck.is_empty() {
        player.top_deck_idx = cards_in_deck.first().copied().map(|x| x as u8);
        player.bottom_deck_idx = cards_in_deck.last().copied().map(|x| x as u8);
        for (i, c) in cards_in_deck.iter().copied().enumerate() {
            let prev = if i > 0 { cards_in_deck[i - 1] } else { c };
            let next = if i < cards_in_deck.len() - 1 { cards_in_deck[i + 1] } else { c };
            cards[c].prev_card = prev as u8;
            cards[c].next_card = next as u8;
        }
    }
}

pub fn place_cards(gs: &mut Gamestate) {
    place_cards_for(&mut gs.p1, &mut gs.cards);
    place_cards_for(&mut gs.p2, &mut gs.cards);
}

fn place_cards_for(player: &mut Player, cards: &mut [CardState; TOTAL_CARDS]) {
    let catalog = get_card_catalog();
    let pid = player.pid;
    let base = player_base(pid);
    player.hand_size = 0;
    player.deck_size = 0;
    // Reset the worn-equipment / weapon slots; they are repopulated below.
    player.weapon_idx = None;
    player.head_idx = None;
    player.chest_idx = None;
    player.arms_idx = None;
    player.legs_idx = None;
    player.chain_link = [None; 5];
    // Walk only this player's half of the shared array, recording each card's
    // global slot position alongside mutating its CardState.
    for idx in base..base + PLAYER_CARDS {
        let data = &catalog[cards[idx].card as usize];
        match data.typ {
            CardType::Equipment => {
                cards[idx].location = equipment_zone(&data.slot, pid);
                cards[idx].visible = CardVisibleState::BothKnow;
                match data.slot {
                    Some(EquipmentSlot::Head) => player.head_idx = Some(idx as u8),
                    Some(EquipmentSlot::Chest) => player.chest_idx = Some(idx as u8),
                    Some(EquipmentSlot::Arms) => player.arms_idx = Some(idx as u8),
                    Some(EquipmentSlot::Legs) => player.legs_idx = Some(idx as u8),
                    _ => {}
                }
            }
            CardType::Weapon | CardType::Club2h | CardType::Sword2h => {
                cards[idx].location = CardLocation::weapon(pid);
                cards[idx].visible = CardVisibleState::BothKnow;
                player.weapon_idx = Some(idx as u8);
            }
            CardType::Hero => {
                // do nothing
            }
            _ => {
                cards[idx].location = CardLocation::deck(pid);
                cards[idx].visible = CardVisibleState::Hidden;
                player.deck_size += 1;
                // everything else
            }
        }
    }
}

// helper function to get the CardStates owned by player `pid` matching `card`.
pub fn get_card_states_from_card(gs: &Gamestate, pid: u8, card: Card) -> Vec<CardState> {
    let base = player_base(pid);
    gs.cards[base..base + PLAYER_CARDS]
        .iter()
        .filter(|cs| cs.card == card)
        .copied()
        .collect()
}

// helper function to get the CardStates owned by player `pid` in `location`.
pub fn get_card_states_from_location(gs: &Gamestate, pid: u8, location: CardLocation) -> Vec<CardState> {
    let base = player_base(pid);
    gs.cards[base..base + PLAYER_CARDS]
        .iter()
        .filter(|cs| cs.location == location)
        .copied()
        .collect()
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
        let bb = get_card_states_from_card(&gs, 0, Card::BoneBasher);

        assert_eq!(bb.len(), 1);
        assert_eq!(bb[0].location, CardLocation::P1Weapon);
        assert_eq!(bb[0].visible, CardVisibleState::BothKnow);

        // check dawnblade on p2
        let db = get_card_states_from_card(&gs, 1, Card::Dawnblade);

        assert_eq!(db.len(), 1);
        assert_eq!(db[0].location, CardLocation::P2Weapon);
        assert_eq!(db[0].visible, CardVisibleState::BothKnow);

        // check one piece of equipment from rhinar
        let ih = get_card_states_from_card(&gs, 0, Card::IronhideLegs);

        assert_eq!(ih.len(), 1);
        assert_eq!(ih[0].location, CardLocation::P1Legs);
        assert_eq!(ih[0].visible, CardVisibleState::BothKnow);

        // check one piece of equipment from dorinthea
        let ih = get_card_states_from_card(&gs, 1, Card::IronrotLegs);

        assert_eq!(ih.len(), 1);
        assert_eq!(ih[0].location, CardLocation::P2Legs);
        assert_eq!(ih[0].visible, CardVisibleState::BothKnow);

        // weapon / equipment slot indices point at the right cards
        assert_eq!(gs.cards[gs.p1.weapon_idx.unwrap() as usize].card, Card::BoneBasher);
        assert_eq!(gs.cards[gs.p1.legs_idx.unwrap() as usize].card, Card::IronhideLegs);
        assert_eq!(gs.cards[gs.p2.weapon_idx.unwrap() as usize].card, Card::Dawnblade);
        assert_eq!(gs.cards[gs.p2.arms_idx.unwrap() as usize].card, Card::GallantryGold);

        // check deck from rhinar
        let rhinardeck = get_card_states_from_location(&gs, 0, CardLocation::P1Deck);

        assert_eq!(rhinardeck.len(), 40);
        assert_eq!(gs.p1.deck_size, 40);

        // check deck from dorinthea
        let dorintheadeck = get_card_states_from_location(&gs, 1, CardLocation::P2Deck);

        assert_eq!(dorintheadeck.len(), 40);
        assert_eq!(gs.p2.deck_size, 40);
    }
}

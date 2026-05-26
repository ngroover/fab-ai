use crate::cards::Card;
use rand::rngs::SmallRng;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum CardVisibleState {
    Hidden,
    SelfKnows,
    OpponentKnows,
    BothKnow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum CardLocation {
    Hand,
    Deck,
    Pitch,
    Graveyard,
    Arsenal,
    BanishZone,
    EquipmentZone,
    Weapon,
    CombatChain,
}

#[derive(Clone, Copy)]
pub struct CardState {
    pub visible :CardVisibleState,
    pub location: CardLocation,
    pub card : Card,
    pub next_card : u8,
    pub prev_card : u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Phase {
    Start,
    ChooseFirst,
}

pub struct Player {
    pub life: u8,
    pub intellect: u8,
    pub hero : Card,
    pub cards : [CardState; 45],
    pub resources: u8,
    pub action_points: u8,
    pub top_deck_idx: Option<u8>,
    pub bottom_deck_idx: Option<u8>,
    pub pitch_idx : Option<u8>,
    pub arsenal_idx : Option<u8>,
    pub hand_idx : Option<u8>,
    pub banish_idx : Option<u8>,
    pub weapon_idx : Option<u8>,
    pub head_idx : Option<u8>,
    pub chest_idx : Option<u8>,
    pub arms_idx : Option<u8>,
    pub legs_idx : Option<u8>
}

pub struct Gamestate {
    pub p1 : Player,
    pub p2 : Player,
    pub active_player : u8,
    pub phase : Phase,
    pub rng : SmallRng,
}

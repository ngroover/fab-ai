use crate::cards::Card;
use rand::rngs::SmallRng;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum CardVisibleState {
    Hidden,
    SelfKnows,
    OpponentKnows,
    BothKnow,
}

#[derive(Clone, Copy)]
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

pub struct CardState {
    pub visible :CardVisibleState,
    pub location: CardLocation,
    pub card : Card,
    pub next_card : u8,
    pub prev_card : u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
}

pub struct Gamestate {
    pub p1 : Player,
    pub p2 : Player,
    pub active_player : u8,
    pub phase : Phase,
    pub rng : SmallRng,
}

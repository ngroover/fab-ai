use crate::cards::Card;


#[derive(Clone, Copy)]
#[repr(u8)]
pub enum CardVisibleState {
    HIDDEN,
    SELF_KNOWS,
    OPPONENT_KNOWS,
    BOTH_KNOW,
}

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum CardLocation {
    HAND,
    DECK,
    PITCH,
    GRAVEYARD,
    ARSENAL,
    BANISHZONE,
    EQUIPMENT_ZONE,
    WEAPON,
    COMBAT_CHAIN,
}

pub struct CardState {
    pub visible :CardVisibleState,
    pub location: CardLocation,
    pub card : Card,
    pub next_card : u8,
    pub prev_card : u8,
}

pub struct CardArea {
    cards : [Option<Card>; 40],
    first : u8,
    last: u8,
}

impl Default for CardArea {
    fn default() -> Self {
        Self {
            cards: std::array::from_fn(|_| None),
            first: 0,
            last: 0,
        }
    }
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
}

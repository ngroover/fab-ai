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
    Action,
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
    pub legs_idx : Option<u8>,
    pub hand_size : u8,
    pub deck_size : u8,
}

pub struct Gamestate {
    pub p1 : Player,
    pub p2 : Player,
    pub active_player : u8,
    pub phase : Phase,
    pub rng : SmallRng,
}

/// Iterator over the cards in a player's hand.
///
/// The hand is stored as a singly-linked list inside `Player::cards`, starting
/// at `Player::hand_idx`. Following the `next_card` indices, the list terminates
/// when a node's `next_card` points back to its own index. This iterator yields
/// each `CardState` in order until (and including) that terminal node.
pub struct HandIter<'a> {
    cards: &'a [CardState; 45],
    current: Option<usize>,
}

impl<'a> Iterator for HandIter<'a> {
    type Item = CardState;

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.current?;
        let card = self.cards[idx];
        let next = card.next_card as usize;
        // A node whose `next_card` points at itself marks the end of the list.
        self.current = if next == idx { None } else { Some(next) };
        Some(card)
    }
}

impl Player {
    /// Iterate over the `CardState`s in this player's hand, starting at
    /// `hand_idx` and following `next_card` until a node points to itself.
    /// Yields nothing if the hand is empty (`hand_idx` is `None`).
    pub fn hand_iter(&self) -> HandIter<'_> {
        HandIter {
            cards: &self.cards,
            current: self.hand_idx.map(|i| i as usize),
        }
    }
}

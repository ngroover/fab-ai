use crate::cards::Card;
use crate::action::ActionType;
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
    Head,
    Chest,
    Arms,
    Legs,
    Weapon,
    CombatChain,
    Stack,
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
    Pitch,
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
    /// The combat chain, link by link. Each slot holds the `cards` index of the
    /// card occupying that chain link, or `None` if the link is empty. Sized at
    /// 5 since a single combat chain is very unlikely to grow longer than that;
    /// the attacking card/weapon is placed at link 0.
    pub chain_link : [Option<u8>; 5],
    pub hand_size : u8,
    pub deck_size : u8,
}

pub struct Gamestate {
    pub p1 : Player,
    pub p2 : Player,
    pub active_player : u8,
    pub phase : Phase,
    pub rng : SmallRng,
    /// Head of the stack: the linked list of cards currently on the stack
    /// waiting to resolve. Holds the slot index (into the owning player's
    /// `cards` array) of the most recently added card, or `None` when the stack
    /// is empty. The list is threaded through `CardState::next_card` /
    /// `prev_card`, newest card at the head.
    pub stack_idx : Option<u8>,
    /// The card the active player has chosen to play or activate and is now
    /// paying for. Set when leaving the `Action` phase for the `Pitch` phase;
    /// holds the slot index into the active player's `cards` array together
    /// with the location it is being played/activated from. `None` outside the
    /// pay-for-a-card flow.
    pub pending_card : Option<PendingCard>,
}

/// A card the active player has committed to play or activate, pending payment
/// during the `Pitch` phase. `index` is the slot into the active player's
/// `cards` array; `typ` is the action that committed it (`PlayCard` or
/// `Activate`), which determines how the card resolves once paid for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingCard {
    pub index : usize,
    pub typ : ActionType,
}

/// Iterator over the cards in a player's hand.
///
/// The hand is stored as a singly-linked list inside `Player::cards`, starting
/// at `Player::hand_idx`. Following the `next_card` indices, the list terminates
/// when a node's `next_card` points back to its own index. This iterator yields
/// `(slot_index, CardState)` for each card in order until (and including) that
/// terminal node, where `slot_index` is the card's position in `Player::cards`.
pub struct HandIter<'a> {
    cards: &'a [CardState; 45],
    current: Option<usize>,
}

impl<'a> Iterator for HandIter<'a> {
    type Item = (usize, CardState);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.current?;
        let card = self.cards[idx];
        let next = card.next_card as usize;
        // A node whose `next_card` points at itself marks the end of the list.
        self.current = if next == idx { None } else { Some(next) };
        Some((idx, card))
    }
}

impl Player {
    /// Iterate over this player's hand as `(slot_index, CardState)` pairs,
    /// starting at `hand_idx` and following `next_card` until a node points to
    /// itself. Yields nothing if the hand is empty (`hand_idx` is `None`).
    pub fn hand_iter(&self) -> HandIter<'_> {
        HandIter {
            cards: &self.cards,
            current: self.hand_idx.map(|i| i as usize),
        }
    }
}

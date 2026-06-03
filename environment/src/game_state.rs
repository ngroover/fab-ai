use crate::cards::Card;
use crate::action::ActionType;
use rand::rngs::SmallRng;

/// Number of (non-hero) cards each player owns. The unified `Gamestate::cards`
/// array is `2 * PLAYER_CARDS` long: player 0 owns slots `0..PLAYER_CARDS` and
/// player 1 owns slots `PLAYER_CARDS..2*PLAYER_CARDS`.
pub const PLAYER_CARDS: usize = 45;
pub const TOTAL_CARDS: usize = PLAYER_CARDS * 2;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum CardVisibleState {
    Hidden,
    P1Knows,
    P2Knows,
    BothKnow,
}

/// Where a card currently sits. Every zone is split per player (`P1*` / `P2*`)
/// so a card's owner is encoded in its location; the only shared zone is the
/// `Stack`, which both players can add to before it resolves.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum CardLocation {
    P1Hand,
    P2Hand,
    P1Deck,
    P2Deck,
    P1Pitch,
    P2Pitch,
    P1Graveyard,
    P2Graveyard,
    P1Arsenal,
    P2Arsenal,
    P1BanishZone,
    P2BanishZone,
    P1Head,
    P2Head,
    P1Chest,
    P2Chest,
    P1Arms,
    P2Arms,
    P1Legs,
    P2Legs,
    P1Weapon,
    P2Weapon,
    P1CombatChain,
    P2CombatChain,
    Stack,
}

// A complete, symmetric set of per-player zone constructors; some are not
// referenced yet as the engine is still being built out.
#[allow(dead_code)]
impl CardLocation {
    /// Per-player zone constructors. `pid` is the owning player (0 or 1); each
    /// returns that player's variant of the zone.
    pub const fn hand(pid: u8) -> Self { if pid == 0 { Self::P1Hand } else { Self::P2Hand } }
    pub const fn deck(pid: u8) -> Self { if pid == 0 { Self::P1Deck } else { Self::P2Deck } }
    pub const fn pitch(pid: u8) -> Self { if pid == 0 { Self::P1Pitch } else { Self::P2Pitch } }
    pub const fn graveyard(pid: u8) -> Self { if pid == 0 { Self::P1Graveyard } else { Self::P2Graveyard } }
    pub const fn arsenal(pid: u8) -> Self { if pid == 0 { Self::P1Arsenal } else { Self::P2Arsenal } }
    pub const fn banish(pid: u8) -> Self { if pid == 0 { Self::P1BanishZone } else { Self::P2BanishZone } }
    pub const fn head(pid: u8) -> Self { if pid == 0 { Self::P1Head } else { Self::P2Head } }
    pub const fn chest(pid: u8) -> Self { if pid == 0 { Self::P1Chest } else { Self::P2Chest } }
    pub const fn arms(pid: u8) -> Self { if pid == 0 { Self::P1Arms } else { Self::P2Arms } }
    pub const fn legs(pid: u8) -> Self { if pid == 0 { Self::P1Legs } else { Self::P2Legs } }
    pub const fn weapon(pid: u8) -> Self { if pid == 0 { Self::P1Weapon } else { Self::P2Weapon } }
    pub const fn combat_chain(pid: u8) -> Self { if pid == 0 { Self::P1CombatChain } else { Self::P2CombatChain } }
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

/// Per-player state. The cards themselves live in `Gamestate::cards`; a player
/// only holds the (global) head/tail indices into that shared array for each of
/// its zones, plus its turn resources. Player 0's cards occupy global slots
/// `0..PLAYER_CARDS`, player 1's occupy `PLAYER_CARDS..2*PLAYER_CARDS`.
pub struct Player {
    pub life: u8,
    pub intellect: u8,
    pub hero : Card,
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
    /// The combat chain, link by link. Each slot holds the global `cards` index
    /// of the card occupying that chain link, or `None` if the link is empty.
    /// Sized at 5 since a single combat chain is very unlikely to grow longer
    /// than that; the attacking card/weapon is placed at link 0.
    pub chain_link : [Option<u8>; 5],
    pub hand_size : u8,
    pub deck_size : u8,
}

pub struct Gamestate {
    pub p1 : Player,
    pub p2 : Player,
    /// All cards in the game, in one flat array shared by both players. Player 0
    /// owns slots `0..PLAYER_CARDS`, player 1 owns `PLAYER_CARDS..2*PLAYER_CARDS`.
    /// Every index stored anywhere (zone heads, `next_card`/`prev_card`, the
    /// stack, the combat chain) is a global index into this array, so the owner
    /// of any card is implied by which half it falls in.
    pub cards : [CardState; TOTAL_CARDS],
    pub active_player : u8,
    pub phase : Phase,
    pub rng : SmallRng,
    /// Head of the stack: the linked list of cards currently on the stack
    /// waiting to resolve. Holds the global slot index of the most recently
    /// added card, or `None` when the stack is empty. The list is threaded
    /// through `CardState::next_card` / `prev_card`, newest card at the head.
    /// Because indices are global, the stack can hold cards owned by either
    /// player.
    pub stack_idx : Option<u8>,
    /// The card the active player has chosen to play or activate and is now
    /// paying for. Set when leaving the `Action` phase for the `Pitch` phase;
    /// holds the global slot index into `cards` together with the location it is
    /// being played/activated from. `None` outside the pay-for-a-card flow.
    pub pending_card : Option<PendingCard>,
}

/// A card the active player has committed to play or activate, pending payment
/// during the `Pitch` phase. `index` is the global slot into `Gamestate::cards`;
/// `typ` is the action that committed it (`PlayCard` or `Activate`), which
/// determines how the card resolves once paid for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingCard {
    pub index : usize,
    pub typ : ActionType,
}

/// Iterator over the cards in a player's hand.
///
/// The hand is stored as a singly-linked list inside `Gamestate::cards`,
/// starting at `Player::hand_idx`. Following the `next_card` indices, the list
/// terminates when a node's `next_card` points back to its own index. This
/// iterator yields `(slot_index, CardState)` for each card in order until (and
/// including) that terminal node, where `slot_index` is the card's global
/// position in `Gamestate::cards`.
pub struct HandIter<'a> {
    cards: &'a [CardState; TOTAL_CARDS],
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
    /// itself. `cards` is the shared `Gamestate::cards` array the indices refer
    /// into. Yields nothing if the hand is empty (`hand_idx` is `None`).
    pub fn hand_iter<'a>(&self, cards: &'a [CardState; TOTAL_CARDS]) -> HandIter<'a> {
        HandIter {
            cards,
            current: self.hand_idx.map(|i| i as usize),
        }
    }
}

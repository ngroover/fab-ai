use crate::cards::Card;
use crate::action::ActionType;
use rand::rngs::SmallRng;

/// A global index into `Gamestate::cards`. Wrapping the raw `u8` in a newtype
/// keeps card indices from being silently confused with the many other small
/// integers in the engine that share the same representation — counts
/// (`hand_size`, `deck_size`), player ids (`pid`, `active_player`), or pitch
/// values. Construct with `CardIdx::new` from a `usize` slot; read back the
/// `usize` with `get` when indexing `Gamestate::cards`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct CardIdx(pub u8);

impl CardIdx {
    /// Wrap a `usize` slot position. The shared `cards` array is only
    /// `TOTAL_CARDS` long, so every valid slot fits in a `u8`.
    #[inline]
    pub const fn new(i: usize) -> Self {
        CardIdx(i as u8)
    }

    /// The slot as a `usize`, for indexing `Gamestate::cards`.
    #[inline]
    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

/// One of the two players (0 or 1), wrapped in a newtype so a player id is never
/// silently confused with the many other small integers in the engine — card
/// indices (`CardIdx`), counts, or pitch values. Player 0 owns the lower half of
/// `Gamestate::cards` (`0..PLAYER_CARDS`); player 1 owns the upper half.
/// Construct with `PlayerIndex::new` (or the `P1`/`P2` constants); read the raw
/// id back with `get` (a `u8`) or `index` (a `usize`, for indexing the shared
/// arrays); get the other player with `opponent`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct PlayerIndex(pub u8);

impl PlayerIndex {
    /// Player 0 — owns global card slots `0..PLAYER_CARDS`.
    pub const P1: PlayerIndex = PlayerIndex(0);
    /// Player 1 — owns global card slots `PLAYER_CARDS..2*PLAYER_CARDS`.
    pub const P2: PlayerIndex = PlayerIndex(1);

    /// Wrap a raw 0/1 player id.
    #[inline]
    pub const fn new(i: u8) -> Self {
        PlayerIndex(i)
    }

    /// The raw 0/1 id as a `u8`.
    #[inline]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The id as a `usize`, for indexing the two halves of the shared arrays.
    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The other player (0 ↔ 1).
    #[inline]
    pub const fn opponent(self) -> Self {
        PlayerIndex(self.0 ^ 1)
    }
}

/// Number of (non-hero) cards each player owns. The unified `Gamestate::cards`
/// array is `2 * PLAYER_CARDS` long: player 0 owns slots `0..PLAYER_CARDS` and
/// player 1 owns slots `PLAYER_CARDS..2*PLAYER_CARDS`.
pub const PLAYER_CARDS: usize = 45;
pub const TOTAL_CARDS: usize = PLAYER_CARDS * 2;
/// Maximum number of cards the stack can hold at once. Committing a card beyond
/// this limit panics (see `Gamestate::push_to_stack`).
pub const STACK_SIZE: usize = 5;

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
    /// Per-player zone constructors. `pid` is the owning player; each returns
    /// that player's variant of the zone.
    pub const fn hand(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Hand } else { Self::P2Hand } }
    pub const fn deck(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Deck } else { Self::P2Deck } }
    pub const fn pitch(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Pitch } else { Self::P2Pitch } }
    pub const fn graveyard(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Graveyard } else { Self::P2Graveyard } }
    pub const fn arsenal(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Arsenal } else { Self::P2Arsenal } }
    pub const fn banish(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1BanishZone } else { Self::P2BanishZone } }
    pub const fn head(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Head } else { Self::P2Head } }
    pub const fn chest(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Chest } else { Self::P2Chest } }
    pub const fn arms(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Arms } else { Self::P2Arms } }
    pub const fn legs(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Legs } else { Self::P2Legs } }
    pub const fn weapon(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Weapon } else { Self::P2Weapon } }
    pub const fn combat_chain(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1CombatChain } else { Self::P2CombatChain } }
}

#[derive(Clone, Copy)]
pub struct CardState {
    pub visible :CardVisibleState,
    pub location: CardLocation,
    pub card : Card,
    pub next_card : CardIdx,
    pub prev_card : CardIdx,
}


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Phase {
    Start,
    ChooseFirst,
    Action,
    ActionPitch,
    ActionInstant,
    Defend,
    Reaction,
    ReactionPitch,
    /// Terminal phase: player 1 has won (player 2's hero was reduced to 0 life).
    Player1Win,
    /// Terminal phase: player 2 has won (player 1's hero was reduced to 0 life).
    Player2Win,
    /// Terminal phase: the game ended in a draw (the turn limit was reached with
    /// both heroes still alive).
    Draw,
}

impl Phase {
    /// True for the three terminal phases (`Player1Win`, `Player2Win`, `Draw`).
    /// Once the game reaches one of these no further actions are processed.
    pub fn is_terminal(self) -> bool {
        matches!(self, Phase::Player1Win | Phase::Player2Win | Phase::Draw)
    }
}

/// The maximum number of turns a game may run before it is declared a draw. A
/// game that reaches this many turns without either hero being reduced to 0 life
/// ends in `Phase::Draw`.
pub const MAX_TURNS: u16 = 40;

/// Per-player state. The cards themselves live in `Gamestate::cards`; a player
/// only holds the (global) head/tail indices into that shared array for each of
/// its zones, plus its turn resources. Player 0's cards occupy global slots
/// `0..PLAYER_CARDS`, player 1's occupy `PLAYER_CARDS..2*PLAYER_CARDS`.
pub struct Player {
    /// Which player this is (0 or 1), and equivalently which half of
    /// `Gamestate::cards` it owns: player 0 owns slots `0..PLAYER_CARDS`,
    /// player 1 owns `PLAYER_CARDS..2*PLAYER_CARDS`. Stored here so the pid
    /// travels with the player rather than being passed as a parallel argument
    /// that could drift out of sync.
    pub pid: PlayerIndex,
    pub life: u8,
    pub intellect: u8,
    pub hero : Card,
    pub resources: u8,
    pub action_points: u8,
    pub top_deck_idx: Option<CardIdx>,
    pub bottom_deck_idx: Option<CardIdx>,
    pub pitch_idx : Option<CardIdx>,
    pub arsenal_idx : Option<CardIdx>,
    pub hand_idx : Option<CardIdx>,
    pub banish_idx : Option<CardIdx>,
    pub weapon_idx : Option<CardIdx>,
    pub head_idx : Option<CardIdx>,
    pub chest_idx : Option<CardIdx>,
    pub arms_idx : Option<CardIdx>,
    pub legs_idx : Option<CardIdx>,
    /// The combat chain, link by link. Each slot holds the global `cards` index
    /// of the card occupying that chain link, or `None` if the link is empty.
    /// Sized at 5 since a single combat chain is very unlikely to grow longer
    /// than that; the attacking card/weapon is placed at link 0.
    pub chain_link : [Option<CardIdx>; 5],
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
    /// The player who currently holds priority and whose action `step` will
    /// apply. During a turn's action/pitch flow this is the turn player; during
    /// the ActionInstant phase it ping-pongs between the players as each passes
    /// priority, returning to `turn_player` once a card resolves.
    pub active_player : PlayerIndex,
    /// The player whose turn it is. Unlike `active_player`, this does not change
    /// as priority passes back and forth during the ActionInstant phase: it is set
    /// when the turn begins and is the player priority returns to after the top
    /// of the stack resolves.
    pub turn_player : PlayerIndex,
    /// Consecutive passes during the ActionInstant phase. Each pass increments it; a
    /// second pass in a row (reaching 2) resolves the top of the stack and
    /// resets it to 0. Playing a card onto the stack also resets it to 0, since
    /// the pending resolution has been interrupted by a new layer.
    pub passes : u8,
    pub phase : Phase,
    /// How many turns have begun so far this game. Incremented at the start of
    /// every turn (see `begin_turn`); once it reaches `MAX_TURNS` the game ends
    /// in a draw (see `check_game_end`).
    pub turn_count : u16,
    pub rng : SmallRng,
    /// The stack: cards currently waiting to resolve, each paired with the
    /// `ActionType` that committed it (so we know how to resolve it). Slot 0 is
    /// the bottom of the stack and slots fill upward; the topmost occupied slot
    /// is the card that resolves next. Empty slots are `None`. Because the stored
    /// indices are global, the stack can hold cards owned by either player.
    /// Committing a card when all `STACK_SIZE` slots are full panics.
    pub stack : [Option<PendingCard>; STACK_SIZE],
    /// The card the active player has chosen to play or activate and is now
    /// paying for. Set when leaving the `Action` phase for the `ActionPitch`
    /// phase (or the `Reaction` phase for the `ReactionPitch` phase);
    /// holds the global slot index into `cards` together with the location it is
    /// being played/activated from. `None` outside the pay-for-a-card flow.
    pub pending_card : Option<PendingCard>,
}

impl Gamestate {
    /// True once the game has reached a terminal phase (a win for either player
    /// or a draw). When this holds, `step` is a no-op and there are no legal
    /// actions.
    pub fn is_game_over(&self) -> bool {
        self.phase.is_terminal()
    }

    /// Evaluate the win/draw conditions and, if any is met, move the game into
    /// the matching terminal phase. A hero reduced to 0 life loses, handing the
    /// win to the opponent (`life` is unsigned and damage saturates at 0, so
    /// `life == 0` captures "0 or less"); reaching `MAX_TURNS` with both heroes
    /// still alive is a draw. Returns `true` when the game has ended.
    ///
    /// Idempotent: once a terminal phase is set it is left untouched, so callers
    /// may invoke this freely after any life change or at the start of a turn.
    pub fn check_game_end(&mut self) -> bool {
        if self.is_game_over() {
            return true;
        }
        if self.p1.life == 0 {
            self.phase = Phase::Player2Win;
            return true;
        }
        if self.p2.life == 0 {
            self.phase = Phase::Player1Win;
            return true;
        }
        if self.turn_count >= MAX_TURNS {
            self.phase = Phase::Draw;
            return true;
        }
        false
    }

    /// Slot index of the topmost (next-to-resolve) card on the stack, or `None`
    /// when the stack is empty. The stack fills from slot 0 upward, so this is
    /// the highest occupied slot.
    pub fn stack_top_slot(&self) -> Option<usize> {
        self.stack.iter().rposition(|slot| slot.is_some())
    }

    /// The `PendingCard` on top of the stack, or `None` when the stack is empty.
    pub fn stack_top(&self) -> Option<PendingCard> {
        self.stack_top_slot().and_then(|i| self.stack[i])
    }

    /// True when no cards are on the stack.
    pub fn stack_is_empty(&self) -> bool {
        self.stack.iter().all(|slot| slot.is_none())
    }

    /// Push a committed card onto the top of the stack. Panics with
    /// "stack size ran out" if all `STACK_SIZE` slots are already occupied.
    pub fn push_to_stack(&mut self, pending: PendingCard) {
        for slot in self.stack.iter_mut() {
            if slot.is_none() {
                *slot = Some(pending);
                return;
            }
        }
        panic!("stack size ran out");
    }

    /// Remove and return the card on top of the stack, or `None` when empty.
    pub fn pop_stack(&mut self) -> Option<PendingCard> {
        let top = self.stack_top_slot()?;
        self.stack[top].take()
    }
}

/// A card the active player has committed to play, activate, or attack with,
/// pending payment during a pitch phase. `index` is the global slot into
/// `Gamestate::cards`; `typ` is the action that committed it (`PlayCard`,
/// `Activate`, or `Attack`), which determines how the card resolves once paid
/// for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingCard {
    pub index : CardIdx,
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
        let next = card.next_card.get();
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
            current: self.hand_idx.map(|i| i.get()),
        }
    }
}

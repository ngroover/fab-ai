use crate::cards::Card;
use crate::action::ActionType;
use crate::card_counts::{CardCounts, DeckCardId, MAX_DECK_IDS};
use crate::deck_cards::{DeckCard, DeckKind};
use crate::ring_buffer::RingBuffer;

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

/// Number of (non-hero) cards each player owns: the 40-card deck plus a weapon
/// and four pieces of equipment. Every one of them is in exactly one zone at
/// any time — the conservation `Gamestate::debug_check_card_totals` checks.
pub const PLAYER_CARDS: usize = 45;
/// Maximum number of layers the stack can hold at once. Committing a card
/// beyond this limit panics (see `Gamestate::push_to_stack`). Both players may
/// keep adding instants, reactions and activations without passing, so a single
/// reaction window can legitimately stack six or more layers (e.g. four attack
/// reactions answered by two Dodges). Sized to the deepest stack random play
/// reaches: a sweep of 20 million random games went no deeper than 7, first in
/// seed 13080313 (see `simulator::tests::stack_depth_histogram` and
/// `random_game_with_a_seven_layer_stack_finishes`). Deliberate play could in
/// principle go deeper; raise this if it ever panics.
pub const STACK_SIZE: usize = 7;
/// How many cards a decklist puts in the deck: everything but the hero, the
/// weapon and the four pieces of equipment.
pub const DECK_CARDS: usize = 40;
/// Capacity of a deck's circular buffer: exactly a whole deck (`DECK_CARDS`).
pub const DECK_CAPACITY: usize = DECK_CARDS;
const _: () = assert!(DECK_CARDS <= DECK_CAPACITY);
/// How many links a combat chain can hold. A single combat chain is very
/// unlikely to grow longer than that; a turn that does produce a sixth attack
/// panics rather than reusing a link and mis-scoring the attack.
pub const CHAIN_LINKS: usize = 5;

/// Who can see a card. Cards carry no visibility of their own: it follows from
/// the zone they are in (see `Player::visibility`), except in the deck, where
/// each slot remembers whether both players have seen that card (a pitched
/// card put on the bottom, or a revealed one).
// Read by tests and, later, by observation encoding; the engine itself never
// needs to ask who can see a card.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum CardVisibleState {
    Hidden,
    P1Knows,
    P2Knows,
    BothKnow,
}

#[allow(dead_code)]
impl CardVisibleState {
    /// Known to `pid` alone.
    pub const fn owner_only(pid: PlayerIndex) -> Self {
        if pid.get() == 0 { CardVisibleState::P1Knows } else { CardVisibleState::P2Knows }
    }
}

/// The zones a player's cards can be in. Zones are per player — the owner is
/// the `Player` holding the zone — so this names the kind of zone only; the
/// stack is the one zone shared by both players.
///
/// How each zone is stored:
/// - **Unordered** zones are a `CardCounts` bitfield on the player: `Hand`,
///   `Pitch`, `Graveyard`, `Banish`, `IntimidateBanish`, `Equipment` (the four
///   equipment zones share one bitfield), and each link of the combat chain
///   (`Player::chain_link`).
/// - **Ordered** zones are circular buffers: the `Deck` (`Player::deck`) and the
///   `Stack` (`Gamestate::stack`).
/// - **Single-card** zones are an `Option<DeckCardId>`: the `Arsenal` and the
///   `Weapon`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum Zone {
    Hand,
    Deck,
    Pitch,
    Graveyard,
    Arsenal,
    Banish,
    /// Cards a player was forced to banish to an opponent's Intimidate. Kept
    /// separate from the ordinary banish zone because they return to hand at
    /// the end of the turn.
    IntimidateBanish,
    Equipment,
    Weapon,
    CombatChain,
    Stack,
}

impl Zone {
    /// Whether the zone is one of the `CardCounts` zones `Player::counts`
    /// returns. The combat chain is unordered too, but it is split into links
    /// and reached through `Player::chain_link` instead.
    pub const fn is_unordered(self) -> bool {
        matches!(
            self,
            Zone::Hand | Zone::Pitch | Zone::Graveyard | Zone::Banish
                | Zone::IntimidateBanish | Zone::Equipment
        )
    }
}

/// One card in a deck, packed into a single byte: which card it is (the low
/// bits, a `DeckCardId`), and whether both players know it is there (the top
/// bit). A card is hidden in the deck unless it went in face up — a pitched
/// card put on the bottom, or a card revealed and kept or bottomed — and loses
/// that the moment the deck is shuffled.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeckSlot(u8);

impl DeckSlot {
    const KNOWN: u8 = 0x80;

    pub const fn hidden(card: DeckCardId) -> Self {
        DeckSlot(card.get() as u8)
    }

    pub const fn known(card: DeckCardId) -> Self {
        DeckSlot(card.get() as u8 | Self::KNOWN)
    }

    /// The card in this slot.
    #[inline]
    pub const fn card(self) -> DeckCardId {
        DeckCardId::new((self.0 & !Self::KNOWN) as usize)
    }

    /// Whether both players know this card is here.
    #[inline]
    pub const fn is_known(self) -> bool {
        self.0 & Self::KNOWN != 0
    }

    #[inline]
    pub fn set_known(&mut self, known: bool) {
        if known { self.0 |= Self::KNOWN } else { self.0 &= !Self::KNOWN }
    }

    /// Turn the slot into `card`, keeping whether it is known.
    #[inline]
    #[allow(dead_code)] // used by tests
    pub fn set_card(&mut self, card: DeckCardId) {
        self.0 = (self.0 & Self::KNOWN) | card.get() as u8;
    }
}

// Every deck card id must fit below the known bit, or `known` would overwrite
// part of the id. A free `const` item is always evaluated, unlike an
// associated const, which is only checked where it is used.
const _: () = assert!(
    MAX_DECK_IDS <= DeckSlot::KNOWN as usize,
    "deck card ids overlap DeckSlot's known bit",
);

impl std::fmt::Debug for DeckSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckSlot")
            .field("card", &self.card())
            .field("known", &self.is_known())
            .finish()
    }
}

/// A deck: a circular buffer of cards, the top (the next card drawn) at the
/// front and the bottom at the back.
pub type Deck = RingBuffer<DeckSlot, DECK_CAPACITY>;

/// An empty deck.
pub const fn empty_deck() -> Deck {
    RingBuffer::new(DeckSlot::hidden(DeckCardId::new(0)))
}

/// The stack: a circular buffer of layers waiting to resolve, the bottom at the
/// front and the top (the next layer to resolve) at the back.
pub type Stack = RingBuffer<PendingCard, STACK_SIZE>;

/// An empty stack.
pub const fn empty_stack() -> Stack {
    RingBuffer::new(PendingCard {
        owner: PlayerIndex::P1,
        card: DeckCardId::new(0),
        from: Zone::Stack,
        typ: ActionType::Pass,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Phase {
    Start,
    ChooseFirst,
    /// The start of the turn player's turn, entered only when a mentor sits face
    /// down in their arsenal: "While [this mentor] is face down in arsenal, at
    /// the start of your turn, you may turn [it] face up." The turn player either
    /// flips it (`FlipFaceUp`) or declines (`Pass`); either way the Action phase
    /// follows. A mentor turned face up stays that way — it is never offered
    /// again — while a declined one is offered again at the start of each of its
    /// owner's later turns, for as long as it stays face down.
    MentorFlip,
    Action,
    ActionPitch,
    ActionInstant,
    Defend,
    /// The defend-trigger window: the defender has finished declaring blockers
    /// and every "when you defend with" trigger for the attack is on the stack
    /// (see `push_defend_triggers`). Entered only when there is at least one
    /// trigger to resolve. Only instants may be played here — a reaction waits
    /// for the `Reaction` window — and the triggers resolve one layer at a time
    /// like any other stack entry. When the stack empties this window hands over
    /// to `Reaction` rather than dealing combat damage, so the ordinary reaction
    /// window still happens.
    DefendTriggers,
    /// Pitching to pay for a card played during the `DefendTriggers` window,
    /// returning there once the cost is covered — what `ActionPitch` is to the
    /// action phase and `ReactionPitch` to the reaction window. Named after the
    /// defend step rather than spelled out as `DefendTriggersPitch`: it is the
    /// pitch phase reached from the defend step, and nothing else there pitches.
    DefendPitch,
    Reaction,
    ReactionPitch,
    /// The end of the turn player's action phase: they have passed, the combat
    /// chain has closed, and they may now move a card into their arsenal.
    Arsenal,
    /// After the Arsenal phase, when two or more cards sit in a player's pitch
    /// zone: the zone's owner (the active player — the turn player first, then
    /// the opponent) chooses, one `BottomPitch` action at a time, the order
    /// their pitched cards go to the bottom of their deck. The turn ends once
    /// both players' pitch zones are empty. (A single pitched card has only one
    /// possible order, so it is bottomed automatically without entering this
    /// phase.)
    PitchOrder,
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
pub const MAX_TURNS: u8 = 40;

bitflags::bitflags! {
    /// A player's boolean state, packed into one byte so `Player` stays small.
    /// Read and written through the `Player` accessors (`attack_go_again_bonus()`,
    /// `set_attack_go_again_bonus(..)`, and so on).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PlayerFlags : u8 {
        /// Whether this player's current attack has been granted Go Again by an
        /// on-play effect (e.g. Wild Ride's "if a card with 6 or more power is
        /// discarded, this gains go again"). Like `Player::attack_power_bonus` this is banked
        /// on the player while the attack is on the chain, folded into the Go Again
        /// check when combat damage resolves, and cleared afterward (and at the start
        /// of each turn) so it never leaks into a later attack.
        const ATTACK_GO_AGAIN_BONUS = 1 << 0;
        /// Whether this player has resolved an Intimidate trigger so far this turn.
        /// Set when this player intimidates (an attack/action with the Intimidate
        /// keyword resolves) and read by on-play effects gated on "if you've
        /// intimidated this turn" (e.g. Beast Mode's conditional +2 power). Cleared
        /// at the start of each turn so it never carries into a later turn.
        const HAS_INTIMIDATED = 1 << 1;
        /// Whether the card in this player's arsenal (`Player::arsenal`) is face up. A
        /// card arsenaled at the end of a turn goes in face down; a mentor may be
        /// turned face up at the start of its owner's turn (the `MentorFlip` phase),
        /// and a card an effect puts into the arsenal "face up" (Chief Ruk'utan
        /// fetching Alpha Rampage) arrives that way. Face up and down only exist in
        /// the arsenal, which holds one card, so this lives on the player rather
        /// than on every card; it is cleared whenever the arsenal empties (see
        /// `take_from_zone`).
        const ARSENAL_FACE_UP = 1 << 2;
    }
}

/// Per-player state: every zone the player's cards can be in, plus the player's
/// turn resources and the bonuses banked for their attacks. Cards are stored by
/// their `DeckCardId` (see `deck_cards`), which only means something together
/// with `deck_kind`.
#[derive(Clone)]
pub struct Player {
    /// Which player this is (0 or 1). Stored here so the pid travels with the
    /// player rather than being passed as a parallel argument that could drift
    /// out of sync.
    pub pid: PlayerIndex,
    pub life: u8,
    pub intellect: u8,
    pub hero : Card,
    /// Which deck card enum this player's `DeckCardId`s belong to — the deck
    /// `hero` plays. Fixed when the game is built.
    pub deck_kind : DeckKind,
    /// The deck, top first. Drawn from the front; pitched cards go on the back.
    pub deck : Deck,
    /// The hand: how many copies of each card are in it. The hand is
    /// unordered, so the counts are all there is to know about it.
    pub hand : CardCounts,
    /// The pitch zone. It is unordered; the order pitched cards go to the
    /// bottom of the deck is chosen as they leave it (the `PitchOrder` phase).
    pub pitch : CardCounts,
    pub graveyard : CardCounts,
    /// The (permanent) banish zone.
    pub banished : CardCounts,
    /// The Intimidate banish zone, emptied back into the hand as the turn
    /// passes into the Arsenal phase.
    pub intimidate_banished : CardCounts,
    /// The worn equipment still in play across the four equipment zones (head,
    /// chest, arms, legs). Each zone holds at most one piece and which zone a
    /// piece sits in follows from the card itself, so one bitfield covers all
    /// four. The weapon zone is not an equipment zone; see `weapon`.
    pub equipment : CardCounts,
    /// The card in the arsenal, if any.
    pub arsenal : Option<DeckCardId>,
    /// The weapon in the weapon zone. Empty while the weapon is on the stack or
    /// the combat chain after being swung; it returns when the chain closes.
    pub weapon : Option<DeckCardId>,
    /// The combat chain, link by link, in order. Each link holds this player's
    /// cards on it as an unordered bitfield: the attacking card on the
    /// attacker's side, the blockers on the defender's. Empty links are
    /// `CardCounts::EMPTY`.
    ///
    /// Each attack in a turn takes a link of its own — the attacker's first
    /// attack goes to link 0, a follow-up after Go Again to link 1, and so on —
    /// and the cards blocking an attack join that same link on the defender's
    /// side. Combat damage is calculated a link at a time, so an earlier attack
    /// this turn adds nothing to a later one's power and the cards that blocked
    /// it do not blunt it. A defender's links can therefore have gaps, where an
    /// attack went unblocked.
    pub chain_link : [CardCounts; CHAIN_LINKS],
    /// Extra defense granted to this player's cards on each chain link on top
    /// of their printed defense, by effects that pump a blocker (Rally the
    /// Rearguard's "gains +3 block"). Only the link's total block matters to
    /// combat damage, so the bonus is kept per link rather than per copy. It
    /// lasts as long as the link, and is cleared as the chain closes.
    pub chain_defense_bonus : [u8; CHAIN_LINKS],
    /// How many copies of each card have used their "once per turn" ability
    /// this turn (a weapon swing, Rally the Rearguard). Another copy may still
    /// activate while fewer have been used than there are copies in the zone it
    /// activates from. Set when the activation goes on the stack — the cost is
    /// paid then, so a countered activation still counts — and cleared at the
    /// start of each turn.
    pub abilities_used : CardCounts,
    pub resources: u8,
    pub action_points: u8,
    /// Extra power added to this player's current attack on top of the cards on
    /// the combat chain. Set by on-play effects that conditionally pump the
    /// attacking card (e.g. Bare Fangs's "+2 power if you discarded a card with
    /// 6 or more power"), consumed when combat damage resolves, and reset to 0
    /// afterward (and at the start of each turn) so it never leaks to a later
    /// attack.
    pub attack_power_bonus : u8,
    /// Extra power banked for the *next Brute attack* this player plays this
    /// turn, on top of the cards on the combat chain. Set by on-play effects
    /// that pump a follow-up brute (e.g. Awakening Bellow's "+3 power to the
    /// next attack with the brute type"). Unlike `attack_power_bonus` it is
    /// applied only when the attacking card is a Brute, so a non-brute attack in
    /// between does not consume it; consumed when a brute attack resolves combat
    /// damage and cleared at the start of each turn so it never leaks.
    pub next_brute_attack_action_bonus : u8,
    /// Extra power banked for the *next attack action card* this player plays
    /// this turn, on top of the cards on the combat chain. Set by on-play
    /// effects that pump a follow-up attack regardless of its class (e.g. Come
    /// to Fight's "+1 power to your next attack action card"). The generic
    /// sibling of `next_brute_attack_action_bonus`: it is applied to any attack
    /// action card rather than only a Brute one, but is likewise left banked by
    /// a weapon swing, consumed when an attack action card resolves combat
    /// damage, and cleared at the start of each turn so it never leaks.
    pub next_attack_action_bonus : u8,
    /// Extra power banked for the *next Brute attack* this player makes this
    /// turn, paid out only if that attack is defended by fewer than two
    /// non-equipment cards. Set by on-play effects that hand a follow-up brute
    /// a defence-sensitive pump (e.g. Barraging Beatdown's "+3 power while
    /// defended by less than 2 non-equipment cards"). It differs from
    /// `next_brute_attack_action_bonus` in two ways: the blocker count is
    /// checked when combat damage resolves, and "Brute attack" covers a brute
    /// *weapon* swing as well as a brute attack action card, so Bone Basher
    /// both takes and spends it. Consumed by any brute attack — whether or not
    /// the blocker count let it pay out — and cleared at the start of each turn
    /// so it never leaks.
    pub next_brute_attack_conditional_bonus : u8,
    /// Per-player yes/no state packed into one byte: see `PlayerFlags` for
    /// what each flag means, and the `Player` accessors of the same names.
    pub flags : PlayerFlags,
    /// Lesson counters on the mentor in this player's arsenal (Chief Ruk'utan
    /// gains one each time his ability triggers and graduates at two). A mentor
    /// only collects them while in the arsenal, so like `arsenal_face_up` they
    /// are kept here and cleared whenever the arsenal empties — the counters
    /// never follow the card into another zone.
    pub arsenal_lesson_counters : u8,
    /// This player's view of the game log: what they would legitimately know,
    /// with hidden information (e.g. the identity of a card the opponent drew)
    /// redacted. `None` when logging is disabled (the default, so simulation
    /// and cloning pay nothing); `reset(gs, decks, rng, true)` turns it on. Entries are
    /// pushed at the moment each event happens, so a single `step` may log
    /// several (e.g. a pass that also resolves the top of the stack).
    /// Only present when built with the `logging` Cargo feature.
    #[cfg(feature = "logging")]
    pub log : Option<Vec<String>>,
}

#[derive(Clone)]
pub struct Gamestate {
    pub p1 : Player,
    pub p2 : Player,
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
    pub turn_count : u8,
    /// The stack: layers waiting to resolve, each a `PendingCard` naming the
    /// card, its owner, the zone it came from and the action that committed it.
    /// A circular buffer with the bottom of the stack at the front and the top —
    /// the layer that resolves next — at the back.
    pub stack : Stack,
    /// The card the active player has chosen to play or activate and is now
    /// paying for. Set when leaving the `Action` phase for the `ActionPitch`
    /// phase (or the `Reaction` phase for the `ReactionPitch` phase). The card
    /// stays in the zone it is played from until it is paid for. `None` outside
    /// the pay-for-a-card flow.
    pub pending_card : Option<PendingCard>,
    /// The omniscient game log: every logged event with nothing redacted (the
    /// per-player views with hidden information removed live on `Player::log`).
    /// `None` when logging is disabled (the default, so simulation and cloning
    /// pay nothing); `reset(gs, decks, rng, true)` turns it on. Entries are pushed at the
    /// moment each event happens, so a single `step` may log several (e.g. a
    /// pass that also resolves the top of the stack).
    /// Only present when built with the `logging` Cargo feature.
    #[cfg(feature = "logging")]
    pub log : Option<Vec<String>>,
}

impl Gamestate {
    /// The player `pid`.
    #[inline]
    pub fn player(&self, pid: PlayerIndex) -> &Player {
        if pid == PlayerIndex::P1 { &self.p1 } else { &self.p2 }
    }

    /// The player `pid`, mutably.
    #[inline]
    pub fn player_mut(&mut self, pid: PlayerIndex) -> &mut Player {
        if pid == PlayerIndex::P1 { &mut self.p1 } else { &mut self.p2 }
    }

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

    /// The layer on top of the stack (the next to resolve), or `None` when the
    /// stack is empty.
    #[allow(dead_code)]
    pub fn stack_top(&self) -> Option<PendingCard> {
        self.stack.back()
    }

    /// True when nothing is on the stack.
    pub fn stack_is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Push a committed layer onto the top of the stack. Panics with
    /// "stack size ran out" if the stack already holds `STACK_SIZE` layers.
    pub fn push_to_stack(&mut self, pending: PendingCard) {
        if self.stack.try_push_back(pending).is_err() {
            panic!("stack size ran out");
        }
    }

    /// Remove and return the layer on top of the stack, or `None` when empty.
    pub fn pop_stack(&mut self) -> Option<PendingCard> {
        self.stack.pop_back()
    }

    /// Check that each player still has exactly `PLAYER_CARDS` cards across all
    /// of their zones and the stack. Cards are only ever moved, never created
    /// or lost, so a different total means some path took a card out of one
    /// zone without putting it in another (or the reverse). Called by `step`
    /// and `legal_actions` in debug builds.
    #[cfg_attr(not(debug_assertions), allow(dead_code))] // read by the debug-build check
    pub fn debug_check_card_totals(&self) {
        for player in [&self.p1, &self.p2] {
            let on_stack = self.stack.iter()
                .filter(|p| p.owner == player.pid && p.holds_card())
                .count();
            let total = player.cards_in_zones() + on_stack;
            assert_eq!(
                total, PLAYER_CARDS,
                "P{} has {} cards across its zones and the stack, not {}",
                player.pid.get() + 1, total, PLAYER_CARDS,
            );
        }
    }

    /// True when logging was enabled by `reset(gs, decks, true)`. Callers should check
    /// this before building log strings so disabled logging costs nothing.
    /// Always `false` without the `logging` Cargo feature, so every guarded
    /// logging block compiles away.
    #[cfg(feature = "logging")]
    #[inline]
    pub fn logging_enabled(&self) -> bool {
        self.log.is_some()
    }

    /// Always `false`: the crate was built without the `logging` feature.
    #[cfg(not(feature = "logging"))]
    #[inline(always)]
    pub const fn logging_enabled(&self) -> bool {
        false
    }

    /// Append a fully-public event: the same message goes to the omniscient
    /// gamestate log and both players' logs. A no-op when logging is disabled.
    pub fn log_public(&mut self, msg: String) {
        if !self.logging_enabled() {
            return;
        }
        self.log_views(msg.clone(), msg.clone(), msg);
    }

    /// Append an event involving hidden information: `full` (nothing redacted)
    /// goes to the omniscient gamestate log, while `p1` and `p2` carry each
    /// player's legitimate view of it. A no-op when logging is disabled.
    #[cfg_attr(not(feature = "logging"), allow(unused_variables))]
    pub fn log_views(&mut self, full: String, p1: String, p2: String) {
        #[cfg(feature = "logging")]
        {
            if let Some(log) = &mut self.p1.log {
                log.push(p1);
            }
            if let Some(log) = &mut self.p2.log {
                log.push(p2);
            }
            if let Some(log) = &mut self.log {
                log.push(full);
            }
        }
    }

    /// Append an event only `pid` is entitled to know about: it goes to the
    /// omniscient gamestate log and `pid`'s own log, and the opponent's log gets
    /// no entry at all — not even a redacted one, since the bare fact that the
    /// event happened would itself give something away (e.g. declining to flip a
    /// mentor reveals that there is a mentor in the arsenal). A no-op when
    /// logging is disabled.
    #[cfg_attr(not(feature = "logging"), allow(unused_variables))]
    pub fn log_private(&mut self, pid: PlayerIndex, msg: String) {
        #[cfg(feature = "logging")]
        {
            let own = if pid == PlayerIndex::P1 { &mut self.p1.log } else { &mut self.p2.log };
            if let Some(log) = own {
                log.push(msg.clone());
            }
            if let Some(log) = &mut self.log {
                log.push(msg);
            }
        }
    }
}


/// A card committed to play, activate, or attack with, pending payment during a
/// pitch phase or waiting on the stack. `card` is the card and `owner` the
/// player whose deck it belongs to; `from` is the zone it was in when it was
/// committed, and `typ` the action that committed it (`PlayCard`, `Activate`,
/// or `DefendTrigger`), which determines how it resolves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingCard {
    pub owner : PlayerIndex,
    pub card : DeckCardId,
    pub from : Zone,
    pub typ : ActionType,
}

impl PendingCard {
    /// Whether the layer holds the card itself, rather than pointing at a card
    /// that stays in its zone: a defend trigger, or an ability activated while
    /// its card is defending, puts only the effect on the stack and leaves the
    /// card on the combat chain.
    #[cfg_attr(not(debug_assertions), allow(dead_code))] // read by the debug-build check
    pub fn holds_card(&self) -> bool {
        self.typ != ActionType::DefendTrigger && self.from != Zone::CombatChain
    }
}

impl Player {
    /// A player with every zone empty, playing `hero`'s deck. `reset` deals the
    /// starting cards out of `fab_game::Decklists`.
    pub fn new(pid: PlayerIndex, hero: Card) -> Player {
        Player {
            pid,
            // Populated by `reset` (see `set_life_and_intellect`); 0 until then.
            life: 0,
            intellect: 0,
            hero,
            deck_kind: DeckKind::for_hero(hero),
            deck: empty_deck(),
            hand: CardCounts::EMPTY,
            pitch: CardCounts::EMPTY,
            graveyard: CardCounts::EMPTY,
            banished: CardCounts::EMPTY,
            intimidate_banished: CardCounts::EMPTY,
            equipment: CardCounts::EMPTY,
            arsenal: None,
            weapon: None,
            chain_link: [CardCounts::EMPTY; CHAIN_LINKS],
            chain_defense_bonus: [0; CHAIN_LINKS],
            abilities_used: CardCounts::EMPTY,
            resources: 0,
            action_points: 0,
            attack_power_bonus: 0,
            next_brute_attack_action_bonus: 0,
            next_attack_action_bonus: 0,
            next_brute_attack_conditional_bonus: 0,
            flags: PlayerFlags::empty(),
            arsenal_lesson_counters: 0,
            // Logging is opted into via `reset(gs, decks, true)`; off until then.
            #[cfg(feature = "logging")]
            log: None,
        }
    }

    #[doc = concat!("Whether `PlayerFlags::", stringify!(ATTACK_GO_AGAIN_BONUS), "` is set.")]
    #[inline]
    pub const fn attack_go_again_bonus(&self) -> bool {
        self.flags.contains(PlayerFlags::ATTACK_GO_AGAIN_BONUS)
    }

    #[doc = concat!("Set or clear `PlayerFlags::", stringify!(ATTACK_GO_AGAIN_BONUS), "`.")]
    #[inline]
    pub fn set_attack_go_again_bonus(&mut self, on: bool) {
        self.flags.set(PlayerFlags::ATTACK_GO_AGAIN_BONUS, on);
    }

    #[doc = concat!("Whether `PlayerFlags::", stringify!(HAS_INTIMIDATED), "` is set.")]
    #[inline]
    pub const fn has_intimidated(&self) -> bool {
        self.flags.contains(PlayerFlags::HAS_INTIMIDATED)
    }

    #[doc = concat!("Set or clear `PlayerFlags::", stringify!(HAS_INTIMIDATED), "`.")]
    #[inline]
    pub fn set_has_intimidated(&mut self, on: bool) {
        self.flags.set(PlayerFlags::HAS_INTIMIDATED, on);
    }

    #[doc = concat!("Whether `PlayerFlags::", stringify!(ARSENAL_FACE_UP), "` is set.")]
    #[inline]
    pub const fn arsenal_face_up(&self) -> bool {
        self.flags.contains(PlayerFlags::ARSENAL_FACE_UP)
    }

    #[doc = concat!("Set or clear `PlayerFlags::", stringify!(ARSENAL_FACE_UP), "`.")]
    #[inline]
    pub fn set_arsenal_face_up(&mut self, on: bool) {
        self.flags.set(PlayerFlags::ARSENAL_FACE_UP, on);
    }

    /// Number of cards in hand, counting every copy.
    #[inline]
    pub const fn hand_size(&self) -> u8 {
        self.hand.len()
    }

    /// Number of cards in the deck.
    #[inline]
    #[allow(dead_code)]
    pub fn deck_size(&self) -> u8 {
        self.deck.len() as u8
    }

    /// `card`'s id in this player's deck. Panics if the deck does not contain
    /// it, since such a card can never be in any of this player's zones.
    #[inline]
    #[allow(dead_code)] // used by tests
    pub fn deck_id(&self, card: Card) -> DeckCardId {
        match self.deck_kind.id_of(card) {
            Some(id) => id,
            None => panic!("{:?} is not in the {:?} deck", card, self.deck_kind),
        }
    }

    /// The `Card` a `DeckCardId` of this player's stands for.
    #[inline]
    pub fn card(&self, id: DeckCardId) -> Card {
        self.deck_kind.card(id)
    }

    /// The bitfield holding an unordered zone, or `None` for the zones stored
    /// some other way (see `Zone`).
    #[allow(dead_code)]
    pub fn counts(&self, zone: Zone) -> Option<CardCounts> {
        match zone {
            Zone::Hand => Some(self.hand),
            Zone::Pitch => Some(self.pitch),
            Zone::Graveyard => Some(self.graveyard),
            Zone::Banish => Some(self.banished),
            Zone::IntimidateBanish => Some(self.intimidate_banished),
            Zone::Equipment => Some(self.equipment),
            Zone::Deck | Zone::Arsenal | Zone::Weapon | Zone::CombatChain | Zone::Stack => None,
        }
    }

    /// Mutable form of `counts`.
    pub fn counts_mut(&mut self, zone: Zone) -> Option<&mut CardCounts> {
        match zone {
            Zone::Hand => Some(&mut self.hand),
            Zone::Pitch => Some(&mut self.pitch),
            Zone::Graveyard => Some(&mut self.graveyard),
            Zone::Banish => Some(&mut self.banished),
            Zone::IntimidateBanish => Some(&mut self.intimidate_banished),
            Zone::Equipment => Some(&mut self.equipment),
            Zone::Deck | Zone::Arsenal | Zone::Weapon | Zone::CombatChain | Zone::Stack => None,
        }
    }

    /// Put one copy of `id` into the unordered zone `zone`. Panics for a zone
    /// that is not one of `counts`'s.
    #[inline]
    pub fn add_to(&mut self, zone: Zone, id: DeckCardId) {
        self.counts_mut(zone)
            .unwrap_or_else(|| panic!("{:?} is not an unordered zone", zone))
            .add(id);
    }

    /// Take one copy of `id` out of the unordered zone `zone`. Panics for a
    /// zone that is not one of `counts`'s, or when the zone holds no copy.
    #[inline]
    pub fn remove_from(&mut self, zone: Zone, id: DeckCardId) {
        self.counts_mut(zone)
            .unwrap_or_else(|| panic!("{:?} is not an unordered zone", zone))
            .remove(id);
    }

    /// How many copies of `card` are in hand, for code that knows which deck
    /// this player has. Debug builds check that `D` is this player's deck.
    #[inline]
    #[allow(dead_code)]
    pub fn hand_count<D: DeckCard>(&self, card: D) -> u8 {
        debug_assert_eq!(D::KIND, self.deck_kind, "{:?} is not this player's deck enum", card);
        self.hand.count_as(card)
    }

    /// How many copies of `card` are in hand, for code that does not know the
    /// deck. A card outside the deck is never in hand, so it counts 0.
    #[inline]
    #[allow(dead_code)]
    pub fn hand_count_card(&self, card: Card) -> u8 {
        match self.deck_kind.id_of(card) {
            Some(id) => self.hand.count(id),
            None => 0,
        }
    }

    /// The pitch value of every card in `counts` added up: one catalog lookup
    /// per distinct card rather than one per copy.
    pub fn pitch_total(&self, counts: CardCounts) -> u8 {
        counts
            .iter()
            .map(|(id, n)| self.card(id).data().pitch * n)
            .sum()
    }

    /// The pitch value of every card in hand added up.
    pub fn hand_pitch_total(&self) -> u8 {
        self.pitch_total(self.hand)
    }

    /// The card attacking on link `link` of this (attacking) player's chain:
    /// the attacker's side of a link holds the attack alone. `None` for an
    /// empty link.
    pub fn attack_on_link(&self, link: usize) -> Option<DeckCardId> {
        self.chain_link[link].iter().next().map(|(id, _)| id)
    }

    /// Whether any of this player's chain links holds a card.
    pub fn chain_occupied(&self) -> bool {
        self.chain_link.iter().any(|link| !link.is_empty())
    }

    /// How many of this player's cards are in their zones (the stack aside).
    #[cfg_attr(not(debug_assertions), allow(dead_code))] // read by the debug-build check
    pub fn cards_in_zones(&self) -> usize {
        let counted = [
            self.hand, self.pitch, self.graveyard, self.banished,
            self.intimidate_banished, self.equipment,
        ];
        counted.iter().map(|c| c.len() as usize).sum::<usize>()
            + self.chain_link.iter().map(|c| c.len() as usize).sum::<usize>()
            + self.deck.len()
            + self.arsenal.is_some() as usize
            + self.weapon.is_some() as usize
    }

    /// Who can see the cards this player has in `zone`. The hand, the
    /// Intimidate banish zone and a face-down arsenal card are known to their
    /// owner alone; a deck card is hidden unless its slot is known (see
    /// `DeckSlot`, and `deck_slot_visibility`); everything else is public.
    #[allow(dead_code)]
    pub fn visibility(&self, zone: Zone) -> CardVisibleState {
        match zone {
            Zone::Hand | Zone::IntimidateBanish => CardVisibleState::owner_only(self.pid),
            Zone::Arsenal if !self.arsenal_face_up() => CardVisibleState::owner_only(self.pid),
            Zone::Deck => CardVisibleState::Hidden,
            _ => CardVisibleState::BothKnow,
        }
    }

    /// Who can see the deck card at position `i` (0 is the top).
    #[allow(dead_code)]
    pub fn deck_slot_visibility(&self, i: usize) -> Option<CardVisibleState> {
        self.deck.get(i).map(|slot| {
            if slot.is_known() { CardVisibleState::BothKnow } else { CardVisibleState::Hidden }
        })
    }
}

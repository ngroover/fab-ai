use crate::cards::Card;
use crate::action::ActionType;
use crate::card_counts::{CardCounts, DeckCardId};
use crate::deck_cards::{DeckCard, DeckKind};
use rand::rngs::SmallRng;

/// A global index into `Gamestate::cards`. Wrapping the raw `u8` in a newtype
/// keeps card indices from being silently confused with the many other small
/// integers in the engine that share the same representation — counts
/// (`hand_size()`, `deck_size`), player ids (`pid`, `active_player`), or pitch
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
/// this limit panics (see `Gamestate::push_to_stack`). Both players may keep
/// adding instants, reactions and activations without passing, so a single
/// reaction window can legitimately stack six or more layers (e.g. four attack
/// reactions answered by two Dodges); 16 leaves ample headroom.
pub const STACK_SIZE: usize = 16;

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
    /// Cards a player was forced to banish to an opponent's Intimidate. Kept
    /// separate from the ordinary banish zone so an Intimidate banish can be
    /// tracked and (later) returned/cleared on its own.
    P1IntimidateBanish,
    P2IntimidateBanish,
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
    pub const fn intimidate_banish(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1IntimidateBanish } else { Self::P2IntimidateBanish } }
    pub const fn head(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Head } else { Self::P2Head } }
    pub const fn chest(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Chest } else { Self::P2Chest } }
    pub const fn arms(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Arms } else { Self::P2Arms } }
    pub const fn legs(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Legs } else { Self::P2Legs } }
    pub const fn weapon(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1Weapon } else { Self::P2Weapon } }
    pub const fn combat_chain(pid: PlayerIndex) -> Self { if pid.get() == 0 { Self::P1CombatChain } else { Self::P2CombatChain } }
}

/// The zones whose cards are unordered, each tracked on its `Player` as a
/// `CardCounts` bitfield (see `Player::zone`). A zone is per player — the
/// owner is the `Player` holding the bitfield — so this names the kind of zone
/// only. The four equipment zones (head, chest, arms, legs) share one
/// `Equipment` bitfield: each holds at most one card, and which slot a piece
/// sits in follows from the card itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum UnorderedZone {
    Hand,
    Pitch,
    Graveyard,
    Banish,
    IntimidateBanish,
    Equipment,
}

impl UnorderedZone {
    pub const ALL: [UnorderedZone; 6] = [
        UnorderedZone::Hand,
        UnorderedZone::Pitch,
        UnorderedZone::Graveyard,
        UnorderedZone::Banish,
        UnorderedZone::IntimidateBanish,
        UnorderedZone::Equipment,
    ];

    /// The unordered zone `location` belongs to (for either player), or `None`
    /// for the ordered and single-card zones: the deck, arsenal, weapon,
    /// combat chain and stack.
    pub const fn of(location: CardLocation) -> Option<Self> {
        use CardLocation::*;
        match location {
            P1Hand | P2Hand => Some(UnorderedZone::Hand),
            P1Pitch | P2Pitch => Some(UnorderedZone::Pitch),
            P1Graveyard | P2Graveyard => Some(UnorderedZone::Graveyard),
            P1BanishZone | P2BanishZone => Some(UnorderedZone::Banish),
            P1IntimidateBanish | P2IntimidateBanish => Some(UnorderedZone::IntimidateBanish),
            P1Head | P2Head | P1Chest | P2Chest | P1Arms | P2Arms | P1Legs | P2Legs => {
                Some(UnorderedZone::Equipment)
            }
            P1Deck | P2Deck | P1Arsenal | P2Arsenal | P1Weapon | P2Weapon
            | P1CombatChain | P2CombatChain | Stack => None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct CardState {
    pub visible :CardVisibleState,
    pub location: CardLocation,
    pub card : Card,
    pub next_card : CardIdx,
    pub prev_card : CardIdx,
    /// Extra defense granted to *this copy* of the card on top of its printed
    /// `defense`, by an effect that pumps one card rather than the player (e.g.
    /// Rally the Rearguard's "gains +3 block"). Unlike the power bonuses, which
    /// are banked on the `Player` because they belong to whichever attack comes
    /// next, this rides the card itself: two copies blocking the same attack are
    /// pumped independently. Counted by `chain_link_total` when the block is
    /// totalled, and cleared as the card leaves the combat chain, so the buff
    /// lasts exactly as long as the combat it was granted in.
    pub defense_bonus : u8,
    /// Whether this copy's activated ability has been used so far this turn, for
    /// the "once per turn" limit on abilities that carry one (e.g. Rally the
    /// Rearguard). Per card rather than per player, so a second copy has its own
    /// activation. Set when the ability is put on the stack — the cost is paid
    /// then, so a countered activation still counts as used — and cleared for
    /// every card at the start of each turn.
    pub ability_used_this_turn : bool,
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
    /// Which deck card enum this player's `DeckCardId`s belong to — the deck
    /// `hero` plays. Fixed when the game is built.
    pub deck : DeckKind,
    /// The hand's contents as a bitfield: how many copies of each of `deck`'s
    /// cards are in hand (see `CardCounts`). The hand is unordered, so this is
    /// all the rules need to know about it. Kept in step with the `hand_idx`
    /// linked list by the engine's attach/detach helpers; the list still exists
    /// so per-copy state (`CardState`) can be reached, and may go away later.
    pub hand : CardCounts,
    /// The pitch zone's contents as a bitfield, kept in step with the
    /// `pitch_idx` list the same way `hand` is with `hand_idx`. The pitch zone
    /// is unordered; the order pitched cards go to the bottom of the deck is
    /// chosen as they leave it (the `PitchOrder` phase), not stored here.
    pub pitch : CardCounts,
    /// The graveyard's contents. The graveyard has no linked list: a card is in
    /// it when its `location` says so, and this bitfield is the zone itself.
    pub graveyard : CardCounts,
    /// The (permanent) banish zone's contents, kept in step with `banish_idx`.
    pub banished : CardCounts,
    /// The Intimidate banish zone's contents, kept in step with
    /// `intimidate_banish_idx`.
    pub intimidate_banished : CardCounts,
    /// The worn equipment still in play across the four equipment zones (head,
    /// chest, arms, legs), kept in step with `head_idx`..`legs_idx`. The slot
    /// indices still exist because activating a piece names the copy; this is
    /// the unordered view of what the player is wearing. The weapon zone is not
    /// an equipment zone and is not counted here.
    pub equipment : CardCounts,
    pub resources: u8,
    pub action_points: u8,
    pub top_deck_idx: Option<CardIdx>,
    pub bottom_deck_idx: Option<CardIdx>,
    pub pitch_idx : Option<CardIdx>,
    pub arsenal_idx : Option<CardIdx>,
    pub hand_idx : Option<CardIdx>,
    pub banish_idx : Option<CardIdx>,
    /// Head of the linked list of cards this player was forced to banish to an
    /// opponent's Intimidate. `None` when no such card has been banished.
    pub intimidate_banish_idx : Option<CardIdx>,
    pub weapon_idx : Option<CardIdx>,
    pub head_idx : Option<CardIdx>,
    pub chest_idx : Option<CardIdx>,
    pub arms_idx : Option<CardIdx>,
    pub legs_idx : Option<CardIdx>,
    /// The combat chain, link by link. Each slot holds the head of the linked
    /// list of cards occupying that chain link, or `None` if the link is empty.
    /// Sized at 5 since a single combat chain is very unlikely to grow longer
    /// than that; a turn that does produce a sixth attack panics rather than
    /// reusing a link and mis-scoring the attack.
    ///
    /// Each attack in a turn takes a link of its own — the attacker's first
    /// attack goes to link 0, a follow-up after Go Again to link 1, and so on —
    /// and the cards blocking an attack join that same link on the defender's
    /// side. Combat damage is calculated a link at a time, so an earlier attack
    /// this turn adds nothing to a later one's power and the cards that blocked
    /// it do not blunt it. A defender's links can therefore have gaps, where an
    /// attack went unblocked.
    pub chain_link : [Option<CardIdx>; 5],
    pub deck_size : u8,
    /// Extra power added to this player's current attack on top of the cards on
    /// the combat chain. Set by on-play effects that conditionally pump the
    /// attacking card (e.g. Bare Fangs's "+2 power if you discarded a card with
    /// 6 or more power"), consumed when combat damage resolves, and reset to 0
    /// afterward (and at the start of each turn) so it never leaks to a later
    /// attack.
    pub attack_power_bonus : u8,
    /// Whether this player's current attack has been granted Go Again by an
    /// on-play effect (e.g. Wild Ride's "if a card with 6 or more power is
    /// discarded, this gains go again"). Like `attack_power_bonus` this is banked
    /// on the player while the attack is on the chain, folded into the Go Again
    /// check when combat damage resolves, and cleared afterward (and at the start
    /// of each turn) so it never leaks into a later attack.
    pub attack_go_again_bonus : bool,
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
    /// Whether this player has resolved an Intimidate trigger so far this turn.
    /// Set when this player intimidates (an attack/action with the Intimidate
    /// keyword resolves) and read by on-play effects gated on "if you've
    /// intimidated this turn" (e.g. Beast Mode's conditional +2 power). Cleared
    /// at the start of each turn so it never carries into a later turn.
    pub has_intimidated : bool,
    /// Whether the card in this player's arsenal (`arsenal_idx`) is face up. A
    /// card arsenaled at the end of a turn goes in face down; a mentor may be
    /// turned face up at the start of its owner's turn (the `MentorFlip` phase),
    /// and a card an effect puts into the arsenal "face up" (Chief Ruk'utan
    /// fetching Alpha Rampage) arrives that way. Face up and down only exist in
    /// the arsenal, which holds one card, so this lives on the player rather
    /// than on every card; it is cleared whenever the arsenal empties (see
    /// `detach_from_current_zone`).
    pub arsenal_face_up : bool,
    /// Lesson counters on the mentor in this player's arsenal (Chief Ruk'utan
    /// gains one each time his ability triggers and graduates at two). A mentor
    /// only collects them while in the arsenal, so like `arsenal_face_up` they
    /// are kept here and cleared whenever the arsenal empties — the counters
    /// never follow the card into another zone.
    pub arsenal_lesson_counters : u8,
    /// This player's view of the game log: what they would legitimately know,
    /// with hidden information (e.g. the identity of a card the opponent drew)
    /// redacted. `None` when logging is disabled (the default, so simulation
    /// and cloning pay nothing); `reset(gs, true)` turns it on. Entries are
    /// pushed at the moment each event happens, so a single `step` may log
    /// several (e.g. a pass that also resolves the top of the stack).
    pub log : Option<Vec<String>>,
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
    pub turn_count : u8,
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
    /// The omniscient game log: every logged event with nothing redacted (the
    /// per-player views with hidden information removed live on `Player::log`).
    /// `None` when logging is disabled (the default, so simulation and cloning
    /// pay nothing); `reset(gs, true)` turns it on. Entries are pushed at the
    /// moment each event happens, so a single `step` may log several (e.g. a
    /// pass that also resolves the top of the stack).
    pub log : Option<Vec<String>>,
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

    /// Turn the copy in global slot `idx` into `card`, as tests do to stage a
    /// specific hand or deck. If the copy is in one of its owner's unordered
    /// zones (hand, pitch, graveyard, ...) that zone's bitfield is relabelled
    /// too, so it stays in step with the cards' locations. `card` must be in
    /// the owner's deck wherever the copy is, so a test can never stage a zone
    /// the per-deck `CardCounts` could not represent.
    #[cfg(test)]
    pub fn set_card(&mut self, idx: usize, card: Card) {
        let owner = if idx < PLAYER_CARDS { &mut self.p1 } else { &mut self.p2 };
        let old = self.cards[idx].card;
        let location = self.cards[idx].location;
        assert!(owner.deck.id_of(card).is_some(),
            "tests may only stage cards from the owner's deck: {:?} is not in the {:?} deck (at {:?})",
            card, owner.deck, location);
        owner.zone_remove(location, old);
        owner.zone_add(location, card);
        self.cards[idx].card = card;
    }

    /// True when logging was enabled by `reset(gs, true)`. Callers should check
    /// this before building log strings so disabled logging costs nothing.
    pub fn logging_enabled(&self) -> bool {
        self.log.is_some()
    }

    /// Append a fully-public event: the same message goes to the omniscient
    /// gamestate log and both players' logs. A no-op when logging is disabled.
    pub fn log_public(&mut self, msg: String) {
        self.log_views(msg.clone(), msg.clone(), msg);
    }

    /// Append an event involving hidden information: `full` (nothing redacted)
    /// goes to the omniscient gamestate log, while `p1` and `p2` carry each
    /// player's legitimate view of it. A no-op when logging is disabled.
    pub fn log_views(&mut self, full: String, p1: String, p2: String) {
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

    /// Append an event only `pid` is entitled to know about: it goes to the
    /// omniscient gamestate log and `pid`'s own log, and the opponent's log gets
    /// no entry at all — not even a redacted one, since the bare fact that the
    /// event happened would itself give something away (e.g. declining to flip a
    /// mentor reveals that there is a mentor in the arsenal). A no-op when
    /// logging is disabled.
    pub fn log_private(&mut self, pid: PlayerIndex, msg: String) {
        let own = if pid == PlayerIndex::P1 { &mut self.p1.log } else { &mut self.p2.log };
        if let Some(log) = own {
            log.push(msg.clone());
        }
        if let Some(log) = &mut self.log {
            log.push(msg);
        }
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
    /// Number of cards in hand, counting every copy.
    #[inline]
    pub const fn hand_size(&self) -> u8 {
        self.hand.len()
    }

    /// `card`'s id in this player's deck. Panics if the deck does not contain
    /// it, since such a card can never be in this player's hand.
    #[inline]
    pub fn deck_id(&self, card: Card) -> DeckCardId {
        match self.deck.id_of(card) {
            Some(id) => id,
            None => panic!("{:?} is not in the {:?} deck", card, self.deck),
        }
    }

    /// Record one more copy of `card` in the hand bitfield. Only updates the
    /// counts; the caller links the copy into `hand_idx`.
    #[inline]
    pub fn hand_add(&mut self, card: Card) {
        self.zone_add(CardLocation::hand(self.pid), card);
    }

    /// Record one copy of `card` leaving the hand bitfield. Only updates the
    /// counts; the caller unlinks the copy from `hand_idx`.
    #[inline]
    pub fn hand_remove(&mut self, card: Card) {
        self.zone_remove(CardLocation::hand(self.pid), card);
    }

    /// How many copies of `card` are in hand, for code that knows which deck
    /// this player has. Debug builds check that `D` is this player's deck.
    #[inline]
    pub fn hand_count<D: DeckCard>(&self, card: D) -> u8 {
        debug_assert_eq!(D::KIND, self.deck, "{:?} is not this player's deck enum", card);
        self.hand.count_as(card)
    }

    /// How many copies of `card` are in hand, for code that does not know the
    /// deck. A card outside the deck is never in hand, so it counts 0.
    #[inline]
    pub fn hand_count_card(&self, card: Card) -> u8 {
        match self.deck.id_of(card) {
            Some(id) => self.hand.count(id),
            None => 0,
        }
    }

    /// The pitch value of every card in hand added up, read from the bitfield:
    /// one catalog lookup per distinct card rather than one per copy.
    pub fn hand_pitch_total(&self) -> u8 {
        self.hand
            .iter()
            .map(|(id, n)| self.deck.card(id).data().pitch * n)
            .sum()
    }

    /// The bitfield for one of this player's unordered zones.
    #[inline]
    pub fn zone(&self, zone: UnorderedZone) -> CardCounts {
        match zone {
            UnorderedZone::Hand => self.hand,
            UnorderedZone::Pitch => self.pitch,
            UnorderedZone::Graveyard => self.graveyard,
            UnorderedZone::Banish => self.banished,
            UnorderedZone::IntimidateBanish => self.intimidate_banished,
            UnorderedZone::Equipment => self.equipment,
        }
    }

    /// Mutable form of `zone`.
    #[inline]
    pub fn zone_mut(&mut self, zone: UnorderedZone) -> &mut CardCounts {
        match zone {
            UnorderedZone::Hand => &mut self.hand,
            UnorderedZone::Pitch => &mut self.pitch,
            UnorderedZone::Graveyard => &mut self.graveyard,
            UnorderedZone::Banish => &mut self.banished,
            UnorderedZone::IntimidateBanish => &mut self.intimidate_banished,
            UnorderedZone::Equipment => &mut self.equipment,
        }
    }

    /// Record one more copy of `card` in the bitfield for `location`. A no-op
    /// when `location` is not an unordered zone. Only
    /// updates the counts; the caller sets the card's `location` and links it
    /// into any list the zone keeps.
    #[inline]
    pub fn zone_add(&mut self, location: CardLocation, card: Card) {
        if let Some(zone) = UnorderedZone::of(location) {
            let id = self.deck_id(card);
            self.zone_mut(zone).add(id);
        }
    }

    /// Record one copy of `card` leaving the bitfield for `location`. A no-op
    /// when `location` is not an unordered zone.
    #[inline]
    pub fn zone_remove(&mut self, location: CardLocation, card: Card) {
        if let Some(zone) = UnorderedZone::of(location) {
            let id = self.deck_id(card);
            self.zone_mut(zone).remove(id);
        }
    }

    /// Check that the `hand` bitfield matches the `hand_idx` linked list:
    /// every copy linked into the hand counted once, and nothing else.
    pub fn debug_check_hand(&self, cards: &[CardState; TOTAL_CARDS]) {
        let mut expected = CardCounts::EMPTY;
        for (_, cs) in self.hand_iter(cards) {
            expected.add(self.deck_id(cs.card));
        }
        assert_eq!(
            self.hand, expected,
            "P{} hand bitfield is out of step with its linked list", self.pid.get() + 1,
        );
    }

    /// Check every unordered-zone bitfield against the cards themselves: walk
    /// this player's half of `cards` and recount each zone from the cards'
    /// `location`s, and check the hand and pitch bitfields against their
    /// linked lists too. Called by `step` and `legal_actions` in debug builds,
    /// so any path that moves a card without going through the engine's
    /// helpers is caught.
    pub fn debug_check_zones(&self, cards: &[CardState; TOTAL_CARDS]) {
        self.debug_check_hand(cards);

        let mut expected = CardCounts::EMPTY;
        for (_, cs) in self.pitch_iter(cards) {
            expected.add(self.deck_id(cs.card));
        }
        assert_eq!(
            self.pitch, expected,
            "P{} pitch bitfield is out of step with its linked list", self.pid.get() + 1,
        );

        let mut recount = [CardCounts::EMPTY; UnorderedZone::ALL.len()];
        let base = self.pid.index() * PLAYER_CARDS;
        for cs in &cards[base..base + PLAYER_CARDS] {
            if let Some(zone) = UnorderedZone::of(cs.location) {
                recount[zone as usize].add(self.deck_id(cs.card));
            }
        }
        for zone in UnorderedZone::ALL {
            assert_eq!(
                self.zone(zone), recount[zone as usize],
                "P{} {:?} bitfield is out of step with the cards' locations", self.pid.get() + 1, zone,
            );
        }
    }

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

    /// Iterate over this player's pitch zone as `(slot_index, CardState)` pairs,
    /// starting at `pitch_idx` and following `next_card` until a node points to
    /// itself. The pitch zone is a linked list just like the hand, so the same
    /// iterator walks it. Yields nothing if the pitch zone is empty.
    pub fn pitch_iter<'a>(&self, cards: &'a [CardState; TOTAL_CARDS]) -> HandIter<'a> {
        HandIter {
            cards,
            current: self.pitch_idx.map(|i| i.get()),
        }
    }

    /// Iterate over the cards on this player's combat chain link `link` as
    /// `(slot_index, CardState)` pairs, head (most recently added) first. Like
    /// the hand, a chain link is a linked list ending at a self-pointing node,
    /// so the same iterator walks it without collecting into a `Vec`. Yields
    /// nothing if the link is empty.
    pub fn chain_link_iter<'a>(&self, link: usize, cards: &'a [CardState; TOTAL_CARDS]) -> HandIter<'a> {
        HandIter {
            cards,
            current: self.chain_link[link].map(|i| i.get()),
        }
    }
}

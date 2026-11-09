//! Helpers shared by the engine's unit tests.
//!
//! Cards are stored by their `DeckCardId` in unordered zones, so tests name a
//! card by its `Card` and ask which zone holds a copy of it, rather than
//! following one particular copy around. Staging helpers (`set_hand`,
//! `relabel`, `seat_on_chain`, ...) may turn one card into another, as a test
//! needs a specific hand, but never create or lose one: every player keeps
//! `PLAYER_CARDS` cards, which `step` and `legal_actions` check in debug builds.
//!
//! The engine keeps its rng outside the `Gamestate`, so tests play a
//! `TestGame`: a game state and the rng it draws from, side by side. It
//! dereferences to the `Gamestate`, so tests read and stage it as one, and the
//! test-side `step` passes its rng to the engine's.

/// A game state together with the rng `step` draws from — what a test plays.
/// Dereferences to the `Gamestate`.
#[derive(Clone)]
pub struct TestGame {
    pub state: Gamestate,
    pub rng: GameRng,
}

impl std::ops::Deref for TestGame {
    type Target = Gamestate;
    fn deref(&self) -> &Gamestate {
        &self.state
    }
}

impl std::ops::DerefMut for TestGame {
    fn deref_mut(&mut self) -> &mut Gamestate {
        &mut self.state
    }
}

/// Apply `act` to `game` with the engine's `step`, drawing from `game`'s rng.
pub fn step(game: &mut TestGame, act: Action) {
    crate::fab_step::step(&mut game.state, &mut game.rng, act);
}

use crate::action::{Action, ActionType, CardRef};
use crate::card_counts::{CardCounts, DeckCardId};
use crate::cards::{Card, CardType};
use crate::fab_game::GameRng;
use crate::game_state::{DeckSlot, Gamestate, PendingCard, PlayerIndex, Zone};

/// `card`'s id in `pid`'s deck.
pub fn id(gs: &Gamestate, pid: PlayerIndex, card: Card) -> DeckCardId {
    gs.player(pid).deck_id(card)
}

/// How many copies of `card` `pid` has in `zone`. The combat chain counts every
/// link; the stack counts the layers that hold one of `pid`'s copies.
pub fn count_in(gs: &Gamestate, pid: PlayerIndex, card: Card, zone: Zone) -> u8 {
    let player = gs.player(pid);
    let Some(id) = player.deck_kind.id_of(card) else {
        return 0;
    };
    match zone {
        Zone::Deck => player.deck.iter().filter(|s| s.card() == id).count() as u8,
        Zone::Arsenal => (player.arsenal == Some(id)) as u8,
        Zone::Weapon => (player.weapon == Some(id)) as u8,
        Zone::CombatChain => player.chain_link.iter().map(|l| l.count(id)).sum(),
        Zone::Stack => gs.stack.iter()
            .filter(|p| p.owner == pid && p.card == id && p.holds_card())
            .count() as u8,
        _ => player.counts(zone).expect("unordered zone").count(id),
    }
}

/// Whether `pid` has a copy of `card` in `zone`.
pub fn in_zone(gs: &Gamestate, pid: PlayerIndex, card: Card, zone: Zone) -> bool {
    count_in(gs, pid, card, zone) > 0
}

/// `card`, after checking `pid` has a copy of it in `zone` — the test-side
/// "find this card" that the engine's copy-free model leaves.
pub fn expect_in(gs: &Gamestate, pid: PlayerIndex, card: Card, zone: Zone, msg: &str) -> Card {
    assert!(in_zone(gs, pid, card, zone), "{} ({:?} not in P{} {:?})", msg, card, pid.get() + 1, zone);
    card
}

/// The zone the active player would take `card` from for an action of type
/// `typ`: a played card from hand (or the arsenal when no copy is in hand), a
/// pitch, block or arsenal from hand, a bottomed card from the pitch zone, a
/// flipped mentor from the arsenal, and an activation from the weapon or
/// equipment zone — or, for any other card, from the combat chain it is
/// defending on.
pub fn source_zone(gs: &Gamestate, typ: ActionType, card: Card) -> Zone {
    let pid = gs.active_player;
    match typ {
        ActionType::PlayCard => {
            // A copy staged in the arsenal is the one meant; play from hand
            // otherwise. `act_from` names the zone outright when both hold one.
            if in_zone(gs, pid, card, Zone::Arsenal) { Zone::Arsenal } else { Zone::Hand }
        }
        ActionType::Pitch | ActionType::Defend | ActionType::Arsenal | ActionType::Reveal => Zone::Hand,
        ActionType::BottomPitch => Zone::Pitch,
        ActionType::FlipFaceUp => Zone::Arsenal,
        ActionType::Activate => match card.data().typ {
            CardType::Weapon => Zone::Weapon,
            CardType::Equipment => Zone::Equipment,
            _ => Zone::CombatChain,
        },
        _ => panic!("{:?} carries no card", typ),
    }
}

/// The active player's action `typ` on `card`, from the zone `source_zone`
/// picks.
pub fn act(gs: &Gamestate, typ: ActionType, card: Card) -> Action {
    act_from(gs, typ, card, source_zone(gs, typ, card))
}

/// The active player's action `typ` on `card` in `zone`.
pub fn act_from(gs: &Gamestate, typ: ActionType, card: Card, zone: Zone) -> Action {
    Action::new(typ, CardRef::new(id(gs, gs.active_player, card), zone))
}

/// Build `act(gs, typ, card)` and step it.
pub fn step_card(gs: &mut TestGame, typ: ActionType, card: Card) {
    let action = act(gs, typ, card);
    step(gs, action);
}

/// Build `act_from(gs, typ, card, zone)` and step it.
#[allow(dead_code)]
pub fn step_card_from(gs: &mut TestGame, typ: ActionType, card: Card, zone: Zone) {
    let action = act_from(gs, typ, card, zone);
    step(gs, action);
}

/// Step an action that carries no card (`Pass`, `ChooseFirst`, ...).
#[allow(dead_code)]
pub fn step_plain(gs: &mut TestGame, typ: ActionType) {
    step(gs, Action::no_card(typ));
}

/// The card an action names, read as the active player's.
pub fn card_of(gs: &Gamestate, action: &Action) -> Card {
    gs.player(gs.active_player).card(action.card_ref().id)
}

/// The card a stack layer names.
pub fn layer_card(gs: &Gamestate, layer: &PendingCard) -> Card {
    gs.player(layer.owner).card(layer.card)
}

/// The card on top of the stack.
pub fn stack_top_card(gs: &Gamestate) -> Option<Card> {
    gs.stack_top().map(|p| layer_card(gs, &p))
}

/// The card currently pending payment.
pub fn pending_card(gs: &Gamestate) -> Option<Card> {
    gs.pending_card.map(|p| layer_card(gs, &p))
}

/// Every card in `counts` of `pid`'s, one entry per copy, in id order.
pub fn cards_in(gs: &Gamestate, pid: PlayerIndex, counts: CardCounts) -> Vec<Card> {
    let player = gs.player(pid);
    counts
        .iter()
        .flat_map(|(id, n)| std::iter::repeat_n(player.card(id), n as usize))
        .collect()
}

/// `pid`'s hand, one entry per copy, in id order.
pub fn hand_cards(gs: &Gamestate, pid: PlayerIndex) -> Vec<Card> {
    cards_in(gs, pid, gs.player(pid).hand)
}

/// `pid`'s deck, top first.
pub fn deck_cards(gs: &Gamestate, pid: PlayerIndex) -> Vec<Card> {
    let player = gs.player(pid);
    player.deck.iter().map(|s| player.card(s.card())).collect()
}

/// Turn one copy of `from` in `pid`'s `zone` into `to`. For the deck, the
/// first copy from the top is relabelled, keeping its place.
pub fn relabel(gs: &mut Gamestate, pid: PlayerIndex, zone: Zone, from: Card, to: Card) {
    let player = gs.player_mut(pid);
    let (from, to) = (player.deck_id(from), player.deck_id(to));
    match zone {
        Zone::Deck => {
            let pos = player.deck.iter().position(|s| s.card() == from).expect("no copy in the deck");
            player.deck.get_mut(pos).unwrap().set_card(to);
        }
        Zone::Arsenal => {
            assert_eq!(player.arsenal, Some(from));
            player.arsenal = Some(to);
        }
        Zone::Weapon => {
            assert_eq!(player.weapon, Some(from));
            player.weapon = Some(to);
        }
        _ => {
            let counts = player.counts_mut(zone).expect("unordered zone");
            counts.remove(from);
            counts.add(to);
            // A relabelled hand card is a different card now: forget it was known.
            if zone == Zone::Hand && player.hand_known.count(from) > player.hand.count(from) {
                player.hand_known.remove(from);
            }
        }
    }
}

/// Replace `pid`'s hand with exactly `desired`. Surplus cards go to the
/// graveyard and the rest are relabelled, so no card is created or lost.
/// Panics if the hand is smaller than requested.
pub fn set_hand(gs: &mut Gamestate, pid: PlayerIndex, desired: &[Card]) {
    let current = hand_cards(gs, pid);
    assert!(current.len() >= desired.len(), "hand too small to set to the requested cards");
    let player = gs.player_mut(pid);
    for &card in &current[desired.len()..] {
        let id = player.deck_id(card);
        player.hand.remove(id);
        player.graveyard.add(id);
    }
    player.hand = CardCounts::EMPTY;
    player.hand_known = CardCounts::EMPTY;
    for &card in desired {
        let id = player.deck_id(card);
        player.hand.add(id);
    }
}

/// Empty `pid`'s hand into their graveyard.
pub fn move_hand_to_graveyard(gs: &mut Gamestate, pid: PlayerIndex) {
    let player = gs.player_mut(pid);
    for (id, n) in player.hand.iter() {
        for _ in 0..n {
            player.graveyard.add(id);
        }
    }
    player.hand = CardCounts::EMPTY;
    player.hand_known = CardCounts::EMPTY;
}

/// Move a copy of `card` already in `pid`'s deck to the top, so it is drawn
/// next.
pub fn put_on_top_of_deck(gs: &mut Gamestate, pid: PlayerIndex, card: Card) {
    let player = gs.player_mut(pid);
    let id = player.deck_id(card);
    let pos = player.deck.iter().position(|s| s.card() == id).expect("no copy in the deck");
    let slot = player.deck.remove(pos).unwrap();
    player.deck.push_front(slot);
}

/// Make the top card of `pid`'s deck a `card` (relabelling the top card in
/// place), and return it.
pub fn set_top_of_deck(gs: &mut Gamestate, pid: PlayerIndex, card: Card) -> Card {
    let player = gs.player_mut(pid);
    let id = player.deck_id(card);
    let top = player.deck.get_mut(0).expect("deck should not be empty");
    *top = DeckSlot::hidden(id);
    card
}

/// Move a card from `pid`'s hand into their (empty) arsenal as `card`, and
/// return it: a copy of `card` itself if the hand holds one, otherwise another
/// hand card relabelled. Mirrors what the Arsenal phase does at the end of a
/// turn, without having to play one out.
pub fn put_in_arsenal(gs: &mut Gamestate, pid: PlayerIndex, card: Card) -> Card {
    let player = gs.player_mut(pid);
    assert!(player.arsenal.is_none(), "the arsenal slot should be free");
    let id = player.deck_id(card);
    let spare = if player.hand.contains(id) {
        id
    } else {
        player.hand.nth(0).expect("hand should hold a card to arsenal")
    };
    player.hand.remove(spare);
    player.arsenal = Some(player.deck_id(card));
    card
}

/// A card in `pid`'s deck whose printed power satisfies `pred`.
pub fn find_deck_card(gs: &Gamestate, pid: PlayerIndex, pred: impl Fn(u8) -> bool) -> Card {
    deck_cards(gs, pid)
        .into_iter()
        .find(|c| pred(c.data().power))
        .expect("the deck should contain a card matching the power predicate")
}

/// Take one card out of `pid`'s hand and put a `card` on their chain link
/// `link` in its place — a staged attacker or blocker.
pub fn seat_on_chain(gs: &mut Gamestate, pid: PlayerIndex, link: usize, card: Card) {
    let player = gs.player_mut(pid);
    let spare = player.hand.nth(0).expect("hand should have a card to seat on the chain");
    player.hand.remove(spare);
    let id = player.deck_id(card);
    player.chain_link[link].add(id);
}

/// Move a copy of `card` from `pid`'s `zone` onto their chain link `link`
/// unchanged (e.g. equipment, as a block would put it there).
pub fn move_to_chain(gs: &mut Gamestate, pid: PlayerIndex, zone: Zone, card: Card, link: usize) {
    let player = gs.player_mut(pid);
    let id = player.deck_id(card);
    crate::fab_step::take_from_zone(player, id, zone);
    player.chain_link[link].add(id);
}

/// Whether a copy of `card` (either player's) is on the stack as a layer that
/// holds the card itself.
pub fn on_stack(gs: &Gamestate, card: Card) -> bool {
    gs.stack.iter().any(|p| p.holds_card() && layer_card(gs, &p) == card)
}

/// The card an action names, if it names one.
pub fn named(gs: &Gamestate, action: &Action) -> Option<Card> {
    action.card.map(|_| card_of(gs, action))
}

/// `pid`'s weapon, which must be in its weapon zone.
pub fn weapon_card(gs: &Gamestate, pid: PlayerIndex) -> Card {
    let player = gs.player(pid);
    player.card(player.weapon.expect("the hero should have its weapon in the weapon zone"))
}

/// The cards on `pid`'s chain link `link`, one entry per copy, in id order.
pub fn link_cards(gs: &Gamestate, pid: PlayerIndex, link: usize) -> Vec<Card> {
    cards_in(gs, pid, gs.player(pid).chain_link[link])
}

/// The card in `pid`'s arsenal.
pub fn arsenal_card(gs: &Gamestate, pid: PlayerIndex) -> Option<Card> {
    let player = gs.player(pid);
    player.arsenal.map(|id| player.card(id))
}

/// The block granted on top of printed defense (Rally the Rearguard's +3) to
/// the chain links holding a copy of `card`, either player's.
pub fn block_bonus(gs: &Gamestate, card: Card) -> u8 {
    [PlayerIndex::P1, PlayerIndex::P2].iter().map(|&pid| {
        let player = gs.player(pid);
        let Some(id) = player.deck_kind.id_of(card) else { return 0 };
        (0..player.chain_link.len())
            .filter(|&l| player.chain_link[l].contains(id))
            .map(|l| player.chain_defense_bonus[l])
            .sum::<u8>()
    }).sum()
}

/// How many copies of `card` have used their once-per-turn ability this turn,
/// across both players.
pub fn uses_this_turn(gs: &Gamestate, card: Card) -> u8 {
    [PlayerIndex::P1, PlayerIndex::P2].iter().map(|&pid| {
        let player = gs.player(pid);
        player.deck_kind.id_of(card).map_or(0, |id| player.abilities_used.count(id))
    }).sum()
}

/// A fresh game between two decklists, dealt and ready for the first
/// decision: the `Decklists`, `gamestate_from_decklists` and `reset` steps in
/// one, with an rng seeded from `seed` (`None` for a random one) that deals
/// the game and that `step` goes on drawing from. Tests that need to reset
/// again build the `Decklists` themselves.
pub fn new_game(p1_deck: [Card; 46], p2_deck: [Card; 46], seed: Option<u64>, logging: bool) -> TestGame {
    let decks = crate::fab_game::Decklists::new(p1_deck, p2_deck);
    let mut game = TestGame {
        state: crate::fab_game::gamestate_from_decklists(&decks),
        rng: crate::fab_game::new_rng(seed),
    };
    crate::fab_game::reset(&mut game.state, &decks, &mut game.rng, logging);
    game
}

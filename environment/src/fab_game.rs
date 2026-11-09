use crate::action::{Action, LegalActions};
use crate::cards::{Card, CardType};
use crate::game_state::{
    empty_stack, DeckSlot, Gamestate, Player, Phase, PlayerIndex, CHAIN_LINKS, DECK_CARDS, PLAYER_CARDS,
};
use crate::card_counts::{CardCounts, DeckCardId};
use crate::deck_cards::DeckKind;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

/// The cards one player starts every game with, read from a decklist: the
/// hero, the deck in decklist order (before shuffling), the worn equipment and
/// the weapon. This is setup, not game state — nothing in a game in progress
/// reads it — so it lives in `Decklists` beside the `Gamestate` rather than in
/// it, and cloning a game state never copies it.
#[derive(Clone, Debug)]
pub struct StartingCards {
    pub hero: Card,
    pub deck: [DeckCardId; DECK_CARDS],
    pub equipment: CardCounts,
    pub weapon: Option<DeckCardId>,
}

impl StartingCards {
    /// Read a 46-card decklist: one hero, then the weapon, equipment and deck
    /// cards in any order. Panics on a decklist the engine cannot represent.
    pub fn from_decklist(decklist: [Card; 46]) -> Self {
        let hero = decklist
            .iter()
            .copied()
            .find(|c| c.data().typ == CardType::Hero)
            .expect("no hero in decklist");
        let kind = DeckKind::for_hero(hero);
        let id = |card: Card| {
            kind.id_of(card)
                .unwrap_or_else(|| panic!("{:?} is not in the {:?} deck", card, kind))
        };

        let mut deck = Vec::with_capacity(DECK_CARDS);
        let mut equipment = CardCounts::EMPTY;
        let mut weapon = None;
        let mut owned = 0;
        for card in decklist {
            match card.data().typ {
                CardType::Hero => continue,
                CardType::Weapon => {
                    assert!(weapon.is_none(), "a decklist holds a single weapon");
                    weapon = Some(id(card));
                }
                CardType::Equipment => equipment.add(id(card)),
                _ => deck.push(id(card)),
            }
            owned += 1;
        }
        assert_eq!(owned, PLAYER_CARDS, "expected exactly {} non-hero cards", PLAYER_CARDS);
        let deck = deck.try_into().unwrap_or_else(|d: Vec<_>| {
            panic!("expected a {}-card deck, got {}", DECK_CARDS, d.len())
        });
        StartingCards { hero, deck, equipment, weapon }
    }
}

/// Both players' starting cards: what `reset` deals a fresh game from.
#[derive(Clone, Debug)]
pub struct Decklists {
    pub p1: StartingCards,
    pub p2: StartingCards,
}

impl Decklists {
    pub fn new(p1_deck: [Card; 46], p2_deck: [Card; 46]) -> Self {
        Decklists {
            p1: StartingCards::from_decklist(p1_deck),
            p2: StartingCards::from_decklist(p2_deck),
        }
    }
}

/// The random number generator a game draws from: the shuffles at `reset`, the
/// coin flip for who chooses to go first, and every random choice `step` makes
/// (Intimidate and random discards, mid-game deck shuffles). It is kept beside
/// the `Gamestate` rather than in it — `FabGame` owns one — so cloning a game
/// state (e.g. for search) neither copies it nor hands the clone the real
/// game's future shuffles.
pub type GameRng = SmallRng;

/// A `GameRng` seeded from `seed`: `Some(seed)` for a reproducible game,
/// `None` for a random seed.
pub fn new_rng(seed: Option<u64>) -> GameRng {
    match seed {
        Some(s) => SmallRng::seed_from_u64(s),
        None => rand::make_rng(),
    }
}

/// Build a `Gamestate` for two players with `decks`' heroes. Every zone starts
/// empty: the game is not playable until `reset` deals it, which is also the
/// single source of truth for life and intellect.
pub fn gamestate_from_decklists(decks: &Decklists) -> Gamestate {
    Gamestate {
        p1: Player::new(PlayerIndex::P1, decks.p1.hero),
        p2: Player::new(PlayerIndex::P2, decks.p2.hero),
        active_player: PlayerIndex::P1,
        turn_player: PlayerIndex::P1,
        passes: 0,
        phase: Phase::Start,
        turn_count: 0,
        stack: empty_stack(),
        pending_card: None,
        choice_return_phase: Phase::Start,
        // Logging is opted into via `reset(gs, decks, rng, true)`; off until then.
        #[cfg(feature = "logging")]
        log: None,
    }
}

/// Reset `gs` to a fresh, playable game dealt from `decks`, which must be the
/// decklists `gs` was built from, drawing the shuffles and the first-player
/// coin flip from `rng`. `logging` opts into the game logs:
/// when `true`, the omniscient `Gamestate::log` and the per-player
/// `Player::log` views are allocated (empty) and `step` appends to them; when
/// `false` all three are `None` and logging costs nothing — the right choice
/// for simulation or heavy gamestate cloning. Without the `logging` Cargo
/// feature the log fields do not exist and `logging` is ignored.
pub fn reset(gs: &mut Gamestate, decks: &Decklists, rng: &mut GameRng, logging: bool) {
    assert!(gs.p1.hero == decks.p1.hero && gs.p2.hero == decks.p2.hero,
        "reset with decklists for other heroes than the game was built with");
    gs.phase = Phase::ChooseFirst;
    gs.stack.clear();
    gs.turn_count = 0;
    gs.pending_card = None;
    gs.passes = 0;

    #[cfg(feature = "logging")]
    {
        let new_log = || if logging { Some(Vec::new()) } else { None };
        gs.log = new_log();
        gs.p1.log = new_log();
        gs.p2.log = new_log();
    }
    #[cfg(not(feature = "logging"))]
    let _ = logging;

    place_cards(gs, decks);
    shuffle_decks(gs, rng);
    set_life_and_intellect(gs);

    gs.active_player = PlayerIndex::new(rng.random_range(0u8..2));
}


pub fn set_life_and_intellect(gs: &mut Gamestate) {
    for player in [&mut gs.p1, &mut gs.p2] {
        player.life = player.hero.data().hero_life;
        player.intellect = player.hero.data().hero_intellect;
    }
}

pub fn shuffle_decks(gs: &mut Gamestate, rng: &mut GameRng) {
    shuffle_deck_for(&mut gs.p1, rng);
    shuffle_deck_for(&mut gs.p2, rng);
}

/// Shuffle `player`'s deck into a random order. Shuffling hides every card in
/// it: a card both players saw go in has an unknown position now. Also used
/// mid-game by effects that search the deck and then shuffle it (Chief
/// Ruk'utan).
pub(crate) fn shuffle_deck_for(player: &mut Player, rng: &mut GameRng) {
    let cards = player.deck.make_contiguous();
    cards.shuffle(rng);
    for slot in cards.iter_mut() {
        slot.set_known(false);
    }
}

pub fn place_cards(gs: &mut Gamestate, decks: &Decklists) {
    place_cards_for(&mut gs.p1, &decks.p1);
    place_cards_for(&mut gs.p2, &decks.p2);
}

/// Deal `player`'s starting cards (`start`) out: every deck card into the deck (in
/// decklist order; `shuffle_decks` shuffles it), the equipment worn and the
/// weapon in its zone, and every other zone emptied, along with the turn
/// resources left over from a played-out game.
fn place_cards_for(player: &mut Player, start: &StartingCards) {
    player.deck.clear();
    for &id in &start.deck {
        player.deck.push_back(DeckSlot::hidden(id));
    }
    player.equipment = start.equipment;
    player.tokens = CardCounts::EMPTY;
    player.weapon = start.weapon;
    player.hand = CardCounts::EMPTY;
    player.hand_known = CardCounts::EMPTY;
    player.pitch = CardCounts::EMPTY;
    player.graveyard = CardCounts::EMPTY;
    player.banished = CardCounts::EMPTY;
    player.intimidate_banished = CardCounts::EMPTY;
    player.arsenal = None;
    player.set_arsenal_face_up(false);
    player.set_arsenal_known(false);
    player.clear_destroy_on_chain_close();
    player.arsenal_lesson_counters = 0;
    player.chain_link = [CardCounts::EMPTY; CHAIN_LINKS];
    player.chain_defense_bonus = [0; CHAIN_LINKS];
    player.abilities_used = CardCounts::EMPTY;
    player.resources = 0;
    player.action_points = 0;
}

/// A game of FaB as the simulator drives it: the decklists it is dealt from
/// together with the state of the game in progress and the rng it draws from.
/// `reset` needs the decklists to deal a new game, and `reset` and `step` the
/// rng; everything else needs only `state`. All three are kept side by side
/// rather than the decklists and rng living (and being cloned) inside every
/// `Gamestate`.
#[derive(Clone)]
pub struct FabGame {
    pub decks: Decklists,
    pub state: Gamestate,
    pub rng: GameRng,
}

impl FabGame {
    /// A game between two decklists, not yet dealt (the simulator resets it
    /// before play). Pass `Some(seed)` for a reproducible game.
    pub fn new(p1_deck: [Card; 46], p2_deck: [Card; 46], seed: Option<u64>) -> Self {
        let decks = Decklists::new(p1_deck, p2_deck);
        let state = gamestate_from_decklists(&decks);
        FabGame { decks, state, rng: new_rng(seed) }
    }
}

/// The generic `simulator::Game` view of a FaB game, bridging the engine's
/// free functions (`reset`, `legal_actions`, `step`) so the simulator can
/// drive a game without knowing anything FaB-specific.
impl crate::simulator::Game for FabGame {
    type Action = Action;

    fn reset(&mut self, logging: bool) {
        reset(&mut self.state, &self.decks, &mut self.rng, logging);
    }

    type ActionList = LegalActions;

    fn legal_actions(&self) -> LegalActions {
        crate::legal_actions::legal_actions(&self.state)
    }

    fn step(&mut self, action: Action) {
        crate::fab_step::step(&mut self.state, &mut self.rng, action);
    }

    fn is_over(&self) -> bool {
        self.state.is_game_over()
    }

    fn player_to_act(&self) -> usize {
        self.state.active_player.index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::deck_cards::{DorintheaCard, RhinarCard};
    use crate::game_state::{CardVisibleState, Zone, STACK_SIZE};
    use crate::test_support::{deck_cards, new_game};

    #[test]
    fn test_reset_cards() {
        let gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), None, false);

        // Each weapon is in its weapon zone, public.
        assert_eq!(gs.p1.weapon, Some(RhinarCard::BoneBasher.id()));
        assert_eq!(gs.p2.weapon, Some(DorintheaCard::Dawnblade.id()));
        assert_eq!(gs.p1.visibility(Zone::Weapon), CardVisibleState::BothKnow);

        // Equipment is worn and public.
        assert_eq!(gs.p1.equipment.count_as(RhinarCard::IronhideLegs), 1);
        assert_eq!(gs.p2.equipment.count_as(DorintheaCard::IronrotLegs), 1);
        assert_eq!(gs.p2.equipment.count_as(DorintheaCard::GallantryGold), 1);
        assert_eq!(gs.p2.visibility(Zone::Equipment), CardVisibleState::BothKnow);

        // The rest of each decklist is in the deck, hidden.
        assert_eq!(gs.p1.deck_size(), 40);
        assert_eq!(gs.p2.deck_size(), 40);
        assert!(gs.p1.deck.iter().all(|slot| !slot.is_known()));
        assert_eq!(gs.p1.deck_slot_visibility(0), Some(CardVisibleState::Hidden));
        gs.debug_check_card_totals();
    }

    #[test]
    fn reset_counts_worn_equipment_and_empties_other_zones() {
        let gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        // Four pieces of worn equipment each; the weapon zone is not counted.
        assert_eq!(gs.p1.equipment.len(), 4);
        assert_eq!(gs.p1.equipment.count_as(RhinarCard::BoneBasher), 0);
        assert_eq!(gs.p2.equipment.len(), 4);
        for p in [&gs.p1, &gs.p2] {
            assert!(p.hand.is_empty());
            assert!(p.pitch.is_empty());
            assert!(p.graveyard.is_empty());
            assert!(p.banished.is_empty());
            assert!(p.intimidate_banished.is_empty());
            assert!(p.arsenal.is_none());
            assert!(!p.chain_occupied());
        }
    }

    #[test]
    fn reset_restores_a_played_out_game() {
        let decks = Decklists::new(build_rhinar_deck(), build_dorinthea_deck());
        let mut rng = new_rng(Some(7));
        let mut gs = gamestate_from_decklists(&decks);
        reset(&mut gs, &decks, &mut rng, false);
        // Scatter some cards, then reset: everything is dealt out afresh.
        let top = gs.p1.deck.pop_front().unwrap().card();
        gs.p1.graveyard.add(top);
        gs.p1.weapon = None;
        gs.p1.arsenal = Some(top);
        reset(&mut gs, &decks, &mut rng, false);
        assert_eq!(gs.p1.deck_size(), 40);
        assert!(gs.p1.graveyard.is_empty());
        assert!(gs.p1.arsenal.is_none());
        assert_eq!(gs.p1.weapon, Some(RhinarCard::BoneBasher.id()));
        gs.debug_check_card_totals();
    }

    #[test]
    fn the_same_seed_deals_the_same_decks() {
        let deal = |seed| {
            let gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(seed), false);
            gs.p1.deck.iter().map(|s| s.card().get()).collect::<Vec<_>>()
        };
        assert_eq!(deal(42), deal(42));
        assert_ne!(deal(42), deal(43));
    }

    #[test]
    #[should_panic(expected = "cards across its zones and the stack")]
    fn losing_a_card_is_caught() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        // Take a card off the deck without putting it anywhere.
        gs.p1.deck.pop_front();
        gs.debug_check_card_totals();
    }

    #[test]
    fn a_deck_slot_packs_card_and_known_bit_into_one_byte() {
        use crate::card_counts::{DeckCardId, MAX_DECK_IDS};
        assert_eq!(std::mem::size_of::<DeckSlot>(), 1);
        for i in 0..MAX_DECK_IDS {
            let id = DeckCardId::new(i);
            let mut slot = DeckSlot::hidden(id);
            assert_eq!((slot.card(), slot.is_known()), (id, false));
            slot.set_known(true);
            assert_eq!((slot.card(), slot.is_known()), (id, true));
            assert_eq!(slot, DeckSlot::known(id));
            // Relabelling keeps the known bit.
            let other = DeckCardId::new(MAX_DECK_IDS - 1 - i);
            slot.set_card(other);
            assert_eq!((slot.card(), slot.is_known()), (other, true));
            slot.set_known(false);
            assert_eq!(slot, DeckSlot::hidden(other));
        }
    }

    #[test]
    fn starting_cards_keep_the_decklist_order() {
        let decks = Decklists::new(build_rhinar_deck(), build_dorinthea_deck());
        let playable: Vec<_> = build_rhinar_deck().into_iter()
            .filter(|c| !matches!(c.data().typ, CardType::Hero | CardType::Weapon | CardType::Equipment))
            .map(|c| DeckKind::Rhinar.id_of(c).unwrap())
            .collect();
        assert_eq!(decks.p1.deck.to_vec(), playable);
        assert_eq!(decks.p1.hero, Card::Rhinar);
        assert_eq!(decks.p1.weapon, Some(RhinarCard::BoneBasher.id()));
        assert_eq!(decks.p2.equipment.len(), 4);
    }

    #[test]
    fn a_game_is_dealt_the_same_way_from_the_same_seed() {
        // `reset` deals from the decklists, so dealing the same decklists with
        // the same seed twice gives the same game, however it was played out
        // in between.
        let decks = Decklists::new(build_rhinar_deck(), build_dorinthea_deck());
        let deal = || {
            let mut rng = new_rng(Some(42));
            let mut gs = gamestate_from_decklists(&decks);
            reset(&mut gs, &decks, &mut rng, false);
            (gs, rng)
        };
        let (a, _) = deal();
        let (mut b, mut b_rng) = deal();
        crate::fab_step::step(&mut b, &mut b_rng, Action::no_card(crate::action::ActionType::ChooseFirst));
        let mut b_rng = new_rng(Some(42));
        reset(&mut b, &decks, &mut b_rng, false);
        assert_eq!(deck_cards(&a, PlayerIndex::P1), deck_cards(&b, PlayerIndex::P1));
        assert_eq!(deck_cards(&a, PlayerIndex::P2), deck_cards(&b, PlayerIndex::P2));
    }

    #[test]
    #[should_panic(expected = "decklists for other heroes")]
    fn reset_refuses_decklists_for_other_heroes() {
        let decks = Decklists::new(build_rhinar_deck(), build_dorinthea_deck());
        let swapped = Decklists::new(build_dorinthea_deck(), build_rhinar_deck());
        let mut gs = gamestate_from_decklists(&decks);
        reset(&mut gs, &swapped, &mut new_rng(Some(42)), false);
    }

    #[test]
    fn stack_capacity_is_the_stack_size() {
        let gs = gamestate_from_decklists(&Decklists::new(build_rhinar_deck(), build_dorinthea_deck()));
        assert_eq!(gs.stack.capacity(), STACK_SIZE);
    }
}

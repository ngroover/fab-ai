use crate::action::{Action, LegalActions};
use crate::cards::{Card, CardType};
use crate::game_state::{
    empty_deck, empty_stack, Deck, DeckSlot, Gamestate, Player, Phase, PlayerIndex, CHAIN_LINKS, PLAYER_CARDS,
};
use crate::card_counts::CardCounts;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

/// Build a `Gamestate` from two decklists.
/// Pass `Some(seed)` for a reproducible game, or `None` for a random seed.
pub fn gamestate_from_decklists(p1_deck: [Card; 46], p2_deck: [Card; 46], seed: Option<u64>) -> Gamestate {
    let rng: SmallRng = match seed {
        Some(s) => SmallRng::seed_from_u64(s),
        None => rand::make_rng(),
    };

    Gamestate {
        p1: player_from_decklist(p1_deck, PlayerIndex::P1),
        p2: player_from_decklist(p2_deck, PlayerIndex::P2),
        active_player: PlayerIndex::P1,
        turn_player: PlayerIndex::P1,
        passes: 0,
        phase: Phase::Start,
        turn_count: 0,
        rng,
        stack: empty_stack(),
        pending_card: None,
        // Logging is opted into via `reset(gs, true)`; off until then.
        log: None,
    }
}

/// Build a player from a decklist: the hero, plus the starting cards `reset`
/// deals out — the deck in decklist order, the worn equipment and the weapon.
/// Every zone starts empty; a `Gamestate` is not playable until `reset` has
/// run, which is also the single source of truth for life and intellect.
fn player_from_decklist(deck: [Card; 46], pid: PlayerIndex) -> Player {
    let hero = deck
        .iter()
        .copied()
        .find(|c| c.data().typ == CardType::Hero)
        .expect("no hero in decklist");
    let mut player = Player::new(pid, hero);

    let mut starting_deck: Deck = empty_deck();
    let mut equipment = CardCounts::EMPTY;
    let mut weapon = None;
    let mut owned = 0;
    for card in deck {
        match card.data().typ {
            CardType::Hero => continue,
            CardType::Weapon => {
                assert!(weapon.is_none(), "a decklist holds a single weapon");
                weapon = Some(player.deck_id(card));
            }
            CardType::Equipment => equipment.add(player.deck_id(card)),
            _ => starting_deck.push_back(DeckSlot::hidden(player.deck_id(card))),
        }
        owned += 1;
    }
    assert_eq!(owned, PLAYER_CARDS, "expected exactly {} non-hero cards", PLAYER_CARDS);
    player.starting_deck = starting_deck;
    player.starting_equipment = equipment;
    player.starting_weapon = weapon;
    player
}


/// Reset `gs` to a fresh, playable game. `logging` opts into the game logs:
/// when `true`, the omniscient `Gamestate::log` and the per-player
/// `Player::log` views are allocated (empty) and `step` appends to them; when
/// `false` all three are `None` and logging costs nothing — the right choice
/// for simulation or heavy gamestate cloning.
pub fn reset(gs: &mut Gamestate, logging: bool) {
    gs.phase = Phase::ChooseFirst;
    gs.stack.clear();
    gs.turn_count = 0;
    gs.pending_card = None;
    gs.passes = 0;

    let new_log = || if logging { Some(Vec::new()) } else { None };
    gs.log = new_log();
    gs.p1.log = new_log();
    gs.p2.log = new_log();

    place_cards(gs);
    shuffle_decks(gs);
    set_life_and_intellect(gs);

    gs.active_player = PlayerIndex::new(gs.rng.random_range(0u8..2));
}


pub fn set_life_and_intellect(gs: &mut Gamestate) {
    for player in [&mut gs.p1, &mut gs.p2] {
        player.life = player.hero.data().hero_life;
        player.intellect = player.hero.data().hero_intellect;
    }
}

pub fn shuffle_decks(gs: &mut Gamestate) {
    shuffle_deck_for(&mut gs.p1, &mut gs.rng);
    shuffle_deck_for(&mut gs.p2, &mut gs.rng);
}

/// Shuffle `player`'s deck into a random order. Shuffling hides every card in
/// it: a card both players saw go in has an unknown position now. Also used
/// mid-game by effects that search the deck and then shuffle it (Chief
/// Ruk'utan).
pub(crate) fn shuffle_deck_for(player: &mut Player, rng: &mut SmallRng) {
    let cards = player.deck.make_contiguous();
    cards.shuffle(rng);
    for slot in cards.iter_mut() {
        slot.known = false;
    }
}

pub fn place_cards(gs: &mut Gamestate) {
    place_cards_for(&mut gs.p1);
    place_cards_for(&mut gs.p2);
}

/// Deal `player`'s starting cards back out: every deck card into the deck (in
/// decklist order; `shuffle_decks` shuffles it), the equipment worn and the
/// weapon in its zone, and every other zone emptied, along with the turn
/// resources left over from a played-out game.
fn place_cards_for(player: &mut Player) {
    player.deck = player.starting_deck;
    player.equipment = player.starting_equipment;
    player.weapon = player.starting_weapon;
    player.hand = CardCounts::EMPTY;
    player.pitch = CardCounts::EMPTY;
    player.graveyard = CardCounts::EMPTY;
    player.banished = CardCounts::EMPTY;
    player.intimidate_banished = CardCounts::EMPTY;
    player.arsenal = None;
    player.arsenal_face_up = false;
    player.arsenal_lesson_counters = 0;
    player.chain_link = [CardCounts::EMPTY; CHAIN_LINKS];
    player.chain_defense_bonus = [0; CHAIN_LINKS];
    player.abilities_used = CardCounts::EMPTY;
    player.resources = 0;
    player.action_points = 0;
}

/// The generic `simulator::Game` view of the FaB gamestate, bridging the
/// engine's free functions (`reset`, `legal_actions`, `step`) so the simulator
/// can drive a game without knowing anything FaB-specific.
impl crate::simulator::Game for Gamestate {
    type Action = Action;

    fn reset(&mut self, logging: bool) {
        reset(self, logging);
    }

    type ActionList = LegalActions;

    fn legal_actions(&self) -> LegalActions {
        crate::legal_actions::legal_actions(self)
    }

    fn step(&mut self, action: Action) {
        crate::fab_step::step(self, action);
    }

    fn is_over(&self) -> bool {
        self.is_game_over()
    }

    fn player_to_act(&self) -> usize {
        self.active_player.index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::deck_cards::{DorintheaCard, RhinarCard};
    use crate::game_state::{CardVisibleState, Zone, STACK_SIZE};

    #[test]
    fn test_reset_cards() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), None);
        reset(&mut gs, false);

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
        assert!(gs.p1.deck.iter().all(|slot| !slot.known));
        assert_eq!(gs.p1.deck_slot_visibility(0), Some(CardVisibleState::Hidden));
        gs.debug_check_card_totals();
    }

    #[test]
    fn reset_counts_worn_equipment_and_empties_other_zones() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);
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
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(7));
        reset(&mut gs, false);
        // Scatter some cards, then reset: everything is dealt out afresh.
        let top = gs.p1.deck.pop_front().unwrap().card;
        gs.p1.graveyard.add(top);
        gs.p1.weapon = None;
        gs.p1.arsenal = Some(top);
        reset(&mut gs, false);
        assert_eq!(gs.p1.deck_size(), 40);
        assert!(gs.p1.graveyard.is_empty());
        assert!(gs.p1.arsenal.is_none());
        assert_eq!(gs.p1.weapon, Some(RhinarCard::BoneBasher.id()));
        gs.debug_check_card_totals();
    }

    #[test]
    fn the_same_seed_deals_the_same_decks() {
        let deal = |seed| {
            let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
            reset(&mut gs, false);
            gs.p1.deck.iter().map(|s| s.card.get()).collect::<Vec<_>>()
        };
        assert_eq!(deal(42), deal(42));
        assert_ne!(deal(42), deal(43));
    }

    #[test]
    #[should_panic(expected = "cards across its zones and the stack")]
    fn losing_a_card_is_caught() {
        let mut gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        reset(&mut gs, false);
        // Take a card off the deck without putting it anywhere.
        gs.p1.deck.pop_front();
        gs.debug_check_card_totals();
    }

    #[test]
    fn stack_capacity_is_the_stack_size() {
        let gs = gamestate_from_decklists(build_rhinar_deck(), build_dorinthea_deck(), Some(42));
        assert_eq!(gs.stack.capacity(), STACK_SIZE);
    }
}

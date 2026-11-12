//! Observation encoding: one player's view of a `Gamestate` as a flat vector
//! of `f32`s, the input of a policy/value network.
//!
//! The vector holds only what `observer` may know. Their own hand, arsenal and
//! Intimidate banish zone are written in full; of the opponent's, only the
//! copies `observer` has seen (`Player::hand_known`, a face-up or deduced
//! arsenal card, a card announced and being paid for). What `observer` cannot
//! place is written as a multiset of where it *could* be — the "unseen" plane
//! — which is the same for every state they cannot tell apart from this one.
//! So `shuffle_hidden_information` never changes a player's observation, which
//! is what the tests below check.
//!
//! Everything is relative to `observer`: their own side comes first, then the
//! opponent's in the same layout, so one network plays either seat.
//!
//! Cards are indexed by their game-wide `Card` number rather than by
//! `DeckCardId`, so a plane means the same whichever deck its owner plays and
//! a card both decks hold (Titanium Bauble) shares a slot.
//!
//! Layout, in order (`layout()` lists every section with its offset):
//! - **global** — phase, whose turn and priority it is, turn count;
//! - **self**, then **opponent** — hero, `PLAYER_PLANES` card planes of
//!   `Card::COUNT` copy counts each, scalars, flags;
//! - **pending** — the card being paid for, as one layer;
//! - **stack** — `STACK_SIZE` layers, the top of the stack first.
//!
//! Counts with a natural maximum are scaled into 0..=1 (life, deck size, turn
//! count); copy counts and small bonuses are written as they are.

use crate::action::ActionType;
use crate::card_counts::CardCounts;
use crate::cards::Card;
use crate::deck_cards::{DeckKind, TokenCard};
use crate::fab_step::current_chain_link;
use crate::game_state::{
    Gamestate, PendingCard, Phase, Player, PlayerFlags, PlayerIndex, Zone, CHAIN_LINKS,
    DECK_CARDS, EQUIPMENT_SLOTS, MAX_QUICKENS, MAX_TURNS, STACK_SIZE, WEAPON_ATTACK_ORDINALS,
};

/// Width of a card plane: one slot per `Card`.
const CARDS: usize = Card::COUNT;
/// Width of a phase one-hot.
const PHASES: usize = Phase::Draw as usize + 1;
/// Width of a zone one-hot.
const ZONES: usize = Zone::Stack as usize + 1;
/// Width of a hero one-hot.
const HEROES: usize = DeckKind::Dorinthea as usize + 1;
/// How many `PlayerFlags` bits there are; each is written as its own 0/1.
const FLAGS: usize = 14;
const _: () = assert!(
    PlayerFlags::all().bits() == (1 << FLAGS) - 1,
    "PlayerFlags changed: update observation::FLAGS",
);

/// The actions that can commit a layer: `PlayCard`, `Activate`, `DefendTrigger`.
const LAYER_TYPES: usize = 3;
/// One stack layer (or the pending card): present, mine, the card, how it was
/// committed, the zone it came from.
const LAYER_SIZE: usize = 2 + CARDS + LAYER_TYPES + ZONES;

/// Card planes per player; see `encode_player` for what each holds.
const PLAYER_PLANES: usize = 15;
/// Scalars per player: the single ones, plus one per weapon attack ordinal and
/// per equipment slot; see `encode_player`.
const PLAYER_SCALARS: usize = 23 + WEAPON_ATTACK_ORDINALS + EQUIPMENT_SLOTS;
const PLAYER_SIZE: usize = HEROES + PLAYER_PLANES * CARDS + PLAYER_SCALARS + FLAGS;
const GLOBAL_SIZE: usize = 2 * PHASES + 4;

/// Length of an observation vector.
pub const OBS_SIZE: usize = GLOBAL_SIZE + 2 * PLAYER_SIZE + (1 + STACK_SIZE) * LAYER_SIZE;

/// An observation vector.
#[allow(dead_code)] // for the network agent to come; exercised by the tests
pub type Observation = [f32; OBS_SIZE];

/// A named stretch of the observation vector: `name` covers
/// `offset..offset + len`.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub offset: usize,
    pub len: usize,
}

/// Writes the vector front to back. With `sections` set it also records where
/// each named section starts, which is how `layout` learns the layout from the
/// encoder itself instead of repeating it.
struct Writer<'a> {
    out: &'a mut [f32],
    pos: usize,
    sections: Option<Vec<Section>>,
}

impl Writer<'_> {
    /// Start the section `prefix` + `name` at the current position.
    #[inline]
    fn section(&mut self, prefix: &str, name: &str) {
        let pos = self.pos;
        if let Some(sections) = &mut self.sections {
            if let Some(last) = sections.last_mut() {
                last.len = pos - last.offset;
            }
            sections.push(Section { name: format!("{prefix}{name}"), offset: pos, len: 0 });
        }
    }

    #[inline]
    fn scalar(&mut self, value: f32) {
        self.out[self.pos] = value;
        self.pos += 1;
    }

    #[inline]
    fn flag(&mut self, on: bool) {
        self.scalar(on as u8 as f32);
    }

    /// `len` slots, a 1 at `index` if there is one.
    #[inline]
    fn one_hot(&mut self, index: Option<usize>, len: usize) {
        if let Some(i) = index {
            debug_assert!(i < len);
            self.out[self.pos + i] = 1.0;
        }
        self.pos += len;
    }

    /// A card plane with nothing in it yet; the caller adds copies to it.
    #[inline]
    fn plane(&mut self) -> &mut [f32] {
        let plane = &mut self.out[self.pos..self.pos + CARDS];
        self.pos += CARDS;
        plane
    }

    /// A card plane holding `counts`, a zone of `owner`'s.
    #[inline]
    fn counts(&mut self, owner: &Player, counts: CardCounts) {
        let plane = self.plane();
        add_counts(plane, owner, counts);
    }

    /// A card plane holding the single card `card` of `owner`'s, if any.
    #[inline]
    fn card(&mut self, owner: &Player, card: Option<crate::card_counts::DeckCardId>) {
        let plane = self.plane();
        if let Some(id) = card {
            plane[owner.card(id) as usize] += 1.0;
        }
    }
}

/// Add `counts`, a zone of `owner`'s, to `plane`.
#[inline]
fn add_counts(plane: &mut [f32], owner: &Player, counts: CardCounts) {
    for (id, copies) in counts.iter() {
        plane[owner.card(id) as usize] += copies as f32;
    }
}

/// Encode `observer`'s view of `gs` into `out`, overwriting all of it.
#[allow(dead_code)] // for the network agent to come; exercised by the tests
pub fn encode_into(gs: &Gamestate, observer: PlayerIndex, out: &mut Observation) {
    out.fill(0.0);
    let mut w = Writer { out, pos: 0, sections: None };
    encode(gs, observer, &mut w);
}

/// `observer`'s view of `gs` as a freshly allocated vector. A search should
/// reuse a buffer with `encode_into` instead.
#[allow(dead_code)] // for callers outside a search loop; exercised by the tests
pub fn observation(gs: &Gamestate, observer: PlayerIndex) -> Vec<f32> {
    let mut out = vec![0.0; OBS_SIZE];
    let mut w = Writer { out: &mut out, pos: 0, sections: None };
    encode(gs, observer, &mut w);
    out
}

/// Every section of the observation vector, in order, covering all of it. For
/// a trainer or a debugger that needs to find a feature by name; the layout is
/// fixed, so it is the same for every game.
#[allow(dead_code)] // for exporting the layout; exercised by the tests
pub fn layout() -> Vec<Section> {
    // The layout does not depend on the state, so encode any one.
    let decks = crate::fab_game::Decklists::new(
        crate::decks::build_rhinar_deck(),
        crate::decks::build_dorinthea_deck(),
    );
    let gs = crate::fab_game::gamestate_from_decklists(&decks);
    let mut out = vec![0.0; OBS_SIZE];
    let mut w = Writer { out: &mut out, pos: 0, sections: Some(Vec::new()) };
    encode(&gs, PlayerIndex::P1, &mut w);
    let mut sections = w.sections.take().expect("recording sections");
    if let Some(last) = sections.last_mut() {
        last.len = OBS_SIZE - last.offset;
    }
    sections
}

fn encode(gs: &Gamestate, observer: PlayerIndex, w: &mut Writer) {
    w.section("", "phase");
    w.one_hot(Some(gs.phase as usize), PHASES);
    // The phase a paused play returns to; stale outside the choice.
    w.section("", "choice_return_phase");
    let returning = (gs.phase == Phase::RevealChoice).then_some(gs.choice_return_phase as usize);
    w.one_hot(returning, PHASES);
    w.section("", "turn");
    w.flag(gs.active_player == observer);
    w.flag(gs.turn_player == observer);
    w.scalar(gs.passes as f32);
    w.scalar(gs.turn_count as f32 / MAX_TURNS as f32);
    debug_assert_eq!(w.pos, GLOBAL_SIZE);

    // Both sides of the combat chain share the attacker's link numbering.
    let link = current_chain_link(gs.player(gs.turn_player));
    encode_player(gs, gs.player(observer), true, link, "self.", w);
    encode_player(gs, gs.player(observer.opponent()), false, link, "opponent.", w);

    w.section("", "pending");
    encode_layer(gs, gs.pending_card, observer, w);
    w.section("", "stack");
    let mut layers = gs.stack.iter().rev();
    for _ in 0..STACK_SIZE {
        encode_layer(gs, layers.next(), observer, w);
    }

    assert_eq!(w.pos, OBS_SIZE, "the encoder and OBS_SIZE disagree");
}

/// One player's side, as `observer` sees it: `own` when the player is the
/// observer, who then sees their own private zones.
fn encode_player(gs: &Gamestate, p: &Player, own: bool, link: usize, prefix: &str, w: &mut Writer) {
    let start = w.pos;
    let pending = gs.pending_in_zone().filter(|pending| pending.owner == p.pid);
    let pending_from = |zone: Zone| pending.filter(|pending| pending.from == zone).map(|pending| pending.card);

    // The hand copies the other player can place: the ones they have seen,
    // and a card announced from hand unless one of those accounts for it.
    let mut hand_seen = p.hand_known;
    if let Some(card) = pending_from(Zone::Hand) {
        if !hand_seen.contains(card) {
            hand_seen.add(card);
        }
    }
    let arsenal_public = p.arsenal.is_some()
        && (p.arsenal_face_up() || p.arsenal_known() || pending_from(Zone::Arsenal).is_some());

    w.section(prefix, "hero");
    w.one_hot(Some(p.deck_kind as usize), HEROES);

    // ── card planes ──
    w.section(prefix, "hand");
    w.counts(p, if own { p.hand } else { hand_seen });
    // The hand copies the *other* player has seen: for the observer's own
    // hand, what the opponent knows of it.
    w.section(prefix, "hand_seen");
    w.counts(p, hand_seen);
    w.section(prefix, "arsenal");
    w.card(p, p.arsenal.filter(|_| own || arsenal_public));
    w.section(prefix, "pitch");
    w.counts(p, p.pitch);
    w.section(prefix, "graveyard");
    w.counts(p, p.graveyard);
    w.section(prefix, "banished");
    w.counts(p, p.banished);
    // Face down: the owner's alone to see.
    w.section(prefix, "intimidate_banished");
    w.counts(p, if own { p.intimidate_banished } else { CardCounts::EMPTY });
    w.section(prefix, "equipment");
    w.counts(p, p.equipment);
    w.section(prefix, "weapon");
    w.card(p, p.weapon);
    // The cards the observer cannot place, as a multiset. Their own: the deck
    // slots in unknown order. The opponent's: one pool of everything hidden —
    // unseen hand copies, the Intimidate banish zone, a face-down arsenal
    // card and the unknown deck slots — exactly what a re-deal shuffles.
    w.section(prefix, "unseen");
    let unseen = w.plane();
    for slot in p.deck.iter().filter(|slot| !slot.is_known()) {
        unseen[p.card(slot.card()) as usize] += 1.0;
    }
    if !own {
        add_counts(unseen, p, p.hand);
        for (id, seen) in hand_seen.iter() {
            unseen[p.card(id) as usize] -= seen as f32;
        }
        add_counts(unseen, p, p.intimidate_banished);
        if let Some(id) = p.arsenal.filter(|_| !arsenal_public) {
            unseen[p.card(id) as usize] += 1.0;
        }
    }
    // Deck cards both players have seen go in (pitched cards put on the
    // bottom, revealed cards), and the top card if it is one of them.
    w.section(prefix, "deck_known");
    let known = w.plane();
    for slot in p.deck.iter().filter(|slot| slot.is_known()) {
        known[p.card(slot.card()) as usize] += 1.0;
    }
    w.section(prefix, "deck_top");
    w.card(p, p.deck.front().filter(|slot| slot.is_known()).map(|slot| slot.card()));
    // The combat chain: this player's cards on the current link (the attack,
    // or the blockers), and on the earlier links taken together.
    w.section(prefix, "chain_link");
    w.counts(p, p.chain_link[link]);
    w.section(prefix, "chain_earlier");
    let earlier = w.plane();
    for i in (0..CHAIN_LINKS).filter(|&i| i != link) {
        add_counts(earlier, p, p.chain_link[i]);
    }
    w.section(prefix, "abilities_used");
    w.counts(p, p.abilities_used);
    debug_assert_eq!(w.pos - start, HEROES + PLAYER_PLANES * CARDS);

    // ── scalars ──
    w.section(prefix, "scalars");
    let scalars = w.pos;
    w.scalar(p.life as f32 / 20.0);
    w.scalar(p.intellect as f32 / 4.0);
    w.scalar(p.resources as f32);
    w.scalar(p.action_points as f32);
    w.scalar(p.hand_size() as f32 / 4.0);
    w.scalar(p.deck.len() as f32 / DECK_CARDS as f32);
    w.scalar(p.intimidate_banished.len() as f32);
    w.flag(p.arsenal.is_some());
    w.flag(arsenal_public);
    w.scalar(p.arsenal_lesson_counters as f32);
    w.scalar(p.token_count(TokenCard::QuickenToken) as f32 / MAX_QUICKENS as f32);
    w.scalar(p.attack_power_bonus as f32);
    w.scalar(p.next_brute_attack_action_bonus as f32);
    w.scalar(p.next_attack_action_bonus as f32);
    w.scalar(p.next_brute_attack_conditional_bonus as f32);
    w.scalar(p.next_weapon_attack_bonus as f32);
    w.scalar(p.next_attack_bonus as f32);
    w.scalar(p.next_sword_attack_bonus as f32);
    w.scalar(p.weapon_power_counters as f32);
    w.scalar(p.dawnblade_hit_counter_effects as f32);
    for power in p.weapon_attack_power {
        w.scalar(power as f32);
    }
    w.scalar(p.weapon_attacks_power_this_turn as f32);
    for counters in p.equipment_defense_counters {
        w.scalar(counters as f32);
    }
    w.scalar(p.chain_defense_bonus[link] as f32);
    w.scalar(p.chain_links_open as f32);
    debug_assert_eq!(w.pos - scalars, PLAYER_SCALARS);

    // Every flag follows from plays both players watched.
    w.section(prefix, "flags");
    for bit in 0..FLAGS {
        w.flag(p.flags.bits() & (1 << bit) != 0);
    }
    debug_assert_eq!(w.pos - start, PLAYER_SIZE);
}

/// One stack layer, or the pending card: all zeros for `None`. Layers are
/// public — a card is announced before it is paid for.
fn encode_layer(gs: &Gamestate, layer: Option<PendingCard>, observer: PlayerIndex, w: &mut Writer) {
    let Some(layer) = layer else {
        w.pos += LAYER_SIZE;
        return;
    };
    w.flag(true);
    w.flag(layer.owner == observer);
    w.one_hot(Some(gs.player(layer.owner).card(layer.card) as usize), CARDS);
    let typ = match layer.typ {
        ActionType::PlayCard => Some(0),
        ActionType::Activate => Some(1),
        ActionType::DefendTrigger => Some(2),
        other => {
            debug_assert!(false, "{other:?} does not commit a layer");
            None
        }
    };
    w.one_hot(typ, LAYER_TYPES);
    w.one_hot(Some(layer.from as usize), ZONES);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::determinize::shuffle_hidden_information;
    use crate::fab_game::{new_rng, FabGame};
    use crate::simulator::Game;

    fn new_game(seed: u64) -> FabGame {
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
        game.reset(false);
        game
    }

    /// A game with both hands dealt.
    fn dealt_game(seed: u64) -> FabGame {
        let mut game = new_game(seed);
        while game.state.p1.hand.is_empty() || game.state.p2.hand.is_empty() {
            let legal = game.legal_actions();
            game.step(legal[0]);
        }
        game
    }

    /// Play `game` to the end with random actions, calling `visit` at every
    /// decision.
    fn play_out(game: &mut FabGame, seed: u64, mut visit: impl FnMut(&FabGame)) {
        let mut choose = new_rng(Some(seed));
        while !game.is_over() {
            visit(game);
            let legal = game.legal_actions();
            let pick = rand::RngExt::random_range(&mut choose, 0..legal.len());
            game.step(legal[pick]);
        }
    }

    /// The part of `obs` the section `name` covers.
    fn section<'a>(layout: &[Section], obs: &'a [f32], name: &str) -> &'a [f32] {
        let s = layout.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no section {name}"));
        &obs[s.offset..s.offset + s.len]
    }

    #[test]
    fn layout_covers_the_whole_vector() {
        let layout = layout();
        println!("OBS_SIZE = {OBS_SIZE}, Card::COUNT = {}", Card::COUNT);
        let mut next = 0;
        for s in &layout {
            assert_eq!(s.offset, next, "{} does not follow the section before it", s.name);
            assert!(s.len > 0, "{} is empty", s.name);
            next += s.len;
        }
        assert_eq!(next, OBS_SIZE);
        // Both players' sides have the same layout.
        let lens = |prefix: &str| -> Vec<(String, usize)> {
            layout.iter()
                .filter_map(|s| s.name.strip_prefix(prefix).map(|n| (n.to_string(), s.len)))
                .collect()
        };
        assert!(!lens("self.").is_empty());
        assert_eq!(lens("self."), lens("opponent."));
    }

    #[test]
    fn encode_into_matches_observation_and_clears_the_buffer() {
        let game = dealt_game(3);
        let mut buffer = [7.0; OBS_SIZE];
        encode_into(&game.state, PlayerIndex::P1, &mut buffer);
        assert_eq!(&buffer[..], &observation(&game.state, PlayerIndex::P1)[..]);
    }

    /// A player sees their own hand card for card, and of the opponent's only
    /// how many there are.
    #[test]
    fn own_hand_is_visible_and_the_opponents_is_not() {
        let layout = layout();
        let game = dealt_game(5);
        for observer in [PlayerIndex::P1, PlayerIndex::P2] {
            let obs = observation(&game.state, observer);
            let me = game.state.player(observer);
            let them = game.state.player(observer.opponent());
            assert!(me.hand_size() > 0 && them.hand_size() > 0);

            let hand = section(&layout, &obs, "self.hand");
            assert_eq!(hand.iter().sum::<f32>(), me.hand_size() as f32);
            for (id, copies) in me.hand.iter() {
                assert_eq!(hand[me.card(id) as usize], copies as f32);
            }

            assert!(section(&layout, &obs, "opponent.hand").iter().all(|&x| x == 0.0));
            assert_eq!(section(&layout, &obs, "opponent.scalars")[4], them.hand_size() as f32 / 4.0);
            // Their hand is somewhere in the pool with their deck.
            let unseen = section(&layout, &obs, "opponent.unseen");
            assert_eq!(unseen.iter().sum::<f32>(), (them.hand_size() + them.deck_size()) as f32);
            // The observer's own unseen cards are just their deck.
            let unseen = section(&layout, &obs, "self.unseen");
            assert_eq!(unseen.iter().sum::<f32>(), me.deck_size() as f32);
        }
    }

    /// Through whole random games, re-dealing what a player cannot see leaves
    /// their observation exactly as it was: nothing hidden leaks into it.
    #[test]
    fn a_redeal_never_changes_the_observation() {
        let mut redeal = new_rng(Some(99));
        let mut checked = 0;
        for seed in 0..40 {
            let mut game = new_game(seed);
            play_out(&mut game, seed ^ 0xABCD, |game| {
                for observer in [PlayerIndex::P1, PlayerIndex::P2] {
                    let before = observation(&game.state, observer);
                    let mut sample = game.clone();
                    shuffle_hidden_information(&mut sample.state, observer, &mut redeal);
                    sample.state.debug_check_card_totals();
                    let after = observation(&sample.state, observer);
                    if before != after {
                        let at = before.iter().zip(&after).position(|(a, b)| a != b).unwrap();
                        let name = &layout().into_iter().rev().find(|s| s.offset <= at).unwrap().name;
                        panic!("seed {seed}: a re-deal changed {observer:?}'s observation in {name} \
                            ({:?})", game.state.phase);
                    }
                    checked += 1;
                }
            });
        }
        assert!(checked > 1000);
    }

    /// What is public reads the same from both seats: one player's "self"
    /// sections are the other's "opponent" sections.
    #[test]
    fn public_sections_agree_between_the_two_seats() {
        let layout = layout();
        const PUBLIC: [&str; 14] = [
            "hero", "hand_seen", "pitch", "graveyard", "banished", "equipment", "weapon",
            "deck_known", "deck_top", "chain_link", "chain_earlier", "abilities_used",
            "scalars", "flags",
        ];
        for seed in 0..10 {
            let mut game = new_game(seed);
            play_out(&mut game, seed, |game| {
                let p1 = observation(&game.state, PlayerIndex::P1);
                let p2 = observation(&game.state, PlayerIndex::P2);
                for name in PUBLIC {
                    for (mine, theirs) in [(&p1, &p2), (&p2, &p1)] {
                        assert_eq!(
                            section(&layout, mine, &format!("self.{name}")),
                            section(&layout, theirs, &format!("opponent.{name}")),
                            "seed {seed}: {name} differs between the seats",
                        );
                    }
                }
                assert_eq!(section(&layout, &p1, "phase"), section(&layout, &p2, "phase"));
            });
        }
    }

    /// Over random games every section is used, and every card either player
    /// owns shows up in a plane: no feature is wired to nothing.
    #[test]
    fn random_games_exercise_every_section() {
        let layout = layout();
        let mut seen = vec![false; OBS_SIZE];
        for seed in 0..60 {
            let mut game = new_game(seed);
            play_out(&mut game, seed, |game| {
                let obs = observation(&game.state, game.state.active_player);
                assert!(obs.iter().all(|x| x.is_finite() && *x >= 0.0));
                for (slot, x) in seen.iter_mut().zip(&obs) {
                    *slot |= *x != 0.0;
                }
            });
        }
        for s in &layout {
            // The pause this records is rare enough for random play to miss.
            // And a player never sees the opponent's face-down banished cards.
            if s.name == "choice_return_phase" || s.name == "opponent.intimidate_banished" {
                continue;
            }
            assert!(seen[s.offset..s.offset + s.len].iter().any(|&x| x), "{} was never set", s.name);
        }
        let graveyard = layout.iter().find(|s| s.name == "self.graveyard").unwrap();
        for deck in [build_rhinar_deck(), build_dorinthea_deck()] {
            for card in deck {
                let data = card.data();
                let reaches_graveyard = !matches!(
                    data.typ,
                    crate::cards::CardType::Hero | crate::cards::CardType::Weapon
                        | crate::cards::CardType::Equipment | crate::cards::CardType::Mentor
                );
                if reaches_graveyard {
                    assert!(seen[graveyard.offset + card as usize], "{card:?} never reached a graveyard plane");
                }
            }
        }
    }
}

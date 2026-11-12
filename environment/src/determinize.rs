//! Determinization for information-set search: re-deal everything one player
//! cannot see, leaving everything they can see exactly as it is.
//!
//! A `Gamestate` holds the whole truth, including the opponent's hand and the
//! order of both decks. A search that plays ahead from it as is would be
//! cheating. Information Set Monte Carlo Tree Search instead samples, on each
//! iteration, one of the states the searching player cannot tell apart from
//! the real one, and plays ahead from that: clone the state, call
//! `shuffle_hidden_information` on the clone, then search it.

use rand::seq::SliceRandom;

use crate::card_counts::{CardCounts, DeckCardId};
use crate::fab_game::GameRng;
use crate::game_state::{Gamestate, Player, PlayerIndex, Zone, PLAYER_CARDS};

/// Replace `gs` with a state drawn uniformly from those `observer` cannot tell
/// apart from it, given the two decklists: every card `observer` cannot see is
/// shuffled together with the others it could be swapped with and dealt back
/// into the same places.
///
/// - **The opponent's hidden cards** form one pool: the hand copies `observer`
///   has not seen (`Player::hand_known` names the ones they have), the
///   Intimidate banish zone, a face-down arsenal card `observer` cannot deduce,
///   and every deck slot not known to both players. The pool is shuffled and
///   dealt back, so each zone keeps its size and the deck keeps its known
///   slots where they were.
/// - **`observer`'s own deck**: they know which cards are in it but not their
///   order, so its hidden slots are shuffled among themselves.
///
/// Everything else — `observer`'s own hand and arsenal, all public zones, life,
/// resources, the phase — is untouched, and so is what each player knows
/// (`hand_known`, known deck slots, `arsenal_known`). A card the opponent has
/// announced but is still paying for (`Gamestate::pending_card`) stays in the
/// zone it is being played from.
///
/// Meant to be called on a clone, at a decision of `observer`'s. The sample is
/// uniform: it does not weigh what the opponent's earlier plays suggest about
/// their hand. The game logs, if on, are left as they are and so still
/// describe the real deal.
#[allow(dead_code)] // for search agents; exercised by the tests below
pub fn shuffle_hidden_information(gs: &mut Gamestate, observer: PlayerIndex, rng: &mut GameRng) {
    let pending = gs.pending_card;
    shuffle_hidden_deck_slots(gs.player_mut(observer), rng);

    let opponent = gs.player_mut(observer.opponent());
    // A card the opponent is paying for was announced, so the observer knows
    // it is in the zone it is played from.
    let pending = pending.filter(|p| p.owner == opponent.pid);
    let pending_from = |zone: Zone| pending.filter(|p| p.from == zone).map(|p| p.card);

    // The hand copies the observer can place: the ones they have seen, and the
    // announced card unless one of those already accounts for it.
    let mut hand_seen = opponent.hand_known;
    if let Some(card) = pending_from(Zone::Hand) {
        if !hand_seen.contains(card) {
            hand_seen.add(card);
        }
    }
    let arsenal_hidden = opponent.arsenal.is_some()
        && !opponent.arsenal_face_up()
        && !opponent.arsenal_known()
        && pending_from(Zone::Arsenal).is_none();

    // Gather the pool. A player owns `PLAYER_CARDS` cards in all.
    let mut pool = [DeckCardId::new(0); PLAYER_CARDS];
    let mut len = 0;
    let mut gather = |id: DeckCardId| {
        pool[len] = id;
        len += 1;
    };
    let mut hand_hidden = 0;
    for (id, held) in opponent.hand.iter() {
        for _ in hand_seen.count(id)..held {
            gather(id);
            hand_hidden += 1;
        }
    }
    let banished = opponent.intimidate_banished.len();
    for (id, copies) in opponent.intimidate_banished.iter() {
        for _ in 0..copies {
            gather(id);
        }
    }
    if arsenal_hidden {
        gather(opponent.arsenal.expect("a hidden arsenal card"));
    }
    for slot in opponent.deck.iter().filter(|slot| !slot.is_known()) {
        gather(slot.card());
    }

    let pool = &mut pool[..len];
    pool.shuffle(rng);

    // Deal it back, zone by zone. The pool is in uniformly random order, so
    // which stretch of it each zone takes does not matter.
    let mut dealt = pool.iter().copied();
    let mut deal = || dealt.next().expect("the pool holds a card for every hidden place");
    opponent.hand = hand_seen;
    for _ in 0..hand_hidden {
        opponent.hand.add(deal());
    }
    opponent.intimidate_banished = CardCounts::EMPTY;
    for _ in 0..banished {
        opponent.intimidate_banished.add(deal());
    }
    if arsenal_hidden {
        opponent.arsenal = Some(deal());
    }
    for i in 0..opponent.deck.len() {
        let slot = opponent.deck.get_mut(i).expect("a slot within the deck");
        if !slot.is_known() {
            slot.set_card(deal());
        }
    }
}

/// Shuffle the cards in `player`'s hidden deck slots among those slots,
/// leaving the slots both players know where they are.
fn shuffle_hidden_deck_slots(player: &mut Player, rng: &mut GameRng) {
    let deck = player.deck.make_contiguous();
    // The positions of the hidden slots.
    let mut hidden = [0u8; PLAYER_CARDS];
    let mut len = 0;
    for (i, slot) in deck.iter().enumerate() {
        if !slot.is_known() {
            hidden[len] = i as u8;
            len += 1;
        }
    }
    let hidden = &mut hidden[..len];
    // Pull their cards out, shuffle them, and put them back.
    let mut cards = [DeckCardId::new(0); PLAYER_CARDS];
    for (card, &i) in cards.iter_mut().zip(hidden.iter()) {
        *card = deck[i as usize].card();
    }
    cards[..len].shuffle(rng);
    for (&card, &i) in cards.iter().zip(hidden.iter()) {
        deck[i as usize].set_card(card);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{new_rng, FabGame};
    use crate::game_state::DeckSlot;
    use crate::legal_actions::legal_actions;
    use crate::simulator::Game;

    fn dealt_game(seed: u64) -> FabGame {
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
        game.reset(false);
        // Hands are dealt once the first player is chosen.
        while game.state.p1.hand.is_empty() || game.state.p2.hand.is_empty() {
            let legal = game.legal_actions();
            game.step(legal[0]);
        }
        game
    }

    /// Every card `player` owns outside the public zones, sorted: the cards a
    /// re-deal may move around but must neither add to nor lose.
    fn private_cards(player: &Player) -> Vec<usize> {
        let mut cards: Vec<usize> = player.deck.iter().map(|slot| slot.card().get()).collect();
        for counts in [player.hand, player.intimidate_banished] {
            for (id, copies) in counts.iter() {
                cards.extend(std::iter::repeat_n(id.get(), copies as usize));
            }
        }
        cards.extend(player.arsenal.map(|id| id.get()));
        cards.sort();
        cards
    }

    /// Assert that `after` is a state `observer` could not tell from `before`.
    fn assert_indistinguishable(before: &Gamestate, after: &Gamestate, observer: PlayerIndex) {
        after.debug_check_card_totals();
        assert_eq!(before.phase, after.phase);
        assert_eq!(before.active_player, after.active_player);
        assert_eq!(before.pending_card, after.pending_card);
        assert_eq!(before.stack, after.stack);

        // The observer's own side: only the order of the hidden deck slots.
        let (mine, mine_after) = (before.player(observer), after.player(observer));
        assert_eq!(mine.hand, mine_after.hand);
        assert_eq!(mine.arsenal, mine_after.arsenal);
        assert_eq!(mine.intimidate_banished, mine_after.intimidate_banished);

        let (theirs, theirs_after) =
            (before.player(observer.opponent()), after.player(observer.opponent()));
        assert_eq!(theirs.hand.len(), theirs_after.hand.len());
        assert_eq!(theirs.intimidate_banished.len(), theirs_after.intimidate_banished.len());
        assert_eq!(theirs.hand_known, theirs_after.hand_known);
        for (id, known) in theirs.hand_known.iter() {
            assert!(theirs_after.hand.count(id) >= known, "a seen hand card was dealt away");
        }
        assert_eq!(theirs.arsenal.is_some(), theirs_after.arsenal.is_some());
        if theirs.arsenal_face_up() || theirs.arsenal_known() {
            assert_eq!(theirs.arsenal, theirs_after.arsenal);
        }

        for (was, is) in [(mine, mine_after), (theirs, theirs_after)] {
            assert_eq!(private_cards(was), private_cards(is));
            assert_eq!(was.deck.len(), is.deck.len());
            for (a, b) in was.deck.iter().zip(is.deck.iter()) {
                assert_eq!(a.is_known(), b.is_known());
                if a.is_known() {
                    assert_eq!(a, b, "a known deck slot moved");
                }
            }
            // Public zones and everything else on the player.
            assert_eq!(was.pitch, is.pitch);
            assert_eq!(was.graveyard, is.graveyard);
            assert_eq!(was.banished, is.banished);
            assert_eq!(was.equipment, is.equipment);
            assert_eq!(was.weapon, is.weapon);
            assert_eq!(was.chain_link, is.chain_link);
            assert_eq!(was.flags, is.flags);
            assert_eq!((was.life, was.resources, was.action_points),
                (is.life, is.resources, is.action_points));
        }
    }

    /// Through whole random games, a re-deal for the player to act changes
    /// nothing they can see — in particular not what they may legally do —
    /// and the re-dealt game plays on to the end.
    #[test]
    fn a_redeal_keeps_everything_the_observer_can_see() {
        let mut playouts = 0;
        for seed in 0..40 {
            let mut game = dealt_game(seed);
            let mut choose = new_rng(Some(seed ^ 0xABCD));
            let mut redeal = new_rng(Some(seed ^ 0x1234));
            let mut decisions = 0;
            while !game.is_over() {
                let legal = legal_actions(&game.state);
                let observer = game.state.active_player;

                let mut sample = game.clone();
                shuffle_hidden_information(&mut sample.state, observer, &mut redeal);
                assert_indistinguishable(&game.state, &sample.state, observer);
                assert_eq!(&legal[..], &legal_actions(&sample.state)[..],
                    "seed {seed}: the re-deal changed {observer:?}'s legal actions in {:?}",
                    game.state.phase);

                // Play some of the samples out, as a search would.
                if decisions % 16 == 0 {
                    while !sample.is_over() {
                        let legal = sample.legal_actions();
                        let pick = rand::RngExt::random_range(&mut choose, 0..legal.len());
                        sample.step(legal[pick]);
                    }
                    playouts += 1;
                }

                let pick = rand::RngExt::random_range(&mut choose, 0..legal.len());
                game.step(legal[pick]);
                decisions += 1;
            }
        }
        assert!(playouts > 40);
    }

    /// The re-deal really does move the hidden cards: over many samples the
    /// opponent's hand, their deck and the observer's own deck all vary.
    #[test]
    fn a_redeal_varies_the_hidden_cards() {
        let game = dealt_game(7);
        let observer = game.state.active_player;
        let mut rng = new_rng(Some(99));
        let (mut hands, mut their_decks, mut my_decks) = (0, 0, 0);
        for _ in 0..50 {
            let mut sample = game.state.clone();
            shuffle_hidden_information(&mut sample, observer, &mut rng);
            let (theirs, mine) = (observer.opponent(), observer);
            hands += (sample.player(theirs).hand != game.state.player(theirs).hand) as u32;
            their_decks += (sample.player(theirs).deck != game.state.player(theirs).deck) as u32;
            my_decks += (sample.player(mine).deck != game.state.player(mine).deck) as u32;
        }
        assert!(hands > 40, "the opponent's hand changed in only {hands} of 50 samples");
        assert!(their_decks > 45);
        assert!(my_decks > 45);
    }

    /// What the observer has seen stays put: a known hand copy, a known deck
    /// slot, and an arsenal card that is face up or deducible.
    #[test]
    fn a_redeal_leaves_seen_cards_in_place() {
        let mut game = dealt_game(3);
        let observer = PlayerIndex::P1;
        let opponent = game.state.player_mut(PlayerIndex::P2);

        let seen_in_hand = opponent.hand.nth(0).expect("a dealt hand");
        opponent.mark_known_in_hand(seen_in_hand);
        let top = opponent.deck.pop_front().expect("a deck").card();
        opponent.arsenal = Some(top);
        opponent.set_arsenal_face_up(true);
        let bottom = opponent.deck.back().expect("a deck").card();
        *opponent.deck.get_mut(opponent.deck.len() - 1).unwrap() = DeckSlot::known(bottom);

        let mut rng = new_rng(Some(5));
        for _ in 0..50 {
            let mut sample = game.state.clone();
            shuffle_hidden_information(&mut sample, observer, &mut rng);
            assert_indistinguishable(&game.state, &sample, observer);
            let dealt = sample.player(PlayerIndex::P2);
            assert!(dealt.hand.contains(seen_in_hand));
            assert_eq!(dealt.arsenal, Some(top));
            assert_eq!(dealt.deck.back(), Some(DeckSlot::known(bottom)));
        }
    }

    /// A face-down arsenal card the observer cannot deduce is part of the
    /// pool, so other cards turn up there.
    #[test]
    fn a_redeal_swaps_a_face_down_arsenal_card() {
        let mut game = dealt_game(3);
        let opponent = game.state.player_mut(PlayerIndex::P2);
        let top = opponent.deck.pop_front().expect("a deck").card();
        opponent.arsenal = Some(top);

        let mut rng = new_rng(Some(5));
        let mut swapped = 0;
        for _ in 0..50 {
            let mut sample = game.state.clone();
            shuffle_hidden_information(&mut sample, PlayerIndex::P1, &mut rng);
            assert_indistinguishable(&game.state, &sample, PlayerIndex::P1);
            swapped += (sample.p2.arsenal != Some(top)) as u32;
        }
        assert!(swapped > 30, "the arsenal card changed in only {swapped} of 50 samples");
    }

    /// A card the opponent has announced and is paying for stays in hand even
    /// though the observer never saw the hand.
    #[test]
    fn a_redeal_keeps_a_card_being_paid_for() {
        let mut game = dealt_game(3);
        let opponent = game.state.player(PlayerIndex::P2);
        let announced = opponent.hand.nth(0).expect("a dealt hand");
        // Make it the only copy in hand, so a re-deal that ignored the
        // announcement would often deal it away.
        assert_eq!(opponent.hand.count(announced), 1, "pick a seed with a single copy");
        game.state.pending_card = Some(crate::game_state::PendingCard {
            owner: PlayerIndex::P2,
            card: announced,
            from: Zone::Hand,
            typ: crate::action::ActionType::PlayCard,
        });

        let mut rng = new_rng(Some(5));
        for _ in 0..50 {
            let mut sample = game.state.clone();
            shuffle_hidden_information(&mut sample, PlayerIndex::P1, &mut rng);
            assert!(sample.p2.hand.contains(announced));
            assert_eq!(sample.p2.hand.len(), game.state.p2.hand.len());
        }
    }
}

//! Action encoding: every action a player can ever be offered as an index
//! into a fixed-size policy vector, the output of a policy/value network.
//!
//! An `Action` names its card by `DeckCardId`, which only means something for
//! one deck. The index names it by game-wide `Card` instead, as the
//! observation's card planes do (see `observation`), so an index means the same
//! action whichever deck the player holds and one network plays either hero.
//!
//! The vector is `KINDS` blocks of `Card::COUNT` slots — one block per
//! `ActionKind`, a way of using a card (play it from hand, pitch it, defend
//! with it, ...) — followed by one slot for each action that names no card:
//!
//! ```text
//! index = kind * Card::COUNT + card          for an action with a card
//! index = KINDS * Card::COUNT + n            for the nth cardless action
//! ```
//!
//! Most slots can never be legal (nobody pitches a weapon); they cost a
//! network output each and are masked out with the rest of the illegal
//! actions (`legal_mask`). The actions legal at one decision always get
//! distinct indices, so a search can key its children by index.

use crate::action::{Action, ActionType, CardRef};
use crate::cards::Card;
use crate::game_state::{Gamestate, Player, Zone};

/// A way of using a card: an action type together with the zone the card is
/// in. The zone is part of it because the same card can be used from two
/// zones at one decision — played from hand or from the arsenal, blocked with
/// from hand or activated on the chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ActionKind {
    PlayFromHand,
    PlayFromArsenal,
    ActivateEquipment,
    ActivateWeapon,
    /// An ability of a card that is defending (Rally the Rearguard), or the
    /// payment a defend trigger offers (Ironhide Gauntlet).
    ActivateOnChain,
    Pitch,
    BottomPitch,
    DefendFromHand,
    DefendWithEquipment,
    Arsenal,
    FlipFaceUp,
    Reveal,
}

impl ActionKind {
    pub const ALL: [ActionKind; KINDS] = [
        ActionKind::PlayFromHand,
        ActionKind::PlayFromArsenal,
        ActionKind::ActivateEquipment,
        ActionKind::ActivateWeapon,
        ActionKind::ActivateOnChain,
        ActionKind::Pitch,
        ActionKind::BottomPitch,
        ActionKind::DefendFromHand,
        ActionKind::DefendWithEquipment,
        ActionKind::Arsenal,
        ActionKind::FlipFaceUp,
        ActionKind::Reveal,
    ];

    /// The action type and card zone this kind stands for.
    pub const fn parts(self) -> (ActionType, Zone) {
        match self {
            ActionKind::PlayFromHand => (ActionType::PlayCard, Zone::Hand),
            ActionKind::PlayFromArsenal => (ActionType::PlayCard, Zone::Arsenal),
            ActionKind::ActivateEquipment => (ActionType::Activate, Zone::Equipment),
            ActionKind::ActivateWeapon => (ActionType::Activate, Zone::Weapon),
            ActionKind::ActivateOnChain => (ActionType::Activate, Zone::CombatChain),
            ActionKind::Pitch => (ActionType::Pitch, Zone::Hand),
            ActionKind::BottomPitch => (ActionType::BottomPitch, Zone::Pitch),
            ActionKind::DefendFromHand => (ActionType::Defend, Zone::Hand),
            ActionKind::DefendWithEquipment => (ActionType::Defend, Zone::Equipment),
            ActionKind::Arsenal => (ActionType::Arsenal, Zone::Hand),
            ActionKind::FlipFaceUp => (ActionType::FlipFaceUp, Zone::Arsenal),
            ActionKind::Reveal => (ActionType::Reveal, Zone::Hand),
        }
    }

    /// The kind of an action of type `typ` on a card in `zone`, or `None` for
    /// a pairing the engine never offers.
    pub fn of(typ: ActionType, zone: Zone) -> Option<ActionKind> {
        ActionKind::ALL.into_iter().find(|kind| kind.parts() == (typ, zone))
    }
}

/// How many `ActionKind`s there are.
pub const KINDS: usize = ActionKind::Reveal as usize + 1;

/// The actions that name no card, in the order of their slots.
const CARDLESS: [ActionType; 4] = [
    ActionType::Pass,
    ActionType::ChooseFirst,
    ActionType::ChooseSecond,
    ActionType::CloseChain,
];
/// The first cardless slot.
const CARDLESS_START: usize = KINDS * Card::COUNT;

/// Length of a policy vector: one more than the largest action index.
pub const ACTION_SIZE: usize = CARDLESS_START + CARDLESS.len();

/// The index of `action`, an action of `player`'s. Panics for an action the
/// engine never offers (a `DefendTrigger`, or a card in a zone its action
/// type does not reach), since such an action has no slot.
pub fn action_index(player: &Player, action: Action) -> usize {
    match action.card {
        Some(card) => {
            let kind = ActionKind::of(action.typ, card.zone)
                .unwrap_or_else(|| panic!("no action slot for {action:?}"));
            kind as usize * Card::COUNT + player.card(card.id) as usize
        }
        None => {
            let n = CARDLESS.iter().position(|&typ| typ == action.typ)
                .unwrap_or_else(|| panic!("no action slot for {action:?}"));
            CARDLESS_START + n
        }
    }
}

/// The action of `player`'s at `index`: the inverse of `action_index`. `None`
/// when the slot's card is not in `player`'s deck, or `index` is out of range.
/// The action need not be legal — check it against `legal_actions`.
#[allow(dead_code)] // for reading a policy back; exercised by the tests
pub fn action_at(player: &Player, index: usize) -> Option<Action> {
    if index >= CARDLESS_START {
        return CARDLESS.get(index - CARDLESS_START).map(|&typ| Action::no_card(typ));
    }
    let (typ, zone) = ActionKind::ALL[index / Card::COUNT].parts();
    let slot = index % Card::COUNT;
    // The deck's ids are dense, so its cards are found by walking them.
    (0..player.deck_kind.count())
        .map(crate::card_counts::DeckCardId::new)
        .find(|&id| player.card(id) as usize == slot)
        .map(|id| Action::new(typ, CardRef::new(id, zone)))
}

/// The kind of card use the slot `index` belongs to, and the `Card` number of
/// its card: what a network needs to build a slot's logit from a "kind" part
/// and a "card" part. `None` for a cardless slot.
#[allow(dead_code)] // for a factored policy head; exercised by the tests
pub const fn slot_parts(index: usize) -> Option<(ActionKind, usize)> {
    if index >= CARDLESS_START {
        return None;
    }
    Some((ActionKind::ALL[index / Card::COUNT], index % Card::COUNT))
}

/// Write the legal-action mask for the player to act into `out`: 1 at the
/// index of each action in `legal`, 0 everywhere else.
#[allow(dead_code)] // for the network agent to come; exercised by the tests
pub fn legal_mask(gs: &Gamestate, legal: &[Action], out: &mut [f32; ACTION_SIZE]) {
    out.fill(0.0);
    let player = gs.player(gs.active_player);
    for &action in legal {
        out[action_index(player, action)] = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::{new_rng, FabGame};
    use crate::simulator::Game;

    /// Play `games` random games, calling `visit` at every decision.
    fn random_games(games: u64, mut visit: impl FnMut(&FabGame)) {
        for seed in 0..games {
            let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(seed));
            game.reset(false);
            let mut choose = new_rng(Some(seed ^ 0x5EED));
            while !game.is_over() {
                visit(&game);
                let legal = game.legal_actions();
                let pick = rand::RngExt::random_range(&mut choose, 0..legal.len());
                game.step(legal[pick]);
            }
        }
    }

    #[test]
    fn kinds_are_numbered_in_order_and_distinct() {
        for (i, kind) in ActionKind::ALL.into_iter().enumerate() {
            assert_eq!(kind as usize, i);
            let (typ, zone) = kind.parts();
            assert_eq!(ActionKind::of(typ, zone), Some(kind));
        }
        assert_eq!(ActionKind::of(ActionType::Pitch, Zone::Arsenal), None);
        assert_eq!(ACTION_SIZE, KINDS * Card::COUNT + 4);
    }

    /// Every slot decodes to an action that encodes back to it, for both
    /// decks; a slot for a card outside the deck decodes to nothing.
    #[test]
    fn every_slot_round_trips() {
        let mut game = FabGame::new(build_rhinar_deck(), build_dorinthea_deck(), Some(1));
        game.reset(false);
        for player in [&game.state.p1, &game.state.p2] {
            let mut decoded = 0;
            for index in 0..ACTION_SIZE {
                match action_at(player, index) {
                    Some(action) => {
                        assert_eq!(action_index(player, action), index);
                        decoded += 1;
                    }
                    None => {
                        let (_, card) = slot_parts(index).expect("a cardless slot always decodes");
                        assert!((0..player.deck_kind.count())
                            .all(|id| player.card(crate::card_counts::DeckCardId::new(id)) as usize != card));
                    }
                }
            }
            assert_eq!(decoded, KINDS * player.deck_kind.count() + CARDLESS.len());
            assert_eq!(action_at(player, ACTION_SIZE), None);
        }
    }

    /// Through random games every legal action has a slot, the actions of one
    /// decision get distinct slots, and each decodes back to itself.
    #[test]
    fn legal_actions_get_distinct_slots() {
        let mut kinds_seen = [false; KINDS];
        let mut cardless_seen = [false; CARDLESS.len()];
        let mut mask = [0.0; ACTION_SIZE];
        random_games(300, |game| {
            let legal = game.legal_actions();
            let player = game.state.player(game.state.active_player);
            legal_mask(&game.state, &legal, &mut mask);
            assert_eq!(mask.iter().sum::<f32>(), legal.len() as f32,
                "two legal actions share a slot in {:?}: {legal:?}", game.state.phase);
            for &action in legal.iter() {
                let index = action_index(player, action);
                assert_eq!(mask[index], 1.0);
                assert_eq!(action_at(player, index), Some(action));
                match slot_parts(index) {
                    Some((kind, card)) => {
                        assert_eq!(card, player.card(action.card_ref().id) as usize);
                        kinds_seen[kind as usize] = true;
                    }
                    None => cardless_seen[index - CARDLESS_START] = true,
                }
            }
        });
        for kind in ActionKind::ALL {
            assert!(kinds_seen[kind as usize], "{kind:?} was never offered");
        }
        assert!(cardless_seen.iter().all(|&seen| seen));
    }
}

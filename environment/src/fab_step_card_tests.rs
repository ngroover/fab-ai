//! Per-card unit tests for on-play effects.
//!
//! Declared as a child module of `fab_step` (via `#[path]` in `fab_step.rs`),
//! so `use super::*` reaches `fab_step`'s private helpers — notably
//! `apply_on_play_effect` and `resolve_combat_damage` — exactly as the main
//! `tests` module does. Staging helpers shared with the other test modules live
//! in `test_support`.
//!
//! Cards covered here:
//!   - Bare Fangs          (`Card::BareFangsR`)
//!   - Wild Ride           (`Card::WildRideR`)
//!   - Alpha Rampage       (`Card::AlphaRampageR`)
//!   - Wrecker Romp        (`Card::WreckerRompB`)
//!   - Awakening Bellow    (`Card::AwakeningBellowR`)
//!   - Beast Mode          (`Card::BeastModeR`)
//!   - Rhinar (hero)       (`Card::Rhinar`)
//!   - Pack Hunt           (`Card::PackHuntR`)
//!   - Smash Instinct      (`Card::SmashInstinctY`)
//!   - Wrecking Ball       (`Card::WreckingBallR`)
//!   - Muscle Mutt         (`Card::MuscleMuttY`)
//!   - Raging Onslaught    (`Card::RagingOnslaughtY`)
//!   - Smash with Big Tree (`Card::SmashWithBigTreeY`)
//!   - Clearing Bellow     (`Card::ClearingBellowB`)
//!   - Wounded Bull        (`Card::WoundedBullY`)
//!   - Sigil of Solace     (`Card::SigilofSolaceB`)
//!   - Come to Fight       (`Card::ComeToFightB`)
//!   - Titanium Bauble     (`Card::TitaniumBaubleB`)
//!   - Pack Call           (`Card::PackCallY`)
//!   - Barraging Beatdown  (`Card::BarragingBeatdownY`)
//!   - Rally the Rearguard (`Card::RallyTheRearguardB`)
//!   - Chief Ruk'utan      (`Card::ChiefRukutan`)
//!   - Bone Basher         (`Card::BoneBasher`)
//!   - Breaking the combat chain (`ActionType::CloseChain`)
//!   - Blossom of Spring   (`Card::BlossomOfSpring`)
//!   - Bone Vizier         (`Card::BoneVizier`), with equipment blocking and
//!     Blade Break
//!   - Ironhide Gauntlet   (`Card::IronhideGauntlet`)
//!   - Ironhide Legs       (`Card::IronhideLegs`)
//!   - En Garde            (`Card::EnGardeR`)
//!   - Flock of the Feather Walkers (`Card::FlockOfTheFeatherWalkersR`) and
//!     the Quicken token (`Card::QuickenToken`)
//!   - Dorinthea (hero)    (`Card::Dorinthea`) and Dawnblade (`Card::Dawnblade`)
//!   - In the Swing        (`Card::InTheSwingR`)
//!   - Ironsong Response   (`Card::IronsongResponseR`), and Reprise
//!   - Second Swing        (`Card::SecondSwingR`)
//!   - Sharpen Steel       (`Card::SharpenSteelR`)
//!   - Thrust              (`Card::ThrustR`)
//!   - Warrior's Valor     (`Card::WarriorsValorR`)
//!   - Driving Blade       (`Card::DrivingBladeY`)
//!   - Glistening Steelblade (`Card::GlisteningSteelbladeY`)
//!   - On a Knife Edge     (`Card::OnAKnifeEdgeY`)
//!   - Run Through         (`Card::RunThroughY`)
//!   - Out for Blood       (`Card::OutForBloodY`)
//!   - Slice and Dice      (`Card::SliceAndDiceY`)

use super::*;
use crate::cards::Card;
use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
use crate::legal_actions::legal_actions;
use std::collections::HashSet;
use crate::test_support::*;
use crate::deck_cards::TokenCard;
use crate::test_support::step;
use crate::game_state::CardVisibleState;

/// Find a card sitting in p1's deck whose power satisfies `pred`.
fn find_p1_deck_card(gs: &Gamestate, pred: impl Fn(u8) -> bool) -> Card {
    find_deck_card(gs, PlayerIndex::P1, pred)
}

#[test]
fn bare_fangs_discarding_power6_grants_plus2_power() {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Force the draw-then-discard to land on a power-6 card: empty p1's hand,
    // then seat a 6-power card on top of the deck. After the forced draw the
    // hand holds exactly that card, so it is the one discarded.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power >= 6);
    assert!(pick.data().power >= 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::BareFangsR
        .data()
        .play_effects
        .first()
        .expect("Bare Fangs should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);

    // Discarding a 6-power card satisfies the condition, banking +2 power for
    // the attack; the discarded card is now in p1's graveyard.
    assert_eq!(gs.p1.attack_power_bonus, 2);
    assert!(in_zone(&gs, PlayerIndex::P1, pick, Zone::Graveyard));
}

#[test]
fn bare_fangs_discarding_below_power6_grants_no_bonus() {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Same setup, but seat a sub-6-power card on top so the forced discard
    // fails the threshold.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power < 6);
    assert!(pick.data().power < 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::BareFangsR
        .data()
        .play_effects
        .first()
        .expect("Bare Fangs should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);

    // The discard missed the power-6 threshold, so no power is banked; the
    // card still moves to the graveyard.
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert!(in_zone(&gs, PlayerIndex::P1, pick, Zone::Graveyard));
}

#[test]
fn wild_ride_discarding_power6_grants_go_again() {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Force the draw-then-discard to land on a power-6 card: empty p1's hand,
    // then seat a 6-power card on top of the deck. After the forced draw the
    // hand holds exactly that card, so it is the one discarded.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power >= 6);
    assert!(pick.data().power >= 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::WildRideR
        .data()
        .play_effects
        .first()
        .expect("Wild Ride should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);

    // Discarding a 6-power card satisfies the condition, banking Go Again for
    // the attack (and no power bonus); the discarded card is now in p1's
    // graveyard.
    assert!(gs.p1.attack_go_again_bonus());
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert!(in_zone(&gs, PlayerIndex::P1, pick, Zone::Graveyard));
}

#[test]
fn wild_ride_discarding_below_power6_grants_no_go_again() {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Same setup, but seat a sub-6-power card on top so the forced discard
    // fails the threshold.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power < 6);
    assert!(pick.data().power < 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::WildRideR
        .data()
        .play_effects
        .first()
        .expect("Wild Ride should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);

    // The discard missed the power-6 threshold, so no Go Again is banked; the
    // card still moves to the graveyard.
    assert!(!gs.p1.attack_go_again_bonus());
    assert!(in_zone(&gs, PlayerIndex::P1, pick, Zone::Graveyard));
}

// ─────────────────────────────────────────────────────────────────────────────
// Alpha Rampage (Card::AlphaRampageR) and Wrecker Romp (Card::WreckerRompB)
//
// Both carry a "discard a card" additional cost. The tests below cover the
// legal-action gating (a card is only playable, and a pitch only legal, when the
// hand can both cover the cost and keep a card back to discard) and the payment
// of the discard at play time.
// ─────────────────────────────────────────────────────────────────────────────

/// Build a fresh game with Rhinar (p1) on his action phase, ready for his hand
/// to be set up for a legal-actions check.
fn setup_rhinar_action_phase() -> TestGame {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    gs
}

/// The seat-swapped `setup_rhinar_action_phase`: Dorinthea (p1) on her action
/// phase against Rhinar (p2). For tests that need Dorinthea's cards — the
/// instants and attack reactions Rhinar's deck has none of — on the turn
/// player's side.
fn setup_dorinthea_action_phase() -> TestGame {
    let mut gs = new_game(build_dorinthea_deck(), build_rhinar_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    gs
}

/// `setup_rhinar_action_phase` in a Rhinar mirror: both players are Rhinar,
/// so either side can hold any Rhinar card from their own deck.
fn setup_rhinar_mirror_action_phase() -> TestGame {
    let mut gs = new_game(build_rhinar_deck(), build_rhinar_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    gs
}

/// The set of distinct cards offered as `PlayCard` actions for the active player
/// in the current phase.
fn playable_cards(gs: &Gamestate) -> HashSet<Card> {
    legal_actions(gs).iter()
        .filter(|a| a.typ == ActionType::PlayCard)
        .map(|a| card_of(&gs, a))
        .collect()
}

#[test]
fn discard_cost_card_unplayable_when_paying_leaves_no_card_to_discard() {
    // Both Alpha Rampage and Wrecker Romp carry a "discard a card" additional
    // cost, so a card must be kept back from the pitch pool to discard. In a
    // two-card hand the only partner is a single pitch-3 card: it alone covers
    // the cost, so each card would be playable but for the discard cost — paying
    // it consumes the one card that would otherwise be discarded.

    // Alpha Rampage (cost 3) + Clearing Bellow (pitch 3): not playable.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::AlphaRampageR, Card::ClearingBellowB]);
    gs.p1.resources = 0;
    assert!(!playable_cards(&gs).contains(&Card::AlphaRampageR));

    // Control: Muscle Mutt is also cost 3 but has no additional cost, so the
    // identical hand shape (Muscle Mutt + a pitch-3 card) leaves it playable —
    // confirming the block above comes from the discard cost, not the pitch.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::MuscleMuttY, Card::ClearingBellowB]);
    gs.p1.resources = 0;
    assert!(playable_cards(&gs).contains(&Card::MuscleMuttY));

    // Wrecker Romp (cost 2) + Clearing Bellow (pitch 3): not playable.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::WreckerRompB, Card::ClearingBellowB]);
    gs.p1.resources = 0;
    assert!(!playable_cards(&gs).contains(&Card::WreckerRompB));
}

#[test]
fn discard_cost_card_playable_with_a_spare_card_to_discard() {
    // With pitch enough to both pay the cost and keep a card back to discard, the
    // discard-cost cards are offered. The hand holds Alpha Rampage, Wrecker Romp,
    // a pitch-3 card (Clearing Bellow) and a pitch-1 card (Bare Fangs): the
    // lowest-pitch card is set aside for the discard and the rest still covers
    // each card's cost.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1,
        &[Card::AlphaRampageR, Card::WreckerRompB, Card::ClearingBellowB, Card::BareFangsR]);
    gs.p1.resources = 0;

    let playable = playable_cards(&gs);
    assert!(playable.contains(&Card::AlphaRampageR));
    assert!(playable.contains(&Card::WreckerRompB));
}

#[test]
fn discard_cost_card_playable_when_floating_resources_cover_the_gap() {
    // Floating resources count toward the cost, so a single point of banked
    // resource can be the difference. Hand: Alpha Rampage (cost 3, pitch 1) plus
    // three pitch-1 cards. Setting one pitch-1 card aside to discard leaves only
    // 2 pitch — short of the cost by itself, but with 1 floating resource the
    // remaining 2 pitch covers the other 2 owed.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1,
        &[Card::AlphaRampageR, Card::AlphaRampageR, Card::AwakeningBellowR, Card::BareFangsR]);

    // Without the floating resource the remaining 2 pitch can't cover cost 3.
    gs.p1.resources = 0;
    assert!(!playable_cards(&gs).contains(&Card::AlphaRampageR));

    // One floating resource drops what's owed to 2, which the spare pitch covers
    // — so Alpha Rampage becomes playable.
    gs.p1.resources = 1;
    assert!(playable_cards(&gs).contains(&Card::AlphaRampageR));
}

#[test]
fn pitch_for_discard_cost_card_excludes_the_card_needed_to_discard() {
    // Paying for a card with a "discard a card" additional cost must leave a card
    // in hand to discard. Hand: Alpha Rampage (cost 3) plus a red 1-pitch card
    // and a blue 3-pitch card.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1,
        &[Card::AlphaRampageR, Card::BareFangsR, Card::ClearingBellowB]);
    gs.p1.resources = 0;

    // Play Alpha Rampage: it can't be paid outright, so we drop into the pitch
    // phase with Alpha Rampage pending in hand.
    let ar = expect_in(&gs, PlayerIndex::P1, Card::AlphaRampageR, Zone::Hand, "Alpha Rampage should be in hand");
    step_card(&mut gs, ActionType::PlayCard, ar);
    assert_eq!(gs.phase, Phase::ActionPitch);

    // Only the blue 3-pitch card is a legal pitch: pitching it covers the cost
    // and leaves the red card to discard. Pitching the red 1-pitch card would
    // force the blue card to be pitched too, leaving nothing to discard — so the
    // red card is not offered.
    let pitchable: HashSet<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::Pitch)
        .map(|a| card_of(&gs, a))
        .collect();
    assert_eq!(pitchable, HashSet::from([Card::ClearingBellowB]));
    assert!(!pitchable.contains(&Card::BareFangsR));
}

#[test]
fn discard_cost_card_discards_a_hand_card_when_played() {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);

    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Relabel Muscle Mutt to Alpha Rampage (cost 3, carries a "discard a card"
    // additional cost). Rhinar's opening hand is then Alpha Rampage, Pack Call,
    // Raging Onslaught and Clearing Bellow.
    let ar = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, ar, Card::AlphaRampageR);

    // The two cards that will be left in hand once Alpha Rampage is committed and
    let ar = Card::AlphaRampageR;
    // Clearing Bellow is pitched — the discard must land on one of them.
    let pack = expect_in(&gs, PlayerIndex::P1, Card::PackCallY, Zone::Hand, "Pack Call should be in the opening hand");
    let ro = expect_in(&gs, PlayerIndex::P1, Card::RagingOnslaughtY, Zone::Hand, "Raging Onslaught should be in the opening hand");

    // Play Alpha Rampage; it can't be paid outright so it stays pending.
    step_card(&mut gs, ActionType::PlayCard, ar);
    assert_eq!(gs.phase, Phase::ActionPitch);

    // Before paying, Pack Call and Raging Onslaught are both still in hand — the
    // discard cost has not been paid yet.
    assert!(in_zone(&gs, PlayerIndex::P1, pack, Zone::Hand));
    assert!(in_zone(&gs, PlayerIndex::P1, ro, Zone::Hand));

    // Pitch Clearing Bellow (pitch 3) to cover the cost. This commits Alpha
    // Rampage to the stack, and the "discard a card" additional cost is paid as
    // it is played: one random card from the rest of the hand (Pack Call or
    // Raging Onslaught) is discarded to the graveyard immediately — well before
    // the card resolves.
    let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
    step_card(&mut gs, ActionType::Pitch, cb);
    assert_eq!(gs.phase, Phase::ActionInstant);
    assert_eq!(stack_top_card(&gs), Some(ar));

    // Exactly one of the two remaining cards was discarded at play time; the
    // other stays in hand, leaving a single card in hand.
    let discarded_at_play = [pack, ro]
            .into_iter()
            .filter(|&c| in_zone(&gs, PlayerIndex::P1, c, Zone::Graveyard))
            .count();
    assert_eq!(discarded_at_play, 1, "exactly one hand card should be discarded when played");
    assert_eq!(gs.p1.hand_size(), 1);

    // Both players pass: Alpha Rampage resolves onto the combat chain. No further
    // discard happens at resolution — the cost was already paid.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    assert_eq!(gs.phase, Phase::Defend);
    assert!(in_zone(&gs, PlayerIndex::P1, ar, Zone::CombatChain));

    let discarded_after_resolve = [pack, ro]
            .into_iter()
            .filter(|&c| in_zone(&gs, PlayerIndex::P1, c, Zone::Graveyard))
            .count();
    assert_eq!(discarded_after_resolve, 1, "the discard cost is paid once, at play time");
    assert_eq!(gs.p1.hand_size(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Awakening Bellow (Card::AwakeningBellowR)
//
// An action with Go Again and Intimidate whose on-play effect banks +3 power for
// the next *brute* attack the owner plays this turn. The tests below verify the
// keywords, that a follow-up brute attack gets the +3, and that a follow-up
// generic (non-brute) attack does not (and leaves the bonus banked).
// ─────────────────────────────────────────────────────────────────────────────

/// Seat `card` on `pid`'s combat-chain link 0 as the attacking card, ready for
/// `resolve_combat_damage`, in place of a card from `pid`'s hand, and return
/// it.
fn place_attacker_on_chain(gs: &mut Gamestate, pid: PlayerIndex, card: Card) -> Card {
    seat_on_chain(gs, pid, 0, card);
    card
}

/// Seat `pid`'s own weapon on chain link 0 as the attacking card, as a swing
/// would put it there, and return it. Weapons are never in hand, so this moves
/// the real one rather than relabelling a hand card.
fn place_weapon_on_chain(gs: &mut Gamestate, pid: PlayerIndex) -> Card {
    let weapon = weapon_card(gs, pid);
    move_to_chain(gs, pid, Zone::Weapon, weapon, 0);
    weapon
}

#[test]
fn awakening_bellow_has_go_again_and_intimidate() {
    use crate::cards::Keyword;
    let data = Card::AwakeningBellowR.data();
    assert!(data.keyword.contains(Keyword::GoAgain));
    assert!(data.keyword.contains(Keyword::Intimidate));
}

#[test]
fn awakening_bellow_banks_plus3_for_the_next_brute_attack() {
    let mut gs = setup_rhinar_action_phase();

    // Resolving Awakening Bellow's on-play effect banks +3 for the next brute.
    let effect = Card::AwakeningBellowR
        .data()
        .play_effects
        .first()
        .expect("Awakening Bellow should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);
}

#[test]
fn awakening_bellow_followup_brute_attack_gets_plus3() {
    let mut gs = setup_rhinar_action_phase();

    // Bank the +3 brute bonus from Awakening Bellow.
    let effect = Card::AwakeningBellowR.data().play_effects.first().unwrap();
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);

    // A follow-up brute attack — Bare Fangs (brute, 6 power) — is seated on the
    // chain against an undefended opponent and resolves combat damage.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    assert_eq!(attacker.data().card_class, CardClass::Brute);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // Brute attack: it gets the +3, so 6 + 3 = 9 damage, and the bonus is spent.
    assert_eq!(gs.p2.life, life_before - 9);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 0);
}

#[test]
fn awakening_bellow_followup_generic_attack_does_not_get_plus3() {
    let mut gs = setup_rhinar_action_phase();

    // Bank the +3 brute bonus from Awakening Bellow.
    let effect = Card::AwakeningBellowR.data().play_effects.first().unwrap();
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);

    // A follow-up generic (non-brute) attack — Muscle Mutt (generic, 6 power) —
    // is seated on the chain against an undefended opponent and resolves.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
    assert_eq!(attacker.data().card_class, CardClass::Generic);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // Generic attack: no +3, so just 6 damage — and the bonus stays banked for a
    // later brute attack this turn rather than being consumed.
    assert_eq!(gs.p2.life, life_before - 6);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);
}

#[test]
fn awakening_bellow_brute_weapon_swing_does_not_get_plus3() {
    let mut gs = setup_rhinar_action_phase();

    // Bank the +3 brute bonus from Awakening Bellow.
    let effect = Card::AwakeningBellowR.data().play_effects.first().unwrap();
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);

    // Bone Basher is a Brute, but it is a *weapon* (not an attack action card),
    // so swinging it does not qualify for the bonus. Seat it on the chain (4
    // power) against an undefended opponent and resolve.
    let attacker = place_weapon_on_chain(&mut gs, PlayerIndex::P1);
    let data = attacker.data();
    assert_eq!(attacker, Card::BoneBasher);
    assert_eq!(data.card_class, CardClass::Brute);
    assert_eq!(data.typ, CardType::Weapon);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // Brute weapon: no +3, so just 4 damage — and the bonus stays banked for a
    // later brute attack action this turn rather than being consumed.
    assert_eq!(gs.p2.life, life_before - 4);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Beast Mode (Card::BeastModeR)
//
// A 6-power brute attack action whose on-play effect grants +2 power "if you've
// intimidated this turn". The owner's `has_intimidated` flag, set whenever an
// Intimidate trigger resolves for them, gates the bonus. The tests below verify
// the attack lands for 8 when the owner has intimidated and for 6 when it hasn't.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn beast_mode_gets_plus2_when_player_has_intimidated() {
    let mut gs = setup_rhinar_action_phase();

    // The owner intimidated earlier this turn.
    gs.p1.set_has_intimidated(true);

    // Resolving Beast Mode's on-play effect banks +2 onto the resolving attack.
    let effect = Card::BeastModeR
        .data()
        .play_effects
        .first()
        .expect("Beast Mode should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.attack_power_bonus, 2);

    // Seat Beast Mode (brute, 6 power) on the chain against an undefended
    // opponent and resolve: 6 + 2 = 8 damage, and the bonus is consumed.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BeastModeR);
    assert_eq!(attacker.data().card_class, CardClass::Brute);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 8);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn beast_mode_no_bonus_when_player_has_not_intimidated() {
    let mut gs = setup_rhinar_action_phase();

    // The owner has not intimidated this turn (the flag starts cleared).
    assert!(!gs.p1.has_intimidated());

    // Resolving Beast Mode's on-play effect does nothing: the condition fails.
    let effect = Card::BeastModeR.data().play_effects.first().unwrap();
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.attack_power_bonus, 0);

    // Seat Beast Mode (brute, 6 power) on the chain against an undefended
    // opponent and resolve: just its base 6 damage, with no bonus.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BeastModeR);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 6);
}

// ─────────────────────────────────────────────────────────────────────────────
// Rhinar's constant hero ability: OnDiscard6Intimidate
//
// Whenever Rhinar discards a card with 6 or more power, he Intimidates (the
// opponent banishes a card from hand). This fires for every discard path — the
// "discard a card" additional cost (Alpha Rampage, Wrecker Romp) and the
// play-effect draw-then-discard (Bare Fangs, Wild Ride) — and stacks with a
// card's own Intimidate keyword.
// ─────────────────────────────────────────────────────────────────────────────

/// Count the cards sitting in `pid`'s Intimidate banish zone.
fn intimidate_banish_count(gs: &Gamestate, pid: PlayerIndex) -> usize {
    gs.player(pid).intimidate_banished.len() as usize
}

#[test]
fn rhinar_constant_ability_is_on_discard6_intimidate() {
    use crate::card_effects::ConstantEffect;
    assert!(matches!(
        Card::Rhinar.data().constant_effect,
        Some(ConstantEffect::OnDiscard6Intimidate)
    ));
    // The opposing hero (Dorinthea) carries no such constant effect.
    assert!(!matches!(Card::Dorinthea.data().constant_effect, Some(ConstantEffect::OnDiscard6Intimidate)));
}

#[test]
fn discard_cost_of_power6_card_triggers_intimidate() {
    let mut gs = setup_rhinar_action_phase();

    // Seat a single power-6 card in Rhinar's hand so the "discard a card" cost
    // is forced to discard it. Wrecker Romp is power 6.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::WreckerRompB]);
    assert!(Card::WreckerRompB.data().power >= 6);
    assert!(gs.p2.hand_size() > 0, "opponent needs a card to be intimidated");
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);

    apply_discard_cost(&mut gs.state, &mut gs.rng, PlayerIndex::P1);

    // The 6-power discard triggered Rhinar's constant ability: the opponent
    // banished one card from hand to their Intimidate banish zone.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
}

#[test]
fn discard_cost_of_sub_power6_card_does_not_trigger_intimidate() {
    let mut gs = setup_rhinar_action_phase();

    // Clearing Bellow is power 0, below the threshold, so the discard cost pays
    // out without triggering the constant ability.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ClearingBellowB]);
    assert!(Card::ClearingBellowB.data().power < 6);

    apply_discard_cost(&mut gs.state, &mut gs.rng, PlayerIndex::P1);

    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
}

#[test]
fn bare_fangs_play_effect_power6_discard_also_intimidates() {
    // The play-effect draw-then-discard path (Bare Fangs / Wild Ride) routes
    // through the same constant-ability check, so a power-6 discard intimidates
    // on top of banking the card's own +2 power.
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power >= 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);

    let effect = Card::BareFangsR.data().play_effects.first().unwrap();
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);

    // The play effect banked its +2, and the 6-power discard also intimidated.
    assert_eq!(gs.p1.attack_power_bonus, 2);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
}

#[test]
fn non_rhinar_hero_discard_does_not_intimidate() {
    // The constant ability keys off the discarding hero, not the discard itself.
    // Dorinthea's deck holds no 6-power card, so the closest she can come is
    // discarding her biggest attack, Flock of the Feather Walkers (5).
    assert!(!matches!(Card::Dorinthea.data().constant_effect, Some(ConstantEffect::OnDiscard6Intimidate)));
    assert!(build_dorinthea_deck().iter().all(|c| c.data().power < 6),
        "Dorinthea's deck has no 6-power card to discard");

    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P2, &[Card::FlockOfTheFeatherWalkersR]);
    apply_discard_cost(&mut gs.state, &mut gs.rng, PlayerIndex::P2);

    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P1), 0);
}

#[test]
fn alpha_rampage_power6_discard_intimidates_twice() {
    // Alpha Rampage carries the Intimidate keyword *and* a "discard a card" cost.
    // When the discarded card is power 6+, the keyword Intimidate (at resolution)
    // and Rhinar's constant ability (at the discard) both fire — two cards leave
    // the opponent's hand.
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Make every non-Alpha-Rampage hand card a power-6 card so whichever one the
    // discard cost picks is guaranteed to clear the threshold. Rhinar's opening
    // hand is Muscle Mutt, Pack Call, Raging Onslaught and Clearing Bellow.
    let hand_len = gs.p1.hand_size() as usize;
    assert!(hand_len > 0, "opening hand should be non-empty");
    let mut hand = vec![Card::AlphaRampageR];
    hand.extend(std::iter::repeat_n(Card::WreckerRompB, hand_len - 1)); // power 6
    set_hand(&mut gs, PlayerIndex::P1, &hand);
    let ar = Card::AlphaRampageR;
    assert!(Card::AlphaRampageR.data().keyword.contains(crate::cards::Keyword::Intimidate));

    // Pay the cost 3 outright so no pitching disturbs the hand.
    gs.p1.resources = 3;
    assert!(gs.p2.hand_size() >= 2, "opponent needs two cards to be intimidated twice");

    // Play Alpha Rampage: paid from resources, it commits to the stack and the
    // discard cost is paid from the (all power-6) remainder of the hand — the
    // constant ability intimidates once here.
    step_card(&mut gs, ActionType::PlayCard, ar);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1,
        "the power-6 discard cost should intimidate once");

    // Both players pass so Alpha Rampage resolves; its Intimidate keyword fires
    // as it lands on the chain, intimidating a second time.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    assert!(in_zone(&gs, PlayerIndex::P1, ar, Zone::CombatChain));
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 2,
        "keyword Intimidate plus the constant ability should intimidate twice in total");
}

// ─────────────────────────────────────────────────────────────────────────────
// Pack Hunt (Card::PackHuntR) and Smash Instinct (Card::SmashInstinctY)
//
// Both are brute attack actions carrying the Intimidate keyword. Intimidate is
// applied generically as a card resolves, so when either attack resolves onto
// the combat chain the defending player banishes a random card from hand into
// their Intimidate banish zone. Neither card has a discard cost or discard play
// effect, so Rhinar's OnDiscard6Intimidate does not also fire — the keyword is
// the sole source of the single Intimidate.
// ─────────────────────────────────────────────────────────────────────────────

/// Play `card` (an attack action costing at most 3) from p1's hand and resolve
/// it onto the combat chain. The first two opening-hand cards are relabelled to
/// `card` and to Clearing Bellow (pitch 3, used to pay the cost); both players
/// then pass so the attack resolves. Returns the game and the played card's
/// card.
fn play_and_resolve_attack(card: Card) -> (TestGame, Card) {
    play_and_resolve_attack_against(build_dorinthea_deck(), card)
}

/// `play_and_resolve_attack` with `defender_deck` as p2's decklist, for tests
/// whose defender needs cards only that deck holds — a Rhinar mirror when the
/// defender blocks or reacts with Rhinar's own cards.
fn play_and_resolve_attack_against(defender_deck: [Card; 46], card: Card) -> (TestGame, Card) {
    let mut gs = new_game(build_rhinar_deck(), defender_deck, Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    let hand = hand_cards(&gs, PlayerIndex::P1);
    let atk = hand[0];
    let pitch = hand[1];
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, atk, card);
    let atk = card;
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, pitch, Card::ClearingBellowB); // pitch 3 covers cost <= 3
    let pitch = Card::ClearingBellowB;
    assert!(card.data().cost <= 3, "helper only pays costs up to a single pitch-3 card");

    // Play the attack, pitch to cover its cost, then both players pass so it
    // resolves onto p1's combat chain (its Intimidate fires as it resolves).
    step_card(&mut gs, ActionType::PlayCard, atk);
    assert_eq!(gs.phase, Phase::ActionPitch);
    step_card(&mut gs, ActionType::Pitch, pitch);
    assert_eq!(gs.phase, Phase::ActionInstant);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    (gs, atk)
}

#[test]
fn pack_hunt_has_intimidate() {
    use crate::cards::Keyword;
    assert!(Card::PackHuntR.data().keyword.contains(Keyword::Intimidate));
}

#[test]
fn smash_instinct_has_intimidate() {
    use crate::cards::Keyword;
    assert!(Card::SmashInstinctY.data().keyword.contains(Keyword::Intimidate));
}

#[test]
fn pack_hunt_intimidates_on_resolve() {
    let (gs, atk) = play_and_resolve_attack(Card::PackHuntR);
    assert_eq!(gs.phase, Phase::Defend);
    assert!(in_zone(&gs, PlayerIndex::P1, atk, Zone::CombatChain));
    // The Intimidate keyword fired once as the attack resolved.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
}

#[test]
fn smash_instinct_intimidates_on_resolve() {
    let (gs, atk) = play_and_resolve_attack(Card::SmashInstinctY);
    assert_eq!(gs.phase, Phase::Defend);
    assert!(in_zone(&gs, PlayerIndex::P1, atk, Zone::CombatChain));
    // The Intimidate keyword fired once as the attack resolved.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Wrecking Ball (Card::WreckingBallR)
//
// Brute attack action. On play, draw a card then discard a card; if the
// discarded card has 6 or more power, Intimidate. Same DrawDiscardHit6 condition
// as Bare Fangs / Wild Ride, but the payoff is a conditional Intimidate rather
// than a power / Go Again buff. Played by Rhinar, a 6-power discard Intimidates
// twice — once from his OnDiscard6Intimidate constant ability (which fires inside
// the discard) and once from Wrecking Ball's own conditional Intimidate.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wrecking_ball_discarding_power6_intimidates_twice_under_rhinar() {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Force the draw-then-discard to land on a power-6 card: empty p1's hand,
    // then seat a 6-power card on top of the deck so it is the one drawn and
    // discarded.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power >= 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);
    assert!(gs.p2.hand_size() >= 2, "opponent needs two cards to be intimidated twice");
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);

    let effect = Card::WreckingBallR
        .data()
        .play_effects
        .first()
        .expect("Wrecking Ball should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);

    // The 6-power discard intimidates twice (constant ability + the card's own
    // conditional Intimidate) and banks no power; the card is now in the
    // graveyard.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 2);
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert!(in_zone(&gs, PlayerIndex::P1, pick, Zone::Graveyard));
}

#[test]
fn wrecking_ball_discarding_below_power6_does_not_intimidate() {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Seat a sub-6-power card on top so the forced discard fails the threshold.
    move_hand_to_graveyard(&mut gs, PlayerIndex::P1);
    let pick = find_p1_deck_card(&gs, |power| power < 6);
    put_on_top_of_deck(&mut gs, PlayerIndex::P1, pick);

    let effect = Card::WreckingBallR.data().play_effects.first().unwrap();
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);

    // The discard missed the power-6 threshold, so neither the constant ability
    // nor the conditional Intimidate fires; the card still moves to the
    // graveyard.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
    assert!(in_zone(&gs, PlayerIndex::P1, pick, Zone::Graveyard));
}

// ─────────────────────────────────────────────────────────────────────────────
// Vanilla cards: Muscle Mutt (Card::MuscleMuttY), Raging Onslaught
// (Card::RagingOnslaughtY) and Smash with Big Tree (Card::SmashWithBigTreeY)
//
// None of the three has rules text. They are covered here to pin down that they
// are *deliberately* effectless — a later card that adds a shared mechanic must
// not quietly give them one — and that each hits for exactly its printed power.
// Smash with Big Tree additionally carries `no_block`, so it cannot be used to
// block even though it sits in hand like any other card.
// ─────────────────────────────────────────────────────────────────────────────

/// Assert `card` is an attack action with the given printed stats and class and
/// carries no rules text at all: no keywords, no on-play effect, no additional
/// cost, no activated ability, and no defend / next-attack / target effect.
fn assert_vanilla_attack(card: Card, cost: u8, power: u8, defense: u8, class: CardClass) {
    let data = card.data();
    assert_eq!(data.typ, CardType::AttackAction);
    assert_eq!(data.cost, cost);
    assert_eq!(data.power, power);
    assert_eq!(data.defense, defense);
    assert_eq!(data.card_class, class);
    assert!(data.keyword.is_empty(), "{:?} should carry no keywords", card);
    assert!(data.play_effects.is_empty(), "{:?} should have no on-play effect", card);
    assert!(data.additional_cost.is_none(), "{:?} should have no additional cost", card);
    assert!(data.ability.is_none(), "{:?} should have no activated ability", card);
    assert!(data.defend_effect.is_none(), "{:?} should have no defend effect", card);
    assert!(data.next_attack_effect.is_none(), "{:?} should have no next-attack effect", card);
    assert!(data.attack_effect.is_none(), "{:?} should have no attack effect", card);
}

/// Seat `card` on p1's combat chain against an undefended opponent and resolve
/// combat damage, asserting the opponent lost exactly `expected` life and that
/// nothing was intimidated along the way.
fn assert_undefended_hit(card: Card, expected: u8) {
    let mut gs = setup_rhinar_action_phase();
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, card);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);
    assert_eq!(gs.p2.life, life_before - expected);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
}

#[test]
fn muscle_mutt_is_a_vanilla_6_power_generic_attack() {
    assert_vanilla_attack(Card::MuscleMuttY, 3, 6, 2, CardClass::Generic);
    assert_undefended_hit(Card::MuscleMuttY, 6);
}

#[test]
fn raging_onslaught_is_a_vanilla_6_power_generic_attack() {
    assert_vanilla_attack(Card::RagingOnslaughtY, 3, 6, 3, CardClass::Generic);
    assert_undefended_hit(Card::RagingOnslaughtY, 6);
}

#[test]
fn smash_with_big_tree_is_a_vanilla_6_power_brute_attack() {
    assert_vanilla_attack(Card::SmashWithBigTreeY, 2, 6, 0, CardClass::Brute);
    assert_undefended_hit(Card::SmashWithBigTreeY, 6);
}

#[test]
fn smash_with_big_tree_cannot_be_used_to_block() {
    // Smash with Big Tree has 0 defense and `no_block`, so it must not be
    // offered as a Defend action even while sitting in the defender's hand.
    // A Rhinar mirror, so the defender holds the card from her own deck.
    let mut gs = rhinar_defending_muscle_mutt();
    set_hand(&mut gs, PlayerIndex::P2, &[Card::SmashWithBigTreeY, Card::RagingOnslaughtY]);
    assert!(Card::SmashWithBigTreeY.data().no_block);

    let defends: Vec<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::Defend)
        .map(|a| card_of(&gs, a))
        .collect();
    assert!(!defends.contains(&Card::SmashWithBigTreeY),
        "a no_block card must not be offered as a block");
    // The rest of the hand is still blockable, so the exclusion is card-specific
    // rather than the defend options having gone empty.
    assert!(!defends.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Clearing Bellow (Card::ClearingBellowB)
//
// A 0-cost brute *action* (not an attack) with Intimidate and Go Again. It has
// no on-play effect: both halves of its text are keywords the engine already
// applies generically — Intimidate fires as the card resolves, whether it goes
// on to the combat chain or, as here, straight to the graveyard, and Go Again
// spares the owner's action point.
// ─────────────────────────────────────────────────────────────────────────────

/// Play the 0-cost action `card` from the head of p1's hand and resolve it. A
/// 0-cost card is affordable outright, so it commits straight to the stack with
/// no pitching; both players then pass so it resolves. Returns the game and the
/// played card.
fn play_and_resolve_zero_cost_action(card: Card) -> (TestGame, Card) {
    let mut gs = setup_rhinar_action_phase();
    assert_eq!(card.data().cost, 0, "helper only plays 0-cost cards");

    let idx = *hand_cards(&gs, PlayerIndex::P1).first().expect("hand should have a card to relabel");
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, idx, card);
    let idx = card;

    step_card(&mut gs, ActionType::PlayCard, idx);
    assert_eq!(gs.phase, Phase::ActionInstant);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    (gs, idx)
}

#[test]
fn clearing_bellow_has_go_again_and_intimidate() {
    use crate::cards::Keyword;
    let data = Card::ClearingBellowB.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!(data.card_class, CardClass::Brute);
    assert!(data.keyword.contains(Keyword::GoAgain));
    assert!(data.keyword.contains(Keyword::Intimidate));
    // Both halves of the text are keywords: there is no on-play effect.
    assert!(data.play_effects.is_empty());
}

#[test]
fn clearing_bellow_intimidates_on_resolve_and_keeps_the_action_point() {
    let (gs, idx) = play_and_resolve_zero_cost_action(Card::ClearingBellowB);

    // A non-attack action resolves to the graveyard rather than the chain, and
    // its Intimidate still fires on the way there.
    assert!(in_zone(&gs, PlayerIndex::P1, idx, Zone::Graveyard));
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
    assert!(gs.p1.has_intimidated());

    // Go Again: the action point is not spent, so Rhinar may act again.
    assert_eq!(gs.p1.action_points, 1);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Wounded Bull (Card::WoundedBullY)
//
// "When you play Wounded Bull, if you have less health than an opposing hero,
// it gains +1 power." A generic 6-power attack action whose on-play effect is
// the `HasLessLife` condition driving the shared `ConditionalPower` payoff: the
// life totals are compared as the card resolves, and the +1 is banked on the
// player and folded into the chain's power when combat damage resolves. The
// comparison is strict, so being level on life grants nothing.
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve Wounded Bull's on-play effect for `owner` with the two heroes on
/// `p1_life` / `p2_life`, returning the game so the banked bonus can be checked.
fn resolve_wounded_bull_at(owner: PlayerIndex, p1_life: u8, p2_life: u8) -> TestGame {
    let mut gs = setup_rhinar_action_phase();
    gs.p1.life = p1_life;
    gs.p2.life = p2_life;

    let effect = Card::WoundedBullY
        .data()
        .play_effects
        .first()
        .expect("Wounded Bull should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, owner, effect);
    gs
}

#[test]
fn wounded_bull_effect_is_conditional_power_on_less_life() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::WoundedBullY.data();
    assert_eq!(data.typ, CardType::AttackAction);
    assert_eq!(data.power, 6);
    assert_eq!(data.card_class, CardClass::Generic);
    let effect = data.play_effects.first().expect("Wounded Bull should carry an on-play effect");
    assert!(matches!(effect.condition, OnPlayConditionType::HasLessLife));
    assert!(matches!(effect.effectType, OnPlayEffectType::ConditionalPower));
    assert_eq!(effect.magnitude, 1);
}

#[test]
fn wounded_bull_banks_plus1_when_behind_on_life() {
    let gs = resolve_wounded_bull_at(PlayerIndex::P1, 15, 20);
    assert_eq!(gs.p1.attack_power_bonus, 1);
    assert_eq!(gs.p2.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_banks_nothing_when_level_on_life() {
    // "Less health" is a strict comparison: level on life grants nothing.
    let gs = resolve_wounded_bull_at(PlayerIndex::P1, 20, 20);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_banks_nothing_when_ahead_on_life() {
    let gs = resolve_wounded_bull_at(PlayerIndex::P1, 20, 15);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_condition_is_relative_to_its_owner() {
    // The condition compares the *owner's* life against their opponent's, so the
    // same life totals that deny p1 the bonus grant it to p2.
    let gs = resolve_wounded_bull_at(PlayerIndex::P2, 20, 15);
    assert_eq!(gs.p2.attack_power_bonus, 1);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_hits_for_7_when_behind_on_life() {
    let mut gs = resolve_wounded_bull_at(PlayerIndex::P1, 15, 20);
    assert_eq!(gs.p1.attack_power_bonus, 1);

    // Seat Wounded Bull on the chain against an undefended opponent: 6 printed
    // power plus the banked +1.
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::WoundedBullY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 7);
    // The on-play bonus is single-use: a follow-up attack does not inherit it.
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn wounded_bull_hits_for_6_when_ahead_on_life() {
    let mut gs = resolve_wounded_bull_at(PlayerIndex::P1, 20, 15);
    assert_eq!(gs.p1.attack_power_bonus, 0);

    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::WoundedBullY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 6);
}

// ─────────────────────────────────────────────────────────────────────────────
// Sigil of Solace (Card::SigilofSolaceB)
//
// "Gain 1 life." A 0-cost blue *instant* with no power or defense, so it never
// blocks and never attacks — it simply resolves to the graveyard, healing its
// owner on the way. Being an instant it costs no action point, and there is no
// maximum life total, so the gain applies even at full health.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sigil_of_solace_effect_is_gain_1_life() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::SigilofSolaceB.data();
    assert_eq!(data.typ, CardType::Instant);
    assert_eq!(data.cost, 0);
    assert_eq!(data.pitch, 3);
    assert_eq!(data.power, 0);
    // No defense and flagged no_block: it cannot be used to block.
    assert_eq!(data.defense, 0);
    assert!(data.no_block);

    let effect = data.play_effects.first().expect("Sigil of Solace should carry an on-play effect");
    // Unconditional: the life is gained whenever it resolves.
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::GainLife));
    assert_eq!(effect.magnitude, 1);
}

#[test]
fn sigil_of_solace_gains_1_life_on_resolve() {
    let mut gs = setup_dorinthea_action_phase();
    let idx = *hand_cards(&gs, PlayerIndex::P1).first().expect("hand should have a card to relabel");
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, idx, Card::SigilofSolaceB);

    // An instant is playable at action speed, so it is offered here.
    let idx = Card::SigilofSolaceB;
    assert!(playable_cards(&gs).contains(&Card::SigilofSolaceB));

    let life_before = gs.p1.life;
    step_card(&mut gs, ActionType::PlayCard, idx);
    assert_eq!(gs.phase, Phase::ActionInstant);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    // Resolved to the graveyard, one life richer. Dorinthea starts at her full
    // 20, so this also pins down that there is no maximum life total to cap at.
    assert!(in_zone(&gs, PlayerIndex::P1, idx, Zone::Graveyard));
    assert_eq!(life_before, 20);
    assert_eq!(gs.p1.life, life_before + 1);

    // An instant costs no action point, so Dorinthea may still act.
    assert_eq!(gs.p1.action_points, 1);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
}

#[test]
fn sigil_of_solace_heals_only_its_owner() {
    let mut gs = setup_rhinar_action_phase();
    let (p1_before, p2_before) = (gs.p1.life, gs.p2.life);

    let effect = Card::SigilofSolaceB.data().play_effects.first().unwrap();
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P2, effect);

    // The gain follows the resolving card's owner, leaving the opponent alone.
    assert_eq!(gs.p2.life, p2_before + 1);
    assert_eq!(gs.p1.life, p1_before);
}

// ─────────────────────────────────────────────────────────────────────────────
// Come to Fight (Card::ComeToFightB)
//
// "Your next attack action card you play this turn gains +1 power. Go again."
// A 1-cost blue action. Go again is the keyword; the +1 is banked on the player
// as `next_attack_action_bonus` and folded into the chain's power when an attack
// action card resolves combat damage. It is the class-agnostic sibling of
// Awakening Bellow's brute-only bonus: any attack action card takes it, but a
// weapon swing — not an attack action card — leaves it banked.
// ─────────────────────────────────────────────────────────────────────────────

/// Bank Come to Fight's +1 for p1 by resolving its on-play effect directly.
fn bank_come_to_fight_bonus() -> TestGame {
    let mut gs = setup_rhinar_action_phase();
    let effect = Card::ComeToFightB
        .data()
        .play_effects
        .first()
        .expect("Come to Fight should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_attack_action_bonus, 1);
    gs
}

#[test]
fn come_to_fight_effect_is_next_attack_action_power() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    use crate::cards::Keyword;
    let data = Card::ComeToFightB.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!(data.cost, 1);
    assert_eq!(data.pitch, 3);
    assert_eq!(data.defense, 3);
    // "Go again" is the keyword half of the text.
    assert!(data.keyword.contains(Keyword::GoAgain));

    let effect = data.play_effects.first().expect("Come to Fight should carry an on-play effect");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::NextAttackActionPower));
    assert_eq!(effect.magnitude, 1);
}

#[test]
fn come_to_fight_banks_plus1_for_the_next_attack() {
    let gs = bank_come_to_fight_bonus();
    assert_eq!(gs.p1.next_attack_action_bonus, 1);
    assert_eq!(gs.p2.next_attack_action_bonus, 0);
    // The brute-only bank is a separate pot and stays untouched.
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 0);
}

#[test]
fn come_to_fight_followup_generic_attack_gets_plus1() {
    let mut gs = bank_come_to_fight_bonus();

    // Muscle Mutt (generic, 6 power) against an undefended opponent.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
    assert_eq!(attacker.data().card_class, CardClass::Generic);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 6 + 1 = 7 damage, and the bonus is spent.
    assert_eq!(gs.p2.life, life_before - 7);
    assert_eq!(gs.p1.next_attack_action_bonus, 0);
}

#[test]
fn come_to_fight_followup_brute_attack_also_gets_plus1() {
    let mut gs = bank_come_to_fight_bonus();

    // Unlike Awakening Bellow's brute-only +3, this bonus is class-agnostic: a
    // brute attack action card takes it just the same.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    assert_eq!(attacker.data().card_class, CardClass::Brute);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    assert_eq!(gs.p2.life, life_before - 7);
    assert_eq!(gs.p1.next_attack_action_bonus, 0);
}

#[test]
fn come_to_fight_weapon_swing_does_not_get_plus1() {
    let mut gs = bank_come_to_fight_bonus();

    // Bone Basher is an attack, but a *weapon* rather than an attack action
    // card, so the text does not cover it. Seat it (4 power) and resolve.
    let attacker = place_weapon_on_chain(&mut gs, PlayerIndex::P1);
    assert_eq!(attacker, Card::BoneBasher);
    assert_eq!(attacker.data().typ, CardType::Weapon);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 4 damage, and the bonus stays banked for a later attack action this turn.
    assert_eq!(gs.p2.life, life_before - 4);
    assert_eq!(gs.p1.next_attack_action_bonus, 1);
}

#[test]
fn come_to_fight_bonus_does_not_survive_the_turn() {
    let mut gs = bank_come_to_fight_bonus();

    // "This turn": an unspent bonus is cleared at the turn boundary rather than
    // leaking into a later turn's attacks.
    begin_turn(&mut gs);
    assert_eq!(gs.p1.next_attack_action_bonus, 0);
    assert_eq!(gs.p2.next_attack_action_bonus, 0);
}

#[test]
fn come_to_fight_played_from_hand_banks_the_bonus_and_keeps_the_action_point() {
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});

    // Relabel Muscle Mutt to Come to Fight (cost 1); Rhinar's seed-42 opening
    // hand is then Come to Fight, Pack Call, Raging Onslaught, Clearing Bellow.
    let ctf = expect_in(&gs, PlayerIndex::P1, Card::MuscleMuttY, Zone::Hand, "Muscle Mutt should be in the opening hand");
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, ctf, Card::ComeToFightB);

    // Cost 1 isn't covered outright, so it waits on a pitch.
    let ctf = Card::ComeToFightB;
    step_card(&mut gs, ActionType::PlayCard, ctf);
    assert_eq!(gs.phase, Phase::ActionPitch);

    // Clearing Bellow (pitch 3) covers it, committing Come to Fight to the stack.
    let cb = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in the opening hand");
    step_card(&mut gs, ActionType::Pitch, cb);
    assert_eq!(gs.phase, Phase::ActionInstant);

    // Both pass: it resolves to the graveyard and banks the +1.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert!(in_zone(&gs, PlayerIndex::P1, ctf, Zone::Graveyard));
    assert_eq!(gs.p1.next_attack_action_bonus, 1);

    // Go Again: the action point survives, so the pumped attack can follow.
    assert_eq!(gs.p1.action_points, 1);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);

    // Raging Onslaught (generic, 6 power) then lands for 6 + 1 = 7.
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::RagingOnslaughtY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);
    assert_eq!(gs.p2.life, life_before - 7);
}

// ─────────────────────────────────────────────────────────────────────────────
// Dodge (Card::DodgeB) and Toughen Up (Card::ToughenUpB)
//
// Both are text-free blue defense reactions — Dodge blocks 2 for 0, Toughen Up
// blocks 4 for 2 — and both are covered here as a pair because everything that
// makes them work is the defense-reaction rules rather than card-specific
// effects:
//
//   * they are never declared as blockers in the defend step, whatever their
//     defense value (`legal_defend_phase` filters the card type out);
//   * only the defender is offered them, and only in the defend reaction step
//     (`is_defender_reaction_playable`), where they go on the stack like an
//     instant and can be responded to;
//   * a cost is paid by pitching, in the reaction window's own pitch phase —
//     Dodge is free and commits straight to the stack, Toughen Up's 2 has to be
//     pitched for;
//   * as the reaction resolves it joins the chain link the attack is on, so its
//     block adds to whatever was declared in the defend step, and it leaves for
//     the graveyard with the blockers when the chain closes.
// ─────────────────────────────────────────────────────────────────────────────

/// Assert `card` is a defense reaction with the given printed cost and defense
/// and carries no rules text at all: no keywords, no on-play effect, no
/// additional cost, no activated ability, and no defend / next-attack / target
/// effect. It blocks with its printed defense and nothing more.
fn assert_vanilla_defense_reaction(card: Card, cost: u8, defense: u8) {
    let data = card.data();
    assert_eq!(data.typ, CardType::DefenseReaction);
    assert_eq!(data.cost, cost);
    assert_eq!(data.defense, defense);
    assert_eq!(data.power, 0);
    assert!(data.keyword.is_empty(), "{:?} should carry no keywords", card);
    assert!(data.play_effects.is_empty(), "{:?} should have no on-play effect", card);
    assert!(data.additional_cost.is_none(), "{:?} should have no additional cost", card);
    assert!(data.ability.is_none(), "{:?} should have no activated ability", card);
    assert!(data.defend_effect.is_none(), "{:?} should have no defend effect", card);
    assert!(data.next_attack_effect.is_none(), "{:?} should have no next-attack effect", card);
    assert!(data.attack_effect.is_none(), "{:?} should have no attack effect", card);
}

/// Rhinar (p1) attacks with Muscle Mutt (a vanilla 6-power generic attack) and
/// Dorinthea (p2) is on defense, in the Defend phase with the attack on link 0
/// of p1's chain. The starting point for every defense-reaction test below: 6
/// power against an untouched 20 life makes the arithmetic of a block obvious.
fn dorinthea_defending_muscle_mutt() -> TestGame {
    let (gs, _) = play_and_resolve_attack(Card::MuscleMuttY);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert_eq!(gs.p2.life, 20);
    gs
}

/// `dorinthea_defending_muscle_mutt` in a Rhinar mirror: Rhinar (p1) attacks
/// with Muscle Mutt and a second Rhinar (p2) defends, so the defender can block
/// and react with Rhinar's cards — Dodge, Pack Call, Rally the Rearguard — from
/// her own deck. Same Defend-phase starting point, same 6 power into 20 life.
fn rhinar_defending_muscle_mutt() -> TestGame {
    let (gs, _) = play_and_resolve_attack_against(build_rhinar_deck(), Card::MuscleMuttY);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert_eq!(gs.p2.hero, Card::Rhinar);
    assert_eq!(gs.p2.life, 20);
    gs
}

/// Dorinthea (p1) attacks with Flock of the Feather Walkers (a vanilla 5-power
/// attack, her only attack action card) and Rhinar (p2) defends, in the Defend
/// phase with the attack on link 0. For tests that need Rhinar's defensive
/// cards on one side and Dorinthea's instants or attack reactions on the other.
fn rhinar_defending_flock() -> TestGame {
    let mut gs = setup_dorinthea_action_phase();
    set_hand(&mut gs, PlayerIndex::P1,
        &[Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR, Card::SharpenSteelR, Card::SharpenSteelR]);
    let hand = vec![Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR, Card::SharpenSteelR, Card::SharpenSteelR];
    // Cost 1, paid with a single pitch-1 card.
    step_card(&mut gs, ActionType::PlayCard, hand[0]);
    assert_eq!(gs.phase, Phase::ActionPitch);
    step_card(&mut gs, ActionType::Pitch, hand[1]);
    assert_eq!(gs.phase, Phase::ActionInstant);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert_eq!(gs.p2.life, 20);
    gs
}

/// Step from the Defend phase into the defend reaction step with the defender
/// (p2) holding priority: she declares no blockers, then the attacker passes
/// his first crack at the reaction window.
fn pass_to_defender_reaction(gs: &mut TestGame) {
    step(gs, Action{ typ: ActionType::Pass, card: None}); // no blockers declared
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    step(gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes priority
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P2);
}

/// The cards offered to the active player as `Defend` actions (blockers).
fn blockable_cards(gs: &Gamestate) -> HashSet<Card> {
    legal_actions(gs).iter()
        .filter(|a| a.typ == ActionType::Defend)
        .map(|a| card_of(&gs, a))
        .collect()
}

#[test]
fn dodge_is_a_vanilla_0_cost_2_block_defense_reaction() {
    assert_vanilla_defense_reaction(Card::DodgeB, 0, 2);
}

#[test]
fn toughen_up_is_a_vanilla_2_cost_4_block_defense_reaction() {
    assert_vanilla_defense_reaction(Card::ToughenUpB, 2, 4);
}

#[test]
fn defense_reactions_cannot_be_declared_as_blockers() {
    // Neither reaction is a legal block, however much defense it prints: a
    // defense reaction is played in the reaction step, never declared here. Each
    // is checked in the hand of the hero whose deck holds it, next to an
    // ordinary card from the same deck that does still block — so the reaction
    // is excluded by its card type rather than the defend options having gone
    // empty.

    // Dorinthea with Toughen Up and Driving Blade.
    let mut gs = dorinthea_defending_muscle_mutt();
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ToughenUpB, Card::DrivingBladeY]);
    let blocks = blockable_cards(&gs);
    assert!(!blocks.contains(&Card::ToughenUpB),
        "Toughen Up is a defense reaction and must not be offered as a block");
    assert!(blocks.contains(&Card::DrivingBladeY));

    // Rhinar with Dodge and Raging Onslaught.
    let mut gs = rhinar_defending_muscle_mutt();
    set_hand(&mut gs, PlayerIndex::P2, &[Card::DodgeB, Card::RagingOnslaughtY]);
    let blocks = blockable_cards(&gs);
    assert!(!blocks.contains(&Card::DodgeB),
        "Dodge is a defense reaction and must not be offered as a block");
    assert!(blocks.contains(&Card::RagingOnslaughtY));
}

#[test]
fn only_the_defender_is_offered_defense_reactions_in_the_reaction_step() {
    // Dorinthea attacks, so the attacker can hold both kinds of reaction from
    // her own deck; Rhinar defends with his.
    let mut gs = rhinar_defending_flock();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);

    // The attacker (p1) holds priority first. Hand her a defense reaction and
    // an attack reaction: only the attack reaction is hers to play. She can
    // afford Toughen Up's 2, so cost is not what rules it out. The attack is
    // restaged as her second Dawnblade swing so In the Swing's own
    // requirements are met and only its speed is in question.
    assert_eq!(gs.active_player, PlayerIndex::P1);
    stage_second_weapon_swing(&mut gs, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ToughenUpB, Card::InTheSwingR]);
    gs.p1.resources = 2;
    let attacker_options = playable_cards(&gs);
    assert!(!attacker_options.contains(&Card::ToughenUpB),
        "the attacking player must not be offered a defense reaction");
    assert!(attacker_options.contains(&Card::InTheSwingR),
        "the attacking player keeps her own attack reactions");

    // Priority passes to the defender, who is offered his.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.active_player, PlayerIndex::P2);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::DodgeB]);
    let defender_options = playable_cards(&gs);
    assert!(defender_options.contains(&Card::DodgeB));
}

#[test]
fn dodge_is_free_and_commits_straight_to_the_stack() {
    let mut gs = rhinar_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    set_hand(&mut gs, PlayerIndex::P2, &[Card::DodgeB]);
    let dodge = *hand_cards(&gs, PlayerIndex::P2).first().expect("Dodge should be the defender's only card");

    // Cost 0: nothing to pitch for, so it goes on the stack and the window
    // stays open for a response rather than dropping into a pitch phase.
    assert_eq!(Card::DodgeB.data().cost, 0);
    step_card(&mut gs, ActionType::PlayCard, dodge);
    assert_eq!(gs.phase, Phase::Reaction);
    assert!(on_stack(&gs, dodge));
    assert!(!gs.stack_is_empty());
    assert!(gs.p2.pitch.is_empty(), "a free reaction pitches nothing");
}

#[test]
fn toughen_up_is_paid_for_by_pitching_in_the_reaction_window() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Two pitch-1 reds pay Toughen Up's cost of 2.
    assert_eq!(Card::ToughenUpB.data().cost, 2);
    assert_eq!(Card::SharpenSteelR.data().pitch, 1);
    set_hand(&mut gs, PlayerIndex::P2,
        &[Card::ToughenUpB, Card::SharpenSteelR, Card::SharpenSteelR]);
    let hand = vec![Card::ToughenUpB, Card::SharpenSteelR, Card::SharpenSteelR];

    // Playing it drops into the reaction window's own pitch phase; it is still
    // in hand (pending, not on the stack) until the cost is covered.
    step_card(&mut gs, ActionType::PlayCard, hand[0]);
    assert_eq!(gs.phase, Phase::ReactionPitch);
    assert!(in_zone(&gs, PlayerIndex::P2, hand[0], Zone::Hand));

    // One pitch leaves a resource short, the second covers it and returns to
    // the reaction window with the card on the stack and the cost spent.
    step_card(&mut gs, ActionType::Pitch, hand[1]);
    assert_eq!(gs.phase, Phase::ReactionPitch);
    step_card(&mut gs, ActionType::Pitch, hand[2]);
    assert_eq!(gs.phase, Phase::Reaction);
    assert!(on_stack(&gs, hand[0]));
    assert_eq!(gs.p2.resources, 0);
    assert!(in_zone(&gs, PlayerIndex::P2, hand[1], Zone::Pitch));
    assert!(in_zone(&gs, PlayerIndex::P2, hand[2], Zone::Pitch));
}

/// Play `card` (a 0-cost defense reaction) from the defender's hand in the
/// defend reaction step and resolve the window out: both players pass, so the
/// reaction resolves and, with the stack empty again, the window closes and
/// combat damage is dealt. Returns the game and the reaction.
fn defender_reacts_with(gs: &mut TestGame, card: Card) -> Card {
    assert_eq!(card.data().cost, 0, "helper only plays a free reaction");
    set_hand(gs, PlayerIndex::P2, &[card]);
    let idx = *hand_cards(&gs, PlayerIndex::P2).first().expect("the reaction should be the only card in hand");
    step_card(gs, ActionType::PlayCard, idx);
    step(gs, Action{ typ: ActionType::Pass, card: None});
    step(gs, Action{ typ: ActionType::Pass, card: None});
    close_reaction_step(gs);
    idx
}

#[test]
fn dodge_adds_its_block_to_the_combat_chain() {
    let mut gs = rhinar_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);
    let dodge = defender_reacts_with(&mut gs, Card::DodgeB);

    // Dodge resolved onto the defender's side of the link the attack is on,
    // rather than to her graveyard.
    assert!(in_zone(&gs, PlayerIndex::P2, dodge, Zone::CombatChain));
    assert_eq!(link_cards(&gs, PlayerIndex::P2, 0), vec![dodge]);

    // Its 2 block came off Muscle Mutt's 6 power: 4 damage, not 6.
    assert_eq!(gs.p2.life, 20 - 4);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
}

#[test]
fn toughen_up_adds_its_block_to_the_combat_chain() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Give the defender the two resources up front so the reaction commits
    // straight to the stack; the pitching path is covered above.
    gs.p2.resources = 2;
    let tu = {
        set_hand(&mut gs, PlayerIndex::P2, &[Card::ToughenUpB]);
        let idx = *hand_cards(&gs, PlayerIndex::P2).first().expect("Toughen Up should be the only card in hand");
        step_card(&mut gs, ActionType::PlayCard, idx);
        assert_eq!(gs.phase, Phase::Reaction);
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
        close_reaction_step(&mut gs);
        idx
    };

    // 4 block against 6 power leaves 2 damage.
    assert!(in_zone(&gs, PlayerIndex::P2, tu, Zone::CombatChain));
    assert_eq!(link_cards(&gs, PlayerIndex::P2, 0), vec![tu]);
    assert_eq!(gs.p2.life, 20 - 2);
}

#[test]
fn a_defense_reaction_stacks_with_a_declared_blocker() {
    let mut gs = rhinar_defending_muscle_mutt();

    // Declare Raging Onslaught (defense 3) as a blocker in the defend step...
    set_hand(&mut gs, PlayerIndex::P2, &[Card::RagingOnslaughtY, Card::DodgeB]);
    let hand = vec![Card::RagingOnslaughtY, Card::DodgeB];
    let (blocker, dodge) = (hand[0], hand[1]);
    assert_eq!(Card::RagingOnslaughtY.data().defense, 3);
    step_card(&mut gs, ActionType::Defend, blocker);
    assert!(in_zone(&gs, PlayerIndex::P2, blocker, Zone::CombatChain));

    // ...then add Dodge (2) from the same hand in the reaction step.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    assert_eq!(gs.active_player, PlayerIndex::P2);
    step_card(&mut gs, ActionType::PlayCard, dodge);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    close_reaction_step(&mut gs);

    // Both sit on the same chain link, so their defense is summed: 3 + 2 = 5
    // against 6 power leaves 1 damage.
    let mut on_link = link_cards(&gs, PlayerIndex::P2, 0);
    on_link.sort_by_key(|c| *c as u8);
    let mut expected = vec![dodge, blocker];
    expected.sort_by_key(|c| *c as u8);
    assert_eq!(on_link, expected);
    assert_eq!(gs.p2.life, 20 - 1);
}

#[test]
fn the_reaction_step_stays_open_after_each_resolution() {
    // Rhinar defends Muscle Mutt (6) with two Dodges, one at a time. After the
    // first resolves, priority returns to the attacker rather than the step
    // closing; she passes, and he can play the second.
    let mut gs = rhinar_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::DodgeB, Card::DodgeB]);
    step_card(&mut gs, ActionType::PlayCard, Card::DodgeB);
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Reaction, "a resolution does not close the step");
    assert_eq!(gs.active_player, PlayerIndex::P1, "priority returns to the turn player");
    assert_eq!(gs.p2.life, 20, "no damage yet");

    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert!(playable_cards(&gs).contains(&Card::DodgeB));
    step_card(&mut gs, ActionType::PlayCard, Card::DodgeB);
    pass_once_each(&mut gs);
    close_reaction_step(&mut gs);
    // 6 against 2 + 2.
    assert_eq!(gs.p2.life, 20 - 2);
    assert_eq!(gs.phase, Phase::Action);
}

#[test]
fn a_defense_reaction_leaves_the_chain_for_the_graveyard_when_it_closes() {
    let mut gs = rhinar_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);
    let dodge = defender_reacts_with(&mut gs, Card::DodgeB);
    assert!(in_zone(&gs, PlayerIndex::P2, dodge, Zone::CombatChain));

    // The turn player passes his action phase: the combat chain closes and
    // every card on it goes to its owner's graveyard, the reaction included.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Arsenal);
    assert!(in_zone(&gs, PlayerIndex::P2, dodge, Zone::Graveyard));
    assert!(gs.p2.chain_link[0].is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Defense reactions played from the arsenal
//
// A card in the arsenal is played as though it were in hand, so a defense
// reaction sitting there is available to the defender in the defend reaction
// step exactly as one in hand would be — paid for by pitching from hand (the
// arsenal card is not in the hand pool, so all of it can pay), and resolving
// onto the chain link the attack sits on.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn dodge_can_be_played_from_the_arsenal() {
    let mut gs = rhinar_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Dodge waits in the arsenal rather than the hand, and is free to play.
    let dodge = put_in_arsenal(&mut gs, PlayerIndex::P2, Card::DodgeB);
    step_card(&mut gs, ActionType::PlayCard, dodge);
    assert_eq!(gs.phase, Phase::Reaction);
    assert!(on_stack(&gs, dodge));
    assert_eq!(gs.p2.arsenal, None, "playing it empties the arsenal slot");

    // It blocks from the chain just as it would played from hand: 6 - 2 = 4.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    close_reaction_step(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, dodge, Zone::CombatChain));
    assert_eq!(link_cards(&gs, PlayerIndex::P2, 0), vec![dodge]);
    assert_eq!(gs.p2.life, 20 - 4);
}

#[test]
fn toughen_up_from_the_arsenal_is_paid_for_by_pitching_the_whole_hand() {
    let mut gs = dorinthea_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Two pitch-1 reds in hand, Toughen Up (cost 2) in the arsenal. The arsenal
    // card is not part of the hand's pitch pool, so both hand cards are free to
    // pay for it.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::SharpenSteelR, Card::SharpenSteelR]);
    let tu = put_in_arsenal(&mut gs, PlayerIndex::P2, Card::ToughenUpB);
    let hand = hand_cards(&gs, PlayerIndex::P2);
    assert_eq!(hand.len(), 1, "one of the two reds went to the arsenal slot");

    // One pitch-1 card can't cover a cost of 2, so it isn't offered yet.
    let offered: Vec<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::PlayCard)
        .map(|a| card_of(&gs, a))
        .collect();
    assert!(!offered.contains(&tu));

    // Bank a resource and it becomes payable with the single card in hand.
    gs.p2.resources = 1;
    step_card(&mut gs, ActionType::PlayCard, tu);
    assert_eq!(gs.phase, Phase::ReactionPitch);
    assert!(in_zone(&gs, PlayerIndex::P2, tu, Zone::Arsenal),
        "it stays in the arsenal until the cost is covered");
    step_card(&mut gs, ActionType::Pitch, hand[0]);
    assert_eq!(gs.phase, Phase::Reaction);
    assert!(on_stack(&gs, tu));
    assert_eq!(gs.p2.arsenal, None);
    assert_eq!(gs.p2.resources, 0);

    // 4 block against Muscle Mutt's 6 power leaves 2 damage.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    close_reaction_step(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, tu, Zone::CombatChain));
    assert_eq!(gs.p2.life, 20 - 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Titanium Bauble (Card::TitaniumBaubleB)
//
// A blue *resource* card: cost 0, pitch 3, 0 power, 3 defense, no rules text.
// It is the catalog's only `CardType::Resource`, and what makes it work is a
// rule rather than an effect — a resource is never played. It contributes the
// two ways any card can without being put on the stack: pitched from hand for
// its 3 resources, or declared as a blocker for its 3 defense. The tests below
// pin both halves: that it is offered in neither the action, instant nor
// reaction window, from hand or from the arsenal (`can_ever_be_played`), and
// that it still pitches and blocks for its printed 3.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn titanium_bauble_is_a_vanilla_3_pitch_3_block_resource() {
    use crate::cards::Color;
    let data = Card::TitaniumBaubleB.data();
    assert_eq!(data.typ, CardType::Resource);
    assert_eq!(data.cost, 0);
    assert_eq!(data.pitch, 3);
    assert_eq!(data.power, 0);
    assert_eq!(data.defense, 3);
    assert_eq!(data.card_class, CardClass::Generic);
    assert!(matches!(data.color, Some(Color::Blue)));
    // Nothing about it is an effect — it has no text at all.
    assert!(!data.no_block, "a resource with defense still blocks");
    assert!(data.keyword.is_empty(), "Titanium Bauble should carry no keywords");
    assert!(data.play_effects.is_empty(), "Titanium Bauble should have no on-play effect");
    assert!(data.additional_cost.is_none(), "Titanium Bauble should have no additional cost");
    assert!(data.ability.is_none(), "Titanium Bauble should have no activated ability");
    assert!(data.defend_effect.is_none(), "Titanium Bauble should have no defend effect");
    assert!(data.next_attack_effect.is_none(), "Titanium Bauble should have no next-attack effect");
    assert!(data.attack_effect.is_none(), "Titanium Bauble should have no attack effect");
    assert!(data.constant_effect.is_none(), "Titanium Bauble should have no constant effect");
}

#[test]
fn titanium_bauble_is_never_offered_as_a_play_from_hand() {
    let mut gs = setup_rhinar_action_phase();
    // A 0-cost action alongside it, so the action window is demonstrably live
    // and affordability is not what rules the bauble out.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::TitaniumBaubleB, Card::ClearingBellowB]);
    assert_eq!(gs.p1.action_points, 1);

    let playable = playable_cards(&gs);
    assert!(playable.contains(&Card::ClearingBellowB),
        "the action window should be offering the 0-cost action next to it");
    assert!(!playable.contains(&Card::TitaniumBaubleB),
        "a resource card must never be offered as a play");
}

#[test]
fn titanium_bauble_is_never_offered_as_a_play_from_the_arsenal() {
    let mut gs = setup_rhinar_action_phase();

    // Park a 0-cost action in the arsenal: it is offered from there, exactly as
    // Dodge is in the reaction tests above.
    let idx = put_in_arsenal(&mut gs, PlayerIndex::P1, Card::ClearingBellowB);
    let offered: Vec<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::PlayCard)
        .map(|a| card_of(&gs, a))
        .collect();
    assert!(offered.contains(&idx), "an arsenal card of the right speed is offered");

    // Relabel that same arsenal slot to Titanium Bauble — nothing else about the
    // game changes — and the offer disappears. The type is what rules it out.
    relabel(&mut gs, PlayerIndex::P1, Zone::Arsenal, idx, Card::TitaniumBaubleB);
    let idx = Card::TitaniumBaubleB;
    let offered: Vec<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::PlayCard)
        .map(|a| card_of(&gs, a))
        .collect();
    assert!(!offered.contains(&idx),
        "a resource card must never be offered as a play from the arsenal either");
}

#[test]
fn titanium_bauble_is_not_offered_in_the_defender_reaction_window() {
    let mut gs = rhinar_defending_muscle_mutt();
    pass_to_defender_reaction(&mut gs);

    // Dodge is free and legal here, so the reaction window is live.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::TitaniumBaubleB, Card::DodgeB]);
    let playable = playable_cards(&gs);
    assert!(playable.contains(&Card::DodgeB),
        "the defender's reaction window should be offering Dodge next to it");
    assert!(!playable.contains(&Card::TitaniumBaubleB),
        "a resource card is not a reaction and must not be offered in the reaction step");
}

#[test]
fn titanium_bauble_pitches_for_3() {
    let mut gs = setup_rhinar_action_phase();
    // Muscle Mutt costs exactly 3, so the bauble's pitch covers it on its own.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::MuscleMuttY, Card::TitaniumBaubleB]);
    let hand = vec![Card::MuscleMuttY, Card::TitaniumBaubleB];
    let (mutt, bauble) = (hand[0], hand[1]);
    assert_eq!(Card::MuscleMuttY.data().cost, 3);

    step_card(&mut gs, ActionType::PlayCard, mutt);
    assert_eq!(gs.phase, Phase::ActionPitch);

    // Pitching the bauble pays the whole cost in one card: the attack leaves the
    // stack for the reaction window with nothing left over.
    step_card(&mut gs, ActionType::Pitch, bauble);
    assert_eq!(gs.phase, Phase::ActionInstant,
        "3 pitch covers a cost of 3, so no further pitching is asked for");
    assert!(in_zone(&gs, PlayerIndex::P1, bauble, Zone::Pitch));
    assert_eq!(gs.p1.resources, 0, "all 3 went into the cost, none floats");
}

#[test]
fn titanium_bauble_blocks_for_3() {
    let mut gs = dorinthea_defending_muscle_mutt();
    set_hand(&mut gs, PlayerIndex::P2, &[Card::TitaniumBaubleB]);
    let bauble = *hand_cards(&gs, PlayerIndex::P2).first().expect("the bauble should be the only card in hand");

    // Unlike a defense reaction, a resource is declared as an ordinary blocker
    // in the defend step.
    assert!(blockable_cards(&gs).contains(&Card::TitaniumBaubleB),
        "a resource with defense is a legal blocker");
    step_card(&mut gs, ActionType::Defend, bauble);
    assert!(in_zone(&gs, PlayerIndex::P2, bauble, Zone::CombatChain));

    // Its 3 block came off Muscle Mutt's 6 power: 3 damage, not 6.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // defender passes
    assert_eq!(gs.p2.life, 20 - 3);
}


// ─────────────────────────────────────────────────────────────────────────────
// Pack Call (Card::PackCallY)
//
// "When you defend with Pack Call, reveal the top card of your deck. If it has
// 6 or more power, put it on top of your deck. Otherwise, put it on the
// bottom." A 6-power brute attack action that also carries the catalog's first
// `defend_effect`.
//
// The trigger is not applied as the card is declared. Blockers are declared
// first, and once the defender finishes the defend step every trigger for that
// attack goes on the stack together (`push_defend_triggers`), to resolve one
// layer at a time in the reaction window — so each can be responded to, exactly
// like a played card. `resolve_top_of_stack` recognises the `DefendTrigger`
// entry and resolves the effect alone: the card is already on the combat chain
// and stays there.
//
// "Your deck" is the deck of whoever defends with it — never the turn player —
// so the tests below block with it as p2 and check that p1's deck is untouched.
// ─────────────────────────────────────────────────────────────────────────────

/// The order of `pid`'s deck, top first.
fn deck_order(gs: &Gamestate, pid: PlayerIndex) -> Vec<Card> {
    deck_cards(gs, pid)
}

/// Declare `cards` as p2's blockers, in the order given, then finish the defend
/// step. Returns them. Any defend triggers are on the stack when this
/// returns, still unresolved: the reaction window below is what resolves them.
fn declare_blockers(gs: &mut TestGame, cards: &[Card]) -> Vec<Card> {
    set_hand(gs, PlayerIndex::P2, cards);
    for &card in cards {
        step_card(gs, ActionType::Defend, card);
    }
    step(gs, Action{ typ: ActionType::Pass, card: None}); // done declaring blockers
    // With triggers to resolve we land in their own window; with none, straight
    // into the ordinary reaction window.
    assert!(matches!(gs.phase, Phase::DefendTriggers | Phase::Reaction));
    cards.to_vec()
}

/// Both players pass in succession, resolving exactly one layer off the stack
/// (or closing the window and dealing combat damage when the stack is empty).
fn pass_once_each(gs: &mut TestGame) {
    step(gs, Action{ typ: ActionType::Pass, card: None});
    step(gs, Action{ typ: ActionType::Pass, card: None});
}

/// The cards whose defend triggers are waiting on the stack, bottom slot first.
fn stacked_defend_triggers(gs: &Gamestate) -> Vec<Card> {
    gs.stack.iter()
        .filter(|p| p.typ == ActionType::DefendTrigger)
        .map(|p| layer_card(gs, &p))
        .collect()
}

#[test]
fn pack_call_is_a_6_power_brute_attack_with_a_defend_trigger() {
    let data = Card::PackCallY.data();
    assert_eq!(data.typ, CardType::AttackAction);
    assert_eq!(data.cost, 3);
    assert_eq!(data.power, 6);
    assert_eq!(data.defense, 3);
    assert_eq!(data.card_class, CardClass::Brute);
    assert!(data.keyword.is_empty(), "Pack Call should carry no keywords");
    // Its whole text is the defend trigger — nothing happens when it is played.
    assert!(matches!(data.defend_effect, Some(DefendEffect::Reveal6BottomOtherwise)));
    assert!(data.play_effects.is_empty(), "Pack Call should have no on-play effect");
    assert!(data.additional_cost.is_none(), "Pack Call should have no additional cost");
    assert!(data.ability.is_none(), "Pack Call should have no activated ability");
}

#[test]
fn pack_call_trigger_waits_on_the_stack_instead_of_firing_on_declaration() {
    let mut gs = rhinar_defending_muscle_mutt();
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let before = deck_order(&gs, PlayerIndex::P2);

    // Declaring the blocker alone changes nothing: the trigger is not applied
    // where the card is committed.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::PackCallY]);
    let pc = *hand_cards(&gs, PlayerIndex::P2).first().expect("Pack Call should be in hand");
    step_card(&mut gs, ActionType::Defend, pc);
    assert!(gs.stack_is_empty(), "nothing goes on the stack until blockers are all declared");
    assert_eq!(deck_order(&gs, PlayerIndex::P2), before);

    // Finishing the defend step opens the trigger window with the trigger on
    // the stack — still unresolved.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::DefendTriggers);
    assert_eq!(stacked_defend_triggers(&gs), vec![Card::PackCallY]);
    assert_eq!(gs.stack_top().map(|p| p.typ), Some(ActionType::DefendTrigger));
    assert_eq!(deck_order(&gs, PlayerIndex::P2), before,
        "the effect has not happened yet — the trigger is only on the stack");

    // It resolves like any other stack layer, on a round of passes.
    pass_once_each(&mut gs);
    assert!(gs.stack_is_empty());
    assert_eq!(*deck_order(&gs, PlayerIndex::P2).last().unwrap(), top,
        "resolving the trigger is what applies the effect");
}

#[test]
fn pack_call_trigger_resolves_leaving_the_card_on_the_combat_chain() {
    let mut gs = rhinar_defending_muscle_mutt();
    set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let pc = declare_blockers(&mut gs, &[Card::PackCallY])[0];

    // The card is on the chain from the moment it is declared, and resolving
    // its trigger neither moves it nor sends it to the graveyard.
    assert!(in_zone(&gs, PlayerIndex::P2, pc, Zone::CombatChain));
    pass_once_each(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, pc, Zone::CombatChain));
    assert_eq!(link_cards(&gs, PlayerIndex::P2, 0), vec![pc]);
}

#[test]
fn pack_call_keeps_a_power6_card_on_top() {
    let mut gs = rhinar_defending_muscle_mutt();
    // Wounded Bull sits exactly on the 6-power boundary, which is inclusive.
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::WoundedBullY);
    assert_eq!(Card::WoundedBullY.data().power, 6);
    let before = deck_order(&gs, PlayerIndex::P2);

    declare_blockers(&mut gs, &[Card::PackCallY]);
    pass_once_each(&mut gs);

    assert_eq!(deck_cards(&gs, PlayerIndex::P2).first().copied(), Some(top),
        "a 6-power card stays on top");
    assert_eq!(deck_order(&gs, PlayerIndex::P2), before,
        "the deck order is untouched when the top card has 6 or more power");
}

#[test]
fn pack_call_bottoms_a_sub_power6_card() {
    let mut gs = rhinar_defending_muscle_mutt();
    // Rally the Rearguard has 4 power — under the line, so it is bottomed.
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    assert_eq!(Card::RallyTheRearguardB.data().power, 4);
    let before = deck_order(&gs, PlayerIndex::P2);
    let size_before = gs.p2.deck_size();

    declare_blockers(&mut gs, &[Card::PackCallY]);
    pass_once_each(&mut gs);

    let after = deck_order(&gs, PlayerIndex::P2);
    assert_eq!(*after.last().expect("deck is non-empty"), top,
        "a sub-6-power card goes to the bottom");
    assert_eq!(deck_cards(&gs, PlayerIndex::P2).last().copied(), Some(top),
        "the bottom pointer follows it");
    assert_ne!(deck_cards(&gs, PlayerIndex::P2).first().copied(), Some(top));

    // The deck was reordered, not resized: the rest keeps its order, with the
    // old top moved from the front to the back.
    assert_eq!(gs.p2.deck_size(), size_before);
    assert_eq!(after.len(), before.len());
    let mut expected = before[1..].to_vec();
    expected.push(top);
    assert_eq!(after, expected);
}

#[test]
fn pack_call_reveals_its_own_controllers_deck() {
    let mut gs = rhinar_defending_muscle_mutt();
    // The defender (p2) is not the turn player, so wiring the reveal to the
    // wrong side would show up here.
    set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let attacker_deck_before = deck_order(&gs, PlayerIndex::P1);
    let defender_deck_before = deck_order(&gs, PlayerIndex::P2);

    declare_blockers(&mut gs, &[Card::PackCallY]);
    pass_once_each(&mut gs);

    assert_eq!(deck_order(&gs, PlayerIndex::P1), attacker_deck_before,
        "the attacker's deck must not be touched");
    assert_ne!(deck_order(&gs, PlayerIndex::P2), defender_deck_before,
        "the defender's own deck is the one revealed from");
}

#[test]
fn pack_call_reveal_is_public_whichever_way_it_goes() {
    // Kept on top: revealing it tells both players what is there.
    let mut gs = rhinar_defending_muscle_mutt();
    let kept = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::WoundedBullY);
    assert_eq!(gs.p2.deck_slot_visibility(0), Some(CardVisibleState::Hidden));
    declare_blockers(&mut gs, &[Card::PackCallY]);
    pass_once_each(&mut gs);
    assert_eq!(deck_cards(&gs, PlayerIndex::P2)[0], kept);
    assert_eq!(gs.p2.deck_slot_visibility(0), Some(CardVisibleState::BothKnow));

    // Bottomed: it stays known there, as a face-up pitched card does.
    let mut gs = rhinar_defending_muscle_mutt();
    let bottomed = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    declare_blockers(&mut gs, &[Card::PackCallY]);
    pass_once_each(&mut gs);
    let bottom = gs.p2.deck.len() - 1;
    assert_eq!(deck_cards(&gs, PlayerIndex::P2)[bottom], bottomed);
    assert_eq!(gs.p2.deck_slot_visibility(bottom), Some(CardVisibleState::BothKnow));
}

#[test]
fn pack_call_on_an_empty_deck_does_nothing() {
    let mut gs = rhinar_defending_muscle_mutt();
    // A decked player can still declare blockers, so an empty deck is a legal
    // state for the trigger to resolve in — it must no-op rather than panic.
    // Park the deck in the graveyard so no card goes missing.
    while let Some(slot) = gs.p2.deck.pop_front() {
        gs.p2.graveyard.add(slot.card());
    }

    let pc = declare_blockers(&mut gs, &[Card::PackCallY])[0];
    pass_once_each(&mut gs);

    assert_eq!(gs.p2.deck_size(), 0);
    assert!(gs.p2.deck.is_empty());
    // The block itself still happened.
    assert!(in_zone(&gs, PlayerIndex::P2, pc, Zone::CombatChain));
}

#[test]
fn a_blocker_without_a_defend_trigger_puts_nothing_on_the_stack() {
    let mut gs = rhinar_defending_muscle_mutt();
    // The same sub-6-power top card that Pack Call would bottom.
    set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let before = deck_order(&gs, PlayerIndex::P2);

    // Raging Onslaught has no defend effect, so an ordinary block costs no extra
    // priority round and leaves the deck alone.
    assert!(Card::RagingOnslaughtY.data().defend_effect.is_none());
    declare_blockers(&mut gs, &[Card::RagingOnslaughtY]);

    assert!(gs.stack_is_empty(), "an ordinary blocker puts no trigger on the stack");
    assert_eq!(deck_order(&gs, PlayerIndex::P2), before);
}

#[test]
fn defending_with_two_pack_calls_fires_the_effect_twice() {
    let mut gs = rhinar_defending_muscle_mutt();

    // Two different sub-6-power cards on top, so each Pack Call bottoms one in
    // turn and the two reveals are told apart by which card moved.
    let first_top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let second = gs.state.p2.deck.get_mut(1).expect("deck has two cards");
    second.set_card(gs.state.p2.deck_kind.id_of(Card::ClearingBellowB).unwrap());
    let second_top = Card::ClearingBellowB;
    let order = deck_order(&gs, PlayerIndex::P2);
    assert_eq!(order[..2], [first_top, second_top]);

    let blockers = declare_blockers(&mut gs, &[Card::PackCallY, Card::PackCallY]);
    assert_eq!(blockers.len(), 2);

    // Both triggers went on the stack together, as one batch: one per copy on
    // the chain link.
    assert_eq!(stacked_defend_triggers(&gs), vec![Card::PackCallY, Card::PackCallY]);
    assert_eq!(deck_order(&gs, PlayerIndex::P2), order, "neither has resolved yet");

    // Each resolves on its own round of passes — two reveals, not one.
    pass_once_each(&mut gs);
    assert_eq!(stacked_defend_triggers(&gs), vec![Card::PackCallY],
        "one trigger resolved, leaving the other's");
    let after_first = deck_order(&gs, PlayerIndex::P2);
    assert_eq!(*after_first.last().unwrap(), first_top,
        "one reveal has happened: the old top card is now at the bottom");
    assert_ne!(*after_first.last().unwrap(), second_top);

    pass_once_each(&mut gs);
    assert!(gs.stack_is_empty(), "both triggers have resolved");

    // The effect happened twice: the deck has rotated by two, both revealed
    // cards now at the bottom in declaration order.
    let after = deck_order(&gs, PlayerIndex::P2);
    assert_eq!(after[after.len() - 2], first_top);
    assert_eq!(after[after.len() - 1], second_top);
    let mut expected = order[2..].to_vec();
    expected.push(first_top);
    expected.push(second_top);
    assert_eq!(after, expected);
}

#[test]
fn only_instants_are_offered_while_a_defend_trigger_is_on_the_stack() {
    // Dorinthea attacks so that the attacker holds an instant and an attack
    // reaction from her own deck; Rhinar defends with Pack Call and Dodge from
    // his. (Rhinar's deck has no instant, so the defender's side is checked
    // with the defense reaction alone.)
    let mut gs = rhinar_defending_flock();

    // Pack Call blocks, leaving the defender a defense reaction.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::PackCallY, Card::DodgeB]);
    let hand = vec![Card::PackCallY, Card::DodgeB];
    step_card(&mut gs, ActionType::Defend, hand[0]);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    assert_eq!(stacked_defend_triggers(&gs), vec![Card::PackCallY]);

    // The attacker holds an instant and an attack reaction; only the instant is
    // offered while the trigger is pending.
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::SigilofSolaceB, Card::IronsongResponseR]);
    gs.p1.resources = 5; // affordability is not what rules the reaction out
    // Ironsong Response is untargeted ("your weapon attack"), so it is
    // playable into Flock on its own terms; only the trigger rules it out.
    assert_eq!(Card::IronsongResponseR.data().typ, CardType::AttackReaction);
    let attacker = playable_cards(&gs);
    assert!(attacker.contains(&Card::SigilofSolaceB),
        "an instant may be played above a pending defend trigger");
    assert!(!attacker.contains(&Card::IronsongResponseR),
        "an attack reaction may not");

    // Playing that instant does not reopen the window. The trigger is no longer
    // the *top* of the stack, but it is still on it, and the gate asks whether
    // one is pending anywhere rather than whether it is next to resolve.
    let sigil = expect_in(&gs, PlayerIndex::P1, Card::SigilofSolaceB, Zone::Hand,
        "Sigil of Solace should still be in hand");
    step_card(&mut gs, ActionType::PlayCard, sigil);
    assert!(on_stack(&gs, sigil));
    assert_eq!(gs.active_player, PlayerIndex::P1, "playing a card keeps priority");
    assert_ne!(gs.stack_top().map(|p| p.typ), Some(ActionType::DefendTrigger),
        "the instant is on top now, the trigger underneath");
    assert!(!playable_cards(&gs).contains(&Card::IronsongResponseR),
        "an attack reaction stays shut out while a trigger is anywhere on the stack");

    // Same for the defender, given priority over the instant: no defense
    // reaction while the trigger waits underneath.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.active_player, PlayerIndex::P2);
    gs.p2.resources = 5;
    assert!(!playable_cards(&gs).contains(&Card::DodgeB),
        "a defense reaction may not be played above a pending defend trigger");
}

#[test]
fn reactions_are_offered_again_when_no_defend_trigger_is_pending() {
    let mut gs = rhinar_defending_muscle_mutt();

    // An ordinary blocker puts no trigger on the stack, so the reaction window
    // is the usual one — the narrowing above is specific to a pending trigger.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::RagingOnslaughtY, Card::DodgeB]);
    let hand = vec![Card::RagingOnslaughtY, Card::DodgeB];
    step_card(&mut gs, ActionType::Defend, hand[0]);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    assert!(gs.stack_is_empty());

    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert!(playable_cards(&gs).contains(&Card::DodgeB),
        "with no trigger pending, a defense reaction is offered as before");
}

#[test]
fn an_instant_resolves_above_a_defend_trigger_without_closing_the_window() {
    // Dorinthea attacks with Flock (5) so the instant comes from her deck;
    // Rhinar blocks with Pack Call from his.
    let mut gs = rhinar_defending_flock();
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);

    // Pack Call blocks; its trigger is on the stack, not yet resolved.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::PackCallY]);
    let pc = *hand_cards(&gs, PlayerIndex::P2).first().expect("Pack Call should be in hand");
    step_card(&mut gs, ActionType::Defend, pc);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    assert_eq!(stacked_defend_triggers(&gs), vec![Card::PackCallY]);

    // The attacker, holding priority first, answers the trigger with a free
    // instant.
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::SigilofSolaceB]);
    let sigil = *hand_cards(&gs, PlayerIndex::P1).first().expect("Sigil should be in hand");
    let attacker_life_before = gs.p1.life;
    step_card(&mut gs, ActionType::PlayCard, sigil);
    assert!(on_stack(&gs, sigil));

    // Last on, first off: the instant resolves while the trigger waits below,
    // and the window stays open instead of closing into combat damage.
    pass_once_each(&mut gs);
    assert_eq!(gs.p1.life, attacker_life_before + 1, "Sigil of Solace resolved");
    assert!(in_zone(&gs, PlayerIndex::P1, sigil, Zone::Graveyard));
    assert_eq!(stacked_defend_triggers(&gs), vec![Card::PackCallY],
        "the trigger is still waiting underneath");
    assert_eq!(gs.phase, Phase::DefendTriggers, "the window must not close early");
    assert_ne!(*deck_order(&gs, PlayerIndex::P2).last().unwrap(), top,
        "the trigger has not resolved yet");
    assert_eq!(gs.p2.life, 20);

    // Then the trigger resolves and its window hands over to the reaction
    // window — still no damage.
    pass_once_each(&mut gs);
    assert!(gs.stack_is_empty());
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(*deck_order(&gs, PlayerIndex::P2).last().unwrap(), top);
    assert_eq!(gs.p2.life, 20, "damage waits for the reaction window");

    // Closing that window deals it: Pack Call's 3 block against Flock's 5.
    pass_once_each(&mut gs);
    assert_eq!(gs.p2.life, 20 - 2);
}

#[test]
fn pack_call_still_blocks_for_3() {
    let mut gs = rhinar_defending_muscle_mutt();
    set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    let pc = declare_blockers(&mut gs, &[Card::PackCallY])[0];
    assert!(in_zone(&gs, PlayerIndex::P2, pc, Zone::CombatChain));

    // Resolving the last trigger empties the stack, which hands the
    // defend-trigger window over to the ordinary reaction window — not to
    // damage.
    pass_once_each(&mut gs);
    assert!(gs.stack_is_empty());
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.p2.life, 20, "no damage until the reaction window closes");

    // Closing that window deals it. Its 3 block comes off Muscle Mutt's 6
    // power: 3 damage, not 6 — the trigger is a bonus on top of an ordinary
    // block, not a replacement for it.
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.p2.life, 20 - 3);
}

#[test]
fn the_defend_trigger_window_hands_over_to_the_reaction_window_not_to_damage() {
    let mut gs = rhinar_defending_muscle_mutt();
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);

    // Pack Call blocks, and the defender keeps a defense reaction in hand — one
    // the trigger window will not let her play.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::PackCallY, Card::DodgeB]);
    let hand = vec![Card::PackCallY, Card::DodgeB];
    let dodge = hand[1];
    step_card(&mut gs, ActionType::Defend, hand[0]);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    assert_eq!(gs.phase, Phase::DefendTriggers);

    // The trigger resolves. Its window then hands over to the ordinary reaction
    // window rather than dealing combat damage.
    pass_once_each(&mut gs);
    assert!(gs.stack_is_empty());
    assert_eq!(gs.phase, Phase::Reaction, "the reaction window comes next");
    assert_eq!(*deck_order(&gs, PlayerIndex::P2).last().unwrap(), top,
        "the trigger did resolve");
    assert_eq!(gs.p2.life, 20, "damage waits for the reaction window to close");
    assert_eq!(gs.active_player, gs.turn_player, "which opens on the turn player");

    // Reactions are legal again here, so blocking with Pack Call no longer costs
    // the defender her reaction window.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert!(playable_cards(&gs).contains(&Card::DodgeB),
        "the defense reaction the trigger window shut out is playable again");

    // She plays it, and it blocks as usual: Pack Call's 3 plus Dodge's 2 against
    // Muscle Mutt's 6 leaves 1.
    step_card(&mut gs, ActionType::PlayCard, dodge);
    pass_once_each(&mut gs);
    close_reaction_step(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, dodge, Zone::CombatChain));
    assert_eq!(gs.p2.life, 20 - 1);
}

#[test]
fn an_ordinary_block_skips_the_defend_trigger_window_entirely() {
    let mut gs = dorinthea_defending_muscle_mutt();

    // No blocker carries a trigger, so there is nothing to open a trigger window
    // for: the defend step goes straight to the reaction window, and an ordinary
    // block costs no extra round of priority.
    declare_blockers(&mut gs, &[Card::DrivingBladeY]);
    assert_eq!(gs.phase, Phase::Reaction);
    assert!(gs.stack_is_empty());

    // One round of passes closes it into damage: 6 power less 3 block.
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.p2.life, 20 - 3);
}

#[test]
fn pitching_inside_the_defend_trigger_window_returns_to_it() {
    // Nothing playable in this window currently costs anything — Sigil of
    // Solace is the catalog's only instant and is free, as is its only
    // instant-speed ability — so the legal-action gate cannot produce this
    // state yet. A costed card is driven through `step` directly to pin the
    // phase plumbing for the first costed instant that comes along: without
    // `DefendPitch`, pitching here would fall through to `ActionPitch`
    // and return to the wrong window.
    let mut gs = rhinar_defending_muscle_mutt();
    set_hand(&mut gs, PlayerIndex::P2,
        &[Card::PackCallY, Card::MuscleMuttY, Card::ClearingBellowB]);
    let hand = vec![Card::PackCallY, Card::MuscleMuttY, Card::ClearingBellowB];
    let (costed, pitcher) = (hand[1], hand[2]);

    step_card(&mut gs, ActionType::Defend, hand[0]);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    assert_eq!(gs.phase, Phase::DefendTriggers);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // priority to the defender
    assert_eq!(gs.active_player, PlayerIndex::P2);

    assert_eq!(Card::MuscleMuttY.data().cost, 3);
    assert_eq!(Card::ClearingBellowB.data().pitch, 3);
    assert_eq!(gs.p2.resources, 0, "the cost is not already covered");

    step_card(&mut gs, ActionType::PlayCard, costed);
    assert_eq!(gs.phase, Phase::DefendPitch,
        "pitching drops into this window's own pitch phase");

    step_card(&mut gs, ActionType::Pitch, pitcher);
    assert_eq!(gs.phase, Phase::DefendTriggers,
        "and returns to the window it interrupted, not to the reaction window");
    assert!(on_stack(&gs, costed));
    assert_eq!(stacked_defend_triggers(&gs), vec![Card::PackCallY],
        "the trigger is still underneath, waiting");
}


// ─────────────────────────────────────────────────────────────────────────────
// Barraging Beatdown (Card::BarragingBeatdownY)
//
// "Intimidate, then your next Brute attack this turn gains 'While this attack
// is defended by less than 2 non-equipment cards it has +3 power'. Go again."
// A 0-cost yellow brute action. Intimidate and Go Again are keywords the engine
// applies generically as it resolves — like Clearing Bellow, it is not an
// attack, so it Intimidates on its way to the graveyard and spares the action
// point. The middle clause is the on-play effect: +3 banked on the player as
// `next_brute_attack_conditional_bonus` and paid out at combat damage only if
// the attack it rides ends up defended by fewer than two non-equipment cards.
//
// It differs from Awakening Bellow's unconditional brute +3 in two ways, and
// both are pinned below: the payout is gated on the defender's blocker count,
// and "your next Brute attack" is read to cover a brute *weapon* swing as well
// as a brute attack action card, so Bone Basher takes it where Awakening
// Bellow's bonus would sit and wait.
// ─────────────────────────────────────────────────────────────────────────────

/// Bank Barraging Beatdown's conditional +3 for p1 by resolving its on-play
/// effect directly.
fn bank_barraging_beatdown_bonus() -> TestGame {
    let mut gs = setup_rhinar_action_phase();
    let effect = Card::BarragingBeatdownY
        .data()
        .play_effects
        .first()
        .expect("Barraging Beatdown should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, effect);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 3);
    gs
}

/// Seat `card` on `pid`'s combat chain as a blocker against the attack on link
/// 0. Mechanically identical to seating an attacker — blockers share the
/// attack's link — so it delegates; the name is here to keep the tests below
/// readable about which side of the chain each card is on.
fn place_blocker_on_chain(gs: &mut Gamestate, pid: PlayerIndex, card: Card) -> Card {
    place_attacker_on_chain(gs, pid, card)
}

#[test]
fn barraging_beatdown_is_a_0_cost_brute_action_with_go_again_and_intimidate() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    use crate::cards::Keyword;
    let data = Card::BarragingBeatdownY.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!(data.card_class, CardClass::Brute);
    assert_eq!(data.cost, 0);
    assert_eq!(data.pitch, 2);
    // Not an attack: it has no power of its own, and blocks for 3.
    assert_eq!(data.power, 0);
    assert_eq!(data.defense, 3);
    // "Intimidate" and "Go again" are the keyword halves of the text.
    assert!(data.keyword.contains(Keyword::GoAgain));
    assert!(data.keyword.contains(Keyword::Intimidate));

    // The middle clause is the on-play effect, and it is unconditional at play
    // time — the "less than 2 non-equipment cards" test belongs to the attack it
    // hands the ability to, not to playing this card.
    let effect = data.play_effects.first().expect("Barraging Beatdown should carry an on-play effect");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::NextBruteConditionalPower));
    assert_eq!(effect.magnitude, 3);
}

#[test]
fn barraging_beatdown_banks_a_conditional_plus3_for_the_next_brute_attack() {
    let gs = bank_barraging_beatdown_bonus();
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 3);
    assert_eq!(gs.p2.next_brute_attack_conditional_bonus, 0);
    // The unconditional pots are separate and stay untouched.
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 0);
    assert_eq!(gs.p1.next_attack_action_bonus, 0);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn barraging_beatdown_undefended_brute_attack_gets_plus3() {
    let mut gs = bank_barraging_beatdown_bonus();

    // Bare Fangs (brute, 6 power) against a defender who declares nothing: zero
    // non-equipment cards is fewer than two, so the bonus pays out.
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 6 + 3 = 9 damage, and the bonus is spent.
    assert_eq!(gs.p2.life, life_before - 9);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 0);
}

#[test]
fn barraging_beatdown_bonus_survives_a_single_blocker() {
    let mut gs = bank_barraging_beatdown_bonus();

    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    // One non-equipment card defends (On a Knife Edge, 2 defense). One is still
    // "less than 2", so the bonus applies.
    place_blocker_on_chain(&mut gs, PlayerIndex::P2, Card::OnAKnifeEdgeY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // (6 + 3) - 2 = 7 damage.
    assert_eq!(gs.p2.life, life_before - 7);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 0);
}

#[test]
fn barraging_beatdown_bonus_is_nullified_by_two_blockers() {
    let mut gs = bank_barraging_beatdown_bonus();

    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    // Two non-equipment cards defend (2 defense each), so the ability's own
    // condition fails and the attack hits for its printed power alone.
    place_blocker_on_chain(&mut gs, PlayerIndex::P2, Card::OnAKnifeEdgeY);
    place_blocker_on_chain(&mut gs, PlayerIndex::P2, Card::OnAKnifeEdgeY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 6 - 4 = 2 damage; with the bonus it would have been 5.
    assert_eq!(gs.p2.life, life_before - 2);
    // Nullified is not unspent: the attack it was banked for has happened, so
    // the bonus is gone rather than saved for a later brute attack this turn.
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 0);
}

#[test]
fn barraging_beatdown_does_not_count_equipment_toward_the_two() {
    let mut gs = bank_barraging_beatdown_bonus();

    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    // Three cards defend, but two of them are equipment (1 defense each) and the
    // text counts only non-equipment cards — so the count is 1 and the bonus
    // still pays out.
    // The equipment is Dorinthea's own Ironrot Helm and Legs, moved from her
    // head and legs slots onto the chain as a block would put them there.
    place_blocker_on_chain(&mut gs, PlayerIndex::P2, Card::OnAKnifeEdgeY);
    let helm = expect_in(&gs, PlayerIndex::P2, Card::IronrotHelm, Zone::Equipment, "Dorinthea should wear a helm");
    let legs = expect_in(&gs, PlayerIndex::P2, Card::IronrotLegs, Zone::Equipment, "Dorinthea should wear legs");
    move_to_chain(&mut gs, PlayerIndex::P2, Zone::Equipment, helm, 0);
    move_to_chain(&mut gs, PlayerIndex::P2, Zone::Equipment, legs, 0);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // (6 + 3) - (2 + 1 + 1) = 5 damage. Equipment adds its defense like any
    // other blocker; it just doesn't count against the ability.
    assert_eq!(gs.p2.life, life_before - 5);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 0);
}

#[test]
fn barraging_beatdown_counts_a_defense_reaction_toward_the_two() {
    let mut gs = bank_barraging_beatdown_bonus();

    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    // A defense reaction joins the chain link the attack sits on when it
    // resolves (see `commit_defense_reaction`), and it is not equipment — so a
    // declared blocker plus a Toughen Up is two non-equipment cards and
    // nullifies the bonus just as two declared blockers would.
    place_blocker_on_chain(&mut gs, PlayerIndex::P2, Card::OnAKnifeEdgeY);
    place_blocker_on_chain(&mut gs, PlayerIndex::P2, Card::ToughenUpB);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 6 - (2 + 4) = 0 damage; with the bonus it would have been 3.
    assert_eq!(gs.p2.life, life_before);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 0);
}

#[test]
fn barraging_beatdown_bonus_applies_to_a_brute_weapon_swing() {
    let mut gs = bank_barraging_beatdown_bonus();

    // Bone Basher is a Brute *weapon*, not an attack action card. "Your next
    // Brute attack" names the attack rather than the card type it is made with,
    // so a swing takes the bonus — where Awakening Bellow's and Come to Fight's
    // bonuses would both sit and wait.
    let attacker = place_weapon_on_chain(&mut gs, PlayerIndex::P1);
    assert_eq!(attacker, Card::BoneBasher);
    assert_eq!(attacker.data().typ, CardType::Weapon);
    assert_eq!(attacker.data().card_class, CardClass::Brute);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 4 + 3 = 7 damage, and the bonus is spent.
    assert_eq!(gs.p2.life, life_before - 7);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 0);
}

#[test]
fn barraging_beatdown_bonus_is_left_banked_by_a_non_brute_attack() {
    let mut gs = bank_barraging_beatdown_bonus();

    // Muscle Mutt is generic, so it is not the "next Brute attack" the ability
    // was handed to: it neither takes the bonus nor spends it.
    let attacker = place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
    assert_eq!(attacker.data().card_class, CardClass::Generic);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 6 damage, and the bonus stays banked for a later brute attack this turn.
    assert_eq!(gs.p2.life, life_before - 6);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 3);
}

#[test]
fn barraging_beatdown_stacks_with_awakening_bellows_unconditional_bonus() {
    let mut gs = bank_barraging_beatdown_bonus();

    // The two brute pumps are separate pots, so a brute attack collects both.
    let ab = Card::AwakeningBellowR
        .data()
        .play_effects
        .first()
        .expect("Awakening Bellow should carry an on-play effect");
    apply_on_play_effect(&mut gs.state, &mut gs.rng, PlayerIndex::P1, ab);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 3);

    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::BareFangsR);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);

    // 6 + 3 + 3 = 12 damage, and both are spent.
    assert_eq!(gs.p2.life, life_before - 12);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 0);
    assert_eq!(gs.p1.next_brute_attack_action_bonus, 0);
}

#[test]
fn barraging_beatdown_bonus_does_not_survive_the_turn() {
    let mut gs = bank_barraging_beatdown_bonus();

    // "This turn": an unspent bonus is cleared at the turn boundary rather than
    // leaking into a later turn's attacks.
    begin_turn(&mut gs);
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 0);
    assert_eq!(gs.p2.next_brute_attack_conditional_bonus, 0);
}

#[test]
fn barraging_beatdown_played_from_hand_intimidates_banks_and_keeps_the_action_point() {
    let (mut gs, idx) = play_and_resolve_zero_cost_action(Card::BarragingBeatdownY);

    // Not an attack, so it resolves to the graveyard rather than the chain, and
    // its Intimidate fires on the way there.
    assert!(in_zone(&gs, PlayerIndex::P1, idx, Zone::Graveyard));
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
    assert!(gs.p1.has_intimidated());

    // The middle clause banked its +3 for the brute attack still to come.
    assert_eq!(gs.p1.next_brute_attack_conditional_bonus, 3);

    // Go Again: the action point survives, so that attack can follow.
    assert_eq!(gs.p1.action_points, 1);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);

    // Smash Instinct (brute, 6 power) then lands unblocked for 6 + 3 = 9.
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::SmashInstinctY);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);
    assert_eq!(gs.p2.life, life_before - 9);
}


// ─────────────────────────────────────────────────────────────────────────────
// Rally the Rearguard (Card::RallyTheRearguardB)
//
// "Once per turn Instant — Discard a card: Rally the Rearguard gains +3 block.
// Activate this ability only while Rally the Rearguard is defending."
// A 2-cost blue generic attack action (4 power, 2 block) whose text is entirely
// an activated ability — the catalog's first ability that is neither an
// equipment piece's nor a weapon swing, and the first whose effect the engine
// actually applies.
//
// What makes it work, beyond `Ability::DiscardCardPlusBlock` itself:
//   * the card is activated from the combat chain, a zone
//     `push_equipment_activations` does not scan, so the new
//     `push_defending_activations` finds it there;
//   * "while defending" is read as the defend-trigger and reaction windows, the
//     two where a declared blocker is answering a live attack — blockers stay on
//     the chain until the action phase ends, so a phase gate is what keeps the
//     ability off a blocker whose combat is already over;
//   * the activation goes on the stack while the card stays on the chain, still
//     blocking — the same shape as a Pack Call defend trigger;
//   * "+3 block" lands on the chain link it defends
//     (`Player::chain_defense_bonus`), and each copy has its own once-per-turn
//     activation;
//   * the discard is a cost, paid as the activation goes on the stack, and an
//     empty hand means the ability is not offered at all.
// ─────────────────────────────────────────────────────────────────────────────

/// The cards whose ability the active player is offered as an `Activate`.
fn activatable_cards(gs: &Gamestate) -> HashSet<Card> {
    legal_actions(gs).iter()
        .filter(|a| a.typ == ActionType::Activate)
        .map(|a| card_of(&gs, a))
        .collect()
}

/// Rhinar (p1) attacks with Muscle Mutt and a mirror Rhinar (p2) blocks with
/// Rally the Rearguard, leaving her holding `spare` as the card her ability can
/// discard. Priority has passed to her in the reaction window, so the
/// activation is the next thing she may do. Returns the game and Rally.
fn rally_blocking_muscle_mutt(spare: &[Card]) -> (TestGame, Card) {
    rally_blocking(rhinar_defending_muscle_mutt(), spare)
}

/// Block the attack in `gs` (Defend phase, p2 defending with a Rhinar deck)
/// with Rally the Rearguard, keeping `spare` in hand, and pass priority to the
/// defender in the reaction window. Returns the game and Rally.
fn rally_blocking(mut gs: TestGame, spare: &[Card]) -> (TestGame, Card) {
    let mut hand = vec![Card::RallyTheRearguardB];
    hand.extend_from_slice(spare);
    set_hand(&mut gs, PlayerIndex::P2, &hand);
    let rally = expect_in(&gs, PlayerIndex::P2, Card::RallyTheRearguardB, Zone::Hand, "Rally should be in hand");

    // Declare it as an ordinary blocker: it is an attack action card with 2
    // printed defense, not a defense reaction, so it blocks in the defend step.
    step_card(&mut gs, ActionType::Defend, rally);
    assert!(in_zone(&gs, PlayerIndex::P2, rally, Zone::CombatChain));

    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    assert_eq!(gs.phase, Phase::Reaction, "Rally has no defend trigger of its own");
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    assert_eq!(gs.active_player, PlayerIndex::P2);

    (gs, rally)
}

#[test]
fn rally_the_rearguard_is_a_4_power_attack_action_with_a_defending_ability() {
    use crate::cards::Keyword;
    let data = Card::RallyTheRearguardB.data();
    assert_eq!(data.typ, CardType::AttackAction);
    assert_eq!(data.card_class, CardClass::Generic);
    assert_eq!(data.cost, 2);
    assert_eq!(data.pitch, 3);
    assert_eq!(data.power, 4);
    assert_eq!(data.defense, 2);
    // The whole text is the ability: no keywords, no on-play or defend effect.
    assert!(data.keyword.is_empty());
    assert!(data.play_effects.is_empty());
    assert!(data.defend_effect.is_none());

    let ability = data.ability.as_ref().expect("Rally should carry an activated ability");
    // "Instant", and free of resources — the cost is the discard.
    assert_eq!(ability.card_type(), CardType::Instant);
    assert_eq!(ability.resource_cost(), 0);
    assert!(ability.discards_a_card());
    assert!(ability.only_while_defending());
    assert!(ability.once_per_turn());
}

#[test]
fn rally_ability_is_not_offered_while_it_sits_in_hand() {
    let mut gs = setup_rhinar_action_phase();

    // "Only while defending": in hand during its controller's own action phase
    // it is a card to play, never an ability to activate.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::RallyTheRearguardB, Card::BareFangsR]);
    assert!(!activatable_cards(&gs).contains(&Card::RallyTheRearguardB));
}

#[test]
fn rally_ability_is_not_offered_to_the_attacker_swinging_it() {
    // Attacking with Rally puts it on the *attacker's* chain. It is not
    // defending there, and the attacker is the turn player, so the ability is
    // not offered even though the card sits on a combat chain.
    let (gs, rally) = play_and_resolve_attack(Card::RallyTheRearguardB);
    assert!(in_zone(&gs, PlayerIndex::P1, rally, Zone::CombatChain));

    // The defender is active in the Defend phase; step out to the reaction
    // window so the attacker gets priority with his attack on the chain.
    let mut gs = gs;
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    assert!(!activatable_cards(&gs).contains(&Card::RallyTheRearguardB));
}

#[test]
fn rally_ability_is_offered_to_the_defender_once_it_blocks() {
    let (gs, _) = rally_blocking_muscle_mutt(&[Card::DodgeB]);
    assert!(activatable_cards(&gs).contains(&Card::RallyTheRearguardB),
        "a blocking Rally with a card to discard should offer its ability");
}

#[test]
fn rally_ability_is_not_offered_without_a_card_to_discard() {
    // Rally alone: once it is on the chain the hand is empty, so the discard
    // cost cannot be paid and the ability is not offered.
    let (gs, _) = rally_blocking_muscle_mutt(&[]);
    assert!(gs.p2.hand.is_empty());
    assert!(!activatable_cards(&gs).contains(&Card::RallyTheRearguardB));
}

#[test]
fn rally_ability_is_not_offered_once_its_combat_is_over() {
    // Dorinthea's turn, so the turn player has an instant to open a window with
    // and the Rhinar opposite her holds Rally and Dodge from his own deck.
    let mut gs = setup_dorinthea_action_phase();

    // Seat Rally on the *defender's* chain, as a blocker whose attack has
    // already resolved: blockers stay on the chain until the action phase ends,
    // so it is still sitting there while the turn player carries on.
    place_attacker_on_chain(&mut gs, PlayerIndex::P2, Card::RallyTheRearguardB);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::DodgeB]);

    // Open an instant window in the turn player's action phase and pass priority
    // to p2. Everything but the phase now says yes — p2 is not the turn player,
    // Rally is on their chain, the ability is an instant, it is unused, and
    // there is a card to discard — so this is what the phase gate alone rules
    // out: the combat that card was defending is over.
    set_hand(&mut gs, PlayerIndex::P1, &[Card::SigilofSolaceB]);
    let sigil = *hand_cards(&gs, PlayerIndex::P1).first().expect("Sigil should be in hand");
    step_card(&mut gs, ActionType::PlayCard, sigil);
    assert_eq!(gs.phase, Phase::ActionInstant);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert_ne!(gs.active_player, gs.turn_player);

    assert!(!activatable_cards(&gs).contains(&Card::RallyTheRearguardB),
        "a blocker whose combat has ended is no longer defending");
}

#[test]
fn rally_ability_discards_a_card_and_leaves_rally_on_the_chain() {
    let (mut gs, rally) = rally_blocking_muscle_mutt(&[Card::DodgeB]);
    let spare = *hand_cards(&gs, PlayerIndex::P2).first().expect("the spare card should be in hand");

    step_card(&mut gs, ActionType::Activate, rally);

    // The cost is paid as the activation goes on the stack: the hand's only
    // other card is discarded, and the ability is marked used.
    assert!(in_zone(&gs, PlayerIndex::P2, spare, Zone::Graveyard));
    assert!(gs.p2.hand.is_empty());
    assert!(uses_this_turn(&gs, rally) > 0);

    // Rally itself has not moved: it is still on the chain, still blocking,
    // while the activation waits on the stack to be responded to.
    assert!(in_zone(&gs, PlayerIndex::P2, rally, Zone::CombatChain));
    assert_eq!(link_cards(&gs, PlayerIndex::P2, 0), vec![rally]);
    assert!(!gs.stack_is_empty());
    assert_eq!(gs.phase, Phase::Reaction, "a free activation needs no pitching");
}

#[test]
fn rally_ability_adds_3_block_when_it_resolves() {
    let (mut gs, rally) = rally_blocking_muscle_mutt(&[Card::DodgeB]);
    assert_eq!(block_bonus(&gs, rally), 0);

    step_card(&mut gs, ActionType::Activate, rally);
    // Still nothing until it resolves — it is on the stack, not yet applied.
    assert_eq!(block_bonus(&gs, rally), 0);

    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(block_bonus(&gs, rally), 3);
}

#[test]
fn rally_ability_blocks_3_more_of_the_attack() {
    let (mut gs, rally) = rally_blocking_muscle_mutt(&[Card::DodgeB]);

    // Both pass: the activation resolves. Both pass again on the empty stack:
    // the step closes and combat damage is dealt.
    step_card(&mut gs, ActionType::Activate, rally);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    close_reaction_step(&mut gs);

    // Muscle Mutt's 6 power against 2 printed + 3 granted = 5 block: 1 damage,
    // where the unpumped block would have let 4 through.
    assert_eq!(gs.p2.life, 20 - 1);
}

#[test]
fn rally_without_its_ability_blocks_only_its_printed_2() {
    // The same combat with the ability never activated, to pin that the 3 above
    // is the ability's doing rather than the block's.
    let (mut gs, _) = rally_blocking_muscle_mutt(&[Card::DodgeB]);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    assert_eq!(gs.p2.life, 20 - 4);
}

#[test]
fn rally_ability_is_once_per_turn() {
    // Two spare cards, so the discard cost can be paid twice over and only the
    // once-per-turn limit stands between her and a second +3.
    let (mut gs, rally) = rally_blocking_muscle_mutt(&[Card::DodgeB, Card::ClearingBellowB]);
    assert!(activatable_cards(&gs).contains(&Card::RallyTheRearguardB));

    // Activating keeps priority with the activator, so the second offer can be
    // checked in the same window the first was taken in.
    step_card(&mut gs, ActionType::Activate, rally);
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert!(!gs.p2.hand.is_empty(), "a card is still in hand to discard");
    assert!(!activatable_cards(&gs).contains(&Card::RallyTheRearguardB),
        "the ability is spent for the turn as soon as it is activated");

    // It stays spent once the activation has resolved, rather than the limit
    // being only about the entry sitting on the stack.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(block_bonus(&gs, rally), 3);
    assert!(uses_this_turn(&gs, rally) > 0);
}

#[test]
fn rally_ability_can_be_responded_to_before_it_resolves() {
    // Dorinthea attacks so the responding attacker has an instant from her own
    // deck; Rhinar blocks with Rally from his.
    let (mut gs, rally) = rally_blocking(rhinar_defending_flock(), &[Card::DodgeB]);
    step_card(&mut gs, ActionType::Activate, rally);

    // An instant-speed activation sits on the stack rather than applying
    // immediately, so it can be responded to. The activator keeps priority
    // first; passing hands it to the attacker with the +3 still pending.
    assert!(!gs.stack_is_empty());
    assert_eq!(gs.active_player, PlayerIndex::P2);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.active_player, PlayerIndex::P1);
    assert!(!gs.stack_is_empty(), "passing priority must not resolve the activation");
    set_hand(&mut gs, PlayerIndex::P1, &[Card::SigilofSolaceB]);
    let sigil = *hand_cards(&gs, PlayerIndex::P1).first().expect("Sigil should be in hand");
    let life_before = gs.p1.life;

    // The instant resolves above the activation: p1 gains life while Rally's +3
    // is still pending underneath.
    step_card(&mut gs, ActionType::PlayCard, sigil);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.p1.life, life_before + 1);
    assert_eq!(block_bonus(&gs, rally), 0, "the activation is still on the stack");

    // Passing it out then resolves the activation underneath.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(block_bonus(&gs, rally), 3);
}

#[test]
fn rally_ability_costs_no_action_point() {
    let (mut gs, rally) = rally_blocking_muscle_mutt(&[Card::DodgeB]);
    // The action point belongs to the turn player, and an instant-speed ability
    // activated by the defender must not touch it.
    let attacker_points = gs.p1.action_points;
    let defender_points = gs.p2.action_points;

    // Activating spends nothing: an instant-speed ability costs no action point,
    // and the defender has no action phase to spend one in anyway.
    step_card(&mut gs, ActionType::Activate, rally);
    assert_eq!(gs.p1.action_points, attacker_points);
    assert_eq!(gs.p2.action_points, defender_points);

    // Nor does resolving it. The attacker's point does go, but to the attack:
    // combat damage spends it as the window closes, exactly as it would have
    // without the activation (see `rally_without_its_ability_blocks_only_its_printed_2`).
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.p1.action_points, attacker_points);
    assert_eq!(gs.p2.action_points, defender_points);
}

#[test]
fn rally_block_bonus_is_cleared_when_the_chain_closes() {
    let (mut gs, rally) = rally_blocking_muscle_mutt(&[Card::DodgeB]);
    // The activation resolves, then the step closes and combat damage is
    // dealt — Rally is still on the chain, pumped.
    step_card(&mut gs, ActionType::Activate, rally);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    close_reaction_step(&mut gs);
    assert_eq!(block_bonus(&gs, rally), 3);
    assert!(in_zone(&gs, PlayerIndex::P2, rally, Zone::CombatChain));

    // The attacker then passes out of the action phase, closing the chain:
    // Rally leaves for the graveyard and the granted block goes with it.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert!(in_zone(&gs, PlayerIndex::P2, rally, Zone::Graveyard));
    assert_eq!(block_bonus(&gs, rally), 0);
}

#[test]
fn rally_once_per_turn_limit_lifts_at_the_turn_boundary() {
    let (mut gs, rally) = rally_blocking_muscle_mutt(&[Card::DodgeB]);
    step_card(&mut gs, ActionType::Activate, rally);
    assert!(uses_this_turn(&gs, rally) > 0);

    begin_turn(&mut gs);
    assert!(uses_this_turn(&gs, rally) == 0);
}

#[test]
fn two_blocking_rallies_each_have_their_own_activation() {
    let mut gs = rhinar_defending_muscle_mutt();

    // Both copies block, leaving one card in hand — enough to pay for exactly
    // one activation.
    set_hand(&mut gs, PlayerIndex::P2,
        &[Card::RallyTheRearguardB, Card::RallyTheRearguardB, Card::DodgeB]);
    let hand = vec![Card::RallyTheRearguardB, Card::RallyTheRearguardB, Card::DodgeB];
    let (first, second) = (hand[0], hand[1]);
    step_card(&mut gs, ActionType::Defend, first);
    step_card(&mut gs, ActionType::Defend, second);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.active_player, PlayerIndex::P2);

    // The ability is offered — once, since the two copies are the same choice.
    let offered: Vec<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::Activate)
        .map(|a| card_of(&gs, a))
        .collect();
    assert_eq!(offered, vec![Card::RallyTheRearguardB]);

    // Activating uses one copy's once-per-turn ability and pumps the link by 3.
    // The other copy's ability is still unused ("once per turn" is per copy),
    // but the hand that paid for the first is now empty.
    step_card(&mut gs, ActionType::Activate, first);
    assert_eq!(uses_this_turn(&gs, first), 1);
    assert_eq!(count_in(&gs, PlayerIndex::P2, second, Zone::CombatChain), 2);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(block_bonus(&gs, first), 3);
    assert_eq!(gs.p2.chain_defense_bonus[0], 3);

    // 2 + 2 printed plus the one granted 3 = 7 block against 6 power: nothing
    // gets through. (Damage lands as the window closes, in the pass above.)
    assert_eq!(gs.p2.life, 20);
}


// ─────────────────────────────────────────────────────────────────────────────
// Rhinar's Intimidate is action-speed, so an off-turn discard does not fire it
//
// Rhinar's hero ability is "whenever you discard a card with 6 or more power,
// Intimidate" — every qualifying discard, with no once-per-turn limit. It is
// action-speed, though, so it only triggers during its controller's own action
// phase, and only the turn player has one. Every discard path the engine had
// until now was taken on your own turn (a "discard a card" additional cost, or a
// draw-then-discard play effect), so the distinction never came up. Rally the
// Rearguard's ability is the first discard made while *defending* — always the
// opponent's turn — and it must not intimidate.
//
// The gate lives in `maybe_discard6_intimidate` rather than at its call sites,
// so it holds for any future off-turn discard too.
// ─────────────────────────────────────────────────────────────────────────────

/// Put a 6-power card in `pid`'s hand as its only card, returning its index.
/// Discarding from this hand is therefore deterministic and over the threshold.
fn set_hand_to_one_power6_card(gs: &mut Gamestate, pid: PlayerIndex) -> Card {
    set_hand(gs, pid, &[Card::SmashInstinctY]);
    assert!(Card::SmashInstinctY.data().power >= 6);
    Card::SmashInstinctY
}

#[test]
fn power6_discard_on_your_own_turn_still_intimidates() {
    // The baseline the rule is carved out of: Rhinar is the turn player, so his
    // action-speed ability triggers exactly as it did before.
    let mut gs = setup_rhinar_action_phase();
    assert_eq!(gs.turn_player, PlayerIndex::P1);
    let pick = set_hand_to_one_power6_card(&mut gs, PlayerIndex::P1);

    apply_discard_cost(&mut gs.state, &mut gs.rng, PlayerIndex::P1);

    assert!(in_zone(&gs, PlayerIndex::P1, pick, Zone::Graveyard));
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
    assert!(gs.p1.has_intimidated());
}

#[test]
fn every_power6_discard_intimidates_there_is_no_once_per_turn_limit() {
    // "Whenever you discard": the trigger has no per-turn cap, so a second
    // qualifying discard in the same action phase intimidates again.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::SmashInstinctY, Card::PackHuntR]);
    assert!(hand_cards(&gs, PlayerIndex::P1).iter().all(|c| c.data().power >= 6));
    // The opponent needs cards to banish for both triggers to be observable.
    assert!(gs.p2.hand_size() >= 2);

    apply_discard_cost(&mut gs.state, &mut gs.rng, PlayerIndex::P1);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);

    apply_discard_cost(&mut gs.state, &mut gs.rng, PlayerIndex::P1);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 2,
        "the second 6-power discard intimidates too — the ability is not once per turn");
}

#[test]
fn power6_discard_on_the_opponents_turn_does_not_intimidate() {
    let mut gs = setup_rhinar_action_phase();
    // Hand the turn to Dorinthea: Rhinar now has no action phase, so his
    // action-speed ability cannot trigger.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.turn_player, PlayerIndex::P2);

    let pick = set_hand_to_one_power6_card(&mut gs, PlayerIndex::P1);
    apply_discard_cost(&mut gs.state, &mut gs.rng, PlayerIndex::P1);

    // The card is discarded and it is over the threshold — only the timing
    // stops it.
    assert!(in_zone(&gs, PlayerIndex::P1, pick, Zone::Graveyard));
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
    assert!(!gs.p1.has_intimidated());
}

/// In a Rhinar mirror, p2 attacks with Muscle Mutt and p1 blocks with Rally the
/// Rearguard, holding `spare` as the card his ability can discard. The seat swap
/// of `rally_blocking_muscle_mutt`, with p1 on defense so the hero ability being
/// tested is the defender's. Priority has passed to him in the reaction window.
/// Returns the game and Rally.
fn rhinar_blocking_with_rally(spare: &[Card]) -> (TestGame, Card) {
    let mut gs = setup_rhinar_mirror_action_phase();
    // p1 passes his turn out; p2 takes hers.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.turn_player, PlayerIndex::P2);
    assert_eq!(gs.phase, Phase::Action);

    // She attacks with Muscle Mutt, paying its cost of 3 with a single pitch-3.
    set_hand(&mut gs, PlayerIndex::P2, &[Card::MuscleMuttY, Card::ClearingBellowB]);
    let p2_hand = vec![Card::MuscleMuttY, Card::ClearingBellowB];
    step_card(&mut gs, ActionType::PlayCard, p2_hand[0]);
    step_card(&mut gs, ActionType::Pitch, p2_hand[1]);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.active_player, PlayerIndex::P1);

    let mut hand = vec![Card::RallyTheRearguardB];
    hand.extend_from_slice(spare);
    set_hand(&mut gs, PlayerIndex::P1, &hand);
    let rally = expect_in(&gs, PlayerIndex::P1, Card::RallyTheRearguardB, Zone::Hand, "Rally should be in hand");

    step_card(&mut gs, ActionType::Defend, rally);
    assert!(in_zone(&gs, PlayerIndex::P1, rally, Zone::CombatChain));
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    assert_eq!(gs.phase, Phase::Reaction);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    assert_eq!(gs.active_player, PlayerIndex::P1);

    (gs, rally)
}

#[test]
fn rally_ability_discarding_a_power6_card_does_not_intimidate() {
    // Rhinar blocks with Rally holding a single 6-power card, so activating the
    // ability discards exactly that card.
    let (mut gs, rally) = rhinar_blocking_with_rally(&[Card::SmashInstinctY]);
    let spare = *hand_cards(&gs, PlayerIndex::P1).first().expect("the 6-power card should be in hand");
    assert!(spare.data().power >= 6);
    // The hero who would intimidate is the one doing the discarding.
    assert!(matches!(
        gs.p1.hero.data().constant_effect,
        Some(crate::card_effects::ConstantEffect::OnDiscard6Intimidate)
    ));

    step_card(&mut gs, ActionType::Activate, rally);

    // The discard happened and cleared the power threshold, but it was made
    // while defending — on p2's turn — so p1's action-speed ability does not
    // trigger and p2 banishes nothing.
    assert!(in_zone(&gs, PlayerIndex::P1, spare, Zone::Graveyard));
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
    assert!(!gs.p1.has_intimidated());
}

#[test]
fn rally_ability_still_works_when_the_discard_would_have_intimidated() {
    // The ability itself is unaffected by the intimidate rule: the +3 lands and
    // the block is 5 either way.
    let (mut gs, rally) = rhinar_blocking_with_rally(&[Card::SmashInstinctY]);
    step_card(&mut gs, ActionType::Activate, rally);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    close_reaction_step(&mut gs);

    assert_eq!(block_bonus(&gs, rally), 3);
    // Muscle Mutt's 6 against 2 printed + 3 granted: 1 damage to Rhinar.
    assert_eq!(gs.p1.life, 20 - 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Chief Ruk'utan (Card::ChiefRukutan)
//
// Rhinar's mentor. "While Chief Ruk'utan is face down in arsenal, at the start
// of your turn, you may turn him face up. While Chief Ruk'utan is face up in
// arsenal, whenever you play a card with 6 or more power, intimidate and put a
// lesson counter on him. Then if there are 2 or more lesson counters on him,
// banish him, search your deck for Alpha Rampage, put it face up in arsenal and
// shuffle."
//
// The tests cover the turn-start flip (only on its owner's turn, only for a
// mentor), the trigger (face up only, 6+ power only, played cards only, its
// owner's plays only, at play time), and graduation at two lessons (banish,
// fetch face up, shuffle — with and without an Alpha Rampage left to find).
// ─────────────────────────────────────────────────────────────────────────────

/// Rhinar (p1) on his action phase with Chief Ruk'utan in his arsenal — face up
/// or face down as asked — and `hand` (at most three cards) as his hand. Returns
/// the game and Ruk'utan's index.
fn rhinar_with_rukutan(face_up: bool, hand: &[Card]) -> (TestGame, Card) {
    let mut gs = setup_rhinar_action_phase();
    let mut opening = vec![Card::ChiefRukutan];
    opening.extend_from_slice(hand);
    set_hand(&mut gs, PlayerIndex::P1, &opening);
    let mentor = put_in_arsenal(&mut gs, PlayerIndex::P1, Card::ChiefRukutan);
    if face_up {
        turn_mentor_face_up(&mut gs, PlayerIndex::P1);
        assert!(gs.p1.arsenal_face_up());
    }
    (gs, mentor)
}

/// Play the first `card` in p1's hand with its cost already banked, so it goes
/// straight onto the stack without a pitch step. Returns its index.
fn play_from_hand_prepaid(gs: &mut TestGame, card: Card) -> Card {
    let idx = expect_in(gs, PlayerIndex::P1, card, Zone::Hand, "the card to play should be in hand");
    gs.p1.resources = card.data().cost;
    step_card(gs, ActionType::PlayCard, idx);
    assert!(on_stack(gs, idx));
    idx
}

/// p1's deck card holding `card`, if there is one.
fn p1_deck_copy_of(gs: &Gamestate, card: Card) -> Option<Card> {
    in_zone(gs, PlayerIndex::P1, card, Zone::Deck).then_some(card)
}

#[test]
fn rukutan_blocks_for_3_like_hala() {
    assert_eq!(Card::ChiefRukutan.data().defense, 3);
    assert_eq!(Card::ChiefRukutan.data().defense, Card::HalaGoldenhelm.data().defense);
    assert!(!Card::ChiefRukutan.data().no_block);

    // In a Rhinar mirror, p2 attacks with Muscle Mutt (6); p1 blocks with
    // Ruk'utan alone.
    let mut gs = setup_rhinar_mirror_action_phase();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // Rhinar's action phase
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // and his arsenal
    set_hand(&mut gs, PlayerIndex::P2, &[Card::MuscleMuttY, Card::ClearingBellowB]);
    let p2_hand = vec![Card::MuscleMuttY, Card::ClearingBellowB];
    step_card(&mut gs, ActionType::PlayCard, p2_hand[0]);
    step_card(&mut gs, ActionType::Pitch, p2_hand[1]);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ChiefRukutan]);
    let ruk = *hand_cards(&gs, PlayerIndex::P1).first().expect("Ruk'utan should be in hand");
    let offered = legal_actions(&gs).iter()
        .any(|a| a.typ == ActionType::Defend && named(&gs, a) == Some(ruk));
    assert!(offered, "Ruk'utan should be offered as a blocker");

    step_card(&mut gs, ActionType::Defend, ruk);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done blocking
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // attacker passes
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // defender passes

    // 6 power against 3 block: Rhinar takes 3.
    assert_eq!(gs.p1.life, 20 - 3);
}

#[test]
fn chief_rukutan_is_a_brute_mentor_with_play6_intimidate_and_alpha_rampage_search() {
    use crate::card_effects::ConstantEffect;
    let data = Card::ChiefRukutan.data();
    assert_eq!(data.typ, CardType::Mentor);
    assert_eq!(data.card_class, CardClass::Brute);
    // The intimidate is the general play-6 constant effect; the lesson counter
    // and Alpha Rampage search are his mentor effect.
    assert!(matches!(data.constant_effect, Some(ConstantEffect::OnPlay6Intimidate)));
    assert!(matches!(data.mentor_effect, Some(crate::card_effects::MentorEffect::AlphaRampageSearch)));
    // The card it fetches is in Rhinar's deck to be found.
    assert!(crate::decks::build_rhinar_deck().contains(&Card::AlphaRampageR));
}

/// Rhinar arsenals Chief Ruk'utan at the end of his first turn (face down),
/// then Dorinthea passes through her turn. Returns the game as Rhinar's second
/// turn begins, and Ruk'utan's index.
fn rhinar_second_turn_with_face_down_rukutan() -> (TestGame, Card) {
    let mut gs = setup_rhinar_action_phase();
    let ruk = *hand_cards(&gs, PlayerIndex::P1).first().expect("opening hand");
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, ruk, Card::ChiefRukutan);

    // Rhinar passes his action phase and arsenals Ruk'utan: it goes in face down.
    let ruk = Card::ChiefRukutan;
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Arsenal);
    step_card(&mut gs, ActionType::Arsenal, ruk);
    assert_eq!(gs.turn_player, PlayerIndex::P2);
    assert!(in_zone(&gs, PlayerIndex::P1, ruk, Zone::Arsenal));

    // Dorinthea's turn is not interrupted — the decision is on its owner's
    // turn — and Ruk'utan stays face down and hidden from her throughout.
    assert_eq!(gs.phase, Phase::Action);
    assert!(!gs.p1.arsenal_face_up());
    assert_eq!(gs.p1.visibility(Zone::Arsenal), CardVisibleState::P1Knows);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // her action phase
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // and her arsenal
    assert_eq!(gs.turn_player, PlayerIndex::P1);
    (gs, ruk)
}

/// Pass Rhinar's turn (and Dorinthea's after it) back round to the start of
/// Rhinar's next turn, without arsenaling anything.
fn pass_round_to_rhinars_next_turn(gs: &mut TestGame) {
    assert_eq!(gs.phase, Phase::Action);
    step(gs, Action{ typ: ActionType::Pass, card: None}); // Rhinar's action phase
    step(gs, Action{ typ: ActionType::Pass, card: None}); // his arsenal (occupied)
    step(gs, Action{ typ: ActionType::Pass, card: None}); // Dorinthea's action phase
    step(gs, Action{ typ: ActionType::Pass, card: None}); // her arsenal
    assert_eq!(gs.turn_player, PlayerIndex::P1);
}

#[test]
fn a_face_down_mentor_opens_its_owners_turn_with_a_flip_decision() {
    let (gs, ruk) = rhinar_second_turn_with_face_down_rukutan();
    assert_eq!(gs.phase, Phase::MentorFlip);
    assert_eq!(gs.active_player, PlayerIndex::P1);

    // The two choices: flip Ruk'utan, or leave him face down.
    let actions = legal_actions(&gs);
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().any(|a| a.typ == ActionType::FlipFaceUp && named(&gs, a) == Some(ruk)));
    assert!(actions.iter().any(|a| a.typ == ActionType::Pass));
}

#[test]
fn flipping_turns_the_mentor_face_up_and_starts_the_action_phase() {
    let (mut gs, ruk) = rhinar_second_turn_with_face_down_rukutan();
    step_card(&mut gs, ActionType::FlipFaceUp, ruk);

    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    assert!(gs.p1.arsenal_face_up());
    assert_eq!(gs.p1.visibility(Zone::Arsenal), CardVisibleState::BothKnow);
    assert!(in_zone(&gs, PlayerIndex::P1, ruk, Zone::Arsenal));
}

#[test]
fn a_flipped_mentor_stays_face_up_and_is_never_offered_again() {
    let (mut gs, ruk) = rhinar_second_turn_with_face_down_rukutan();
    step_card(&mut gs, ActionType::FlipFaceUp, ruk);

    // Two more full rounds: each of Rhinar's turns goes straight to the Action
    // phase, with Ruk'utan still face up.
    for _ in 0..2 {
        pass_round_to_rhinars_next_turn(&mut gs);
        assert_eq!(gs.phase, Phase::Action);
        assert!(gs.p1.arsenal_face_up());
        assert!(in_zone(&gs, PlayerIndex::P1, ruk, Zone::Arsenal));
    }
}

#[test]
fn declining_leaves_the_mentor_face_down_and_it_is_offered_again_next_turn() {
    let (mut gs, ruk) = rhinar_second_turn_with_face_down_rukutan();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    assert_eq!(gs.phase, Phase::Action);
    assert!(!gs.p1.arsenal_face_up());
    assert_eq!(gs.p1.visibility(Zone::Arsenal), CardVisibleState::P1Knows, "a declined mentor stays hidden");

    // Still face down, so the next turn opens on the same decision.
    pass_round_to_rhinars_next_turn(&mut gs);
    assert_eq!(gs.phase, Phase::MentorFlip);
    assert_eq!(face_down_mentor_in_arsenal(&gs, PlayerIndex::P1), Some(gs.p1.deck_id(ruk)));
}

#[cfg(feature = "logging")]
#[test]
fn declining_the_flip_is_not_logged_to_the_opponent() {
    // Same lead-up as `rhinar_second_turn_with_face_down_rukutan`, with logging
    // on so each player's view can be checked.
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), true);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
    let ruk = *hand_cards(&gs, PlayerIndex::P1).first().expect("opening hand");
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, ruk, Card::ChiefRukutan);
    let ruk = Card::ChiefRukutan;
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step_card(&mut gs, ActionType::Arsenal, ruk);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::MentorFlip);

    let p2_before = gs.p2.log.as_ref().unwrap().len();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    let declined = "P1 leaves their arsenal face down";
    assert_eq!(gs.log.as_ref().unwrap().last().map(String::as_str), Some(declined));
    assert_eq!(gs.p1.log.as_ref().unwrap().last().map(String::as_str), Some(declined));
    // Dorinthea's log gains nothing, so she cannot tell a decision was made.
    assert_eq!(gs.p2.log.as_ref().unwrap().len(), p2_before);
    assert!(gs.p2.log.as_ref().unwrap().iter().all(|l| !l.contains("face down")));
}

#[cfg(feature = "logging")]
#[test]
fn flipping_is_logged_publicly() {
    // Turning the mentor face up reveals it anyway, so both players see it.
    let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), true);
    step(&mut gs, Action{ typ: ActionType::ChooseFirst, card: None});
    let ruk = *hand_cards(&gs, PlayerIndex::P1).first().expect("opening hand");
    relabel(&mut gs, PlayerIndex::P1, Zone::Hand, ruk, Card::ChiefRukutan);
    let ruk = Card::ChiefRukutan;
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step_card(&mut gs, ActionType::Arsenal, ruk);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step_card(&mut gs, ActionType::FlipFaceUp, ruk);

    let flipped = "P1 turns ChiefRukutan face up in their arsenal";
    for log in [&gs.log, &gs.p1.log, &gs.p2.log] {
        assert_eq!(log.as_ref().unwrap().last().map(String::as_str), Some(flipped));
    }
}

#[test]
fn a_face_down_mentor_does_not_trigger_after_being_declined() {
    let (mut gs, ruk) = rhinar_second_turn_with_face_down_rukutan();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    maybe_play6_intimidate(&mut gs.state, &mut gs.rng, PlayerIndex::P1, 6);
    assert_eq!(gs.p1.arsenal_lesson_counters, 0);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
}

#[test]
fn a_non_mentor_arsenal_card_opens_no_flip_decision() {
    let mut gs = setup_rhinar_action_phase();
    put_in_arsenal(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
    assert_eq!(face_down_mentor_in_arsenal(&gs, PlayerIndex::P1), None);
    pass_round_to_rhinars_next_turn(&mut gs);
    assert_eq!(gs.phase, Phase::Action);
}

#[test]
fn rukutan_is_never_offered_as_a_play_from_the_arsenal() {
    let (gs, ruk) = rhinar_with_rukutan(true, &[Card::ClearingBellowB, Card::ClearingBellowB]);
    let offered: Vec<Card> = legal_actions(&gs).iter()
        .filter_map(|a| named(&gs, a))
        .collect();
    assert!(!offered.contains(&ruk));
}

#[test]
fn face_up_rukutan_intimidates_and_takes_a_lesson_when_a_6_power_card_is_played() {
    let (mut gs, ruk) = rhinar_with_rukutan(true, &[Card::MuscleMuttY]);
    assert!(Card::MuscleMuttY.data().power >= 6);
    assert!(Card::MuscleMuttY.data().keyword.is_empty(), "the intimidate must come from Ruk'utan");

    let mutt = play_from_hand_prepaid(&mut gs, Card::MuscleMuttY);

    // The trigger fires as the card is played: Muscle Mutt is still on the stack
    // and Dorinthea has already banished a card.
    assert_eq!(stack_top_card(&gs), Some(mutt));
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);
    assert!(gs.p1.has_intimidated(), "a Ruk'utan intimidate counts for 'if you've intimidated'");
    assert_eq!(gs.p1.arsenal_lesson_counters, 1);
    // One lesson is not enough to graduate: Ruk'utan stays, face up.
    assert!(in_zone(&gs, PlayerIndex::P1, ruk, Zone::Arsenal));
    assert!(gs.p1.arsenal_face_up());
}

#[test]
fn face_down_rukutan_does_not_trigger() {
    let (mut gs, ruk) = rhinar_with_rukutan(false, &[Card::MuscleMuttY]);
    play_from_hand_prepaid(&mut gs, Card::MuscleMuttY);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
    assert_eq!(gs.p1.arsenal_lesson_counters, 0);
}

#[test]
fn rukutan_ignores_a_card_with_less_than_6_power() {
    // Rally the Rearguard is a 4-power attack action: short of the threshold.
    assert!(Card::RallyTheRearguardB.data().power < 6);
    let (mut gs, ruk) = rhinar_with_rukutan(true, &[Card::RallyTheRearguardB]);
    play_from_hand_prepaid(&mut gs, Card::RallyTheRearguardB);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
    assert_eq!(gs.p1.arsenal_lesson_counters, 0);
}

#[test]
fn rukutan_ignores_the_opponents_6_power_plays() {
    let (mut gs, ruk) = rhinar_with_rukutan(true, &[]);
    maybe_play6_intimidate(&mut gs.state, &mut gs.rng, PlayerIndex::P2, 9);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P1), 0);
    assert_eq!(gs.p1.arsenal_lesson_counters, 0);
}

#[test]
fn rukutan_graduates_on_his_second_lesson_fetching_alpha_rampage_face_up() {
    let (mut gs, ruk) = rhinar_with_rukutan(true, &[Card::MuscleMuttY]);
    gs.p1.arsenal_lesson_counters = 1;
    let rampage = p1_deck_copy_of(&gs, Card::AlphaRampageR).expect("Alpha Rampage should be in the deck");
    let deck_before = gs.p1.deck_size();

    play_from_hand_prepaid(&mut gs, Card::MuscleMuttY);

    // The second lesson still intimidates first.
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 1);

    // Ruk'utan is banished.
    assert!(in_zone(&gs, PlayerIndex::P1, ruk, Zone::Banish));
    assert_eq!(gs.p1.banished.len(), 1);
    // The arsenal now holds Alpha Rampage, starting with no lesson counters.
    assert_eq!(gs.p1.arsenal_lesson_counters, 0);

    // Alpha Rampage now sits in the arsenal, face up and public.
    assert_eq!(arsenal_card(&gs, PlayerIndex::P1), Some(rampage));
    assert!(in_zone(&gs, PlayerIndex::P1, rampage, Zone::Arsenal));
    assert!(gs.p1.arsenal_face_up());
    assert_eq!(gs.p1.visibility(Zone::Arsenal), CardVisibleState::BothKnow);

    // The deck lost exactly that card, and it was the deck's only copy.
    assert_eq!(gs.p1.deck_size(), deck_before - 1);
    let order = deck_order(&gs, PlayerIndex::P1);
    assert_eq!(order.len(), gs.p1.deck_size() as usize);
    assert!(!order.contains(&rampage));
}

#[test]
fn rukutan_graduation_shuffles_the_deck() {
    let (mut gs, ruk) = rhinar_with_rukutan(true, &[]);
    gs.p1.arsenal_lesson_counters = 1;
    let rampage = p1_deck_copy_of(&gs, Card::AlphaRampageR).expect("Alpha Rampage should be in the deck");
    let mut expected_unshuffled = deck_order(&gs, PlayerIndex::P1);
    expected_unshuffled.retain(|&c| c != rampage);

    maybe_play6_intimidate(&mut gs.state, &mut gs.rng, PlayerIndex::P1, 6);

    let after = deck_order(&gs, PlayerIndex::P1);
    // Same cards, new order (a 30-odd card deck coming back in its old order
    // would be a one-in-astronomical shuffle).
    let mut a = after.clone();
    let mut b = expected_unshuffled.clone();
    a.sort_by_key(|c| *c as u8);
    b.sort_by_key(|c| *c as u8);
    assert_eq!(a, b);
    assert_ne!(after, expected_unshuffled);
}

#[test]
fn rukutan_graduates_even_with_no_alpha_rampage_left_to_find() {
    let (mut gs, ruk) = rhinar_with_rukutan(true, &[]);
    gs.p1.arsenal_lesson_counters = 1;
    // Alpha Rampage has already left the deck (here: relabelled away).
    let rampage = p1_deck_copy_of(&gs, Card::AlphaRampageR).expect("Alpha Rampage should be in the deck");
    relabel(&mut gs, PlayerIndex::P1, Zone::Deck, rampage, Card::MuscleMuttY);
    let deck_before = gs.p1.deck_size();

    maybe_play6_intimidate(&mut gs.state, &mut gs.rng, PlayerIndex::P1, 6);

    // He is banished all the same; the arsenal is simply left empty.
    assert!(in_zone(&gs, PlayerIndex::P1, ruk, Zone::Banish));
    assert_eq!(gs.p1.arsenal, None);
    assert_eq!(gs.p1.deck_size(), deck_before);
    assert_eq!(deck_order(&gs, PlayerIndex::P1).len(), deck_before as usize);
}

#[test]
fn alpha_rampage_fetched_by_rukutan_can_be_played_from_the_arsenal_that_turn() {
    // Clearing Bellow pitches 3 to pay for it; Pack Call stays back to be the
    // discard its additional cost demands.
    let (mut gs, ruk) = rhinar_with_rukutan(true, &[Card::ClearingBellowB, Card::PackCallY]);
    gs.p1.arsenal_lesson_counters = 1;
    maybe_play6_intimidate(&mut gs.state, &mut gs.rng, PlayerIndex::P1, 6);
    let rampage = arsenal_card(&gs, PlayerIndex::P1).expect("Alpha Rampage should be in the arsenal");
    assert_eq!(rampage, Card::AlphaRampageR);
    assert_eq!(gs.phase, Phase::Action);

    let offered = legal_actions(&gs).iter()
        .any(|a| a.typ == ActionType::PlayCard && named(&gs, a) == Some(rampage));
    assert!(offered, "the fetched Alpha Rampage should be playable from the arsenal");

    // Playing it takes it out of the arsenal, and with it its face-up state.
    step_card(&mut gs, ActionType::PlayCard, rampage);
    let bellow = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "Clearing Bellow should be in hand");
    step_card(&mut gs, ActionType::Pitch, bellow);
    assert!(on_stack(&gs, rampage));
    assert_eq!(gs.p1.arsenal, None);
    assert!(!gs.p1.arsenal_face_up());
}

#[test]
fn weapon_swing_does_not_teach_rukutan_a_lesson() {
    // Swinging a weapon activates it rather than playing a card, so it never
    // reaches the trigger, whatever its power. (Bone Basher's 4 would miss the
    // threshold anyway; this pins down that an Activate is not even checked.)
    let (mut gs, ruk) = rhinar_with_rukutan(true, &[Card::ClearingBellowB, Card::ClearingBellowB]);
    let basher = weapon_card(&gs, PlayerIndex::P1);
    gs.p1.resources = 2;
    step_card(&mut gs, ActionType::Activate, basher);
    assert!(on_stack(&gs, basher));
    assert_eq!(gs.p1.arsenal_lesson_counters, 0);
    assert_eq!(intimidate_banish_count(&gs, PlayerIndex::P2), 0);
}

#[test]
fn a_face_up_card_is_always_visible_to_both_players() {
    // Random play through whole games, checking after every action that a
    // face-up arsenal card is known to both players, that a face-down one is
    // shown to its owner's opponent only when they could deduce it, and that an
    // empty arsenal carries no face-up or known state or lesson counters.
    use rand::{RngExt, SeedableRng};
    let mut face_up_seen = 0;
    for seed in 0..300u64 {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(seed), false);
        let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
        while !gs.is_game_over() {
            let actions = legal_actions(&gs);
            let act = actions[rng.random_range(0..actions.len())];
            step(&mut gs, act);
            for player in [&gs.p1, &gs.p2] {
                let Some(arsenal) = player.arsenal else {
                    // An empty arsenal has nothing face up and no counters.
                    assert!(!player.arsenal_face_up());
                    assert_eq!(player.arsenal_lesson_counters, 0);
                    continue;
                };
                let card = player.card(arsenal);
                let visible = player.visibility(Zone::Arsenal);
                if player.arsenal_face_up() {
                    face_up_seen += 1;
                    assert_eq!(visible, CardVisibleState::BothKnow,
                        "face-up {:?} should be visible to both players", card);
                } else if player.arsenal_known() {
                    // Face down, but the opponent could tell what went in.
                    assert_eq!(visible, CardVisibleState::BothKnow);
                } else {
                    // Face down: known to its owner, never shown to the opponent.
                    assert_eq!(visible, CardVisibleState::owner_only(player.pid));
                }
            }
            for player in [&gs.p1, &gs.p2] {
                if player.arsenal.is_none() {
                    assert!(!player.arsenal_known(), "an empty arsenal is not known");
                }
            }
        }
    }
    assert!(face_up_seen > 0, "random play should turn some mentor face up");
}

// ─────────────────────────────────────────────────────────────────────────────
// Bone Basher (Card::BoneBasher)
//
// Rhinar's weapon: a 4-power two-handed brute club with "Once per Turn Action —
// {r}{r}: Attack" and no other text. The swing is an Activate of its `Attack2`
// ability, gated and paid like any other activated ability: it costs an action
// point plus 2 resources. "Once per turn" falls out of the zones rather than a
// flag: a swing carries the weapon onto the combat chain, and it only returns
// to its slot when the chain closes as the action phase ends, so it is not in
// its slot to be swung again until the next turn.
// ─────────────────────────────────────────────────────────────────────────────

/// Whether Bone Basher's swing is among the active player's legal actions.
fn bone_basher_swing_offered(gs: &Gamestate) -> bool {
    legal_actions(gs).iter().any(|a| {
        a.typ == ActionType::Activate && card_of(&gs, a) == Card::BoneBasher
    })
}

/// Swing Bone Basher on Rhinar's first turn, paying with a pitch-3 card, and
/// carry the attack through an unblocked combat back to his action phase.
/// Returns the game and the weapon.
fn swing_bone_basher_unblocked() -> (TestGame, Card) {
    swing_bone_basher_unblocked_keeping(&[])
}

/// As `swing_bone_basher_unblocked`, but Rhinar still holds `kept` once the
/// swing is paid for.
fn swing_bone_basher_unblocked_keeping(kept: &[Card]) -> (TestGame, Card) {
    let mut gs = setup_rhinar_action_phase();
    let mut hand = vec![Card::ClearingBellowB];
    hand.extend_from_slice(kept);
    set_hand(&mut gs, PlayerIndex::P1, &hand);
    let pitch = expect_in(&gs, PlayerIndex::P1, Card::ClearingBellowB, Zone::Hand, "in hand");

    let weapon = weapon_card(&gs, PlayerIndex::P1);
    assert_eq!(weapon, Card::BoneBasher);
    step_card(&mut gs, ActionType::Activate, weapon);
    assert_eq!(gs.phase, Phase::ActionPitch);
    step_card(&mut gs, ActionType::Pitch, pitch);
    assert_eq!(gs.phase, Phase::ActionInstant);
    // 3 pitched against a cost of 2 leaves 1 floating.
    assert_eq!(gs.p1.resources, 1);

    // Resolve the swing onto the chain, then pass through defend, triggers and
    // reactions with no blockers so the full 4 lands.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Defend);
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.turn_player, PlayerIndex::P1);
    (gs, weapon)
}

#[test]
fn bone_basher_is_a_4_power_brute_club_swung_for_2() {
    use crate::cards::WeaponType;
    let data = Card::BoneBasher.data();
    assert_eq!(data.typ, CardType::Weapon);
    assert_eq!(data.weapon_type, Some(WeaponType::Club2h));
    assert_eq!(data.card_class, CardClass::Brute);
    assert_eq!(data.power, 4);
    assert!(data.keyword.is_empty(), "Bone Basher should carry no keywords (no Go Again)");
    assert!(data.play_effects.is_empty());
    assert!(data.next_attack_effect.is_none());
    assert!(data.constant_effect.is_none());

    // The swing is an attack-action-speed ability costing {r}{r}.
    let ability = data.ability.as_ref().expect("Bone Basher should have an attack ability");
    assert!(matches!(ability, Ability::Attack2));
    assert_eq!(ability.resource_cost(), 2);
    assert_eq!(ability.card_type(), CardType::AttackAction);
    assert!(!ability.discards_a_card());
    assert!(!ability.only_while_defending());
}

#[test]
fn bone_basher_swing_is_offered_only_when_2_resources_can_be_paid() {
    // A lone pitch-1 card cannot pay {r}{r}.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::AlphaRampageR]);
    gs.p1.resources = 0;
    assert!(!bone_basher_swing_offered(&gs));

    // A floating resource makes up the difference.
    gs.p1.resources = 1;
    assert!(bone_basher_swing_offered(&gs));

    // So does a pitch-2 card on its own.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::MuscleMuttY]);
    gs.p1.resources = 0;
    assert!(bone_basher_swing_offered(&gs));
}

#[test]
fn bone_basher_swing_needs_an_action_point() {
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ClearingBellowB]);
    assert!(bone_basher_swing_offered(&gs));
    gs.p1.action_points = 0;
    assert!(!bone_basher_swing_offered(&gs));
}

#[test]
fn bone_basher_unblocked_swing_hits_for_4_and_spends_the_action_point() {
    let life_before = Card::Dorinthea.data().hero_life;
    let (gs, weapon) = swing_bone_basher_unblocked();

    assert_eq!(gs.p2.life, life_before - 4);
    // No Go Again: the swing used Rhinar's only action point.
    assert_eq!(gs.p1.action_points, 0);
    // The weapon sits on the chain until it closes.
    assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::CombatChain));
    assert_eq!(gs.p1.weapon, None);
}

#[test]
fn bone_basher_cannot_swing_twice_in_a_turn() {
    let (mut gs, weapon) = swing_bone_basher_unblocked();

    // Even with an action point and resources to spare (as if the swing had
    // gone again), the weapon is still on the chain and cannot swing again.
    gs.p1.action_points = 1;
    gs.p1.resources = 2;
    assert!(uses_this_turn(&gs, weapon) > 0);
    assert!(!bone_basher_swing_offered(&gs));

    // Breaking the chain puts the weapon back in its slot, but it has already
    // swung this turn, so it is still not offered.
    step(&mut gs, Action{ typ: ActionType::CloseChain, card: None});
    assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));
    assert_eq!(gs.phase, Phase::Action);
    assert!(!bone_basher_swing_offered(&gs));
}

#[test]
fn bone_basher_can_swing_again_on_rhinars_next_turn() {
    let (mut gs, weapon) = swing_bone_basher_unblocked();

    // Pass out of Rhinar's turn and through Dorinthea's.
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    while !(gs.turn_player == PlayerIndex::P1 && gs.phase == Phase::Action) {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }

    // Back in its slot with a fresh action point, the swing is on offer again.
    assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));
    assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));
    assert_eq!(gs.p1.action_points, 1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ClearingBellowB]);
    assert!(bone_basher_swing_offered(&gs));
}

// ─────────────────────────────────────────────────────────────────────────────
// Breaking the combat chain (ActionType::CloseChain)
//
// Once combat damage is dealt and play is back in the Action phase, the turn
// player may break the open chain before their next attack: every card on it
// leaves — a weapon back to its slot, everything else to its owner's graveyard
// — and the Action phase carries on. It is offered only while a chain is open
// and the turn player has an action point left for another attack.
// ─────────────────────────────────────────────────────────────────────────────

fn close_chain_offered(gs: &Gamestate) -> bool {
    legal_actions(gs).iter().any(|a| a.typ == ActionType::CloseChain)
}

/// Swing Bone Basher on Rhinar's first turn and have Dorinthea block it with
/// one hand card, then carry play back to Rhinar's action phase. Returns the
/// game, the weapon's index and the blocker's index.
fn swing_bone_basher_blocked() -> (TestGame, Card, Card) {
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ClearingBellowB]);
    let weapon = weapon_card(&gs, PlayerIndex::P1);
    step_card(&mut gs, ActionType::Activate, weapon);
    let pitch = *hand_cards(&gs, PlayerIndex::P1).first().unwrap();
    step_card(&mut gs, ActionType::Pitch, pitch);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Defend);

    let block = *legal_actions(&gs).iter()
        .find(|a| a.typ == ActionType::Defend)
        .expect("Dorinthea should have a card to block with");
    let blocker = card_of(&gs, &block);
    step(&mut gs, block);
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert!(in_zone(&gs, PlayerIndex::P2, blocker, Zone::CombatChain));
    (gs, weapon, blocker)
}

#[test]
fn close_chain_is_not_offered_without_an_open_chain() {
    let gs = setup_rhinar_action_phase();
    assert!(!close_chain_offered(&gs));
}

#[test]
fn close_chain_is_not_offered_without_an_action_point() {
    // Bone Basher has no Go Again, so the swing spent Rhinar's only point:
    // there is no next attack to break the chain for.
    let (gs, _) = swing_bone_basher_unblocked();
    assert_eq!(gs.p1.action_points, 0);
    assert!(!close_chain_offered(&gs));
}

#[test]
fn close_chain_is_offered_after_combat_with_an_action_point_left() {
    let (mut gs, _) = swing_bone_basher_unblocked();
    gs.p1.action_points = 1; // as if the attack had gone again
    assert!(close_chain_offered(&gs));
}

#[test]
fn close_chain_returns_the_weapon_and_bins_the_blockers() {
    let (mut gs, weapon, blocker) = swing_bone_basher_blocked();
    gs.p1.action_points = 1;
    assert!(close_chain_offered(&gs));
    step(&mut gs, Action{ typ: ActionType::CloseChain, card: None});

    // The weapon is back in its slot, the blocker in Dorinthea's graveyard,
    // and both chains are empty.
    assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));
    assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));
    assert!(in_zone(&gs, PlayerIndex::P2, blocker, Zone::Graveyard));
    assert!(gs.p1.chain_link.iter().all(|l| l.is_empty()));
    assert!(gs.p2.chain_link.iter().all(|l| l.is_empty()));

    // The action phase carries on with the action point intact, and with the
    // chain gone there is nothing left to break.
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    assert_eq!(gs.p1.action_points, 1);
    assert!(!close_chain_offered(&gs));
}

#[test]
fn the_next_attack_after_breaking_the_chain_opens_a_fresh_chain() {
    // Keep a 2-cost attack back; the swing leaves 1 resource floating, and a
    // second pitch-3 card covers the rest.
    let (mut gs, _) = swing_bone_basher_unblocked_keeping(
        &[Card::SmashWithBigTreeY, Card::ClearingBellowB]);
    gs.p1.action_points = 1;
    step(&mut gs, Action{ typ: ActionType::CloseChain, card: None});

    let find = |gs: &Gamestate, card: Card| expect_in(gs, PlayerIndex::P1, card, Zone::Hand, "in hand");
    let attack = find(&gs, Card::SmashWithBigTreeY);
    let pitch = find(&gs, Card::ClearingBellowB);
    step_card(&mut gs, ActionType::PlayCard, attack);
    assert_eq!(gs.phase, Phase::ActionPitch);
    step_card(&mut gs, ActionType::Pitch, pitch);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});

    // It takes link 0 rather than queueing behind the swing.
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(link_cards(&gs, PlayerIndex::P1, 0), vec![attack]);
    assert!(gs.p1.chain_link[1..].iter().all(|l| l.is_empty()));
}

#[test]
fn passing_still_closes_the_chain_and_ends_the_action_phase() {
    let (mut gs, weapon) = swing_bone_basher_unblocked();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Arsenal);
    assert!(in_zone(&gs, PlayerIndex::P1, weapon, Zone::Weapon));
}

// ─────────────────────────────────────────────────────────────────────────────
// Blossom of Spring (Card::BlossomOfSpring)
//
// Generic chest equipment, 0 defense: "Action — Destroy Blossom of Spring:
// Gain {r}. Go again." The activation costs no resources but is action speed,
// so it needs an action point; destroying the card is its cost, so it leaves
// the equipment slot as it is activated and ends in the graveyard. Resolving
// gains a floating resource, and Go Again hands the action point back.
// ─────────────────────────────────────────────────────────────────────────────

fn blossom_offered(gs: &Gamestate) -> bool {
    legal_actions(gs).iter().any(|a| {
        a.typ == ActionType::Activate && card_of(gs, a) == Card::BlossomOfSpring
    })
}

/// Activate Blossom of Spring on Rhinar's first turn holding `hand`, and let
/// both players pass so the ability resolves. Rhinar starts with no floating
/// resources.
fn activate_and_resolve_blossom(hand: &[Card]) -> TestGame {
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, hand);
    gs.p1.resources = 0;
    assert!(in_zone(&gs, PlayerIndex::P1, Card::BlossomOfSpring, Zone::Equipment));

    step_card(&mut gs, ActionType::Activate, Card::BlossomOfSpring);
    // Free to activate: straight onto the stack, no pitch phase.
    assert_eq!(gs.phase, Phase::ActionInstant);
    assert_eq!(stack_top_card(&gs), Some(Card::BlossomOfSpring));
    // Destroying it is the cost, so it is already out of its slot.
    assert!(!in_zone(&gs, PlayerIndex::P1, Card::BlossomOfSpring, Zone::Equipment));

    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    gs
}

#[test]
fn blossom_of_spring_is_a_free_action_ability_with_go_again_gaining_1() {
    let data = Card::BlossomOfSpring.data();
    assert_eq!(data.typ, CardType::Equipment);
    assert_eq!(data.card_class, CardClass::Generic);
    assert_eq!(data.defense, 0);
    assert!(data.keyword.contains(Keyword::GoAgain));
    let ability = data.ability.as_ref().expect("Blossom of Spring has an ability");
    assert!(matches!(ability, Ability::DestroyGain1Resource));
    assert_eq!(ability.resource_cost(), 0);
    assert_eq!(ability.card_type(), CardType::Action);
    assert_eq!(ability.resources_gained(), 1);
    assert!(!ability.once_per_turn());
    assert!(!ability.discards_a_card());
    assert!(!ability.only_while_defending());
}

#[test]
fn blossom_of_spring_is_offered_with_an_action_point_and_an_empty_hand() {
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[]);
    gs.p1.resources = 0;
    assert!(blossom_offered(&gs), "it costs nothing, so an empty hand can activate it");
}

#[test]
fn blossom_of_spring_needs_an_action_point() {
    let mut gs = setup_rhinar_action_phase();
    gs.p1.action_points = 0;
    assert!(!blossom_offered(&gs));
}

#[test]
fn blossom_of_spring_gains_a_resource_keeps_the_action_point_and_is_destroyed() {
    let gs = activate_and_resolve_blossom(&[]);
    assert_eq!(gs.p1.resources, 1, "Gain {{r}}");
    assert_eq!(gs.p1.action_points, 1, "Go again: the action point comes back");
    assert!(in_zone(&gs, PlayerIndex::P1, Card::BlossomOfSpring, Zone::Graveyard));
    assert!(!in_zone(&gs, PlayerIndex::P1, Card::BlossomOfSpring, Zone::Equipment));
    // Nothing went to the combat chain: an armor ability is not an attack.
    assert!(gs.p1.chain_link.iter().all(|l| l.is_empty()));
    assert!(gs.stack_is_empty());
}

#[test]
fn blossom_of_spring_cannot_be_activated_again_once_destroyed() {
    let gs = activate_and_resolve_blossom(&[]);
    assert!(!blossom_offered(&gs));
}

#[test]
fn blossom_of_spring_resource_pays_for_a_card_the_hand_could_not() {
    // Awakening Bellow costs 1, and a lone card cannot pitch for itself.
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::AwakeningBellowR]);
    gs.p1.resources = 0;
    let bellow_offered = |gs: &Gamestate| legal_actions(gs).iter().any(|a| {
        a.typ == ActionType::PlayCard && card_of(gs, a) == Card::AwakeningBellowR
    });
    assert!(!bellow_offered(&gs));

    let mut gs = activate_and_resolve_blossom(&[Card::AwakeningBellowR]);
    assert!(bellow_offered(&gs), "the gained resource and kept action point allow the play");

    // Playing it spends the floating resource with no pitch phase.
    step_card(&mut gs, ActionType::PlayCard, Card::AwakeningBellowR);
    assert_eq!(gs.phase, Phase::ActionInstant);
    assert_eq!(gs.p1.resources, 0);
}

#[test]
fn blossom_of_spring_stays_destroyed_on_rhinars_next_turn() {
    let mut gs = activate_and_resolve_blossom(&[]);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    while !(gs.turn_player == PlayerIndex::P1 && gs.phase == Phase::Action) {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.resources, 0, "the unspent resource was lost at end of turn");
    assert!(in_zone(&gs, PlayerIndex::P1, Card::BlossomOfSpring, Zone::Graveyard));
    assert!(!blossom_offered(&gs));
}

// ─────────────────────────────────────────────────────────────────────────────
// Bone Vizier (Card::BoneVizier), and blocking with equipment
//
// Brute head equipment, 1 defense: "When Bone Vizier is destroyed, reveal the
// top card of your deck. If it has 6 or more power, put it on top of your
// deck. Otherwise, put it on the bottom. Blade Break."
//
// Any worn equipment may be declared as a blocker (bar Battleworn pieces, whose
// counter is not modelled yet): it leaves its slot for the chain link and goes
// back when the chain closes. Blade Break instead destroys it as the chain
// closes, which is what fires Bone Vizier's reveal.
//
// Dorinthea (p1) attacks with Flock of the Feather Walkers (5 power) and Rhinar
// (p2) defends, so "your deck" is p2's and p1's must stay untouched.
// ─────────────────────────────────────────────────────────────────────────────

/// Rhinar (p2) declares `equipment` (worn pieces) and nothing from hand as
/// blockers against Flock, and combat damage resolves. Play is back in
/// Dorinthea's action phase with the chain still open.
fn rhinar_blocks_flock_with_equipment(equipment: &[Card]) -> TestGame {
    let mut gs = rhinar_defending_flock();
    for &card in equipment {
        assert!(in_zone(&gs, PlayerIndex::P2, card, Zone::Equipment));
        step_card_from(&mut gs, ActionType::Defend, card, Zone::Equipment);
        assert!(!in_zone(&gs, PlayerIndex::P2, card, Zone::Equipment), "it leaves its slot to defend");
        assert!(link_cards(&gs, PlayerIndex::P2, 0).contains(&card));
    }
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done declaring
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.turn_player, PlayerIndex::P1);
    gs
}

/// Dorinthea passes her action phase, closing the combat chain.
fn close_chain_by_passing(gs: &mut TestGame) {
    step(gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Arsenal);
    assert!(!gs.p1.chain_occupied() && !gs.p2.chain_occupied());
}

fn equipment_blockers(gs: &Gamestate) -> HashSet<Card> {
    legal_actions(gs).iter()
        .filter(|a| a.typ == ActionType::Defend && a.card_ref().zone == Zone::Equipment)
        .map(|a| card_of(gs, a))
        .collect()
}

#[test]
fn bone_vizier_is_a_1_block_brute_helm_with_blade_break_and_a_destroy_trigger() {
    let data = Card::BoneVizier.data();
    assert_eq!(data.typ, CardType::Equipment);
    assert!(matches!(data.slot, Some(crate::cards::EquipmentSlot::Head)));
    assert_eq!(data.card_class, CardClass::Brute);
    assert_eq!(data.defense, 1);
    assert!(data.keyword.contains(Keyword::BladeBreak));
    assert!(matches!(data.constant_effect, Some(ConstantEffect::OnDestroyedReveal6BottomOtherwise)));
    assert!(data.ability.is_none());
    assert!(data.defend_effect.is_none(), "the reveal is on destruction, not on defending");
}

#[test]
fn rhinar_may_defend_with_any_of_his_worn_equipment() {
    let gs = rhinar_defending_flock();
    assert_eq!(equipment_blockers(&gs), HashSet::from([
        Card::BoneVizier,
        Card::BlossomOfSpring,
        Card::IronhideGauntlet,
        Card::IronhideLegs,
    ]));
}

#[test]
fn battleworn_equipment_is_not_offered_as_a_blocker_yet() {
    let gs = dorinthea_defending_muscle_mutt();
    let offered = equipment_blockers(&gs);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::GallantryGold, Zone::Equipment));
    assert!(!offered.contains(&Card::GallantryGold));
    assert!(offered.contains(&Card::IronrotHelm));
}

#[test]
fn bone_vizier_blocks_for_1() {
    let gs = rhinar_blocks_flock_with_equipment(&[Card::BoneVizier]);
    assert_eq!(gs.p2.life, 20 - (5 - 1));
    // Still defending until the chain closes.
    assert_eq!(link_cards(&gs, PlayerIndex::P2, 0), vec![Card::BoneVizier]);
}

#[test]
fn bone_vizier_is_destroyed_when_the_chain_closes_and_keeps_a_6_power_card_on_top() {
    let mut gs = rhinar_blocks_flock_with_equipment(&[Card::BoneVizier]);
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::WoundedBullY);
    assert!(Card::WoundedBullY.data().power >= 6);
    let before = deck_order(&gs, PlayerIndex::P2);
    let attacker_before = deck_order(&gs, PlayerIndex::P1);

    close_chain_by_passing(&mut gs);

    assert!(in_zone(&gs, PlayerIndex::P2, Card::BoneVizier, Zone::Graveyard));
    assert!(!in_zone(&gs, PlayerIndex::P2, Card::BoneVizier, Zone::Equipment));
    assert_eq!(deck_order(&gs, PlayerIndex::P2), before, "a 6-power card stays put");
    assert_eq!(deck_order(&gs, PlayerIndex::P2)[0], top);
    assert!(gs.p2.deck.front().unwrap().is_known(), "the revealed card is public");
    assert_eq!(deck_order(&gs, PlayerIndex::P1), attacker_before);
}

#[test]
fn bone_vizier_destruction_bottoms_a_card_with_less_than_6_power() {
    let mut gs = rhinar_blocks_flock_with_equipment(&[Card::BoneVizier]);
    let top = set_top_of_deck(&mut gs, PlayerIndex::P2, Card::ClearingBellowB);
    assert!(Card::ClearingBellowB.data().power < 6);
    let before = deck_order(&gs, PlayerIndex::P2);

    close_chain_by_passing(&mut gs);

    let mut expected = before[1..].to_vec();
    expected.push(top);
    assert_eq!(deck_order(&gs, PlayerIndex::P2), expected);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::BoneVizier, Zone::Graveyard));
}

#[test]
fn bone_vizier_that_did_not_defend_is_not_destroyed() {
    let mut gs = rhinar_blocks_flock_with_equipment(&[]);
    let before = deck_order(&gs, PlayerIndex::P2);
    close_chain_by_passing(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::BoneVizier, Zone::Equipment));
    assert!(!in_zone(&gs, PlayerIndex::P2, Card::BoneVizier, Zone::Graveyard));
    assert_eq!(deck_order(&gs, PlayerIndex::P2), before, "no reveal without a destruction");
}

#[test]
fn equipment_without_blade_break_returns_to_its_slot_when_the_chain_closes() {
    let mut gs = rhinar_blocks_flock_with_equipment(&[Card::IronhideLegs, Card::BlossomOfSpring]);
    // Both 0-defense pieces: the whole 5 lands.
    assert_eq!(gs.p2.life, 15);
    close_chain_by_passing(&mut gs);
    for card in [Card::IronhideLegs, Card::BlossomOfSpring] {
        assert!(in_zone(&gs, PlayerIndex::P2, card, Zone::Equipment), "{:?} back in its slot", card);
        assert!(!in_zone(&gs, PlayerIndex::P2, card, Zone::Graveyard));
    }
}

#[test]
fn blade_break_destroys_only_the_piece_that_has_it() {
    let mut gs = rhinar_blocks_flock_with_equipment(&[Card::BoneVizier, Card::IronhideGauntlet]);
    close_chain_by_passing(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::BoneVizier, Zone::Graveyard));
    assert!(in_zone(&gs, PlayerIndex::P2, Card::IronhideGauntlet, Zone::Equipment));
}

#[test]
fn a_destroyed_bone_vizier_cannot_defend_again() {
    let mut gs = rhinar_blocks_flock_with_equipment(&[Card::BoneVizier]);
    close_chain_by_passing(&mut gs);
    assert!(!in_zone(&gs, PlayerIndex::P2, Card::BoneVizier, Zone::Equipment));
    // Nothing anywhere offers it again: it is only in the graveyard.
    assert_eq!(count_in(&gs, PlayerIndex::P2, Card::BoneVizier, Zone::Graveyard), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Ironrot Helm (Card::IronrotHelm) and Ironrot Legs (Card::IronrotLegs)
//
// Generic head and legs equipment, 1 defense, Blade Break: each blocks for 1
// and is destroyed (to the graveyard) when the combat chain closes, if it
// defended.
//
// Rhinar (p1) attacks with Muscle Mutt (6 power) and Dorinthea (p2) defends.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea (p2) declares `equipment` (worn pieces) and nothing from hand as
/// blockers against Muscle Mutt, and combat damage resolves. Play is back in
/// Rhinar's action phase with the chain still open.
fn dorinthea_blocks_muscle_mutt_with_equipment(equipment: &[Card]) -> TestGame {
    let mut gs = dorinthea_defending_muscle_mutt();
    for &card in equipment {
        assert!(in_zone(&gs, PlayerIndex::P2, card, Zone::Equipment));
        step_card_from(&mut gs, ActionType::Defend, card, Zone::Equipment);
        assert!(!in_zone(&gs, PlayerIndex::P2, card, Zone::Equipment), "it leaves its slot to defend");
        assert!(link_cards(&gs, PlayerIndex::P2, 0).contains(&card));
    }
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done declaring
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.turn_player, PlayerIndex::P1);
    gs
}

#[test]
fn ironrot_pieces_are_1_block_generic_equipment_with_blade_break() {
    assert!(matches!(Card::IronrotHelm.data().slot, Some(crate::cards::EquipmentSlot::Head)));
    assert!(matches!(Card::IronrotLegs.data().slot, Some(crate::cards::EquipmentSlot::Legs)));
    for card in [Card::IronrotHelm, Card::IronrotLegs] {
        let data = card.data();
        assert_eq!(data.typ, CardType::Equipment, "{:?}", card);
        assert_eq!(data.card_class, CardClass::Generic, "{:?}", card);
        assert_eq!(data.defense, 1, "{:?}", card);
        assert!(data.keyword.contains(Keyword::BladeBreak), "{:?}", card);
        assert!(data.constant_effect.is_none(), "{:?}", card);
        assert!(data.ability.is_none(), "{:?}", card);
        assert!(data.defend_effect.is_none(), "{:?}", card);
    }
}

#[test]
fn dorinthea_may_defend_with_either_ironrot_piece() {
    let gs = dorinthea_defending_muscle_mutt();
    let offered = equipment_blockers(&gs);
    assert!(offered.contains(&Card::IronrotHelm));
    assert!(offered.contains(&Card::IronrotLegs));
}

#[test]
fn each_ironrot_piece_blocks_for_1() {
    let gs = dorinthea_blocks_muscle_mutt_with_equipment(&[Card::IronrotHelm]);
    assert_eq!(gs.p2.life, 20 - (6 - 1));
    let gs = dorinthea_blocks_muscle_mutt_with_equipment(&[Card::IronrotLegs]);
    assert_eq!(gs.p2.life, 20 - (6 - 1));
    let gs = dorinthea_blocks_muscle_mutt_with_equipment(&[Card::IronrotHelm, Card::IronrotLegs]);
    assert_eq!(gs.p2.life, 20 - (6 - 2));
    // Still defending until the chain closes.
    let mut on_link = link_cards(&gs, PlayerIndex::P2, 0);
    on_link.sort_by_key(|c| format!("{:?}", c));
    assert_eq!(on_link, vec![Card::IronrotHelm, Card::IronrotLegs]);
}

#[test]
fn ironrot_pieces_that_defended_go_to_the_graveyard_when_the_chain_closes() {
    let mut gs = dorinthea_blocks_muscle_mutt_with_equipment(&[Card::IronrotHelm, Card::IronrotLegs]);
    close_chain_by_passing(&mut gs);
    for card in [Card::IronrotHelm, Card::IronrotLegs] {
        assert!(!in_zone(&gs, PlayerIndex::P2, card, Zone::Equipment), "{:?} left its slot", card);
        assert_eq!(count_in(&gs, PlayerIndex::P2, card, Zone::Graveyard), 1, "{:?} destroyed", card);
    }
    // Nothing else of Dorinthea's went to the graveyard (no blockers from hand).
    assert!(!in_zone(&gs, PlayerIndex::P2, Card::GallantryGold, Zone::Graveyard));
    assert!(in_zone(&gs, PlayerIndex::P2, Card::GallantryGold, Zone::Equipment));
}

#[test]
fn only_the_ironrot_piece_that_defended_is_destroyed() {
    let mut gs = dorinthea_blocks_muscle_mutt_with_equipment(&[Card::IronrotLegs]);
    close_chain_by_passing(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::IronrotLegs, Zone::Graveyard));
    assert!(in_zone(&gs, PlayerIndex::P2, Card::IronrotHelm, Zone::Equipment));
    assert!(!in_zone(&gs, PlayerIndex::P2, Card::IronrotHelm, Zone::Graveyard));
}

#[test]
fn ironrot_pieces_that_did_not_defend_stay_in_their_slots() {
    let mut gs = dorinthea_blocks_muscle_mutt_with_equipment(&[]);
    assert_eq!(gs.p2.life, 20 - 6);
    close_chain_by_passing(&mut gs);
    for card in [Card::IronrotHelm, Card::IronrotLegs] {
        assert!(in_zone(&gs, PlayerIndex::P2, card, Zone::Equipment), "{:?} still worn", card);
        assert!(!in_zone(&gs, PlayerIndex::P2, card, Zone::Graveyard));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ironhide Gauntlet (Card::IronhideGauntlet) and Ironhide Legs
// (Card::IronhideLegs)
//
// Generic 0-defense arms and legs: "When you defend with this, you may pay
// {r}. If you do, it gains +2 block and 'When the combat chain closes, destroy
// this.'"
//
// Defending puts the trigger on the stack (the `DefendTriggers` window). As it
// resolves the defender chooses in `Phase::DefendPayment`: `Activate` the card
// on the chain to pay, or `Pass` to decline. Paying beyond the banked
// resources goes through `DefendPitch`. A defender who cannot pay is not asked.
//
// Dorinthea (p1) attacks with Flock (5 power); Rhinar (p2) defends.
// ─────────────────────────────────────────────────────────────────────────────

/// Rhinar (p2) holds `hand` with `resources` banked and defends Flock with the
/// worn `blocks`. Blockers are declared and the top defend trigger resolves,
/// so the game is either asking for the payment or has moved past it.
fn ironhide_trigger_resolving(blocks: &[Card], hand: &[Card], resources: u8) -> TestGame {
    let mut gs = rhinar_defending_flock();
    set_hand(&mut gs, PlayerIndex::P2, hand);
    gs.p2.resources = resources;
    for &card in blocks {
        step_card_from(&mut gs, ActionType::Defend, card, Zone::Equipment);
    }
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done declaring
    assert_eq!(gs.phase, Phase::DefendTriggers);
    assert_eq!(stacked_defend_triggers(&gs).len(), blocks.len());
    pass_once_each(&mut gs);
    gs
}

fn pay_for(gs: &mut TestGame, card: Card) {
    assert_eq!(gs.phase, Phase::DefendPayment);
    step_card_from(gs, ActionType::Activate, card, Zone::CombatChain);
}

fn decline(gs: &mut TestGame) {
    assert_eq!(gs.phase, Phase::DefendPayment);
    step(gs, Action{ typ: ActionType::Pass, card: None});
}

/// Run the rest of combat out with nobody acting, back to Dorinthea's action
/// phase.
fn finish_combat(gs: &mut TestGame) {
    assert_eq!(gs.phase, Phase::Reaction, "the payment is done with");
    while gs.phase != Phase::Action {
        step(gs, Action{ typ: ActionType::Pass, card: None});
    }
}

#[test]
fn ironhide_pieces_are_0_block_generic_equipment_with_a_paid_defend_trigger() {
    use crate::cards::EquipmentSlot;
    assert!(matches!(Card::IronhideGauntlet.data().slot, Some(EquipmentSlot::Arms)));
    assert!(matches!(Card::IronhideLegs.data().slot, Some(EquipmentSlot::Legs)));
    for card in [Card::IronhideGauntlet, Card::IronhideLegs] {
        let data = card.data();
        assert_eq!(data.typ, CardType::Equipment);
        assert_eq!(data.card_class, CardClass::Generic);
        assert_eq!(data.defense, 0);
        assert!(data.keyword.is_empty(), "{:?} has no Blade Break of its own", card);
        let effect = data.defend_effect.as_ref().expect("a defend trigger");
        assert!(matches!(effect, DefendEffect::PayForPlus2BlockThenDestroy));
        assert_eq!(effect.resource_cost(), 1);
    }
}

#[test]
fn defending_with_ironhide_asks_the_defender_whether_to_pay() {
    let gs = ironhide_trigger_resolving(&[Card::IronhideGauntlet], &[Card::ClearingBellowB], 0);
    assert_eq!(gs.phase, Phase::DefendPayment);
    assert_eq!(gs.active_player, PlayerIndex::P2);
    let actions = legal_actions(&gs);
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().any(|a| a.typ == ActionType::Pass));
    assert!(actions.iter().any(|a| a.typ == ActionType::Activate
        && a.card_ref().zone == Zone::CombatChain
        && card_of(&gs, a) == Card::IronhideGauntlet));
}

#[test]
fn paying_from_banked_resources_gives_plus_2_block_and_destroys_it_at_chain_close() {
    let mut gs = ironhide_trigger_resolving(&[Card::IronhideGauntlet], &[], 1);
    pay_for(&mut gs, Card::IronhideGauntlet);
    assert_eq!(gs.p2.resources, 0);
    assert_eq!(gs.p2.chain_defense_bonus[0], 2);
    assert!(gs.pending_card.is_none());

    finish_combat(&mut gs);
    assert_eq!(gs.p2.life, 20 - (5 - 2));
    // Still defending until the chain closes, then destroyed.
    assert!(link_cards(&gs, PlayerIndex::P2, 0).contains(&Card::IronhideGauntlet));
    close_chain_by_passing(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::IronhideGauntlet, Zone::Graveyard));
    assert!(!in_zone(&gs, PlayerIndex::P2, Card::IronhideGauntlet, Zone::Equipment));
}

#[test]
fn paying_with_nothing_banked_pitches_for_it() {
    let mut gs = ironhide_trigger_resolving(&[Card::IronhideLegs], &[Card::ClearingBellowB], 0);
    pay_for(&mut gs, Card::IronhideLegs);
    assert_eq!(gs.phase, Phase::DefendPitch);
    let pitches: Vec<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::Pitch)
        .map(|a| card_of(&gs, a))
        .collect();
    assert_eq!(pitches, vec![Card::ClearingBellowB]);

    step_card(&mut gs, ActionType::Pitch, Card::ClearingBellowB);
    // Pitch 3 less the cost of 1 leaves 2 floating.
    assert_eq!(gs.p2.resources, 2);
    assert_eq!(gs.p2.chain_defense_bonus[0], 2);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::ClearingBellowB, Zone::Pitch));
    finish_combat(&mut gs);
    assert_eq!(gs.p2.life, 17);
}

#[test]
fn declining_leaves_it_a_0_block_piece_that_returns_to_its_slot() {
    let mut gs = ironhide_trigger_resolving(&[Card::IronhideGauntlet], &[Card::ClearingBellowB], 1);
    decline(&mut gs);
    assert_eq!(gs.p2.resources, 1);
    assert_eq!(gs.p2.chain_defense_bonus[0], 0);
    assert!(gs.pending_card.is_none());
    finish_combat(&mut gs);
    assert_eq!(gs.p2.life, 15);
    close_chain_by_passing(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::IronhideGauntlet, Zone::Equipment));
}

#[test]
fn a_defender_who_cannot_pay_is_not_asked() {
    let mut gs = ironhide_trigger_resolving(&[Card::IronhideGauntlet], &[], 0);
    assert_eq!(gs.phase, Phase::Reaction, "straight past the payment");
    assert_eq!(gs.p2.chain_defense_bonus[0], 0);
    finish_combat(&mut gs);
    close_chain_by_passing(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P2, Card::IronhideGauntlet, Zone::Equipment));
}

#[test]
fn both_pieces_can_be_paid_for_and_both_are_destroyed() {
    let mut gs = ironhide_trigger_resolving(
        &[Card::IronhideGauntlet, Card::IronhideLegs], &[Card::ClearingBellowB], 0);
    // The first trigger to resolve: pitch for it, leaving 2 floating.
    let first = gs.pending_card.map(|p| gs.p2.card(p.card)).unwrap();
    pay_for(&mut gs, first);
    step_card(&mut gs, ActionType::Pitch, Card::ClearingBellowB);
    // Back in the trigger window with the second trigger still to resolve.
    assert_eq!(gs.phase, Phase::DefendTriggers);
    assert_eq!(stacked_defend_triggers(&gs).len(), 1);
    pass_once_each(&mut gs);
    let second = if first == Card::IronhideGauntlet { Card::IronhideLegs } else { Card::IronhideGauntlet };
    pay_for(&mut gs, second);
    assert_eq!(gs.p2.resources, 1, "the second paid from what the pitch left over");
    assert_eq!(gs.p2.chain_defense_bonus[0], 4);

    finish_combat(&mut gs);
    assert_eq!(gs.p2.life, 20 - (5 - 4));
    close_chain_by_passing(&mut gs);
    for card in [Card::IronhideGauntlet, Card::IronhideLegs] {
        assert!(in_zone(&gs, PlayerIndex::P2, card, Zone::Graveyard), "{:?} destroyed", card);
    }
}

#[test]
fn only_the_piece_paid_for_is_destroyed() {
    let mut gs = ironhide_trigger_resolving(
        &[Card::IronhideGauntlet, Card::IronhideLegs], &[], 1);
    let first = gs.pending_card.map(|p| gs.p2.card(p.card)).unwrap();
    pay_for(&mut gs, first);
    pass_once_each(&mut gs);
    // Nothing left to pay with, so the second is not even offered.
    assert_eq!(gs.phase, Phase::Reaction);
    finish_combat(&mut gs);
    close_chain_by_passing(&mut gs);
    let other = if first == Card::IronhideGauntlet { Card::IronhideLegs } else { Card::IronhideGauntlet };
    assert!(in_zone(&gs, PlayerIndex::P2, first, Zone::Graveyard));
    assert!(in_zone(&gs, PlayerIndex::P2, other, Zone::Equipment));
}

#[test]
fn the_destroy_mark_does_not_outlive_the_chain() {
    let mut gs = ironhide_trigger_resolving(&[Card::IronhideGauntlet], &[], 1);
    pay_for(&mut gs, Card::IronhideGauntlet);
    finish_combat(&mut gs);
    use crate::cards::EquipmentSlot;
    assert!(gs.p2.destroy_on_chain_close(&EquipmentSlot::Arms));
    close_chain_by_passing(&mut gs);
    assert!(!gs.p2.destroy_on_chain_close(&EquipmentSlot::Arms));
}

// ─────────────────────────────────────────────────────────────────────────────
// En Garde (Card::EnGardeR)
//
// Warrior action, cost 1, 3 block: "Your next weapon attack this turn gains +3
// power. Go again." The bonus is banked on `Player::next_weapon_attack_bonus`
// as the card resolves and folded into the next weapon swing's power at combat
// damage, which spends it. An attack action card in between leaves it banked.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea (p1) plays En Garde, pitching Visit the Blacksmith (pitch 3), and
/// it resolves. She is back in her action phase with 2 floating resources and
/// the rest of `rest` in hand.
fn play_en_garde(rest: &[Card]) -> TestGame {
    let mut gs = setup_dorinthea_action_phase();
    let mut hand = vec![Card::EnGardeR, Card::VisitTheBlacksmithB];
    hand.extend_from_slice(rest);
    set_hand(&mut gs, PlayerIndex::P1, &hand);
    gs.p1.resources = 0;
    step_card(&mut gs, ActionType::PlayCard, Card::EnGardeR);
    assert_eq!(gs.phase, Phase::ActionPitch);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    gs
}

/// Swing Dawnblade (cost 1, from the floating resources) and let it go
/// unblocked, back to Dorinthea's action phase.
fn swing_dawnblade_unblocked(gs: &mut TestGame) {
    let weapon = weapon_card(gs, PlayerIndex::P1);
    assert_eq!(weapon, Card::Dawnblade);
    step_card(gs, ActionType::Activate, weapon);
    assert_eq!(gs.phase, Phase::ActionInstant, "paid from floating resources");
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Defend);
    while gs.phase != Phase::Action {
        step(gs, Action{ typ: ActionType::Pass, card: None});
    }
}

#[test]
fn en_garde_is_a_go_again_warrior_action_banking_plus_3_for_the_next_weapon_attack() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::EnGardeR.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!(data.card_class, CardClass::Warrior);
    assert_eq!(data.cost, 1);
    assert_eq!(data.pitch, 1);
    assert_eq!(data.defense, 3);
    assert!(data.keyword.contains(Keyword::GoAgain));
    let effect = data.play_effects.first().expect("En Garde should carry an on-play effect");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::NextWeaponPower));
    assert_eq!(effect.magnitude, 3);
}

#[test]
fn en_garde_banks_the_bonus_and_keeps_the_action_point() {
    let gs = play_en_garde(&[]);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 3);
    assert_eq!(gs.p2.next_weapon_attack_bonus, 0);
    // The other banked bonuses are untouched.
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert_eq!(gs.p1.next_attack_action_bonus, 0);
    assert_eq!(gs.p1.action_points, 1, "Go again");
    assert!(in_zone(&gs, PlayerIndex::P1, Card::EnGardeR, Zone::Graveyard));
    assert_eq!(gs.p1.resources, 2);
}

#[test]
fn en_garde_pumps_the_next_dawnblade_swing_by_3() {
    let mut gs = play_en_garde(&[]);
    swing_dawnblade_unblocked(&mut gs);
    // Dawnblade's 2 + 3.
    assert_eq!(gs.p2.life, 20 - 5);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0, "spent by the swing");
}

#[test]
fn en_garde_bonus_is_not_taken_by_an_attack_action_card() {
    let mut gs = play_en_garde(&[Card::SharpenSteelR]);
    // Flock of the Feather Walkers (5 power) is not a weapon attack.
    place_attacker_on_chain(&mut gs, PlayerIndex::P1, Card::FlockOfTheFeatherWalkersR);
    let life_before = gs.p2.life;
    resolve_combat_damage(&mut gs);
    assert_eq!(gs.p2.life, life_before - 5);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 3, "still banked for a weapon attack");
}

#[test]
fn en_garde_bonus_applies_once() {
    // The bonus is handed to a swing as it is made, so these are real swings:
    // a Quicken gives the first go again, and Dorinthea an extra attack.
    let mut gs = play_en_garde(&[]);
    give_quicken(&mut gs, PlayerIndex::P1);
    // A swing takes it: 2 + 3...
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 5);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0, "spent by the swing");
    // ...and the next weapon attack gets nothing extra: 2 + 1 (second attack).
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 5 - 3);
}

#[test]
fn two_en_gardes_stack() {
    let mut gs = play_en_garde(&[Card::EnGardeR]);
    // The second copy is paid out of the 2 floating resources.
    step_card(&mut gs, ActionType::PlayCard, Card::EnGardeR);
    assert_eq!(gs.phase, Phase::ActionInstant);
    pass_once_each(&mut gs);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 6);
    assert_eq!(gs.p1.action_points, 1);
}

#[test]
fn an_unused_en_garde_bonus_is_gone_next_turn() {
    let mut gs = play_en_garde(&[]);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 3);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    while !(gs.turn_player == PlayerIndex::P1 && gs.phase == Phase::Action) {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Flock of the Feather Walkers (Card::FlockOfTheFeatherWalkersR) and Quicken
// (Card::QuickenToken)
//
// Generic attack, cost 1, 5 power: "As an additional cost to play Flock of the
// Feather Walkers, reveal a card in your hand with cost 1 or less. When you
// attack with Flock of the Feather Walkers, create a Quicken token."
//
// Quicken: "When you play an attack action card or attack with a weapon,
// destroy Quicken and the attack gains go again." Tokens live in the owner's
// `Player::tokens`, counted by `TokenCard` id, capped at `MAX_QUICKENS`.
//
// The reveal is gated like Alpha Rampage's discard: a card costing 1 or less
// must still be in hand once the pitching is done. In Dorinthea's deck only
// Driving Blade and Toughen Up (cost 2) cannot be revealed.
// ─────────────────────────────────────────────────────────────────────────────

fn flock_offered(gs: &Gamestate) -> bool {
    legal_actions(gs).iter().any(|a| {
        a.typ == ActionType::PlayCard && card_of(gs, a) == Card::FlockOfTheFeatherWalkersR
    })
}

fn quickens(gs: &Gamestate, pid: PlayerIndex) -> u8 {
    gs.player(pid).token_count(TokenCard::QuickenToken)
}

fn give_quicken(gs: &mut Gamestate, pid: PlayerIndex) {
    gs.player_mut(pid).add_token(TokenCard::QuickenToken);
}

/// Dorinthea (p1) holding `hand`, with `resources` floating, in her action phase.
fn dorinthea_holding(hand: &[Card], resources: u8) -> TestGame {
    let mut gs = setup_dorinthea_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, hand);
    gs.p1.resources = resources;
    gs
}

/// Play Flock, pitching `pitch` if given, and let it resolve onto the chain:
/// the game is in Rhinar's Defend phase.
fn attack_with_flock(gs: &mut TestGame, pitch: Option<Card>) {
    step_card(gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    if let Some(card) = pitch {
        assert_eq!(gs.phase, Phase::ActionPitch);
        step_card(gs, ActionType::Pitch, card);
    }
    assert_eq!(gs.phase, Phase::ActionInstant);
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Defend);
}

/// Rhinar declares no blockers and everyone passes until Dorinthea is back in
/// her action phase, or her turn has moved on.
fn no_blocks_back_to_action(gs: &mut TestGame) {
    while gs.phase != Phase::Action {
        step(gs, Action{ typ: ActionType::Pass, card: None});
    }
}

#[test]
fn flock_is_a_5_power_attack_with_a_reveal_cost_that_creates_quicken() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::FlockOfTheFeatherWalkersR.data();
    assert_eq!(data.typ, CardType::AttackAction);
    assert_eq!(data.card_class, CardClass::Generic);
    assert_eq!((data.cost, data.pitch, data.power, data.defense), (1, 1, 5, 2));
    assert!(data.keyword.is_empty(), "Flock has no go again of its own");
    assert!(matches!(data.additional_cost, Some(AdditionalCostType::RevealCost1OrLess)));
    let effect = data.play_effects.first().expect("Flock creates a Quicken");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::CreateQuicken));
}

#[test]
fn quicken_is_a_token_that_trades_itself_for_go_again() {
    let data = Card::QuickenToken.data();
    assert_eq!(data.typ, CardType::Token);
    assert!(matches!(data.constant_effect, Some(ConstantEffect::OnAttackDestroyForGoAgain)));
    // Tokens have their own enum, shared by every deck, and are in no deck
    // enum.
    use crate::deck_cards::{DorintheaCard, RhinarCard};
    assert_eq!(TokenCard::QuickenToken.card(), Card::QuickenToken);
    assert_eq!(TokenCard::from_id(TokenCard::QuickenToken.id()), TokenCard::QuickenToken);
    assert!(RhinarCard::from_card(Card::QuickenToken).is_none());
    assert!(DorintheaCard::from_card(Card::QuickenToken).is_none());
}

#[test]
fn flock_needs_a_card_costing_1_or_less_to_reveal() {
    // Toughen Up and Driving Blade cost 2: they can pay but not be revealed.
    let gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::DrivingBladeY], 0);
    assert!(!flock_offered(&gs));
    // Add a cost-0 card and it can be.
    let gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    assert!(flock_offered(&gs));
}

#[test]
fn the_revealed_card_cannot_also_be_the_one_pitched() {
    // Visit the Blacksmith is the only other card: it would have to pay and be
    // revealed.
    let gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::VisitTheBlacksmithB], 0);
    assert!(!flock_offered(&gs));
    // Floating resources pay instead, leaving it free to reveal.
    let gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::VisitTheBlacksmithB], 1);
    assert!(flock_offered(&gs));
    // With nothing else in hand there is nothing to reveal at all.
    let gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR], 1);
    assert!(!flock_offered(&gs));
}

#[test]
fn a_second_flock_can_be_the_revealed_card() {
    let gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::FlockOfTheFeatherWalkersR], 1);
    assert!(flock_offered(&gs));
}

#[test]
fn flock_from_the_arsenal_can_reveal_from_the_whole_hand() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR, Card::FlockOfTheFeatherWalkersR], 1);
    put_in_arsenal(&mut gs, PlayerIndex::P1, Card::FlockOfTheFeatherWalkersR);
    assert_eq!(hand_cards(&gs, PlayerIndex::P1), vec![Card::SharpenSteelR]);
    assert!(legal_actions(&gs).iter().any(|a| a.typ == ActionType::PlayCard
        && a.card_ref().zone == Zone::Arsenal));
}

#[test]
fn pitching_keeps_the_only_revealable_card_back() {
    // Sharpen Steel (pitch 1) would cover the cost, but then only Toughen Up
    // is left, which cannot be revealed. Toughen Up must pay instead.
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    assert_eq!(gs.phase, Phase::ActionPitch);
    let pitches: Vec<Card> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::Pitch)
        .map(|a| card_of(&gs, a))
        .collect();
    assert_eq!(pitches, vec![Card::ToughenUpB]);
}

#[test]
fn revealing_leaves_the_card_in_hand() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    attack_with_flock(&mut gs, Some(Card::ToughenUpB));
    assert_eq!(hand_cards(&gs, PlayerIndex::P1), vec![Card::SharpenSteelR]);
    assert!(!in_zone(&gs, PlayerIndex::P1, Card::SharpenSteelR, Zone::Graveyard));
}

#[test]
fn flock_creates_a_quicken_when_it_attacks() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    step_card(&mut gs, ActionType::Pitch, Card::ToughenUpB);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 0, "not yet: Flock is only on the stack");
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 1);
    assert_eq!(quickens(&gs, PlayerIndex::P2), 0);
}

#[test]
fn flock_without_quicken_has_no_go_again_and_its_quicken_waits_for_next_turn() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    attack_with_flock(&mut gs, Some(Card::ToughenUpB));
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 15);
    assert_eq!(gs.p1.action_points, 0);
    // The token outlives the turn.
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    while !(gs.turn_player == PlayerIndex::P1 && gs.phase == Phase::Action) {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(quickens(&gs, PlayerIndex::P1), 1);
}

#[test]
fn a_quicken_gives_flock_go_again_and_flock_makes_a_fresh_one() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    step_card(&mut gs, ActionType::Pitch, Card::ToughenUpB);
    // Playing the attack spent the Quicken already.
    assert_eq!(quickens(&gs, PlayerIndex::P1), 0);
    assert!(gs.p1.attack_go_again_bonus());
    pass_once_each(&mut gs);
    // Attacking made a new one.
    assert_eq!(quickens(&gs, PlayerIndex::P1), 1);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 15);
    assert_eq!(gs.p1.action_points, 1, "go again from the Quicken");
}

#[test]
fn a_quicken_gives_a_weapon_attack_go_again() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    step_card(&mut gs, ActionType::Activate, Card::Dawnblade);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 0);
    pass_once_each(&mut gs);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 18);
    assert_eq!(gs.p1.action_points, 1);
}

#[test]
fn a_non_attack_action_leaves_the_quicken_alone() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    step_card(&mut gs, ActionType::PlayCard, Card::SharpenSteelR);
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 1);
    assert!(!gs.p1.attack_go_again_bonus());
}

#[test]
fn the_opponents_attack_leaves_your_quicken_alone() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    give_quicken(&mut gs, PlayerIndex::P2);
    attack_with_flock(&mut gs, Some(Card::ToughenUpB));
    assert_eq!(quickens(&gs, PlayerIndex::P2), 1);
    assert!(!gs.p2.attack_go_again_bonus());
}

#[test]
fn quickens_are_capped_at_two_and_all_go_at_once() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB], 0);
    for _ in 0..3 {
        create_quicken(&mut gs, PlayerIndex::P1);
    }
    assert_eq!(quickens(&gs, PlayerIndex::P1), crate::game_state::MAX_QUICKENS);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 2);
    // A single attack sets both off.
    step_card(&mut gs, ActionType::Activate, Card::Dawnblade);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 0);
    assert!(gs.p1.attack_go_again_bonus());
}

#[test]
fn tokens_are_not_counted_among_the_players_cards() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB], 0);
    let before = gs.p1.cards_in_zones();
    give_quicken(&mut gs, PlayerIndex::P1);
    assert_eq!(gs.p1.cards_in_zones(), before);
    // The debug card-total check still passes with a token in play.
    gs.debug_check_card_totals();
}

// ─────────────────────────────────────────────────────────────────────────────
// Known hand cards (Player::hand_known)
//
// The hand's counterpart of the deck's known bit: how many copies of each hand
// card the opponent has seen. Revealing (Flock's cost) and drawing a known
// deck card add to it; any copy leaving the hand takes a known copy with it.
// ─────────────────────────────────────────────────────────────────────────────

fn known_in_hand(gs: &Gamestate, pid: PlayerIndex, card: Card) -> u8 {
    let player = gs.player(pid);
    player.hand_known_count(player.deck_id(card))
}

#[test]
fn flocks_reveal_makes_the_card_known_in_hand() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    assert_eq!(known_in_hand(&gs, PlayerIndex::P1, Card::SharpenSteelR), 0);
    attack_with_flock(&mut gs, Some(Card::ToughenUpB));
    assert_eq!(known_in_hand(&gs, PlayerIndex::P1, Card::SharpenSteelR), 1);
    assert_eq!(gs.p1.hand_known.len(), 1, "only the revealed card");
    assert!(gs.p2.hand_known.is_empty());
}

#[test]
fn a_known_card_leaving_the_hand_is_no_longer_known() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    attack_with_flock(&mut gs, Some(Card::ToughenUpB));
    no_blocks_back_to_action(&mut gs);
    // Playing the revealed Sharpen Steel takes it, and its knowledge, out.
    let id = gs.p1.deck_id(Card::SharpenSteelR);
    gs.p1.action_points = 1;
    step_card(&mut gs, ActionType::PlayCard, Card::SharpenSteelR);
    assert_eq!(gs.p1.hand_known_count(id), 0);
}

#[test]
fn losing_one_of_two_copies_forgets_the_known_one() {
    // The opponent saw one Sharpen Steel; once either copy goes they cannot
    // be sure the seen one is still there.
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR, Card::SharpenSteelR], 0);
    let id = gs.p1.deck_id(Card::SharpenSteelR);
    gs.p1.mark_known_in_hand(id);
    gs.p1.remove_from(Zone::Hand, id);
    assert_eq!(gs.p1.hand.count(id), 1);
    assert_eq!(gs.p1.hand_known_count(id), 0);
}

#[test]
fn marking_known_never_exceeds_the_copies_in_hand() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR], 0);
    let id = gs.p1.deck_id(Card::SharpenSteelR);
    gs.p1.mark_known_in_hand(id);
    gs.p1.mark_known_in_hand(id);
    assert_eq!(gs.p1.hand_known_count(id), 1);
    gs.debug_check_card_totals();
}

#[test]
fn with_two_cards_to_reveal_the_player_chooses() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR, Card::HitAndRunB], 1);
    gs.p1.action_points = 1;
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    // Paid from floating resources and on the stack, but waiting on the choice.
    assert_eq!(gs.phase, Phase::RevealChoice);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    assert_eq!(stack_top_card(&gs), Some(Card::FlockOfTheFeatherWalkersR));
    assert!(gs.pending_card.is_some());
    let offered: HashSet<Card> = legal_actions(&gs).iter()
        .map(|a| { assert_eq!(a.typ, ActionType::Reveal); card_of(&gs, a) })
        .collect();
    assert_eq!(offered, HashSet::from([Card::SharpenSteelR, Card::HitAndRunB]));

    // Choosing Hit and Run, though Sharpen Steel has the lower id.
    step_card(&mut gs, ActionType::Reveal, Card::HitAndRunB);
    assert_eq!(known_in_hand(&gs, PlayerIndex::P1, Card::HitAndRunB), 1);
    assert_eq!(known_in_hand(&gs, PlayerIndex::P1, Card::SharpenSteelR), 0);
    // The play finishes as it would have without the pause.
    assert_eq!(gs.phase, Phase::ActionInstant);
    assert!(gs.pending_card.is_none());
    assert_eq!(gs.passes, 0);
    assert_eq!(hand_cards(&gs, PlayerIndex::P1).len(), 2, "revealing leaves both in hand");
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 1);
}

#[test]
fn copies_of_one_card_are_a_single_option_and_need_no_choice() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR, Card::SharpenSteelR], 1);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    assert_eq!(gs.phase, Phase::ActionInstant, "only one card to show: revealed without asking");
    assert_eq!(known_in_hand(&gs, PlayerIndex::P1, Card::SharpenSteelR), 1);
}

#[test]
fn cards_costing_2_are_not_offered_to_reveal() {
    let mut gs = dorinthea_holding(
        &[Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR, Card::HitAndRunB, Card::ToughenUpB], 1);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    assert_eq!(gs.phase, Phase::RevealChoice);
    let offered: HashSet<Card> = legal_actions(&gs).iter().map(|a| card_of(&gs, a)).collect();
    assert_eq!(offered, HashSet::from([Card::SharpenSteelR, Card::HitAndRunB]));
}

#[test]
fn the_reveal_choice_comes_after_pitching_and_returns_to_the_instant_window() {
    let mut gs = dorinthea_holding(
        &[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR, Card::HitAndRunB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    assert_eq!(gs.phase, Phase::ActionPitch);
    step_card(&mut gs, ActionType::Pitch, Card::ToughenUpB);
    assert_eq!(gs.phase, Phase::RevealChoice);
    // The Quicken waits for the play to finish.
    assert_eq!(quickens(&gs, PlayerIndex::P1), 1);
    step_card(&mut gs, ActionType::Reveal, Card::SharpenSteelR);
    assert_eq!(gs.phase, Phase::ActionInstant);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 0);
    assert!(gs.p1.attack_go_again_bonus());
}

#[test]
fn drawing_a_known_deck_card_keeps_it_known() {
    let mut gs = dorinthea_holding(&[], 0);
    let top = gs.p1.deck.front().unwrap().card();
    gs.p1.deck.get_mut(0).unwrap().set_known(true);
    let second = gs.p1.deck.get(1).unwrap().card();
    assert!(!gs.p1.deck.get(1).unwrap().is_known());
    draw_cards(&mut gs.p1, 2);
    assert_eq!(gs.p1.hand_known_count(top), 1);
    if second != top {
        assert_eq!(gs.p1.hand_known_count(second), 0, "a hidden card stays hidden");
    }
    assert_eq!(gs.p1.hand_known.len(), 1);
}

#[test]
fn a_reset_forgets_known_hand_cards() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR], 0);
    let id = gs.p1.deck_id(Card::SharpenSteelR);
    gs.p1.mark_known_in_hand(id);
    let decks = crate::fab_game::Decklists::new(build_dorinthea_deck(), build_rhinar_deck());
    let TestGame { state, rng } = &mut gs;
    crate::fab_game::reset(state, &decks, rng, false);
    assert!(gs.p1.hand_known.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Flock of the Feather Walkers: offered only when payable
//
// Flock (cost 1) may only be offered when its costs can all be met together:
// the resource cost from floating resources plus cards pitched from hand, and
// the reveal from a card costing 1 or less that was *not* pitched. Flock never
// pays for itself (a hand copy can't be pitched for itself; an arsenal copy
// isn't in hand).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_card_to_reveal_but_nothing_else_to_pitch_is_not_enough() {
    // Sharpen Steel could be revealed, or pitched for the cost, but not both.
    let gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR], 0);
    assert!(!flock_offered(&gs));
    // A second card to pitch fixes it.
    let gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR, Card::HitAndRunB], 0);
    assert!(flock_offered(&gs));
}

#[test]
fn flock_in_the_arsenal_still_needs_both_a_pitch_and_a_reveal() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR, Card::FlockOfTheFeatherWalkersR], 0);
    put_in_arsenal(&mut gs, PlayerIndex::P1, Card::FlockOfTheFeatherWalkersR);
    assert!(!legal_actions(&gs).iter().any(|a| a.typ == ActionType::PlayCard
        && card_of(&gs, a) == Card::FlockOfTheFeatherWalkersR));
    // With a floating resource, Sharpen Steel is free to be revealed.
    gs.p1.resources = 1;
    assert!(legal_actions(&gs).iter().any(|a| a.typ == ActionType::PlayCard
        && card_of(&gs, a) == Card::FlockOfTheFeatherWalkersR));
}

/// Brute force: can `others` (the hand, less the Flock being played) pay a
/// cost of `cost` on top of `resources`, while keeping back a card costing 1
/// or less to reveal? Tries every subset of `others` as the pitched cards.
fn flock_payable_by_brute_force(others: &[Card], resources: u8, cost: u8) -> bool {
    (0u32..1 << others.len()).any(|pitched| {
        let pitch: u8 = others.iter().enumerate()
            .filter(|(i, _)| pitched & (1 << i) != 0)
            .map(|(_, c)| c.data().pitch)
            .sum();
        let revealable_left = others.iter().enumerate()
            .any(|(i, c)| pitched & (1 << i) == 0 && c.data().cost <= 1);
        resources + pitch >= cost && revealable_left
    })
}

/// Follow every legal pitch and reveal from here until the play is on the
/// stack and finished; panic if any line runs out of legal actions first.
fn every_payment_line_completes(gs: &TestGame, path: &mut Vec<Action>) {
    if !matches!(gs.phase, Phase::ActionPitch | Phase::RevealChoice) {
        assert_eq!(gs.phase, Phase::ActionInstant, "after {:?}", path);
        assert_eq!(stack_top_card(gs), Some(Card::FlockOfTheFeatherWalkersR));
        assert!(gs.pending_card.is_none());
        // The reveal was paid: some card costing 1 or less is known in hand.
        assert!(gs.p1.hand_known.iter().any(|(id, _)| gs.p1.card(id).data().cost <= 1),
            "no card was revealed after {:?}", path);
        return;
    }
    let actions = legal_actions(gs);
    assert!(!actions.is_empty(), "stuck paying for Flock after {:?}", path);
    for a in actions.iter() {
        let mut next = gs.clone();
        step(&mut next, *a);
        path.push(*a);
        every_payment_line_completes(&next, path);
        path.pop();
    }
}

#[test]
fn flock_is_offered_exactly_when_it_can_be_paid_and_never_dead_ends() {
    // One card per (cost <= 1?, pitch) class, plus a second Flock.
    let pool = [
        Card::SharpenSteelR,   // cost 0, pitch 1
        Card::OnAKnifeEdgeY,   // cost 0, pitch 2
        Card::HitAndRunB,      // cost 0, pitch 3
        Card::EnGardeR,        // cost 1, pitch 1
        Card::FlockOfTheFeatherWalkersR, // cost 1, pitch 1
        Card::DrivingBladeY,   // cost 2, pitch 2 — cannot be revealed
        Card::ToughenUpB,      // cost 2, pitch 3 — cannot be revealed
    ];
    let cost = Card::FlockOfTheFeatherWalkersR.data().cost;
    let mut checked = 0;
    let mut offered_count = 0;
    // Every multiset of up to 3 other cards from the pool.
    let mut hands: Vec<Vec<Card>> = vec![vec![]];
    for size in 1..=3 {
        fn extend(pool: &[Card], start: usize, left: usize, cur: &mut Vec<Card>, out: &mut Vec<Vec<Card>>) {
            if left == 0 { out.push(cur.clone()); return; }
            for i in start..pool.len() {
                cur.push(pool[i]);
                extend(pool, i, left - 1, cur, out);
                cur.pop();
            }
        }
        extend(&pool, 0, size, &mut vec![], &mut hands);
    }
    // A Blitz deck holds at most two copies of a card, and a zone counts at
    // most three, so skip hands with more Flocks than that.
    hands.retain(|h| h.iter().filter(|&&c| c == Card::FlockOfTheFeatherWalkersR).count() <= 1);
    for others in &hands {
        for resources in [0u8, 1] {
            for from_arsenal in [false, true] {
                let mut hand = vec![Card::FlockOfTheFeatherWalkersR];
                hand.extend_from_slice(others);
                let mut gs = dorinthea_holding(&hand, resources);
                let zone = if from_arsenal {
                    put_in_arsenal(&mut gs, PlayerIndex::P1, Card::FlockOfTheFeatherWalkersR);
                    Zone::Arsenal
                } else {
                    Zone::Hand
                };
                let flock = legal_actions(&gs).iter().copied().find(|a| {
                    a.typ == ActionType::PlayCard && a.card_ref().zone == zone
                        && card_of(&gs, a) == Card::FlockOfTheFeatherWalkersR
                });
                let expected = flock_payable_by_brute_force(others, resources, cost);
                assert_eq!(flock.is_some(), expected,
                    "hand {:?} + Flock (from {:?}), {} floating", others, zone, resources);
                if let Some(play) = flock {
                    offered_count += 1;
                    let mut next = gs.clone();
                    step(&mut next, play);
                    every_payment_line_completes(&next, &mut vec![play]);
                }
                checked += 1;
            }
        }
    }
    // Make sure the sweep really covered both outcomes.
    assert!(checked > 400 && offered_count > 0 && offered_count < checked,
        "checked {} hands, Flock offered in {}", checked, offered_count);
}

// ─────────────────────────────────────────────────────────────────────────────
// Knowledge through face-down moves: the arsenal, and Intimidate
//
// A card leaving the hand face down (arsenaled, or banished by Intimidate)
// does not show the opponent which card went, so every known hand card loses a
// known copy. The arsenal card itself is known (`Player::arsenal_known`) only
// when the hand was entirely known copies of one card.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea (p1), holding `hand` with the copies in `known` marked as seen by
/// Rhinar, passes her action phase into the Arsenal phase.
fn dorinthea_at_arsenal(hand: &[Card], known: &[Card]) -> TestGame {
    let mut gs = dorinthea_holding(hand, 0);
    for &card in known {
        let id = gs.p1.deck_id(card);
        gs.p1.mark_known_in_hand(id);
    }
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Arsenal);
    gs
}

fn arsenal(gs: &mut TestGame, card: Card) {
    step_card(gs, ActionType::Arsenal, card);
    assert_eq!(arsenal_card(gs, PlayerIndex::P1), Some(card));
}

#[test]
fn arsenaling_the_last_known_card_makes_the_arsenal_known() {
    let mut gs = dorinthea_at_arsenal(&[Card::SharpenSteelR], &[Card::SharpenSteelR]);
    arsenal(&mut gs, Card::SharpenSteelR);
    assert!(gs.p1.arsenal_known());
    assert!(!gs.p1.arsenal_face_up());
    assert_eq!(gs.p1.visibility(Zone::Arsenal), crate::game_state::CardVisibleState::BothKnow);
}

#[test]
fn arsenaling_one_of_two_known_copies_keeps_both_known() {
    let mut gs = dorinthea_at_arsenal(
        &[Card::SharpenSteelR, Card::SharpenSteelR], &[Card::SharpenSteelR, Card::SharpenSteelR]);
    arsenal(&mut gs, Card::SharpenSteelR);
    assert!(gs.p1.arsenal_known());
    // The copy left in hand is still known; the cards drawn after are not.
    assert_eq!(known_in_hand(&gs, PlayerIndex::P1, Card::SharpenSteelR), 1);
    assert_eq!(gs.p1.hand_known.len(), 1);
}

#[test]
fn an_unknown_card_in_the_arsenal_stays_hidden() {
    let mut gs = dorinthea_at_arsenal(&[Card::SharpenSteelR], &[]);
    arsenal(&mut gs, Card::SharpenSteelR);
    assert!(!gs.p1.arsenal_known());
    assert_eq!(gs.p1.visibility(Zone::Arsenal), crate::game_state::CardVisibleState::P1Knows);
}

#[test]
fn a_choice_of_cards_hides_the_arsenal_and_the_known_card_is_forgotten() {
    // Rhinar saw Sharpen Steel; Dorinthea arsenals the other card face down.
    // He cannot tell which went, so Sharpen Steel is no longer certain.
    let mut gs = dorinthea_at_arsenal(&[Card::SharpenSteelR, Card::HitAndRunB], &[Card::SharpenSteelR]);
    arsenal(&mut gs, Card::HitAndRunB);
    assert!(!gs.p1.arsenal_known());
    assert_eq!(known_in_hand(&gs, PlayerIndex::P1, Card::SharpenSteelR), 0);
    assert!(gs.p1.hand_known.is_empty());
}

#[test]
fn a_hand_of_different_known_cards_still_hides_the_arsenal() {
    let mut gs = dorinthea_at_arsenal(
        &[Card::SharpenSteelR, Card::HitAndRunB], &[Card::SharpenSteelR, Card::HitAndRunB]);
    arsenal(&mut gs, Card::SharpenSteelR);
    assert!(!gs.p1.arsenal_known());
    // Either could have gone, so neither is certain now.
    assert!(gs.p1.hand_known.is_empty());
}

#[test]
fn playing_a_known_arsenal_card_clears_the_known_flag() {
    let mut gs = dorinthea_at_arsenal(&[Card::SharpenSteelR], &[Card::SharpenSteelR]);
    arsenal(&mut gs, Card::SharpenSteelR);
    assert!(gs.p1.arsenal_known());
    while !(gs.turn_player == PlayerIndex::P1 && gs.phase == Phase::Action) {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert!(gs.p1.arsenal_known(), "still known on her next turn");
    step_card_from(&mut gs, ActionType::PlayCard, Card::SharpenSteelR, Zone::Arsenal);
    assert!(gs.p1.arsenal.is_none());
    assert!(!gs.p1.arsenal_known());
}

#[test]
fn intimidate_makes_every_known_hand_card_uncertain() {
    // Rhinar (p2) intimidates Dorinthea: a random card is banished face down.
    for seed in 0..8 {
        let mut gs = dorinthea_holding(&[Card::SharpenSteelR, Card::HitAndRunB], 0);
        let id = gs.p1.deck_id(Card::SharpenSteelR);
        gs.p1.mark_known_in_hand(id);
        gs.rng = <crate::fab_game::GameRng as rand::SeedableRng>::seed_from_u64(seed);
        let TestGame { state, rng } = &mut gs;
        apply_intimidate(state, rng, PlayerIndex::P2);
        assert_eq!(gs.p1.hand.len(), 1);
        assert!(gs.p1.hand_known.is_empty(), "whichever card was banished (seed {})", seed);
    }
}

#[test]
fn a_card_leaving_in_plain_view_keeps_the_rest_known() {
    // Playing Hit and Run shows Rhinar exactly which card left.
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR, Card::HitAndRunB], 0);
    let id = gs.p1.deck_id(Card::SharpenSteelR);
    gs.p1.mark_known_in_hand(id);
    step_card(&mut gs, ActionType::PlayCard, Card::HitAndRunB);
    assert_eq!(known_in_hand(&gs, PlayerIndex::P1, Card::SharpenSteelR), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Dorinthea, Quicksilver Prodigy (Card::Dorinthea) and Dawnblade, Resplendent
// (Card::Dawnblade)
//
// Dorinthea: "The first time Dawnblade, Resplendent gains go again each turn,
// you may attack an additional time with it this turn."
// Dawnblade: "Once per Turn Action — {r}: Attack. The second time you attack
// with Dawnblade each turn, it gains +1 power until the end of turn."
//
// Quicken is the only way Dawnblade gains go again today. After swinging,
// Dawnblade sits on the combat chain; the extra attack is activated from
// there, and Dawnblade moves on to the next chain link — the chain stays open.
// ─────────────────────────────────────────────────────────────────────────────

fn dawnblade_swing_offered(gs: &Gamestate) -> bool {
    legal_actions(gs).iter().any(|a| a.typ == ActionType::Activate && card_of(gs, a) == Card::Dawnblade)
}

/// The zone Dawnblade would be swung from now: its slot, or the chain.
fn dawnblade_zone(gs: &Gamestate) -> Zone {
    if gs.p1.weapon.is_some() { Zone::Weapon } else { Zone::CombatChain }
}

/// Swing Dawnblade (cost 1) from Dorinthea's action phase — from its slot or,
/// for the extra attack, from the chain — pitching `pitch` if the floating
/// resources fall short, and let it through unblocked back to her action phase.
fn dawnblade_unblocked(gs: &mut TestGame, pitch: Option<Card>) {
    let zone = dawnblade_zone(gs);
    step_card_from(gs, ActionType::Activate, Card::Dawnblade, zone);
    if let Some(card) = pitch {
        step_card(gs, ActionType::Pitch, card);
    }
    assert_eq!(gs.phase, Phase::ActionInstant);
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Defend);
    while gs.phase != Phase::Action && gs.turn_player == PlayerIndex::P1 {
        step(gs, Action{ typ: ActionType::Pass, card: None});
    }
}

/// Dorinthea, holding a Quicken and Visit the Blacksmith (pitch 3), swings
/// Dawnblade once: the Quicken gives it go again, so her hero ability grants
/// an extra attack. Back in her action phase with 2 floating resources.
fn dawnblade_swung_with_quicken() -> TestGame {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    gs
}

#[test]
fn dorinthea_and_dawnblade_carry_their_abilities() {
    assert!(matches!(Card::Dorinthea.data().constant_effect,
        Some(ConstantEffect::OnDawnbladeGoAgainExtraSwing)));
    let dawnblade = Card::Dawnblade.data();
    assert!(matches!(dawnblade.constant_effect, Some(ConstantEffect::SecondAttackEachTurnPlus1Power)));
    assert_eq!((dawnblade.power, dawnblade.ability.as_ref().unwrap().resource_cost()), (2, 1));
    assert!(dawnblade.ability.as_ref().unwrap().once_per_turn());
}

#[test]
fn dawnblade_gaining_go_again_grants_an_extra_attack() {
    let gs = dawnblade_swung_with_quicken();
    assert!(gs.p1.extra_weapon_attack());
    assert_eq!(gs.p2.life, 18, "the first swing is 2 power");
    assert_eq!(gs.p1.action_points, 1, "go again");
    assert_eq!(gs.p1.resources, 2);
}

#[test]
fn without_go_again_dawnblade_swings_once() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB], 0);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert!(!gs.p1.extra_weapon_attack());
    assert_eq!(gs.p1.action_points, 0);
    // Even with an action point, it has used its once per turn: not from the
    // chain, nor from its slot once the chain is broken.
    gs.p1.action_points = 1;
    assert!(!dawnblade_swing_offered(&gs));
    step(&mut gs, Action{ typ: ActionType::CloseChain, card: None});
    assert!(!dawnblade_swing_offered(&gs));
}

#[test]
fn the_extra_attack_is_swung_from_the_chain_onto_the_next_link() {
    let mut gs = dawnblade_swung_with_quicken();
    // Dawnblade is on link 0, and is offered from there.
    assert_eq!(link_cards(&gs, PlayerIndex::P1, 0), vec![Card::Dawnblade]);
    assert!(gs.p1.weapon.is_none());
    let offered: Vec<Zone> = legal_actions(&gs).iter()
        .filter(|a| a.typ == ActionType::Activate && card_of(&gs, a) == Card::Dawnblade)
        .map(|a| a.card_ref().zone)
        .collect();
    assert_eq!(offered, vec![Zone::CombatChain]);

    // Swinging moves it to the stack, then onto link 1; the chain stays open.
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::CombatChain);
    assert!(gs.p1.chain_link[0].is_empty(), "it left its first link");
    assert_eq!(stack_top_card(&gs), Some(Card::Dawnblade));
    assert!(on_stack(&gs, Card::Dawnblade));
    gs.debug_check_card_totals();
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    assert_eq!(link_cards(&gs, PlayerIndex::P1, 1), vec![Card::Dawnblade]);
    assert_eq!(gs.p1.chain_links_open, 2);
    assert_eq!(current_chain_link(&gs.p1), 1);
}

#[test]
fn the_second_swing_hits_for_3_and_the_chain_closes_with_dawnblade_back_in_its_slot() {
    let mut gs = dawnblade_swung_with_quicken();
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 18 - 3, "+1 power on the second attack");
    assert_eq!(uses_this_turn(&gs, Card::Dawnblade), 2);
    assert_eq!(gs.p1.action_points, 0, "no go again this time");
    // Passing closes the chain; Dawnblade returns to its slot from link 1.
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.p1.weapon.map(|id| gs.p1.card(id)), Some(Card::Dawnblade));
    assert_eq!(gs.p1.chain_links_open, 0);
    assert!(!gs.p1.chain_occupied());
}

#[test]
fn the_second_swing_is_blocked_on_its_own_link() {
    // Rhinar blocks the second swing; his block counts against that attack
    // only, on link 1.
    let mut gs = dawnblade_swung_with_quicken();
    set_hand(&mut gs, PlayerIndex::P2, &[Card::DodgeB, Card::ClearingBellowB]);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::CombatChain);
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    step_card(&mut gs, ActionType::Defend, Card::ClearingBellowB);
    assert_eq!(link_cards(&gs, PlayerIndex::P2, 1), vec![Card::ClearingBellowB]);
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    // 3 power against Clearing Bellow's block.
    let block = Card::ClearingBellowB.data().defense;
    assert_eq!(gs.p2.life, 18 - 3u8.saturating_sub(block));
}

#[test]
fn breaking_the_chain_first_still_works() {
    let mut gs = dawnblade_swung_with_quicken();
    step(&mut gs, Action{ typ: ActionType::CloseChain, card: None});
    assert_eq!(dawnblade_zone(&gs), Zone::Weapon);
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 18 - 3);
    assert_eq!(link_cards(&gs, PlayerIndex::P1, 0), vec![Card::Dawnblade], "a fresh chain");
}

#[test]
fn there_is_no_third_attack() {
    let mut gs = dawnblade_swung_with_quicken();
    dawnblade_unblocked(&mut gs, None);
    gs.p1.action_points = 1;
    gs.p1.resources = 1;
    assert!(!dawnblade_swing_offered(&gs));
    step(&mut gs, Action{ typ: ActionType::CloseChain, card: None});
    assert!(!dawnblade_swing_offered(&gs));
}

#[test]
fn the_extra_attack_is_granted_only_the_first_time() {
    // Two Quickens: the first swing takes both, and the grant happens once.
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    give_quicken(&mut gs, PlayerIndex::P1);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert!(gs.p1.extra_weapon_attack());
    // A Quicken on the second swing gives it go again but no third attack.
    give_quicken(&mut gs, PlayerIndex::P1);
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p1.action_points, 1, "the second swing had go again");
    assert!(!dawnblade_swing_offered(&gs));
}

#[test]
fn go_again_on_another_attack_grants_no_extra_dawnblade_attack() {
    let mut gs = dorinthea_holding(&[Card::FlockOfTheFeatherWalkersR, Card::ToughenUpB, Card::SharpenSteelR], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    attack_with_flock(&mut gs, Some(Card::ToughenUpB));
    assert!(gs.p1.attack_go_again_bonus(), "Flock gained go again");
    assert!(!gs.p1.extra_weapon_attack());
}

#[test]
fn rhinar_has_no_extra_attack_for_bone_basher() {
    let mut gs = setup_rhinar_action_phase();
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ClearingBellowB]);
    gs.p1.add_token(TokenCard::QuickenToken);
    step_card(&mut gs, ActionType::Activate, Card::BoneBasher);
    step_card(&mut gs, ActionType::Pitch, Card::ClearingBellowB);
    assert!(gs.p1.attack_go_again_bonus());
    assert!(!gs.p1.extra_weapon_attack());
    // Through combat with go again, Bone Basher on the chain is not offered.
    pass_once_each(&mut gs);
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.action_points, 1);
    assert!(!legal_actions(&gs).iter().any(|a| a.typ == ActionType::Activate
        && card_of(&gs, a) == Card::BoneBasher));
}

#[test]
fn the_extra_attack_and_the_plus_1_end_with_the_turn() {
    let mut gs = dawnblade_swung_with_quicken();
    dawnblade_unblocked(&mut gs, None);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    while !(gs.turn_player == PlayerIndex::P1 && gs.phase == Phase::Action) {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert!(!gs.p1.extra_weapon_attack());
    assert_eq!(uses_this_turn(&gs, Card::Dawnblade), 0);
    // The first swing of the new turn is back to 2 power.
    let life_before = gs.p2.life;
    set_hand(&mut gs, PlayerIndex::P1, &[Card::VisitTheBlacksmithB]);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, life_before - 2);
}

#[test]
fn the_plus_1_stacks_with_en_garde() {
    let mut gs = dawnblade_swung_with_quicken();
    gs.p1.next_weapon_attack_bonus = 3;
    dawnblade_unblocked(&mut gs, None);
    // 2 + 1 (second attack) + 3 (En Garde).
    assert_eq!(gs.p2.life, 18 - 6);
}

// ─────────────────────────────────────────────────────────────────────────────
// In the Swing (Card::InTheSwingR)
//
// Warrior attack reaction, cost 0, 3 block: "Play only if you have attacked 2
// or more times with weapons this turn. Target weapon attack gains +3 power."
// The attack it is played into counts towards the two: weapon swings are
// counted in `Player::abilities_used` as they are committed. The +3 is banked
// on `attack_power_bonus` as it resolves and spent at combat damage.
// ─────────────────────────────────────────────────────────────────────────────

/// Whether In the Swing is offered as a play right now.
fn in_the_swing_offered(gs: &Gamestate) -> bool {
    playable_cards(gs).contains(&Card::InTheSwingR)
}

/// From Dorinthea's action phase, swing Dawnblade (from its slot or the chain)
/// and let Rhinar decline to block: the game is in the reaction step with
/// Dorinthea, the attacker, holding priority.
fn dawnblade_into_reaction_step(gs: &mut TestGame, pitch: Option<Card>) {
    let zone = dawnblade_zone(gs);
    step_card_from(gs, ActionType::Activate, Card::Dawnblade, zone);
    if let Some(card) = pitch {
        step_card(gs, ActionType::Pitch, card);
    }
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Defend);
    step(gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
}

/// Dorinthea holds In the Swing, and Dawnblade has gained go again from a
/// Quicken on its first swing; she swings it the second time and Rhinar does
/// not block. In the reaction step with Dorinthea holding priority.
fn second_dawnblade_swing_reaction_step() -> TestGame {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::InTheSwingR], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 18);
    dawnblade_into_reaction_step(&mut gs, None);
    assert_eq!(gs.p1.weapon_attacks_this_turn(), 2);
    gs
}

#[test]
fn in_the_swing_is_a_free_attack_reaction_that_pumps_a_weapon_attack_by_3() {
    use crate::card_effects::{AttackEffectType, AttackKind, OnPlayConditionType, PlayCondition};
    let data = Card::InTheSwingR.data();
    assert_eq!(data.typ, CardType::AttackReaction);
    assert_eq!((data.cost, data.pitch, data.defense), (0, 1, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    assert!(matches!(data.play_condition, Some(PlayCondition::Played2WeaponAttacks)));
    let effect = data.attack_effect.as_ref().expect("In the Swing should carry an attack effect");
    assert!(matches!(effect.attack, AttackKind::Weapon));
    assert!(effect.targeted, "\"target weapon attack\"");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effect, AttackEffectType::GainPower));
    assert_eq!(effect.magnitude, 3);
    assert!(data.play_effects.is_empty());
}

#[test]
fn in_the_swing_is_not_offered_on_the_first_weapon_attack() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::InTheSwingR], 0);
    dawnblade_into_reaction_step(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p1.weapon_attacks_this_turn(), 1);
    assert!(!in_the_swing_offered(&gs));
}

#[test]
fn in_the_swing_is_offered_on_the_second_weapon_attack() {
    let gs = second_dawnblade_swing_reaction_step();
    assert!(in_the_swing_offered(&gs));
}

#[test]
fn in_the_swing_gives_the_second_swing_plus_3_power() {
    let mut gs = second_dawnblade_swing_reaction_step();
    step_card(&mut gs, ActionType::PlayCard, Card::InTheSwingR);
    assert!(on_stack(&gs, Card::InTheSwingR));
    pass_once_each(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P1, Card::InTheSwingR, Zone::Graveyard));
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    // 2 + 1 (Dawnblade's second attack) + 3 (In the Swing).
    assert_eq!(gs.p2.life, 18 - 6);
    assert_eq!(gs.p1.attack_power_bonus, 0, "the bonus is spent with the attack");
}

#[test]
fn in_the_swing_needs_a_weapon_attack_to_target() {
    // Two weapon attacks made this turn, but the current attack is Flock of
    // the Feather Walkers, an attack action card: nothing for In the Swing to
    // target.
    let mut gs = rhinar_defending_flock();
    let dawnblade = gs.p1.deck_id(Card::Dawnblade);
    gs.p1.abilities_used.add(dawnblade);
    gs.p1.abilities_used.add(dawnblade);
    assert_eq!(gs.p1.weapon_attacks_this_turn(), 2);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::InTheSwingR]);
    assert!(!in_the_swing_offered(&gs));
    // The same attack restaged as a second Dawnblade swing can be targeted.
    stage_second_weapon_swing(&mut gs, PlayerIndex::P1);
    assert!(in_the_swing_offered(&gs));
}

#[test]
fn in_the_swing_is_not_offered_to_the_defender() {
    // Rhinar, defending against the second swing, holds In the Swing (relabelled
    // from his hand): an attack reaction is never the defender's to play.
    let mut gs = second_dawnblade_swing_reaction_step();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.active_player, PlayerIndex::P2);
    assert!(!in_the_swing_offered(&gs));
}

// ─────────────────────────────────────────────────────────────────────────────
// Ironsong Response (Card::IronsongResponseR)
//
// Warrior attack reaction, cost 0, 3 block: "Reprise - If the defending hero
// has defended with a card from their hand this chain link, your weapon attack
// gains +3 power." Not a target, so it is playable into any attack; the +3
// needs both the Reprise condition (the defender's
// `PlayerFlags::DEFENDED_FROM_HAND`) and a weapon swing as the current attack,
// read as the card resolves.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea (p1), holding Ironsong Response and Visit the Blacksmith, swings
/// Dawnblade (pitching Visit) and Rhinar (p2) declares `blockers` — from hand,
/// or from an equipment slot when he is wearing it. In the reaction step with
/// Dorinthea holding priority.
fn ironsong_vs_dawnblade(blockers: &[Card]) -> TestGame {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::IronsongResponseR], 0);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB, Card::DodgeB]);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    for &card in blockers {
        let zone = if in_zone(&gs, PlayerIndex::P2, card, Zone::Equipment) { Zone::Equipment } else { Zone::Hand };
        step_card_from(&mut gs, ActionType::Defend, card, zone);
    }
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done declaring
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    gs
}

/// Dorinthea plays Ironsong Response and play runs on to her action phase.
fn play_ironsong_through_damage(gs: &mut TestGame) {
    step_card(gs, ActionType::PlayCard, Card::IronsongResponseR);
    while gs.phase != Phase::Action {
        step(gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert!(in_zone(gs, PlayerIndex::P1, Card::IronsongResponseR, Zone::Graveyard));
    assert_eq!(gs.p1.attack_power_bonus, 0, "the bonus never outlives the attack");
}

#[test]
fn ironsong_response_is_a_free_reprise_attack_reaction_pumping_a_weapon_attack_by_3() {
    use crate::card_effects::{AttackEffectType, AttackKind, OnPlayConditionType};
    let data = Card::IronsongResponseR.data();
    assert_eq!(data.typ, CardType::AttackReaction);
    assert_eq!((data.cost, data.pitch, data.defense), (0, 1, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    assert!(data.keyword.contains(Keyword::Reprise));
    let effect = data.attack_effect.as_ref().expect("Ironsong Response should carry an attack effect");
    assert!(matches!(effect.attack, AttackKind::Weapon));
    assert!(!effect.targeted, "\"your weapon attack\" is not a target");
    assert!(matches!(effect.condition, OnPlayConditionType::Reprise));
    assert!(matches!(effect.effect, AttackEffectType::GainPower));
    assert_eq!(effect.magnitude, 3);
    assert!(data.play_condition.is_none() && data.play_effects.is_empty());
}

#[test]
fn ironsong_response_pumps_a_weapon_attack_blocked_from_hand() {
    let mut gs = ironsong_vs_dawnblade(&[Card::ClearingBellowB]);
    assert!(gs.p2.defended_from_hand());
    assert!(playable_cards(&gs).contains(&Card::IronsongResponseR));
    play_ironsong_through_damage(&mut gs);
    // 2 + 3 against Clearing Bellow's 3.
    assert_eq!(gs.p2.life, 20 - 2);
}

#[test]
fn ironsong_response_does_nothing_against_an_unblocked_attack() {
    let mut gs = ironsong_vs_dawnblade(&[]);
    assert!(!gs.p2.defended_from_hand());
    // Still playable: Reprise is checked as it resolves, not to play it.
    assert!(playable_cards(&gs).contains(&Card::IronsongResponseR));
    play_ironsong_through_damage(&mut gs);
    assert_eq!(gs.p2.life, 20 - 2);
}

#[test]
fn ironsong_response_reprise_ignores_equipment_blocks() {
    let mut gs = ironsong_vs_dawnblade(&[Card::BoneVizier]);
    assert!(!gs.p2.defended_from_hand());
    play_ironsong_through_damage(&mut gs);
    // 2 against Bone Vizier's 1.
    assert_eq!(gs.p2.life, 20 - 1);
}

#[test]
fn ironsong_response_does_nothing_for_an_attack_action_card() {
    // Rhinar blocks Flock of the Feather Walkers (5 power) from hand: Reprise
    // is met, but the attack is not a weapon attack.
    let mut gs = rhinar_defending_flock();
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB]);
    step_card(&mut gs, ActionType::Defend, Card::ClearingBellowB);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    assert_eq!(gs.phase, Phase::Reaction);
    assert!(gs.p2.defended_from_hand());
    set_hand(&mut gs, PlayerIndex::P1, &[Card::IronsongResponseR]);
    play_ironsong_through_damage(&mut gs);
    assert_eq!(gs.p2.life, 20 - (5 - 3));
}

/// Against an unblocked Dawnblade swing, Dorinthea passes her first priority,
/// Rhinar plays Dodge from `zone` (hand or arsenal), and it resolves onto the
/// link. The step stays open with Dorinthea holding priority again.
fn dodge_from(zone: Zone) -> TestGame {
    let mut gs = ironsong_vs_dawnblade(&[]);
    if zone == Zone::Arsenal {
        set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB]);
        put_in_arsenal(&mut gs, PlayerIndex::P2, Card::DodgeB);
    }
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // Dorinthea
    step_card_from(&mut gs, ActionType::PlayCard, Card::DodgeB, zone);
    pass_once_each(&mut gs);
    assert!(link_cards(&gs, PlayerIndex::P2, 0).contains(&Card::DodgeB));
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1, "the attacker answers the defense reaction");
    gs
}

#[test]
fn ironsong_response_answers_a_dodge_from_hand_with_plus_3() {
    let mut gs = dodge_from(Zone::Hand);
    assert!(gs.p2.defended_from_hand());
    assert!(playable_cards(&gs).contains(&Card::IronsongResponseR));
    play_ironsong_through_damage(&mut gs);
    // 2 + 3 against Dodge's 2.
    assert_eq!(gs.p2.life, 20 - 3);
}

#[test]
fn ironsong_response_gets_no_reprise_from_a_dodge_out_of_the_arsenal() {
    let mut gs = dodge_from(Zone::Arsenal);
    assert!(!gs.p2.defended_from_hand());
    play_ironsong_through_damage(&mut gs);
    // 2 against Dodge's 2.
    assert_eq!(gs.p2.life, 20);
}

#[test]
fn ironsong_response_played_before_the_dodge_resolves_gets_no_reprise() {
    // Answering Dodge while it is still on the stack: Ironsong resolves first,
    // before Dodge is defending.
    let mut gs = ironsong_vs_dawnblade(&[]);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // Dorinthea
    step_card(&mut gs, ActionType::PlayCard, Card::DodgeB);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // Rhinar
    assert_eq!(gs.active_player, PlayerIndex::P1);
    assert!(on_stack(&gs, Card::DodgeB));
    play_ironsong_through_damage(&mut gs);
    assert!(link_cards(&gs, PlayerIndex::P2, 0).contains(&Card::DodgeB));
    // 2 against Dodge's 2.
    assert_eq!(gs.p2.life, 20);
}

#[test]
fn reprise_is_reset_for_each_chain_link() {
    // The first swing is blocked from hand; the second, from the chain, is
    // not. Ironsong Response on the second swing gets no Reprise.
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::IronsongResponseR], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB]);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(&mut gs);
    step_card(&mut gs, ActionType::Defend, Card::ClearingBellowB);
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert!(gs.p2.defended_from_hand(), "still the first link");
    assert_eq!(gs.p2.life, 20, "2 power against a 3 block");
    dawnblade_into_reaction_step(&mut gs, None);
    assert!(!gs.p2.defended_from_hand(), "the second swing opened a fresh link");
    play_ironsong_through_damage(&mut gs);
    // 2 + 1 (Dawnblade's second attack), no +3.
    assert_eq!(gs.p2.life, 20 - 3);
}

#[test]
fn reprise_is_cleared_as_the_chain_closes() {
    let mut gs = ironsong_vs_dawnblade(&[Card::ClearingBellowB]);
    play_ironsong_through_damage(&mut gs);
    assert!(gs.p2.defended_from_hand());
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // closes the chain
    assert!(!gs.p2.defended_from_hand());
}

#[test]
fn a_sword_attack_effect_applies_to_dawnblade_but_not_bone_basher() {
    use crate::card_effects::{AttackEffect, AttackEffectType, AttackKind, OnPlayConditionType};
    let effect = |attack| AttackEffect {
        attack,
        targeted: true,
        condition: OnPlayConditionType::Always,
        effect: AttackEffectType::GainPower,
        magnitude: 1,
    };
    let (sword, weapon) = (effect(AttackKind::Sword), effect(AttackKind::Weapon));

    // Dawnblade (a two-handed sword) swinging on link 0.
    let mut gs = setup_dorinthea_action_phase();
    move_to_chain(&mut gs, PlayerIndex::P1, Zone::Weapon, Card::Dawnblade, 0);
    assert!(affected_attack(&gs.p1, &sword).is_some());
    assert!(affected_attack(&gs.p1, &weapon).is_some());

    // Bone Basher (a two-handed club) is a weapon attack, not a sword attack.
    let mut gs = setup_rhinar_action_phase();
    move_to_chain(&mut gs, PlayerIndex::P1, Zone::Weapon, Card::BoneBasher, 0);
    assert!(affected_attack(&gs.p1, &sword).is_none());
    assert!(affected_attack(&gs.p1, &weapon).is_some());

    // An attack action card is neither.
    let gs = rhinar_defending_flock();
    assert!(affected_attack(&gs.p1, &sword).is_none());
    assert!(affected_attack(&gs.p1, &weapon).is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// Second Swing (Card::SecondSwingR)
//
// Warrior action, cost 1, 3 block: "If you have attacked with a weapon this
// turn, your next attack this turn gains +4 power. Go again." The condition is
// read as the card resolves; the +4 is banked on `Player::next_attack_bonus`
// and spent by the next attack of any kind to reach combat damage.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea (p1) holding `rest` plus Visit the Blacksmith (pitch 3) and a
/// Quicken swings Dawnblade unblocked, pitching Visit: 2 damage, go again, 2
/// resources floating, back in her action phase.
fn after_a_dawnblade_swing(rest: &[Card]) -> TestGame {
    let mut hand = vec![Card::VisitTheBlacksmithB];
    hand.extend_from_slice(rest);
    let mut gs = dorinthea_holding(&hand, 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 18);
    assert_eq!((gs.p1.action_points, gs.p1.resources), (1, 2));
    gs
}

/// Dorinthea plays Second Swing out of floating resources and it resolves.
fn play_second_swing(gs: &mut TestGame) {
    step_card(gs, ActionType::PlayCard, Card::SecondSwingR);
    assert_eq!(gs.phase, Phase::ActionInstant);
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Action);
    assert!(in_zone(gs, PlayerIndex::P1, Card::SecondSwingR, Zone::Graveyard));
}

#[test]
fn second_swing_is_a_go_again_action_that_banks_4_for_the_next_attack() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::SecondSwingR.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!((data.cost, data.pitch, data.defense), (1, 1, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    assert!(data.keyword.contains(Keyword::GoAgain));
    let effect = data.play_effects.first().expect("Second Swing should carry an on-play effect");
    assert!(matches!(effect.condition, OnPlayConditionType::HasAttackedWithWeapon));
    assert!(matches!(effect.effectType, OnPlayEffectType::NextAttackPower));
    assert_eq!(effect.magnitude, 4);
    assert!(data.attack_effect.is_none() && data.play_condition.is_none());
}

#[test]
fn second_swing_before_any_weapon_attack_banks_nothing() {
    let mut gs = dorinthea_holding(&[Card::SecondSwingR, Card::VisitTheBlacksmithB], 0);
    step_card(&mut gs, ActionType::PlayCard, Card::SecondSwingR);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.p1.next_attack_bonus, 0);
    assert_eq!(gs.p1.action_points, 1, "go again all the same");
    // The swing that follows is a plain 2.
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2);
}

#[test]
fn second_swing_after_a_weapon_attack_pumps_the_next_swing_by_4() {
    let mut gs = after_a_dawnblade_swing(&[Card::SecondSwingR]);
    play_second_swing(&mut gs);
    assert_eq!(gs.p1.next_attack_bonus, 4);
    assert_eq!(gs.p1.action_points, 1, "go again");
    // Dawnblade's extra attack: 2 + 1 (second attack) + 4.
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 18 - 7);
    assert_eq!(gs.p1.next_attack_bonus, 0, "spent by that attack");
}

#[test]
fn second_swing_pumps_an_attack_action_card_too() {
    // After the swing, Second Swing then Flock of the Feather Walkers (5),
    // revealing Sharpen Steel for Flock's cost.
    let mut gs = after_a_dawnblade_swing(&[
        Card::SecondSwingR, Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR,
    ]);
    play_second_swing(&mut gs);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    if gs.phase == Phase::RevealChoice {
        step_card(&mut gs, ActionType::Reveal, Card::SharpenSteelR);
    }
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 18 - (5 + 4));
    assert_eq!(gs.p1.next_attack_bonus, 0);
}

#[test]
fn second_swings_bonus_does_not_outlive_the_turn() {
    let mut gs = after_a_dawnblade_swing(&[Card::SecondSwingR]);
    play_second_swing(&mut gs);
    assert_eq!(gs.p1.next_attack_bonus, 4);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.next_attack_bonus, 0);
}

#[test]
fn second_swing_played_before_both_dawnblade_swings_adds_nothing() {
    // Second Swing first (pitching Visit the Blacksmith for 3), then a
    // Quicken-powered Dawnblade swing and its extra attack. Second Swing's
    // condition failed as it resolved, and attacking afterwards does not bring
    // the +4 back: the swings hit for 2 and 2 + 1.
    let mut gs = dorinthea_holding(&[Card::SecondSwingR, Card::VisitTheBlacksmithB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    step_card(&mut gs, ActionType::PlayCard, Card::SecondSwingR);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.p1.next_attack_bonus, 0, "no weapon attack yet: nothing banked");
    assert_eq!((gs.p1.action_points, gs.p1.resources), (1, 2), "Second Swing's own go again");

    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2);
    assert!(gs.p1.extra_weapon_attack(), "the Quicken, not Second Swing, unlocks the extra swing");
    assert_eq!(gs.p1.next_attack_bonus, 0);

    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2 - 3);
    assert_eq!(uses_this_turn(&gs, Card::Dawnblade), 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Sharpen Steel (Card::SharpenSteelR)
//
// Warrior action, cost 0, 3 block: "Your next weapon attack this turn gains +3
// power. Go again." En Garde's effect at no cost: the +3 is banked on
// `Player::next_weapon_attack_bonus` and spent by the next weapon swing; an
// attack action card in between leaves it banked.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea plays Sharpen Steel (free) and it resolves.
fn play_sharpen_steel(gs: &mut TestGame) {
    step_card(gs, ActionType::PlayCard, Card::SharpenSteelR);
    assert_eq!(gs.phase, Phase::ActionInstant, "free: straight to the stack");
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Action);
}

#[test]
fn sharpen_steel_is_a_free_go_again_action_banking_3_for_the_next_weapon_attack() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::SharpenSteelR.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!((data.cost, data.pitch, data.defense), (0, 1, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    assert!(data.keyword.contains(Keyword::GoAgain));
    let effect = data.play_effects.first().expect("Sharpen Steel should carry an on-play effect");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::NextWeaponPower));
    assert_eq!(effect.magnitude, 3);
    assert!(data.attack_effect.is_none() && data.play_condition.is_none());
}

#[test]
fn sharpen_steel_banks_3_and_keeps_the_action_point() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR], 0);
    assert!(playable_cards(&gs).contains(&Card::SharpenSteelR), "playable with nothing to pitch");
    play_sharpen_steel(&mut gs);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 3);
    assert_eq!(gs.p1.action_points, 1);
    assert!(in_zone(&gs, PlayerIndex::P1, Card::SharpenSteelR, Zone::Graveyard));
}

#[test]
fn sharpen_steel_pumps_the_next_dawnblade_swing_by_3() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR, Card::VisitTheBlacksmithB], 0);
    play_sharpen_steel(&mut gs);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 20 - (2 + 3));
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0, "spent by the swing");
}

#[test]
fn sharpen_steel_bonus_is_left_banked_by_an_attack_action_card() {
    // Sharpen Steel, then Flock of the Feather Walkers (pitching Visit the
    // Blacksmith, revealing the second Sharpen Steel): Flock hits for its own 5
    // and the +3 waits for a weapon attack.
    let mut gs = dorinthea_holding(&[
        Card::SharpenSteelR, Card::SharpenSteelR,
        Card::FlockOfTheFeatherWalkersR, Card::VisitTheBlacksmithB,
    ], 0);
    play_sharpen_steel(&mut gs);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    if gs.phase == Phase::RevealChoice {
        step_card(&mut gs, ActionType::Reveal, Card::SharpenSteelR);
    }
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - 5);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 3);
}

#[test]
fn two_sharpen_steels_stack() {
    let mut gs = dorinthea_holding(&[
        Card::SharpenSteelR, Card::SharpenSteelR, Card::VisitTheBlacksmithB,
    ], 0);
    play_sharpen_steel(&mut gs);
    play_sharpen_steel(&mut gs);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 6);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 20 - (2 + 6));
}

#[test]
fn sharpen_steel_bonus_does_not_outlive_the_turn() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR], 0);
    play_sharpen_steel(&mut gs);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Thrust (Card::ThrustR)
//
// Generic attack reaction, cost 1, 2 block: "Target sword attack gains +3
// power." An `AttackEffect` (sword, targeted, +3): it is offered only into a
// sword attack — Dawnblade, a two-handed sword — and the +3 is banked on
// `attack_power_bonus` as it resolves. (Bone Basher, a club, is not a sword
// attack, but Rhinar's deck holds no Thrust; the sword/club split is covered by
// `a_sword_attack_effect_applies_to_dawnblade_but_not_bone_basher`.)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn thrust_is_a_1_cost_attack_reaction_giving_a_targeted_sword_attack_plus_3() {
    use crate::card_effects::{AttackEffectType, AttackKind, OnPlayConditionType};
    let data = Card::ThrustR.data();
    assert_eq!(data.typ, CardType::AttackReaction);
    assert_eq!((data.cost, data.pitch, data.defense), (1, 1, 2));
    assert_eq!(data.card_class, CardClass::Generic);
    let effect = data.attack_effect.as_ref().expect("Thrust should carry an attack effect");
    assert!(matches!(effect.attack, AttackKind::Sword));
    assert!(effect.targeted, "\"target sword attack\"");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effect, AttackEffectType::GainPower));
    assert_eq!(effect.magnitude, 3);
    assert!(data.play_effects.is_empty() && data.play_condition.is_none());
}

#[test]
fn thrust_gives_a_dawnblade_swing_plus_3() {
    // Visit the Blacksmith pays for the swing with 2 left over, one of which
    // pays for Thrust.
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::ThrustR], 0);
    dawnblade_into_reaction_step(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert!(playable_cards(&gs).contains(&Card::ThrustR));
    step_card(&mut gs, ActionType::PlayCard, Card::ThrustR);
    assert!(on_stack(&gs, Card::ThrustR));
    assert_eq!(gs.p1.resources, 1);
    pass_once_each(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P1, Card::ThrustR, Zone::Graveyard));
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - (2 + 3));
    assert_eq!(gs.p1.attack_power_bonus, 0, "spent with the attack");
}

#[test]
fn thrust_is_paid_for_by_pitching_in_the_reaction_step() {
    // Sharpen Steel (pitch 1) pays exactly for the swing; Thrust is then paid
    // by pitching Visit the Blacksmith.
    let mut gs = dorinthea_holding(&[
        Card::SharpenSteelR, Card::VisitTheBlacksmithB, Card::ThrustR,
    ], 0);
    dawnblade_into_reaction_step(&mut gs, Some(Card::SharpenSteelR));
    assert_eq!(gs.p1.resources, 0);
    assert!(playable_cards(&gs).contains(&Card::ThrustR));
    step_card(&mut gs, ActionType::PlayCard, Card::ThrustR);
    assert_eq!(gs.phase, Phase::ReactionPitch);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.p1.resources, 2);
    pass_once_each(&mut gs);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - (2 + 3));
}

#[test]
fn thrust_is_not_offered_against_an_attack_action_card() {
    let mut gs = rhinar_defending_flock();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::ThrustR, Card::VisitTheBlacksmithB]);
    assert!(!playable_cards(&gs).contains(&Card::ThrustR), "no sword attack to target");
}

#[test]
fn thrust_is_not_offered_without_the_resources_to_pay_for_it() {
    // Sharpen Steel pays for the swing and is the last card to pitch: Thrust
    // is a sword attack's to target, but cannot be paid for.
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR, Card::ThrustR], 0);
    dawnblade_into_reaction_step(&mut gs, Some(Card::SharpenSteelR));
    assert_eq!(gs.p1.resources, 0);
    assert!(!playable_cards(&gs).contains(&Card::ThrustR));
}

#[test]
fn two_thrusts_stack() {
    let mut gs = dorinthea_holding(&[
        Card::VisitTheBlacksmithB, Card::ThrustR, Card::ThrustR,
    ], 0);
    dawnblade_into_reaction_step(&mut gs, Some(Card::VisitTheBlacksmithB));
    step_card(&mut gs, ActionType::PlayCard, Card::ThrustR);
    step_card(&mut gs, ActionType::PlayCard, Card::ThrustR);
    assert_eq!(gs.p1.resources, 0);
    while gs.phase != Phase::Action {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p2.life, 20 - (2 + 3 + 3));
}

// ─────────────────────────────────────────────────────────────────────────────
// Blade Flash (Card::BladeFlashB)
//
// Generic attack reaction, cost 1, 2 block: "Target sword attack gains go
// again." Thrust's `AttackEffect` with `GainGoAgain`: offered only into a sword
// attack (Dawnblade), and the go again is granted as it resolves through
// `attack_gains_go_again`, so on Dawnblade it also sets off Dorinthea's extra
// attack. The go again belongs to that attack alone.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn blade_flash_is_a_1_cost_attack_reaction_giving_a_targeted_sword_attack_go_again() {
    use crate::card_effects::{AttackEffectType, AttackKind, OnPlayConditionType};
    let data = Card::BladeFlashB.data();
    assert_eq!(data.typ, CardType::AttackReaction);
    assert_eq!((data.cost, data.pitch, data.defense), (1, 3, 2));
    assert_eq!(data.card_class, CardClass::Generic);
    assert!(data.keyword.is_empty());
    let effect = data.attack_effect.as_ref().expect("Blade Flash should carry an attack effect");
    assert!(matches!(effect.attack, AttackKind::Sword));
    assert!(effect.targeted, "\"target sword attack\"");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effect, AttackEffectType::GainGoAgain));
    assert!(data.play_effects.is_empty() && data.play_condition.is_none());
    assert!(data.next_attack_effect.is_none());
}

#[test]
fn blade_flash_gives_a_dawnblade_swing_go_again_and_an_extra_attack() {
    // Visit the Blacksmith pays for the swing with 2 left over, one of which
    // pays for Blade Flash.
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::BladeFlashB], 0);
    dawnblade_into_reaction_step(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert!(playable_cards(&gs).contains(&Card::BladeFlashB));
    step_card(&mut gs, ActionType::PlayCard, Card::BladeFlashB);
    assert!(on_stack(&gs, Card::BladeFlashB));
    assert_eq!(gs.p1.resources, 1);
    assert!(!gs.p1.attack_go_again_bonus(), "not before it resolves");
    pass_once_each(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P1, Card::BladeFlashB, Zone::Graveyard));
    assert!(gs.p1.attack_go_again_bonus());
    assert!(gs.p1.extra_weapon_attack(), "Dawnblade gained go again");
    close_reaction_step(&mut gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.p2.life, 20 - 2, "no power from Blade Flash");
    assert_eq!(gs.p1.action_points, 1, "go again");

    // The extra swing (2 + 1) gets no go again of its own: Blade Flash's
    // belonged to the first attack.
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2 - 3);
    assert_eq!(gs.p1.action_points, 0);
}

#[test]
fn blade_flash_is_paid_for_by_pitching_in_the_reaction_step() {
    // Sharpen Steel (pitch 1) pays exactly for the swing; Blade Flash is then
    // paid by pitching Visit the Blacksmith.
    let mut gs = dorinthea_holding(&[
        Card::SharpenSteelR, Card::VisitTheBlacksmithB, Card::BladeFlashB,
    ], 0);
    dawnblade_into_reaction_step(&mut gs, Some(Card::SharpenSteelR));
    assert_eq!(gs.p1.resources, 0);
    assert!(playable_cards(&gs).contains(&Card::BladeFlashB));
    step_card(&mut gs, ActionType::PlayCard, Card::BladeFlashB);
    assert_eq!(gs.phase, Phase::ReactionPitch);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.p1.resources, 2);
    pass_once_each(&mut gs);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - 2);
    assert_eq!(gs.p1.action_points, 1);
}

#[test]
fn blade_flash_is_not_offered_against_an_attack_action_card() {
    let mut gs = rhinar_defending_flock();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::BladeFlashB, Card::VisitTheBlacksmithB]);
    assert!(!playable_cards(&gs).contains(&Card::BladeFlashB), "no sword attack to target");
}

#[test]
fn blade_flash_is_not_offered_without_the_resources_to_pay_for_it() {
    let mut gs = dorinthea_holding(&[Card::SharpenSteelR, Card::BladeFlashB], 0);
    dawnblade_into_reaction_step(&mut gs, Some(Card::SharpenSteelR));
    assert_eq!(gs.p1.resources, 0);
    assert!(!playable_cards(&gs).contains(&Card::BladeFlashB));
}

#[test]
fn blade_flash_on_the_extra_swing_gives_go_again_but_no_third_attack() {
    // A Quicken gives the first swing go again (and the extra attack); Blade
    // Flash on the second swing gives it go again too, but Dorinthea's extra
    // attack is once per turn.
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::BladeFlashB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 18);
    dawnblade_into_reaction_step(&mut gs, None);
    assert_eq!(gs.p1.weapon_attacks_this_turn(), 2);
    step_card(&mut gs, ActionType::PlayCard, Card::BladeFlashB);
    pass_once_each(&mut gs);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 18 - 3);
    assert_eq!(gs.p1.action_points, 1, "the second swing gained go again");
    assert!(!dawnblade_swing_offered(&gs), "but Dawnblade may not attack a third time");
}

// ─────────────────────────────────────────────────────────────────────────────
// Warrior's Valor (Card::WarriorsValorR)
//
// Warrior action, cost 1, 3 block: "Your next weapon attack this turn gains +3
// power and 'When this attack hits, it gains go again.' Go again." The +3 is
// En Garde's `NextWeaponPower`; the on-hit go again is a `next_attack_effect`
// banked as `PlayerFlags::NEXT_WEAPON_ON_HIT_GO_AGAIN` and read at combat
// damage. On Dawnblade, gaining go again sets off Dorinthea's extra attack.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea plays Warrior's Valor, pitching Visit the Blacksmith (2 left
/// floating), and it resolves.
fn play_warriors_valor(gs: &mut TestGame) {
    step_card(gs, ActionType::PlayCard, Card::WarriorsValorR);
    step_card(gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!((gs.p1.action_points, gs.p1.resources), (1, 2));
}

#[test]
fn warriors_valor_banks_3_and_on_hit_go_again_for_the_next_weapon_attack() {
    use crate::card_effects::{NextAttackEffectType, NextAttackType, OnPlayConditionType, OnPlayEffectType};
    let data = Card::WarriorsValorR.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!((data.cost, data.pitch, data.defense), (1, 1, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    assert!(data.keyword.contains(Keyword::GoAgain));
    let power = data.play_effects.first().expect("Warrior's Valor should carry an on-play effect");
    assert!(matches!(power.condition, OnPlayConditionType::Always));
    assert!(matches!(power.effectType, OnPlayEffectType::NextWeaponPower));
    assert_eq!(power.magnitude, 3);
    let next = data.next_attack_effect.as_ref().expect("Warrior's Valor should carry a next-attack effect");
    assert_eq!(next.attackType, NextAttackType::IsWeapon);
    assert!(matches!(next.effectType, NextAttackEffectType::GainOnHitGoAgain));

    let mut gs = dorinthea_holding(&[Card::WarriorsValorR, Card::VisitTheBlacksmithB], 0);
    play_warriors_valor(&mut gs);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 3);
    assert!(gs.p1.next_weapon_on_hit_go_again());
}

#[test]
fn a_dawnblade_hit_with_warriors_valor_gains_go_again_and_an_extra_attack() {
    let mut gs = dorinthea_holding(&[Card::WarriorsValorR, Card::VisitTheBlacksmithB], 0);
    play_warriors_valor(&mut gs);
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - (2 + 3));
    assert_eq!(gs.p1.action_points, 1, "it hit, so it gained go again");
    assert!(gs.p1.extra_weapon_attack(), "and Dorinthea may attack with it again");
    assert!(!gs.p1.next_weapon_on_hit_go_again(), "spent");

    // The extra swing: 2 + 1, and no go again this time.
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 5 - 3);
    assert_eq!(gs.p1.action_points, 0);
}

#[test]
fn a_fully_blocked_swing_with_warriors_valor_gains_no_go_again() {
    // Clearing Bellow (3) declared and Dodge (2) played: 5 block against
    // 2 + 3 power, so the attack does not hit.
    let mut gs = dorinthea_holding(&[Card::WarriorsValorR, Card::VisitTheBlacksmithB], 0);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB, Card::DodgeB]);
    play_warriors_valor(&mut gs);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    pass_once_each(&mut gs);
    step_card(&mut gs, ActionType::Defend, Card::ClearingBellowB);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done declaring
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // Dorinthea
    step_card(&mut gs, ActionType::PlayCard, Card::DodgeB);
    pass_once_each(&mut gs);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20);
    assert_eq!(gs.p1.action_points, 0, "no hit, no go again");
    assert!(!gs.p1.extra_weapon_attack());
    assert!(!gs.p1.next_weapon_on_hit_go_again(), "spent by the swing all the same");
}

#[test]
fn warriors_valor_is_left_banked_by_an_attack_action_card() {
    // Warrior's Valor, then Flock of the Feather Walkers (revealing Sharpen
    // Steel): Flock hits for its own 5 without go again, and both halves wait
    // for a weapon attack.
    let mut gs = dorinthea_holding(&[
        Card::WarriorsValorR, Card::VisitTheBlacksmithB,
        Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR,
    ], 0);
    play_warriors_valor(&mut gs);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    if gs.phase == Phase::RevealChoice {
        step_card(&mut gs, ActionType::Reveal, Card::SharpenSteelR);
    }
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - 5);
    assert_eq!(gs.p1.action_points, 0, "Flock hit, but has no go again of its own");
    assert_eq!(gs.p1.next_weapon_attack_bonus, 3);
    assert!(gs.p1.next_weapon_on_hit_go_again());
}

#[test]
fn warriors_valor_does_not_outlive_the_turn() {
    let mut gs = dorinthea_holding(&[Card::WarriorsValorR, Card::VisitTheBlacksmithB], 0);
    play_warriors_valor(&mut gs);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0);
    assert!(!gs.p1.next_weapon_on_hit_go_again());
}

// ─────────────────────────────────────────────────────────────────────────────
// Driving Blade (Card::DrivingBladeY)
//
// Warrior action, cost 2, 3 block: "Your next weapon attack this turn gains +2
// power and go again. Go again." The +2 is `NextWeaponPower`; the go again is a
// `next_attack_effect` banked as `PlayerFlags::NEXT_WEAPON_GO_AGAIN` and
// granted as the next weapon attack is made, like a Quicken — so it does not
// depend on the attack hitting, and on Dawnblade it sets off Dorinthea's extra
// attack.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea plays Driving Blade, pitching Visit the Blacksmith (1 left
/// floating), and it resolves.
fn play_driving_blade(gs: &mut TestGame) {
    step_card(gs, ActionType::PlayCard, Card::DrivingBladeY);
    step_card(gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!((gs.p1.action_points, gs.p1.resources), (1, 1));
}

#[test]
fn driving_blade_banks_2_and_go_again_for_the_next_weapon_attack() {
    use crate::card_effects::{NextAttackEffectType, NextAttackType, OnPlayConditionType, OnPlayEffectType};
    let data = Card::DrivingBladeY.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!((data.cost, data.pitch, data.defense), (2, 2, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    assert!(data.keyword.contains(Keyword::GoAgain));
    let power = data.play_effects.first().expect("Driving Blade should carry an on-play effect");
    assert!(matches!(power.condition, OnPlayConditionType::Always));
    assert!(matches!(power.effectType, OnPlayEffectType::NextWeaponPower));
    assert_eq!(power.magnitude, 2);
    let next = data.next_attack_effect.as_ref().expect("Driving Blade should carry a next-attack effect");
    assert_eq!(next.attackType, NextAttackType::IsWeapon);
    assert!(matches!(next.effectType, NextAttackEffectType::GoAgain));

    let mut gs = dorinthea_holding(&[Card::DrivingBladeY, Card::VisitTheBlacksmithB], 0);
    play_driving_blade(&mut gs);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 2);
    assert!(gs.p1.next_weapon_go_again());
}

#[test]
fn driving_blade_gives_dawnblade_go_again_and_an_extra_attack() {
    let mut gs = dorinthea_holding(&[
        Card::DrivingBladeY, Card::VisitTheBlacksmithB, Card::SharpenSteelR,
    ], 0);
    play_driving_blade(&mut gs);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    // Granted as the attack is made, before it resolves.
    assert!(gs.p1.attack_go_again_bonus());
    assert!(gs.p1.extra_weapon_attack());
    assert!(!gs.p1.next_weapon_go_again(), "spent");
    pass_once_each(&mut gs);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - (2 + 2));
    assert_eq!(gs.p1.action_points, 1);

    // The extra swing (paid with Sharpen Steel): 2 + 1, no go again.
    dawnblade_unblocked(&mut gs, Some(Card::SharpenSteelR));
    assert_eq!(gs.p2.life, 20 - 4 - 3);
    assert_eq!(gs.p1.action_points, 0);
}

#[test]
fn driving_blades_go_again_does_not_need_the_swing_to_hit() {
    // Clearing Bellow (3) declared and Dodge (2) played: 5 block against
    // 2 + 2 power. Unlike Warrior's Valor, the go again holds.
    let mut gs = dorinthea_holding(&[Card::DrivingBladeY, Card::VisitTheBlacksmithB], 0);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB, Card::DodgeB]);
    play_driving_blade(&mut gs);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    pass_once_each(&mut gs);
    step_card(&mut gs, ActionType::Defend, Card::ClearingBellowB);
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done declaring
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // Dorinthea
    step_card(&mut gs, ActionType::PlayCard, Card::DodgeB);
    pass_once_each(&mut gs);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20);
    assert_eq!(gs.p1.action_points, 1, "go again all the same");
    assert!(gs.p1.extra_weapon_attack());
}

#[test]
fn driving_blade_is_left_banked_by_an_attack_action_card() {
    // Driving Blade, then Flock of the Feather Walkers (its 1 paid from the
    // float, revealing Sharpen Steel): Flock hits for 5 without go again, and
    // both halves wait for a weapon attack.
    let mut gs = dorinthea_holding(&[
        Card::DrivingBladeY, Card::VisitTheBlacksmithB,
        Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR,
    ], 0);
    play_driving_blade(&mut gs);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    if gs.phase == Phase::RevealChoice {
        step_card(&mut gs, ActionType::Reveal, Card::SharpenSteelR);
    }
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - 5);
    assert_eq!(gs.p1.action_points, 0);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 2);
    assert!(gs.p1.next_weapon_go_again());
}

#[test]
fn driving_blade_with_a_quicken_still_grants_one_extra_attack() {
    // Both grant Dawnblade go again as it attacks: the Quicken's trigger is not
    // optional, so it is destroyed even though Driving Blade already gives go
    // again. Dorinthea's ability fires only the first time, so there is one
    // extra swing and no third.
    let mut gs = dorinthea_holding(&[
        Card::DrivingBladeY, Card::VisitTheBlacksmithB, Card::SharpenSteelR,
    ], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    play_driving_blade(&mut gs);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 1, "Driving Blade is not an attack: the Quicken waits");
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    assert_eq!(quickens(&gs, PlayerIndex::P1), 0, "the swing destroys the Quicken");
    assert!(!gs.p1.next_weapon_go_again(), "and spends Driving Blade's go again");
    pass_once_each(&mut gs);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - (2 + 2));
    assert_eq!(gs.p1.action_points, 1);
    dawnblade_unblocked(&mut gs, Some(Card::SharpenSteelR));
    assert_eq!(gs.p2.life, 20 - 4 - 3);
    assert_eq!(gs.p1.action_points, 0);
    gs.p1.action_points = 1;
    assert!(!dawnblade_swing_offered(&gs), "no third attack");
}

#[test]
fn driving_blade_does_not_outlive_the_turn() {
    let mut gs = dorinthea_holding(&[Card::DrivingBladeY, Card::VisitTheBlacksmithB], 0);
    play_driving_blade(&mut gs);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0);
    assert!(!gs.p1.next_weapon_go_again());
}

// ─────────────────────────────────────────────────────────────────────────────
// Glistening Steelblade (Card::GlisteningSteelbladeY)
//
// Warrior action, Dorinthea Specialization, cost 1, 3 block: "Your next
// Dawnblade attack this turn has go again. Whenever Dawnblade hits a hero this
// turn, put a +1 counter on it. Go again." The go again is a
// `next_attack_effect` `{ IsDawnblade, GoAgain }`, granted as the swing is made
// (so it sets off Dorinthea's extra attack, hit or not). The hit trigger is an
// on-play `DawnbladeHitCounterThisTurn`, counted per turn in
// `Player::dawnblade_hit_counter_effects`; each hit adds that many counters to
// `Player::weapon_power_counters`, which stay and add to every later swing.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea plays `copies` Glistening Steelblades, the first pitching Visit
/// the Blacksmith, and they resolve.
fn play_glistening_steelblades(gs: &mut TestGame, copies: usize) {
    for n in 0..copies {
        step_card(gs, ActionType::PlayCard, Card::GlisteningSteelbladeY);
        if n == 0 {
            step_card(gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
        }
        pass_once_each(gs);
        assert_eq!(gs.phase, Phase::Action);
    }
    assert_eq!(gs.p1.action_points, 1, "go again");
}

#[test]
fn glistening_steelblade_is_a_dorinthea_specialization_with_go_again_and_a_hit_trigger() {
    use crate::card_effects::{NextAttackEffectType, NextAttackType, OnPlayConditionType, OnPlayEffectType};
    let data = Card::GlisteningSteelbladeY.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!((data.cost, data.pitch, data.defense), (1, 2, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    assert!(data.keyword.contains(Keyword::DorintheaSpecialization));
    assert!(data.keyword.contains(Keyword::GoAgain));
    let next = data.next_attack_effect.as_ref().expect("it should carry a next-attack effect");
    assert_eq!(next.attackType, NextAttackType::IsDawnblade);
    assert!(matches!(next.effectType, NextAttackEffectType::GoAgain));
    let hit = data.play_effects.first().expect("it should carry an on-play effect");
    assert!(matches!(hit.condition, OnPlayConditionType::Always));
    assert!(matches!(hit.effectType, OnPlayEffectType::DawnbladeHitCounterThisTurn));
    assert_eq!(hit.magnitude, 1);

    let mut gs = dorinthea_holding(&[Card::GlisteningSteelbladeY, Card::VisitTheBlacksmithB], 0);
    play_glistening_steelblades(&mut gs, 1);
    assert!(gs.p1.next_dawnblade_go_again());
    assert_eq!(gs.p1.dawnblade_hit_counter_effects, 1);
    assert_eq!(gs.p1.weapon_power_counters, 0, "no counter until Dawnblade hits");
}

#[test]
fn each_dawnblade_hit_after_glistening_steelblade_adds_a_counter() {
    // Go again unlocks the extra swing. First swing: 2, then a counter. Second
    // swing: 2 + 1 (second attack) + 1 (counter), then another counter.
    let mut gs = dorinthea_holding(&[Card::GlisteningSteelbladeY, Card::VisitTheBlacksmithB], 0);
    play_glistening_steelblades(&mut gs, 1);
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2);
    assert_eq!(gs.p1.action_points, 1, "go again");
    assert!(gs.p1.extra_weapon_attack());
    assert_eq!(gs.p1.weapon_power_counters, 1);

    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2 - 4);
    assert_eq!(gs.p1.weapon_power_counters, 2);
    assert_eq!(gs.p1.action_points, 0, "the go again was for the next attack only");
}

#[test]
fn dawnblades_counters_outlast_the_turn_but_the_hit_trigger_does_not() {
    let mut gs = dorinthea_holding(&[Card::GlisteningSteelbladeY, Card::VisitTheBlacksmithB], 0);
    play_glistening_steelblades(&mut gs, 1);
    dawnblade_unblocked(&mut gs, None);
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p1.weapon_power_counters, 2);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    while !(gs.turn_player == PlayerIndex::P1 && gs.phase == Phase::Action) {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.weapon_power_counters, 2, "counters stay on Dawnblade");
    assert_eq!(gs.p1.dawnblade_hit_counter_effects, 0, "\"this turn\" is over");
    assert!(!gs.p1.next_dawnblade_go_again());

    // The next turn's first swing: 2 + 2 counters, and no new counter.
    let life_before = gs.p2.life;
    set_hand(&mut gs, PlayerIndex::P1, &[Card::VisitTheBlacksmithB]);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, life_before - 4);
    assert_eq!(gs.p1.weapon_power_counters, 2);
}

#[test]
fn a_blocked_dawnblade_swing_gets_no_counter_but_keeps_its_go_again() {
    // Clearing Bellow blocks the 2-power swing completely.
    let mut gs = dorinthea_holding(&[Card::GlisteningSteelbladeY, Card::VisitTheBlacksmithB], 0);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB]);
    play_glistening_steelblades(&mut gs, 1);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    pass_once_each(&mut gs);
    step_card(&mut gs, ActionType::Defend, Card::ClearingBellowB);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20);
    assert_eq!(gs.p1.weapon_power_counters, 0, "it did not hit");
    assert_eq!(gs.p1.action_points, 1, "go again all the same");
    assert!(gs.p1.extra_weapon_attack());
}

#[test]
fn two_glistening_steelblades_put_two_counters_on_each_hit() {
    let mut gs = dorinthea_holding(&[
        Card::GlisteningSteelbladeY, Card::GlisteningSteelbladeY, Card::VisitTheBlacksmithB,
    ], 0);
    play_glistening_steelblades(&mut gs, 2);
    assert_eq!(gs.p1.dawnblade_hit_counter_effects, 2);
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2);
    assert_eq!(gs.p1.weapon_power_counters, 2);
}

#[test]
fn glistening_steelblades_go_again_waits_for_dawnblade() {
    // Flock of the Feather Walkers in between (revealing Sharpen Steel): it
    // takes neither the go again nor a counter.
    let mut gs = dorinthea_holding(&[
        Card::GlisteningSteelbladeY, Card::VisitTheBlacksmithB,
        Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR,
    ], 0);
    play_glistening_steelblades(&mut gs, 1);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    if gs.phase == Phase::RevealChoice {
        step_card(&mut gs, ActionType::Reveal, Card::SharpenSteelR);
    }
    pass_once_each(&mut gs);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - 5);
    assert_eq!(gs.p1.action_points, 0, "Flock has no go again of its own");
    assert!(gs.p1.next_dawnblade_go_again());
    assert_eq!(gs.p1.weapon_power_counters, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// On a Knife Edge (Card::OnAKnifeEdgeY)
//
// Generic action, cost 0, 2 block: "Your next sword attack this turn gains go
// again. Go again." A `next_attack_effect` `{ IsSword, GoAgain }` on
// `PlayerFlags::NEXT_SWORD_GO_AGAIN`, granted as the next sword swing is made
// — hit or not, and on Dawnblade it sets off Dorinthea's extra attack.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea plays On a Knife Edge (free) and it resolves.
fn play_on_a_knife_edge(gs: &mut TestGame) {
    step_card(gs, ActionType::PlayCard, Card::OnAKnifeEdgeY);
    assert_eq!(gs.phase, Phase::ActionInstant, "free: straight to the stack");
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.p1.action_points, 1, "go again");
}

#[test]
fn on_a_knife_edge_banks_go_again_for_the_next_sword_attack() {
    use crate::card_effects::{NextAttackEffectType, NextAttackType};
    let data = Card::OnAKnifeEdgeY.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!((data.cost, data.pitch, data.defense), (0, 2, 2));
    assert_eq!(data.card_class, CardClass::Generic);
    assert!(data.keyword.contains(Keyword::GoAgain));
    let next = data.next_attack_effect.as_ref().expect("it should carry a next-attack effect");
    assert_eq!(next.attackType, NextAttackType::IsSword);
    assert!(matches!(next.effectType, NextAttackEffectType::GoAgain));
    assert!(data.play_effects.is_empty() && data.attack_effect.is_none());

    let mut gs = dorinthea_holding(&[Card::OnAKnifeEdgeY], 0);
    play_on_a_knife_edge(&mut gs);
    assert!(gs.p1.next_sword_go_again());
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0, "no power");
}

#[test]
fn on_a_knife_edge_gives_dawnblade_go_again_and_an_extra_attack() {
    let mut gs = dorinthea_holding(&[Card::OnAKnifeEdgeY, Card::VisitTheBlacksmithB], 0);
    play_on_a_knife_edge(&mut gs);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    assert!(gs.p1.attack_go_again_bonus(), "granted as the attack is made");
    assert!(gs.p1.extra_weapon_attack());
    assert!(!gs.p1.next_sword_go_again(), "spent");
    pass_once_each(&mut gs);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - 2);
    assert_eq!(gs.p1.action_points, 1);

    // The extra swing: 2 + 1, no go again.
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2 - 3);
    assert_eq!(gs.p1.action_points, 0);
}

#[test]
fn on_a_knife_edges_go_again_does_not_need_the_swing_to_hit() {
    let mut gs = dorinthea_holding(&[Card::OnAKnifeEdgeY, Card::VisitTheBlacksmithB], 0);
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB]);
    play_on_a_knife_edge(&mut gs);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(&mut gs);
    step_card(&mut gs, ActionType::Defend, Card::ClearingBellowB);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20, "Clearing Bellow blocks all 2");
    assert_eq!(gs.p1.action_points, 1, "go again all the same");
    assert!(gs.p1.extra_weapon_attack());
}

#[test]
fn on_a_knife_edge_is_left_banked_by_an_attack_action_card() {
    let mut gs = dorinthea_holding(&[
        Card::OnAKnifeEdgeY, Card::VisitTheBlacksmithB,
        Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR,
    ], 0);
    play_on_a_knife_edge(&mut gs);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    if gs.phase == Phase::RevealChoice {
        step_card(&mut gs, ActionType::Reveal, Card::SharpenSteelR);
    }
    pass_once_each(&mut gs);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - 5);
    assert_eq!(gs.p1.action_points, 0, "Flock is not a sword attack");
    assert!(gs.p1.next_sword_go_again());
}

#[test]
fn a_club_swing_does_not_take_a_banked_sword_go_again() {
    // Rhinar's deck holds no On a Knife Edge, so the grant is staged: Bone
    // Basher is a club, so its swing leaves it banked and gains no go again.
    let mut gs = setup_rhinar_action_phase();
    gs.p1.set_next_sword_go_again(true);
    gs.p1.resources = 2;
    step_card_from(&mut gs, ActionType::Activate, Card::BoneBasher, Zone::Weapon);
    assert!(!gs.p1.attack_go_again_bonus());
    assert!(gs.p1.next_sword_go_again(), "still waiting for a sword attack");
}

#[test]
fn on_a_knife_edge_does_not_outlive_the_turn() {
    let mut gs = dorinthea_holding(&[Card::OnAKnifeEdgeY], 0);
    play_on_a_knife_edge(&mut gs);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert!(!gs.p1.next_sword_go_again());
}

// ─────────────────────────────────────────────────────────────────────────────
// Run Through (Card::RunThroughY)
//
// Warrior attack reaction, cost 1, 3 block: "Target sword attack gains go
// again. Your next sword attack this turn gains +2 power." The go again is an
// `AttackEffect` (sword, targeted, `GainGoAgain`), so it is only offered into a
// sword attack and, on Dawnblade, sets off Dorinthea's extra attack. The +2 is
// an on-play `NextSwordPower`, banked on `next_sword_attack_bonus` and handed
// to the next sword attack as it is made — never to the attack Run Through
// was played into.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea swings Dawnblade paying with Visit the Blacksmith (2 floating),
/// Rhinar does not block, and she plays Run Through from the float and lets it
/// resolve. Still in the reaction step, 1 resource floating.
fn dawnblade_run_through(gs: &mut TestGame) {
    dawnblade_into_reaction_step(gs, Some(Card::VisitTheBlacksmithB));
    assert!(playable_cards(gs).contains(&Card::RunThroughY));
    step_card(gs, ActionType::PlayCard, Card::RunThroughY);
    assert!(on_stack(gs, Card::RunThroughY));
    assert_eq!(gs.p1.resources, 1);
    pass_once_each(gs);
    assert!(in_zone(gs, PlayerIndex::P1, Card::RunThroughY, Zone::Graveyard));
}

#[test]
fn run_through_is_a_targeted_sword_go_again_banking_2_for_the_next_sword_attack() {
    use crate::card_effects::{AttackEffectType, AttackKind, OnPlayConditionType, OnPlayEffectType};
    let data = Card::RunThroughY.data();
    assert_eq!(data.typ, CardType::AttackReaction);
    assert_eq!((data.cost, data.pitch, data.defense), (1, 2, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    let effect = data.attack_effect.as_ref().expect("Run Through should carry an attack effect");
    assert!(matches!(effect.attack, AttackKind::Sword));
    assert!(effect.targeted, "\"target sword attack\"");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effect, AttackEffectType::GainGoAgain));
    let power = data.play_effects.first().expect("Run Through should carry an on-play effect");
    assert!(matches!(power.condition, OnPlayConditionType::Always));
    assert!(matches!(power.effectType, OnPlayEffectType::NextSwordPower));
    assert_eq!(power.magnitude, 2);
    assert!(data.next_attack_effect.is_none() && data.play_condition.is_none());
}

#[test]
fn run_through_gives_dawnblade_go_again_and_an_extra_attack_with_plus_2() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::RunThroughY], 0);
    dawnblade_run_through(&mut gs);
    assert!(gs.p1.attack_go_again_bonus());
    assert!(gs.p1.extra_weapon_attack());
    assert_eq!(gs.p1.next_sword_attack_bonus, 2);
    close_reaction_step(&mut gs);
    // The +2 is for the *next* sword attack, not this one.
    assert_eq!(gs.p2.life, 20 - 2);
    assert_eq!(gs.p1.next_sword_attack_bonus, 2, "still banked");
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.p1.action_points, 1, "go again");

    // The extra swing, from the float: 2 + 1 (second attack) + 2 (Run Through).
    dawnblade_into_reaction_step(&mut gs, None);
    assert_eq!(gs.p1.next_sword_attack_bonus, 0, "spent as the attack was made");
    assert_eq!(gs.p1.attack_power_bonus, 2);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - 2 - 5);
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert_eq!(gs.p1.action_points, 0);
}

#[test]
fn run_through_is_not_offered_against_an_attack_action_card() {
    let mut gs = rhinar_defending_flock();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::RunThroughY, Card::VisitTheBlacksmithB]);
    assert!(!playable_cards(&gs).contains(&Card::RunThroughY), "no sword attack to target");
}

#[test]
fn run_throughs_bonus_is_left_banked_by_an_attack_action_card() {
    // After Run Through, Flock of the Feather Walkers (its 1 paid from the
    // float, revealing Sharpen Steel) hits for its own 5: only a sword attack
    // takes the +2.
    let mut gs = dorinthea_holding(&[
        Card::VisitTheBlacksmithB, Card::RunThroughY,
        Card::FlockOfTheFeatherWalkersR, Card::SharpenSteelR,
    ], 0);
    dawnblade_run_through(&mut gs);
    close_reaction_step(&mut gs);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    if gs.phase == Phase::RevealChoice {
        step_card(&mut gs, ActionType::Reveal, Card::SharpenSteelR);
    }
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - 2 - 5);
    assert_eq!(gs.p1.next_sword_attack_bonus, 2);
}

#[test]
fn run_through_on_the_extra_swing_grants_no_third_attack_and_its_bonus_ends_with_the_turn() {
    // Run Through on the first swing, then again on the extra one: the second
    // copy gives go again but Dorinthea's ability has fired already, so there
    // is no third swing; its +2 waits for a sword attack that never comes.
    let mut gs = dorinthea_holding(&[
        Card::VisitTheBlacksmithB, Card::RunThroughY, Card::RunThroughY, Card::SharpenSteelR,
    ], 0);
    dawnblade_run_through(&mut gs);
    close_reaction_step(&mut gs);
    dawnblade_into_reaction_step(&mut gs, None);
    step_card(&mut gs, ActionType::PlayCard, Card::RunThroughY);
    assert_eq!(gs.phase, Phase::ReactionPitch, "nothing floating");
    step_card(&mut gs, ActionType::Pitch, Card::SharpenSteelR);
    pass_once_each(&mut gs);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - 2 - 5);
    assert_eq!(gs.p1.action_points, 1, "go again");
    assert_eq!(gs.p1.next_sword_attack_bonus, 2);
    assert!(!dawnblade_swing_offered(&gs), "no third attack");
    // Pass the turn away; with two cards pitched, the turn ends on ordering
    // them to the bottom of the deck, which a pass does not answer.
    while gs.turn_player == PlayerIndex::P1 {
        let act = if gs.phase == Phase::PitchOrder {
            legal_actions(&gs)[0]
        } else {
            Action{ typ: ActionType::Pass, card: None}
        };
        step(&mut gs, act);
    }
    assert_eq!(gs.p1.next_sword_attack_bonus, 0);
}

#[test]
fn a_club_swing_does_not_take_a_banked_sword_power_bonus() {
    // Rhinar's deck holds no Run Through, so the bonus is staged: Bone Basher
    // is a club, not a sword, so its swing leaves the +2 banked.
    let mut gs = setup_rhinar_action_phase();
    gs.p1.next_sword_attack_bonus = 2;
    gs.p1.resources = 2;
    step_card_from(&mut gs, ActionType::Activate, Card::BoneBasher, Zone::Weapon);
    assert_eq!(gs.p1.attack_power_bonus, 0);
    assert_eq!(gs.p1.next_sword_attack_bonus, 2, "still waiting for a sword attack");
}

// ─────────────────────────────────────────────────────────────────────────────
// Visit the Blacksmith (Card::VisitTheBlacksmithB)
//
// Generic action, cost 0, pitch 3, 2 block: "Your next sword attack this turn
// gains +1 power. Go again." An on-play `NextSwordPower` of 1, Run Through's
// path: banked on `Player::next_sword_attack_bonus` and handed to the next
// sword attack (Dawnblade) as it is made. An attack action card in between
// leaves it banked; a club swing does too
// (`a_club_swing_does_not_take_a_banked_sword_power_bonus`).
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea plays Visit the Blacksmith (free) and it resolves.
fn play_visit_the_blacksmith(gs: &mut TestGame) {
    step_card(gs, ActionType::PlayCard, Card::VisitTheBlacksmithB);
    assert_eq!(gs.phase, Phase::ActionInstant, "free: straight to the stack");
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Action);
}

#[test]
fn visit_the_blacksmith_is_a_free_go_again_action_banking_1_for_the_next_sword_attack() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::VisitTheBlacksmithB.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!((data.cost, data.pitch, data.defense), (0, 3, 2));
    assert_eq!(data.card_class, CardClass::Generic);
    assert!(data.keyword.contains(Keyword::GoAgain));
    assert_eq!(data.play_effects.len(), 1);
    let effect = &data.play_effects[0];
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effectType, OnPlayEffectType::NextSwordPower));
    assert_eq!(effect.magnitude, 1);
    assert!(data.attack_effect.is_none() && data.play_condition.is_none());
    assert!(data.next_attack_effect.is_none(), "power only: no go again for the sword attack");
}

#[test]
fn visit_the_blacksmith_banks_1_and_keeps_the_action_point() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB], 0);
    assert!(playable_cards(&gs).contains(&Card::VisitTheBlacksmithB), "playable with nothing to pitch");
    play_visit_the_blacksmith(&mut gs);
    assert_eq!(gs.p1.next_sword_attack_bonus, 1);
    assert_eq!(gs.p1.next_weapon_attack_bonus, 0, "a sword bonus, not a weapon one");
    assert_eq!(gs.p1.action_points, 1);
    assert!(in_zone(&gs, PlayerIndex::P1, Card::VisitTheBlacksmithB, Zone::Graveyard));
}

#[test]
fn visit_the_blacksmith_pumps_the_next_dawnblade_swing_by_1() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::VisitTheBlacksmithB], 0);
    play_visit_the_blacksmith(&mut gs);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 20 - (2 + 1));
    assert_eq!(gs.p1.next_sword_attack_bonus, 0, "spent by the swing");
}

#[test]
fn visit_the_blacksmith_bonus_applies_to_one_swing_only() {
    // A Quicken gives the first swing go again, so Dorinthea's ability grants
    // a second swing (paid from the 2 left floating by the pitch). Only the
    // first takes Visit's +1; the second has just Dawnblade's own +1 for a
    // second attack this turn.
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::VisitTheBlacksmithB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    play_visit_the_blacksmith(&mut gs);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 20 - (2 + 1));
    assert_eq!(gs.p1.next_sword_attack_bonus, 0);
    assert!(gs.p1.extra_weapon_attack());
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - (2 + 1) - (2 + 1));
}

#[test]
fn visit_the_blacksmith_bonus_is_left_banked_by_an_attack_action_card() {
    // Visit the Blacksmith, then Flock of the Feather Walkers (pitching a
    // second Visit, revealing a third): Flock hits for its own 5 and the +1
    // waits for a sword attack.
    let mut gs = dorinthea_holding(&[
        Card::VisitTheBlacksmithB, Card::FlockOfTheFeatherWalkersR,
        Card::VisitTheBlacksmithB, Card::VisitTheBlacksmithB,
    ], 0);
    play_visit_the_blacksmith(&mut gs);
    step_card(&mut gs, ActionType::PlayCard, Card::FlockOfTheFeatherWalkersR);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    if gs.phase == Phase::RevealChoice {
        step_card(&mut gs, ActionType::Reveal, Card::VisitTheBlacksmithB);
    }
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    no_blocks_back_to_action(&mut gs);
    assert_eq!(gs.p2.life, 20 - 5);
    assert_eq!(gs.p1.next_sword_attack_bonus, 1);
}

#[test]
fn two_visits_to_the_blacksmith_stack() {
    let mut gs = dorinthea_holding(&[
        Card::VisitTheBlacksmithB, Card::VisitTheBlacksmithB, Card::VisitTheBlacksmithB,
    ], 0);
    play_visit_the_blacksmith(&mut gs);
    play_visit_the_blacksmith(&mut gs);
    assert_eq!(gs.p1.next_sword_attack_bonus, 2);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 20 - (2 + 2));
}

#[test]
fn visit_the_blacksmith_bonus_does_not_outlive_the_turn() {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB], 0);
    play_visit_the_blacksmith(&mut gs);
    while gs.turn_player == PlayerIndex::P1 {
        step(&mut gs, Action{ typ: ActionType::Pass, card: None});
    }
    assert_eq!(gs.p1.next_sword_attack_bonus, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Out for Blood (Card::OutForBloodY)
//
// Warrior attack reaction, cost 1, 3 block: "Target weapon attack gains +2
// power. Reprise - If the defending hero has defended with a card from their
// hand this chain link, your next attack this turn gains +1 power." The +2 is
// an `AttackEffect` (weapon, targeted); the Reprise +1 is an on-play
// `NextAttackPower` with a `Reprise` condition, banked on `next_attack_bonus`
// and handed to the next attack as it is made — not to the attack Out for
// Blood was played into.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea, holding Out for Blood and Visit the Blacksmith (and a Quicken
/// when `quicken`), swings Dawnblade pitching Visit; Rhinar declares
/// `blockers` from hand; Dorinthea plays Out for Blood from the float and it
/// resolves. Still in the reaction step, 1 resource floating.
fn out_for_blood_vs_dawnblade(blockers: &[Card], quicken: bool) -> TestGame {
    let mut gs = dorinthea_holding(&[Card::VisitTheBlacksmithB, Card::OutForBloodY], 0);
    if quicken {
        give_quicken(&mut gs, PlayerIndex::P1);
    }
    set_hand(&mut gs, PlayerIndex::P2, &[Card::ClearingBellowB]);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    pass_once_each(&mut gs);
    assert_eq!(gs.phase, Phase::Defend);
    for &card in blockers {
        step_card(&mut gs, ActionType::Defend, card);
    }
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // done declaring
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    assert!(playable_cards(&gs).contains(&Card::OutForBloodY));
    step_card(&mut gs, ActionType::PlayCard, Card::OutForBloodY);
    assert_eq!(gs.p1.resources, 1);
    pass_once_each(&mut gs);
    assert!(in_zone(&gs, PlayerIndex::P1, Card::OutForBloodY, Zone::Graveyard));
    gs
}

#[test]
fn out_for_blood_pumps_a_targeted_weapon_attack_by_2_with_a_reprise_plus_1_for_the_next_attack() {
    use crate::card_effects::{AttackEffectType, AttackKind, OnPlayConditionType, OnPlayEffectType};
    let data = Card::OutForBloodY.data();
    assert_eq!(data.typ, CardType::AttackReaction);
    assert_eq!((data.cost, data.pitch, data.defense), (1, 2, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    let effect = data.attack_effect.as_ref().expect("Out for Blood should carry an attack effect");
    assert!(matches!(effect.attack, AttackKind::Weapon));
    assert!(effect.targeted, "\"target weapon attack\"");
    assert!(matches!(effect.condition, OnPlayConditionType::Always));
    assert!(matches!(effect.effect, AttackEffectType::GainPower));
    assert_eq!(effect.magnitude, 2);
    let next = data.play_effects.first().expect("Out for Blood should carry an on-play effect");
    assert!(matches!(next.condition, OnPlayConditionType::Reprise));
    assert!(matches!(next.effectType, OnPlayEffectType::NextAttackPower));
    assert_eq!(next.magnitude, 1);
    assert!(data.next_attack_effect.is_none() && data.play_condition.is_none());
    assert!(data.keyword.is_empty(), "no go again");
}

#[test]
fn out_for_blood_gives_an_unblocked_swing_plus_2_and_no_reprise() {
    let mut gs = out_for_blood_vs_dawnblade(&[], false);
    assert!(!gs.p2.defended_from_hand());
    assert_eq!(gs.p1.attack_power_bonus, 2);
    assert_eq!(gs.p1.next_attack_bonus, 0, "no Reprise");
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - (2 + 2));
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn out_for_blood_reprise_banks_plus_1_that_the_current_attack_does_not_take() {
    // Clearing Bellow (3) from hand: Reprise. 2 + 2 against 3.
    let mut gs = out_for_blood_vs_dawnblade(&[Card::ClearingBellowB], false);
    assert!(gs.p2.defended_from_hand());
    assert_eq!(gs.p1.next_attack_bonus, 1);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - 1);
    assert_eq!(gs.p1.next_attack_bonus, 1, "still banked for the next attack");
}

#[test]
fn out_for_blood_reprise_plus_1_goes_to_the_next_attack() {
    // A Quicken gives the first swing go again and Dorinthea an extra attack:
    // 2 + 2 against Clearing Bellow's 3, then 2 + 1 (second attack) + 1.
    let mut gs = out_for_blood_vs_dawnblade(&[Card::ClearingBellowB], true);
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - 1);
    assert_eq!(gs.p1.action_points, 1, "go again from the Quicken");
    dawnblade_into_reaction_step(&mut gs, None);
    assert_eq!(gs.p1.next_attack_bonus, 0, "spent as the attack was made");
    close_reaction_step(&mut gs);
    assert_eq!(gs.p2.life, 20 - 1 - 4);
}

#[test]
fn out_for_blood_is_not_offered_against_an_attack_action_card() {
    let mut gs = rhinar_defending_flock();
    step(&mut gs, Action{ typ: ActionType::Pass, card: None}); // no blockers
    assert_eq!(gs.phase, Phase::Reaction);
    assert_eq!(gs.active_player, PlayerIndex::P1);
    set_hand(&mut gs, PlayerIndex::P1, &[Card::OutForBloodY, Card::VisitTheBlacksmithB]);
    assert!(!playable_cards(&gs).contains(&Card::OutForBloodY), "no weapon attack to target");
}

#[test]
fn out_for_blood_reprise_bonus_does_not_outlive_the_turn() {
    let mut gs = out_for_blood_vs_dawnblade(&[Card::ClearingBellowB], false);
    close_reaction_step(&mut gs);
    while gs.turn_player == PlayerIndex::P1 {
        let act = if gs.phase == Phase::PitchOrder {
            legal_actions(&gs)[0]
        } else {
            Action{ typ: ActionType::Pass, card: None}
        };
        step(&mut gs, act);
    }
    assert_eq!(gs.p1.next_attack_bonus, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Slice and Dice (Card::SliceAndDiceY)
//
// Warrior action, cost 0, 3 block: "The first time you attack with a weapon
// this turn, if it's a sword or dagger it gains +1 power. The second time you
// attack with a weapon this turn if it's a sword or dagger it gains +2 power.
// Go again." Two on-play effects, `FirstWeaponAttackPower` (+1) and
// `SecondWeaponAttackPower` (+2), bank on `Player::weapon_attack_power` for the
// turn; as each weapon attack is made it is counted, and the Nth takes the
// power banked for it onto `attack_power_bonus`.
// ─────────────────────────────────────────────────────────────────────────────

/// Dorinthea plays Slice and Dice (free) and it resolves.
fn play_slice_and_dice(gs: &mut TestGame) {
    let ap = gs.p1.action_points;
    step_card(gs, ActionType::PlayCard, Card::SliceAndDiceY);
    assert_eq!(gs.phase, Phase::ActionInstant, "free: straight to the stack");
    pass_once_each(gs);
    assert_eq!(gs.phase, Phase::Action);
    assert_eq!(gs.p1.action_points, ap, "go again");
}

#[test]
fn slice_and_dice_is_a_free_go_again_action_pumping_the_first_two_weapon_attacks() {
    use crate::card_effects::{OnPlayConditionType, OnPlayEffectType};
    let data = Card::SliceAndDiceY.data();
    assert_eq!(data.typ, CardType::Action);
    assert_eq!((data.cost, data.pitch, data.defense), (0, 2, 3));
    assert_eq!(data.card_class, CardClass::Warrior);
    assert!(data.keyword.contains(Keyword::GoAgain));
    let [first, second] = data.play_effects else {
        panic!("Slice and Dice should carry two on-play effects");
    };
    assert!(matches!(first.condition, OnPlayConditionType::Always));
    assert!(matches!(first.effectType, OnPlayEffectType::FirstWeaponAttackPower));
    assert_eq!(first.magnitude, 1);
    assert!(matches!(second.condition, OnPlayConditionType::Always));
    assert!(matches!(second.effectType, OnPlayEffectType::SecondWeaponAttackPower));
    assert_eq!(second.magnitude, 2);
    assert!(data.attack_effect.is_none() && data.next_attack_effect.is_none());

    let mut gs = dorinthea_holding(&[Card::SliceAndDiceY], 0);
    play_slice_and_dice(&mut gs);
    assert_eq!(gs.p1.weapon_attack_power, [1, 2]);
}

#[test]
fn slice_and_dice_gives_the_first_swing_plus_1_and_the_second_plus_2() {
    // A Quicken gives the first swing go again, and Dorinthea an extra one.
    let mut gs = dorinthea_holding(&[Card::SliceAndDiceY, Card::VisitTheBlacksmithB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    play_slice_and_dice(&mut gs);
    // 2 + 1.
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 20 - 3);
    // 2 + 1 (Dawnblade's second attack) + 2.
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 3 - 5);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn slice_and_dice_pumps_the_swing_as_it_is_made() {
    let mut gs = dorinthea_holding(&[Card::SliceAndDiceY, Card::VisitTheBlacksmithB], 0);
    play_slice_and_dice(&mut gs);
    step_card_from(&mut gs, ActionType::Activate, Card::Dawnblade, Zone::Weapon);
    step_card(&mut gs, ActionType::Pitch, Card::VisitTheBlacksmithB);
    assert_eq!(gs.p1.weapon_attacks_this_turn(), 1);
    assert_eq!(gs.p1.attack_power_bonus, 1);
}

#[test]
fn slice_and_dice_played_after_the_first_swing_still_pumps_the_second() {
    // The first swing has already happened, so its +1 is missed; the second
    // still takes +2.
    let mut gs = dorinthea_holding(&[Card::SliceAndDiceY, Card::VisitTheBlacksmithB], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 20 - 2);
    play_slice_and_dice(&mut gs);
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 2 - 5);
}

#[test]
fn two_slice_and_dices_stack() {
    let mut gs = dorinthea_holding(&[
        Card::SliceAndDiceY, Card::SliceAndDiceY, Card::VisitTheBlacksmithB,
    ], 0);
    give_quicken(&mut gs, PlayerIndex::P1);
    play_slice_and_dice(&mut gs);
    play_slice_and_dice(&mut gs);
    assert_eq!(gs.p1.weapon_attack_power, [2, 4]);
    // 2 + 2, then 2 + 1 + 4.
    dawnblade_unblocked(&mut gs, Some(Card::VisitTheBlacksmithB));
    assert_eq!(gs.p2.life, 20 - 4);
    dawnblade_unblocked(&mut gs, None);
    assert_eq!(gs.p2.life, 20 - 4 - 7);
}

#[test]
fn slice_and_dice_does_not_pump_a_club_swing() {
    // Rhinar's deck holds no Slice and Dice, so the effect is staged: Bone
    // Basher is a club, neither a sword nor a dagger.
    let mut gs = setup_rhinar_action_phase();
    gs.p1.weapon_attack_power = [1, 2];
    gs.p1.resources = 2;
    step_card_from(&mut gs, ActionType::Activate, Card::BoneBasher, Zone::Weapon);
    assert_eq!(gs.p1.weapon_attacks_this_turn(), 1);
    assert_eq!(gs.p1.attack_power_bonus, 0);
}

#[test]
fn slice_and_dice_does_not_outlive_the_turn() {
    let mut gs = dorinthea_holding(&[Card::SliceAndDiceY], 0);
    play_slice_and_dice(&mut gs);
    while gs.turn_player == PlayerIndex::P1 {
        let act = if gs.phase == Phase::PitchOrder {
            legal_actions(&gs)[0]
        } else {
            Action{ typ: ActionType::Pass, card: None}
        };
        step(&mut gs, act);
    }
    assert_eq!(gs.p1.weapon_attack_power, [0, 0]);
}

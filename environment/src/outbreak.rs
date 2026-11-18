//! Outbreak: Mortimer, Living Virus — his hero card and his decklist — and
//! the set's disease aura tokens (Exertion, Inertia, Bloodrot Pox, Frailty).
//!
//! Bloodrot Courier is the only Assassin card implemented so far, so the rest
//! of his deck is a placeholder built from the Generic cards of the Rhinar vs
//! Dorinthea classic battle (`classic_battles`). It is smaller than a full
//! Blitz list: 22 deck cards, four pieces of equipment and no weapon.

use crate::card_effects::ConstantEffect;
use crate::cards::{Card, CardClass, CardData, CardType, Color, Keyword, Subtype, Talent};

/// Mortimer, Living Virus — Chaos Assassin hero, 20 life, intellect 4.
/// `classic_battles::CARD_CATALOG` holds this at `Card::Mortimer`'s index.
/// His hero ability is not implemented yet.
pub const MORTIMER: CardData = CardData {
    typ: CardType::Hero,
    weapon_type: None,
    cost: 0,
    pitch: 0,
    power: 0,
    defense: 0,
    color: None,
    no_block: false,
    slot: None,
    card_class: CardClass::Assassin,
    talent: Some(Talent::Chaos),
    subtype: None,
    keyword: Keyword::empty(),
    hero_life: 20,
    hero_intellect: 4,
    constant_effect: None,
    ability: None,
    defend_effect: None,
    next_attack_effect: None,
    additional_cost: None,
    attack_effect: None,
    play_condition: None,
    play_effects: &[],
    mentor_effect: None,
};

/// Bloodrot Courier (red) — Chaos Assassin attack action, cost 2, pitch 1,
/// power 4, defense 3, go again. "When this hits a hero, create a Bloodrot
/// Pox under the attacking and the defending hero's control."
pub const BLOODROT_COURIER_R: CardData = CardData {
    typ: CardType::AttackAction,
    weapon_type: None,
    cost: 2,
    pitch: 1,
    power: 4,
    defense: 3,
    color: Some(Color::Red),
    no_block: false,
    slot: None,
    card_class: CardClass::Assassin,
    talent: Some(Talent::Chaos),
    subtype: None,
    keyword: Keyword::GoAgain,
    hero_life: 0,
    hero_intellect: 0,
    constant_effect: Some(ConstantEffect::OnHitBothHeroesCreateBloodrotPox),
    ability: None,
    defend_effect: None,
    next_attack_effect: None,
    additional_cost: None,
    attack_effect: None,
    play_condition: None,
    play_effects: &[],
    mentor_effect: None,
};

/// What the disease aura tokens share: a Generic token that cannot
/// block. Exertion has no effect implemented yet, and nothing creates any of
/// them yet.
const DISEASE_AURA_TOKEN: CardData = CardData {
    typ: CardType::Token,
    weapon_type: None,
    cost: 0,
    pitch: 0,
    power: 0,
    defense: 0,
    color: None,
    no_block: true,
    slot: None,
    card_class: CardClass::Generic,
    talent: None,
    subtype: Some(Subtype::DiseaseAura),
    keyword: Keyword::empty(),
    hero_life: 0,
    hero_intellect: 0,
    constant_effect: None,
    ability: None,
    defend_effect: None,
    next_attack_effect: None,
    additional_cost: None,
    attack_effect: None,
    play_condition: None,
    play_effects: &[],
    mentor_effect: None,
};

/// Exertion — disease aura token. `classic_battles::CARD_CATALOG` holds this
/// at `Card::ExertionToken`'s index, and likewise the two below.
pub const EXERTION_TOKEN: CardData = DISEASE_AURA_TOKEN;
/// Inertia — disease aura token. "At the beginning of your end phase,
/// destroy this. Put all cards from your hand and arsenal on the bottom of
/// your deck."
pub const INERTIA_TOKEN: CardData = CardData {
    constant_effect: Some(ConstantEffect::OnEndPhaseDestroyBottomHandAndArsenal),
    ..DISEASE_AURA_TOKEN
};
/// Bloodrot Pox — disease aura token. At the beginning of its controller's
/// end phase it is destroyed, and deals 2 damage to them unless they pay 3.
pub const BLOODROT_POX_TOKEN: CardData = CardData {
    constant_effect: Some(ConstantEffect::OnEndPhaseDestroy2DamageUnlessPay3),
    ..DISEASE_AURA_TOKEN
};

/// Frailty — disease aura token. "Your attack action cards played from
/// arsenal and weapon attacks have -1 power. At the beginning of your end
/// phase, destroy this."
pub const FRAILTY_TOKEN: CardData = CardData {
    constant_effect: Some(ConstantEffect::AttacksFromArsenalAndWeaponsMinus1Power),
    ..DISEASE_AURA_TOKEN
};

/// Mortimer's deck: the hero, four pieces of Generic equipment, two Bloodrot
/// Couriers, and as placeholders until more Assassin cards are implemented
/// two copies each of ten Generic cards that do not need a weapon. Every card
/// here must have a variant in `deck_cards::MortimerCard`.
pub fn build_mortimer_deck() -> Vec<Card> {
    vec![
        Card::Mortimer,
        Card::IronrotHelm,
        Card::BlossomOfSpring,
        Card::IronhideGauntlet,
        Card::IronrotLegs,
        // ── RED ──
        Card::BloodrotCourierR,
        Card::BloodrotCourierR,
        Card::FlockOfTheFeatherWalkersR,
        Card::FlockOfTheFeatherWalkersR,
        // ── YELLOW ──
        Card::MuscleMuttY,
        Card::MuscleMuttY,
        Card::RagingOnslaughtY,
        Card::RagingOnslaughtY,
        Card::WoundedBullY,
        Card::WoundedBullY,
        // ── BLUE ──
        Card::ComeToFightB,
        Card::ComeToFightB,
        Card::DodgeB,
        Card::DodgeB,
        Card::RallyTheRearguardB,
        Card::RallyTheRearguardB,
        Card::SigilofSolaceB,
        Card::SigilofSolaceB,
        Card::TitaniumBaubleB,
        Card::TitaniumBaubleB,
        Card::ToughenUpB,
        Card::ToughenUpB,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::fab_game::FabGame;
    use crate::simulator::Game;
    use rand::rngs::SmallRng;
    use rand::seq::IndexedRandom;
    use rand::SeedableRng;

    #[test]
    fn mortimer_is_a_chaos_assassin_hero_with_20_life_and_4_intellect() {
        let data = Card::Mortimer.data();
        assert_eq!(data.typ, CardType::Hero);
        assert_eq!(data.card_class, CardClass::Assassin);
        assert_eq!(data.talent, Some(Talent::Chaos));
        assert_eq!((data.hero_life, data.hero_intellect), (20, 4));
    }

    #[test]
    fn his_deck_is_assassin_and_generic_cards_only() {
        for card in build_mortimer_deck().into_iter().filter(|&c| c != Card::Mortimer) {
            assert!(matches!(card.data().card_class, CardClass::Generic | CardClass::Assassin), "{:?}", card);
        }
    }

    #[test]
    fn a_game_deals_him_20_life_and_a_hand_of_four() {
        let mut game = FabGame::new(&build_mortimer_deck(), &build_rhinar_deck(), Some(42));
        game.reset(false);
        let p = &game.state.p1;
        assert_eq!((p.life, p.intellect), (20, 4));
        assert_eq!(p.weapon, None);
        assert_eq!(p.equipment.len(), 4);
        assert_eq!(p.owned_cards, 26);
    }

    #[test]
    fn the_disease_tokens_are_disease_auras_and_quicken_is_a_plain_aura() {
        for token in [TokenCard::ExertionToken, TokenCard::InertiaToken, TokenCard::BloodrotPoxToken,
                      TokenCard::FrailtyToken] {
            let data = token.card().data();
            assert_eq!(data.typ, CardType::Token, "{:?}", token);
            assert_eq!(data.subtype, Some(Subtype::DiseaseAura), "{:?}", token);
            assert!(data.no_block, "{:?}", token);
            assert_eq!(TokenCard::from_id(token.id()), token);
        }
        let quicken = Card::QuickenToken.data().subtype.expect("Quicken has a subtype");
        assert!(quicken.is_aura() && !quicken.is_disease());
        assert!(Subtype::DiseaseAura.is_aura() && Subtype::DiseaseAura.is_disease());
        // Only the tokens have a subtype so far.
        assert_eq!(Card::Mortimer.data().subtype, None);
    }

    #[test]
    fn a_player_holds_disease_tokens_beside_their_cards() {
        let mut game = FabGame::new(&build_mortimer_deck(), &build_rhinar_deck(), Some(42));
        game.reset(false);
        let p = &mut game.state.p2;
        p.add_token(TokenCard::BloodrotPoxToken);
        p.add_token(TokenCard::BloodrotPoxToken);
        p.add_token(TokenCard::InertiaToken);
        assert_eq!(p.token_count(TokenCard::BloodrotPoxToken), 2);
        assert_eq!(p.token_count(TokenCard::InertiaToken), 1);
        assert_eq!(p.token_count(TokenCard::ExertionToken), 0);
        assert_eq!(p.token_count(TokenCard::QuickenToken), 0);
        // Tokens are not among a player's owned cards.
        game.state.debug_check_card_totals();
        let shown = crate::display::render_state(&game.state, None);
        assert!(shown.contains("Tokens: Inertia; 2x Bloodrot Pox"), "{shown}");
    }

    // ── Bloodrot Pox ────────────────────────────────────────────────────

    use crate::action::ActionType;
    use crate::deck_cards::TokenCard;
    use crate::game_state::{Phase, PlayerIndex, Zone};
    use crate::legal_actions::legal_actions;
    use crate::test_support::{deck_cards, hand_cards, new_game, put_in_arsenal, set_hand, step_card, step_card_from, step_plain, TestGame};

    /// Rhinar against Dorinthea with the first turn's Action phase open, the
    /// turn player holding `pox` Bloodrot Pox. Returns the game and the turn
    /// player.
    fn turn_player_with_pox(pox: u8) -> (TestGame, PlayerIndex) {
        let mut gs = new_game(&build_rhinar_deck(), &build_dorinthea_deck(), Some(42), false);
        step_plain(&mut gs, ActionType::ChooseFirst);
        assert_eq!(gs.phase, Phase::Action);
        let pid = gs.turn_player;
        for _ in 0..pox {
            gs.player_mut(pid).add_token(TokenCard::BloodrotPoxToken);
        }
        (gs, pid)
    }

    fn pox(gs: &TestGame, pid: PlayerIndex) -> u8 {
        gs.player(pid).token_count(TokenCard::BloodrotPoxToken)
    }

    fn types(gs: &TestGame) -> Vec<ActionType> {
        legal_actions(gs).iter().map(|a| a.typ).collect()
    }

    #[test]
    fn bloodrot_pox_asks_at_the_start_of_the_end_phase_and_passing_takes_2() {
        let (mut gs, pid) = turn_player_with_pox(1);
        step_plain(&mut gs, ActionType::Pass);
        // Before the arsenal decision: pay or take the damage.
        assert_eq!((gs.phase, gs.active_player), (Phase::BloodrotPox, pid));
        assert_eq!(types(&gs), vec![ActionType::Pay, ActionType::Pass]);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.player(pid).life, 18);
        assert_eq!(pox(&gs, pid), 0);
        assert_eq!((gs.phase, gs.active_player), (Phase::Arsenal, pid));
    }

    #[test]
    fn paying_3_by_pitching_destroys_bloodrot_pox_without_damage() {
        let (mut gs, pid) = turn_player_with_pox(1);
        // A blue pitches for 3, two yellows for 2 each.
        set_hand(&mut gs, pid, &[Card::DodgeB, Card::MuscleMuttY, Card::MuscleMuttY, Card::PackCallY]);
        step_plain(&mut gs, ActionType::Pass);
        step_plain(&mut gs, ActionType::Pay);
        assert_eq!(gs.phase, Phase::BloodrotPoxPitch);
        assert!(types(&gs).iter().all(|&t| t == ActionType::Pitch), "only pitching is offered");
        // Two yellows: 2 is not enough, 4 is.
        step_card(&mut gs, ActionType::Pitch, Card::MuscleMuttY);
        assert_eq!((gs.phase, pox(&gs, pid)), (Phase::BloodrotPoxPitch, 1));
        step_card(&mut gs, ActionType::Pitch, Card::MuscleMuttY);
        assert_eq!(pox(&gs, pid), 0);
        assert_eq!(gs.player(pid).life, 20);
        assert_eq!(gs.player(pid).resources, 1, "the overpayment stays banked until the turn ends");

        // The rest of the end phase, in order: arsenal, then the pitched
        // cards go under the deck, then the draw up to intellect.
        assert_eq!((gs.phase, gs.active_player), (Phase::Arsenal, pid));
        step_card(&mut gs, ActionType::Arsenal, Card::DodgeB);
        assert_eq!(gs.phase, Phase::PitchOrder);
        assert_eq!(gs.player(pid).hand_size(), 1, "no draw before the pitch zone is cleared");
        step_card_from(&mut gs, ActionType::BottomPitch, Card::MuscleMuttY, Zone::Pitch);
        step_card_from(&mut gs, ActionType::BottomPitch, Card::MuscleMuttY, Zone::Pitch);
        assert_eq!(gs.turn_player, pid.opponent(), "the turn has passed");
        assert_eq!(gs.player(pid).hand_size(), 4, "drawn back up to intellect");
        assert_eq!(gs.player(pid).resources, 0);
        assert_eq!(hand_cards(&gs, pid).iter().filter(|&&c| c == Card::MuscleMuttY).count(), 0);
    }

    #[test]
    fn banked_resources_pay_for_bloodrot_pox_without_pitching() {
        let (mut gs, pid) = turn_player_with_pox(1);
        gs.player_mut(pid).resources = 3;
        step_plain(&mut gs, ActionType::Pass);
        step_plain(&mut gs, ActionType::Pay);
        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!((gs.player(pid).life, gs.player(pid).resources, pox(&gs, pid)), (20, 0, 0));
    }

    #[test]
    fn each_bloodrot_pox_is_its_own_choice() {
        let (mut gs, pid) = turn_player_with_pox(2);
        set_hand(&mut gs, pid, &[Card::DodgeB, Card::MuscleMuttY, Card::PackCallY, Card::PackCallY]);
        step_plain(&mut gs, ActionType::Pass);
        // Pay for the first with the blue...
        step_plain(&mut gs, ActionType::Pay);
        step_card(&mut gs, ActionType::Pitch, Card::DodgeB);
        assert_eq!((gs.phase, pox(&gs, pid), gs.player(pid).life), (Phase::BloodrotPox, 1, 20));
        // ...and take the second.
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!((gs.phase, pox(&gs, pid), gs.player(pid).life), (Phase::Arsenal, 0, 18));
    }

    #[test]
    fn a_bloodrot_pox_that_cannot_be_paid_for_deals_its_damage_unasked() {
        let (mut gs, pid) = turn_player_with_pox(2);
        // One yellow: 2 resources at most, short of 3.
        set_hand(&mut gs, pid, &[Card::MuscleMuttY]);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!((gs.phase, pox(&gs, pid), gs.player(pid).life), (Phase::Arsenal, 0, 16));
        assert_eq!(gs.player(pid).hand_size(), 1);
    }

    #[test]
    fn bloodrot_pox_can_end_the_game() {
        let (mut gs, pid) = turn_player_with_pox(1);
        gs.player_mut(pid).life = 2;
        step_plain(&mut gs, ActionType::Pass);
        step_plain(&mut gs, ActionType::Pass);
        let won = if pid == PlayerIndex::P1 { Phase::Player2Win } else { Phase::Player1Win };
        assert_eq!(gs.phase, won);
    }

    #[test]
    fn only_the_turn_players_bloodrot_pox_resolves() {
        let (mut gs, pid) = turn_player_with_pox(0);
        let other = pid.opponent();
        gs.player_mut(other).add_token(TokenCard::BloodrotPoxToken);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.phase, Phase::Arsenal, "the opponent's token waits for their own end phase");
        assert_eq!((pox(&gs, other), gs.player(other).life), (1, 20));
    }

    // ── Inertia ─────────────────────────────────────────────────────────

    /// As `turn_player_with_pox`, with `inertia` Inertia tokens instead.
    fn turn_player_with_inertia(inertia: u8) -> (TestGame, PlayerIndex) {
        let (mut gs, pid) = turn_player_with_pox(0);
        for _ in 0..inertia {
            gs.player_mut(pid).add_token(TokenCard::InertiaToken);
        }
        (gs, pid)
    }

    /// The bottom `n` cards of `pid`'s deck, the last one bottommost.
    fn deck_bottom(gs: &TestGame, pid: PlayerIndex, n: usize) -> Vec<Card> {
        let deck = deck_cards(gs, pid);
        deck[deck.len() - n..].to_vec()
    }

    #[test]
    fn inertia_puts_hand_and_arsenal_under_the_deck_in_the_chosen_order() {
        let (mut gs, pid) = turn_player_with_inertia(1);
        // `put_in_arsenal` moves the Wild Ride out of the hand.
        set_hand(&mut gs, pid, &[Card::DodgeB, Card::MuscleMuttY, Card::PackCallY, Card::WildRideR]);
        put_in_arsenal(&mut gs, pid, Card::WildRideR);
        let deck_before = gs.player(pid).deck.len();
        step_plain(&mut gs, ActionType::Pass);

        // The token is gone and its controller orders the four cards.
        assert_eq!(gs.player(pid).token_count(TokenCard::InertiaToken), 0);
        assert_eq!((gs.phase, gs.active_player), (Phase::InertiaOrder, pid));
        let legal = legal_actions(&gs);
        assert_eq!(legal.len(), 4, "three hand cards and the arsenal card: {legal:?}");
        assert!(legal.iter().all(|a| a.typ == ActionType::BottomCard));

        step_card(&mut gs, ActionType::BottomCard, Card::PackCallY);
        step_card_from(&mut gs, ActionType::BottomCard, Card::WildRideR, Zone::Arsenal);
        assert_eq!(gs.phase, Phase::InertiaOrder);
        step_card(&mut gs, ActionType::BottomCard, Card::DodgeB);
        // The last card has nowhere else to go: it follows without asking.
        assert_eq!((gs.phase, gs.active_player), (Phase::Arsenal, pid));
        assert_eq!(gs.player(pid).hand_size(), 0);
        assert_eq!(gs.player(pid).arsenal, None);
        assert_eq!(gs.player(pid).deck.len(), deck_before + 4);
        assert_eq!(deck_bottom(&gs, pid, 4),
            vec![Card::PackCallY, Card::WildRideR, Card::DodgeB, Card::MuscleMuttY]);
        gs.debug_check_card_totals();

        // Nothing is left to arsenal; the turn ends with a fresh hand.
        assert_eq!(types(&gs), vec![ActionType::Pass]);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.turn_player, pid.opponent());
        assert_eq!(gs.player(pid).hand_size(), 4, "drawn back up to intellect");
    }

    #[test]
    fn inertia_with_a_single_card_asks_nothing() {
        let (mut gs, pid) = turn_player_with_inertia(1);
        set_hand(&mut gs, pid, &[Card::DodgeB]);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.player(pid).hand_size(), 0);
        assert_eq!(deck_bottom(&gs, pid, 1), vec![Card::DodgeB]);
    }

    #[test]
    fn inertia_with_no_cards_is_just_destroyed() {
        let (mut gs, pid) = turn_player_with_inertia(1);
        set_hand(&mut gs, pid, &[]);
        let deck_before = gs.player(pid).deck.len();
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.player(pid).token_count(TokenCard::InertiaToken), 0);
        assert_eq!(gs.player(pid).deck.len(), deck_before);
    }

    #[test]
    fn several_inertias_are_all_destroyed_and_do_no_more_than_one() {
        let (mut gs, pid) = turn_player_with_inertia(3);
        set_hand(&mut gs, pid, &[Card::DodgeB, Card::MuscleMuttY]);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.player(pid).token_count(TokenCard::InertiaToken), 0);
        assert_eq!(gs.phase, Phase::InertiaOrder);
        step_card(&mut gs, ActionType::BottomCard, Card::MuscleMuttY);
        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(deck_bottom(&gs, pid, 2), vec![Card::MuscleMuttY, Card::DodgeB]);
    }

    #[test]
    fn bloodrot_pox_resolves_before_inertia_so_the_hand_can_still_pay() {
        let (mut gs, pid) = turn_player_with_inertia(1);
        gs.player_mut(pid).add_token(TokenCard::BloodrotPoxToken);
        set_hand(&mut gs, pid, &[Card::DodgeB, Card::MuscleMuttY, Card::PackCallY]);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.phase, Phase::BloodrotPox);
        assert_eq!(gs.player(pid).token_count(TokenCard::InertiaToken), 1, "Inertia waits its turn");
        step_plain(&mut gs, ActionType::Pay);
        step_card(&mut gs, ActionType::Pitch, Card::DodgeB);
        // Paid with the blue; Inertia then takes the two cards left in hand.
        assert_eq!((gs.phase, gs.player(pid).life), (Phase::InertiaOrder, 20));
        step_card(&mut gs, ActionType::BottomCard, Card::PackCallY);
        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.player(pid).hand_size(), 0);
        // The pitched blue goes under after them, when the turn winds down.
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(deck_bottom(&gs, pid, 3), vec![Card::PackCallY, Card::MuscleMuttY, Card::DodgeB]);
    }

    #[test]
    fn only_the_turn_players_inertia_resolves() {
        let (mut gs, pid) = turn_player_with_inertia(0);
        let other = pid.opponent();
        gs.player_mut(other).add_token(TokenCard::InertiaToken);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.player(other).token_count(TokenCard::InertiaToken), 1);
        assert_eq!(gs.player(other).hand_size(), 4);
    }

    #[test]
    fn cards_inertia_puts_under_the_deck_stay_hidden_from_the_opponent() {
        let (mut gs, pid) = turn_player_with_inertia(1);
        set_hand(&mut gs, pid, &[Card::DodgeB, Card::MuscleMuttY]);
        step_plain(&mut gs, ActionType::Pass);
        step_card(&mut gs, ActionType::BottomCard, Card::DodgeB);
        let deck = &gs.player(pid).deck;
        let n = deck.len();
        assert!(!deck.get(n - 1).unwrap().is_known());
        assert!(!deck.get(n - 2).unwrap().is_known());
    }

    // ── Frailty ─────────────────────────────────────────────────────────

    use crate::action::Action;
    use crate::test_support::step;

    /// Rhinar (P1) against Dorinthea with Rhinar's first turn open and his
    /// hand set to `hand`; `frailty` Frailty tokens are his.
    fn rhinar_turn_with_frailty(frailty: u8, hand: &[Card]) -> TestGame {
        let mut gs = new_game(&build_rhinar_deck(), &build_dorinthea_deck(), Some(42), false);
        let choice = if gs.active_player == PlayerIndex::P1 { ActionType::ChooseFirst } else { ActionType::ChooseSecond };
        step_plain(&mut gs, choice);
        assert_eq!((gs.phase, gs.turn_player), (Phase::Action, PlayerIndex::P1));
        set_hand(&mut gs, PlayerIndex::P1, hand);
        for _ in 0..frailty {
            gs.p1.add_token(TokenCard::FrailtyToken);
        }
        gs
    }

    /// Make the attack `act`, paying with whatever pitch is offered first, and
    /// play it through to combat damage with nobody blocking or reacting.
    /// Returns the damage Dorinthea took.
    fn attack_unblocked(gs: &mut TestGame, act: Action) -> u8 {
        let life = gs.p2.life;
        step(gs, act);
        while gs.phase != Phase::Action {
            let legal = legal_actions(gs);
            let pass = legal.iter().copied().find(|a| a.typ == ActionType::Pass);
            step(gs, pass.unwrap_or(legal[0]));
        }
        life - gs.p2.life
    }

    fn bone_basher_swing(gs: &TestGame) -> Action {
        *legal_actions(gs).iter()
            .find(|a| a.typ == ActionType::Activate && a.card.is_some_and(|c| c.zone == Zone::Weapon))
            .expect("Bone Basher can attack")
    }

    const HAND: [Card; 4] = [Card::MuscleMuttY, Card::DodgeB, Card::DodgeB, Card::ClearingBellowB];

    #[test]
    fn frailty_takes_1_power_off_a_weapon_attack_per_token() {
        let damage = |frailty| {
            let mut gs = rhinar_turn_with_frailty(frailty, &HAND);
            let swing = bone_basher_swing(&gs);
            attack_unblocked(&mut gs, swing)
        };
        let full = damage(0);
        assert!(full >= 3, "Bone Basher should hit for a few: {full}");
        assert_eq!(damage(1), full - 1);
        assert_eq!(damage(2), full - 2);
    }

    #[test]
    fn frailty_takes_1_power_off_an_attack_action_played_from_arsenal() {
        let damage = |frailty| {
            let mut gs = rhinar_turn_with_frailty(frailty, &HAND);
            put_in_arsenal(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
            let play = crate::test_support::act_from(&gs, ActionType::PlayCard, Card::MuscleMuttY, Zone::Arsenal);
            attack_unblocked(&mut gs, play)
        };
        assert_eq!(damage(0), 6);
        assert_eq!(damage(1), 5);
    }

    #[test]
    fn frailty_leaves_an_attack_action_played_from_hand_alone() {
        let mut gs = rhinar_turn_with_frailty(1, &HAND);
        let play = crate::test_support::act(&gs, ActionType::PlayCard, Card::MuscleMuttY);
        assert_eq!(attack_unblocked(&mut gs, play), 6);
        assert!(!gs.p1.attack_from_arsenal());
    }

    #[test]
    fn frailty_from_arsenal_does_not_carry_to_the_next_attack() {
        // Muscle Mutt from arsenal, then a second Muscle Mutt from hand, made
        // possible by giving the first go again.
        let mut gs = rhinar_turn_with_frailty(1, &[Card::MuscleMuttY, Card::MuscleMuttY,
            Card::DodgeB, Card::DodgeB]);
        put_in_arsenal(&mut gs, PlayerIndex::P1, Card::MuscleMuttY);
        gs.p1.set_attack_go_again_bonus(true);
        // Paid from banked resources, so the second Muscle Mutt stays in hand.
        gs.p1.resources = 3;
        let play = crate::test_support::act_from(&gs, ActionType::PlayCard, Card::MuscleMuttY, Zone::Arsenal);
        assert_eq!(attack_unblocked(&mut gs, play), 5);
        assert!(!gs.p1.attack_from_arsenal(), "cleared once damage is dealt");
        // The arsenal is empty now; this one comes from hand at full power.
        gs.p1.resources = 3;
        let play = crate::test_support::act(&gs, ActionType::PlayCard, Card::MuscleMuttY);
        assert_eq!(attack_unblocked(&mut gs, play), 6);
    }

    #[test]
    fn frailty_cannot_take_power_below_0() {
        let mut gs = rhinar_turn_with_frailty(3, &HAND);
        // Bone Basher's printed power less three Frailty, against no block,
        // never wraps round to a huge number.
        let swing = bone_basher_swing(&gs);
        assert!(attack_unblocked(&mut gs, swing) <= 20);
    }

    #[test]
    fn frailty_is_destroyed_at_the_beginning_of_its_controllers_end_phase() {
        let mut gs = rhinar_turn_with_frailty(2, &HAND);
        gs.p2.add_token(TokenCard::FrailtyToken);
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.phase, Phase::Arsenal);
        assert_eq!(gs.p1.token_count(TokenCard::FrailtyToken), 0);
        assert_eq!(gs.p2.token_count(TokenCard::FrailtyToken), 1,
            "the opponent's Frailty lasts until their own end phase");
    }

    // ── Bloodrot Courier ────────────────────────────────────────────────

    /// Mortimer (P1) against Rhinar with Mortimer's first turn open, his hand
    /// set to `hand` and 2 resources banked to pay for a Courier.
    fn mortimer_turn(hand: &[Card]) -> TestGame {
        let mut gs = new_game(&build_mortimer_deck(), &build_rhinar_deck(), Some(42), false);
        let choice = if gs.active_player == PlayerIndex::P1 { ActionType::ChooseFirst } else { ActionType::ChooseSecond };
        step_plain(&mut gs, choice);
        assert_eq!((gs.phase, gs.turn_player), (Phase::Action, PlayerIndex::P1));
        set_hand(&mut gs, PlayerIndex::P1, hand);
        gs.p1.resources = 2;
        gs
    }

    #[test]
    fn bloodrot_courier_is_a_red_chaos_assassin_attack_with_go_again() {
        let d = Card::BloodrotCourierR.data();
        assert_eq!(d.typ, CardType::AttackAction);
        assert_eq!((d.cost, d.pitch, d.power, d.defense), (2, 1, 4, 3));
        assert!(matches!(d.color, Some(Color::Red)));
        assert_eq!((d.card_class, d.talent), (CardClass::Assassin, Some(Talent::Chaos)));
        assert!(d.keyword.contains(Keyword::GoAgain));
        assert_eq!(crate::display::card_name(Card::BloodrotCourierR), "Bloodrot Courier (red)");
        assert_eq!(build_mortimer_deck().iter().filter(|&&c| c == Card::BloodrotCourierR).count(), 2);
    }

    #[test]
    fn a_bloodrot_courier_hit_gives_both_heroes_a_bloodrot_pox_and_go_again() {
        let mut gs = mortimer_turn(&[Card::BloodrotCourierR, Card::DodgeB]);
        let play = crate::test_support::act(&gs, ActionType::PlayCard, Card::BloodrotCourierR);
        assert_eq!(attack_unblocked(&mut gs, play), 4);
        assert_eq!(gs.p1.token_count(TokenCard::BloodrotPoxToken), 1);
        assert_eq!(gs.p2.token_count(TokenCard::BloodrotPoxToken), 1);
        assert_eq!(gs.p1.action_points, 1, "go again keeps the action point");
        // Mortimer's own Pox comes due at the beginning of this end phase.
        step_plain(&mut gs, ActionType::Pass);
        assert_eq!(gs.phase, Phase::BloodrotPox);
    }

    #[test]
    fn a_blocked_bloodrot_courier_creates_no_bloodrot_pox() {
        let mut gs = mortimer_turn(&[Card::BloodrotCourierR, Card::DodgeB]);
        step_card(&mut gs, ActionType::PlayCard, Card::BloodrotCourierR);
        // Rhinar blocks with enough to stop all 4.
        while gs.phase != Phase::Defend {
            step_plain(&mut gs, ActionType::Pass);
        }
        set_hand(&mut gs, PlayerIndex::P2, &[Card::RagingOnslaughtY, Card::WreckerRompB]);
        step_card(&mut gs, ActionType::Defend, Card::RagingOnslaughtY);
        step_card(&mut gs, ActionType::Defend, Card::WreckerRompB);
        let life = gs.p2.life;
        while gs.phase != Phase::Action {
            step_plain(&mut gs, ActionType::Pass);
        }
        assert_eq!(gs.p2.life, life, "fully blocked");
        assert_eq!(gs.p1.token_count(TokenCard::BloodrotPoxToken), 0);
        assert_eq!(gs.p2.token_count(TokenCard::BloodrotPoxToken), 0);
    }

    #[test]
    fn a_bloodrot_courier_hit_creates_no_bloodrot_pox_past_the_token_limit() {
        let mut gs = mortimer_turn(&[Card::BloodrotCourierR, Card::DodgeB]);
        for _ in 0..crate::game_state::MAX_TOKEN_COPIES {
            gs.p2.add_token(TokenCard::BloodrotPoxToken);
        }
        let play = crate::test_support::act(&gs, ActionType::PlayCard, Card::BloodrotCourierR);
        attack_unblocked(&mut gs, play);
        assert_eq!(gs.p2.token_count(TokenCard::BloodrotPoxToken), crate::game_state::MAX_TOKEN_COPIES);
        assert_eq!(gs.p1.token_count(TokenCard::BloodrotPoxToken), 1);
    }

    /// Random play to the end of the game, in either seat and against either
    /// hero: the debug-build card conservation check runs on every step.
    #[test]
    fn random_games_with_mortimer_finish() {
        for seed in 0..40u64 {
            let mut game = match seed % 4 {
                0 => FabGame::new(&build_mortimer_deck(), &build_rhinar_deck(), Some(seed)),
                1 => FabGame::new(&build_dorinthea_deck(), &build_mortimer_deck(), Some(seed)),
                2 => FabGame::new(&build_rhinar_deck(), &build_mortimer_deck(), Some(seed)),
                _ => FabGame::new(&build_mortimer_deck(), &build_mortimer_deck(), Some(seed)),
            };
            let mut rng = SmallRng::seed_from_u64(seed);
            game.reset(false);
            // Each player starts with disease tokens, so their end phase steps
            // are played through too.
            for p in [&mut game.state.p1, &mut game.state.p2] {
                for _ in 0..(seed % 4) {
                    p.add_token(TokenCard::BloodrotPoxToken);
                }
                if seed % 3 == 0 {
                    p.add_token(TokenCard::InertiaToken);
                }
                if seed % 2 == 0 {
                    p.add_token(TokenCard::FrailtyToken);
                }
            }
            let mut steps = 0;
            while !game.is_over() {
                let legal = game.legal_actions();
                let action = *legal.as_ref().choose(&mut rng).expect("a live game has a legal action");
                game.step(action);
                steps += 1;
                assert!(steps < 100_000, "seed {seed} did not finish");
            }
        }
    }
}

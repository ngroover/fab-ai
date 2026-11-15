//! Outbreak: Mortimer, Living Virus — his hero card and his decklist — and
//! the set's disease aura tokens (Exertion, Inertia, Bloodrot Pox).
//!
//! No Assassin cards are implemented yet, so for now his deck is a placeholder
//! built from the Generic cards of the Rhinar vs Dorinthea classic battle
//! (`classic_battles`). It is smaller than a full Blitz list: 20 deck cards,
//! four pieces of equipment and no weapon.

use crate::card_effects::ConstantEffect;
use crate::cards::{Card, CardClass, CardData, CardType, Keyword, Subtype, Talent};

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

/// What the three disease aura tokens share: a Generic token that cannot
/// block. Exertion and Inertia have no effect implemented yet, and nothing
/// creates any of the three yet.
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
/// Inertia — disease aura token.
pub const INERTIA_TOKEN: CardData = DISEASE_AURA_TOKEN;
/// Bloodrot Pox — disease aura token. At the beginning of its controller's
/// end phase it is destroyed, and deals 2 damage to them unless they pay 3.
pub const BLOODROT_POX_TOKEN: CardData = CardData {
    constant_effect: Some(ConstantEffect::OnEndPhaseDestroy2DamageUnlessPay3),
    ..DISEASE_AURA_TOKEN
};

/// Mortimer's placeholder deck: the hero, four pieces of Generic equipment and
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
    fn his_deck_is_generic_cards_only() {
        for card in build_mortimer_deck().into_iter().filter(|&c| c != Card::Mortimer) {
            assert_eq!(card.data().card_class, CardClass::Generic, "{:?}", card);
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
        assert_eq!(p.owned_cards, 24);
    }

    #[test]
    fn the_disease_tokens_are_disease_auras_and_quicken_is_a_plain_aura() {
        for token in [TokenCard::ExertionToken, TokenCard::InertiaToken, TokenCard::BloodrotPoxToken] {
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
    use crate::test_support::{hand_cards, new_game, set_hand, step_card, step_card_from, step_plain, TestGame};

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
            // Each player starts with Bloodrot Pox, so its end phase step is
            // played through too.
            for p in [&mut game.state.p1, &mut game.state.p2] {
                for _ in 0..(seed % 4) {
                    p.add_token(TokenCard::BloodrotPoxToken);
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

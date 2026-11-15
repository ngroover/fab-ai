//! Outbreak: Mortimer, Living Virus — his hero card and his decklist — and
//! the set's disease aura tokens (Exertion, Inertia, Bloodrot Pox).
//!
//! No Assassin cards are implemented yet, so for now his deck is a placeholder
//! built from the Generic cards of the Rhinar vs Dorinthea classic battle
//! (`classic_battles`). It is smaller than a full Blitz list: 20 deck cards,
//! four pieces of equipment and no weapon.

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
/// block, with no effect implemented yet. Nothing creates one yet either.
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
/// Bloodrot Pox — disease aura token.
pub const BLOODROT_POX_TOKEN: CardData = DISEASE_AURA_TOKEN;

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
        use crate::deck_cards::TokenCard;
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
        use crate::deck_cards::TokenCard;
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

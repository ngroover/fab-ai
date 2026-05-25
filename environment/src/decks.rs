use crate::cards::Card;

/// Rhinar Blitz deck — young Rhinar (20 life, intellect 4).
/// First six entries are hero, weapon, and equipment; the remaining 40 are the
/// playable deck. Mirrors `build_rhinar_deck()` in `cards.py`.
pub fn build_rhinar_deck() -> [Card; 46] {
    [
        Card::Rhinar,
        Card::BoneBasher,
        Card::BlossomOfSpring,
        Card::BoneVizier,
        Card::IronhideGauntlet,
        Card::IronhideLegs,
        // ── RED ──
        Card::AlphaRampageR,
        Card::AwakeningBellowR,
        Card::AwakeningBellowR,
        Card::BareFangsR,
        Card::BareFangsR,
        Card::BeastModeR,
        Card::BeastModeR,
        Card::PackHuntR,
        Card::PackHuntR,
        Card::WildRideR,
        Card::WildRideR,
        Card::WreckingBallR,
        Card::WreckingBallR,
        // ── YELLOW ──
        Card::BarragingBeatdownY,
        Card::BarragingBeatdownY,
        Card::MuscleMuttY,
        Card::MuscleMuttY,
        Card::PackCallY,
        Card::PackCallY,
        Card::RagingOnslaughtY,
        Card::RagingOnslaughtY,
        Card::SmashInstinctY,
        Card::SmashInstinctY,
        Card::SmashWithBigTreeY,
        Card::SmashWithBigTreeY,
        Card::WoundedBullY,
        Card::WoundedBullY,
        // ── BLUE ──
        Card::ClearingBellowB,
        Card::ClearingBellowB,
        Card::ComeToFightB,
        Card::ComeToFightB,
        Card::DodgeB,
        Card::DodgeB,
        Card::RallyTheRearguardB,
        Card::RallyTheRearguardB,
        Card::TitaniumBaubleB,
        Card::TitaniumBaubleB,
        Card::WreckerRompB,
        Card::WreckerRompB,
        Card::ChiefRukutan,
    ]
}

/// Dorinthea Blitz deck — young Dorinthea, Quicksilver Prodigy (20 life,
/// intellect 4). First six entries are hero, weapon, and equipment; the
/// remaining 40 are the playable deck. Mirrors `build_dorinthea_deck()` in
/// `cards.py`.
pub fn build_dorinthea_deck() -> [Card; 46] {
    [
        Card::Dorinthea,
        Card::Dawnblade,
        Card::GallantryGold,
        Card::BlossomOfSpring,
        Card::IronrotHelm,
        Card::IronrotLegs,
        // ── RED ──
        Card::EnGardeR,
        Card::EnGardeR,
        Card::FlockOfTheFeatherWalkersR,
        Card::FlockOfTheFeatherWalkersR,
        Card::InTheSwingR,
        Card::InTheSwingR,
        Card::IronsongResponseR,
        Card::IronsongResponseR,
        Card::SecondSwingR,
        Card::SecondSwingR,
        Card::SharpenSteelR,
        Card::SharpenSteelR,
        Card::ThrustR,
        Card::ThrustR,
        Card::WarriorsValorR,
        Card::WarriorsValorR,
        // ── YELLOW ──
        Card::DrivingBladeY,
        Card::DrivingBladeY,
        Card::GlisteningSteelbladeY,
        Card::OnAKnifeEdgeY,
        Card::OnAKnifeEdgeY,
        Card::OutForBloodY,
        Card::OutForBloodY,
        Card::RunThroughY,
        Card::RunThroughY,
        Card::SliceAndDiceY,
        Card::SliceAndDiceY,
        // ── BLUE ──
        Card::BladeFlashB,
        Card::BladeFlashB,
        Card::HitAndRunB,
        Card::HitAndRunB,
        Card::SigilofSolaceB,
        Card::SigilofSolaceB,
        Card::TitaniumBaubleB,
        Card::TitaniumBaubleB,
        Card::ToughenUpB,
        Card::ToughenUpB,
        Card::VisitTheBlacksmithB,
        Card::VisitTheBlacksmithB,
        Card::HalaGoldenhelm,
    ]
}

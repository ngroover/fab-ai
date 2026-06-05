use bitflags::bitflags;
use crate::card_effects::{ConstantEffect,Ability,DefendEffect,
    NextAttackEffect,AdditionalCostType,OnPlayEffect,TargetEffect,
    PlayCondition};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum CardType {
    AttackAction,
    Action,
    Instant,
    AttackReaction,
    DefenseReaction,
    Equipment,
    Weapon,
    Sword2h,
    Club2h,
    Hero,
    Mentor,
    Resource,
    Token
}

#[repr(u8)]
pub enum Color {
    Red,
    Yellow,
    Blue
}

#[repr(u8)]
pub enum EquipmentSlot {
    Head,
    Chest,
    Arms,
    Legs,
    Weapon
}

#[repr(u8)]
pub enum CardClass {
    Generic,
    Brute,
    Warrior
}

bitflags! {
    pub struct Keyword : u8 {
        const GoAgain = 0b1;
        const Intimidate = 0b10;
        const BladeBreak = 0b100;
        const Battleworn = 0b1000;
        const Reprise = 0b10000;
        const RhinarSpecialization = 0b100000;
        const DorintheaSpecialization = 0b1000000;
    }
}

pub struct CardData {
    pub typ: CardType,
    pub cost: u8,
    pub pitch: u8,
    pub power: u8,
    pub defense: u8,
    pub color: Option<Color>,
    pub no_block: bool,
    pub slot: Option<EquipmentSlot>,
    pub card_class: CardClass,
    pub keyword: Keyword,
    pub hero_life: u8,
    pub hero_intellect: u8,
    pub constant_effect : Option<ConstantEffect>,
    pub ability : Option<Ability>,
    pub defend_effect : Option<DefendEffect>,
    pub next_attack_effect : Option<NextAttackEffect>,
    pub additional_cost : Option<AdditionalCostType>,
    pub target_effect : Option<TargetEffect>,
    pub play_condition : Option<PlayCondition>,
    pub play_effect : Option<OnPlayEffect>
}

#[derive(Clone, Copy, PartialEq, Debug, Eq, Hash)]
#[repr(u8)]
pub enum Card {
    Rhinar,
    Dorinthea,
    AlphaRampageR,
    AwakeningBellowR,
    BareFangsR,
    BeastModeR,
    PackHuntR,
    WildRideR,
    WreckingBallR,
    BarragingBeatdownY,
    MuscleMuttY,
    PackCallY,
    RagingOnslaughtY,
    SmashInstinctY,
    SmashWithBigTreeY,
    WoundedBullY,
    ClearingBellowB,
    ComeToFightB,
    DodgeB,
    RallyTheRearguardB,
    TitaniumBaubleB,
    WreckerRompB,
    ChiefRukutan,
    BoneBasher, 
    BlossomOfSpring,
    BoneVizier,
    IronhideGauntlet,
    IronhideLegs,
    EnGardeR,
    FlockOfTheFeatherWalkersR,
    InTheSwingR,
    IronsongResponseR,
    SecondSwingR,
    SharpenSteelR,
    ThrustR,
    WarriorsValorR,
    DrivingBladeY,
    GlisteningSteelbladeY,
    OnAKnifeEdgeY,
    OutForBloodY,
    RunThroughY,
    SliceAndDiceY,
    BladeFlashB,
    HitAndRunB,
    SigilofSolaceB,
    ToughenUpB,
    VisitTheBlacksmithB,
    HalaGoldenhelm,
    Dawnblade,
    GallantryGold,
    IronrotHelm,
    IronrotLegs,
}


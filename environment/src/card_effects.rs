use bitflags::bitflags;

#[repr(u8)]
pub enum ConstantEffect {
    OnDiscard6Intimidate,
    OnDawnbladeGoAgainExtraSwing,
    OnDiscard6Mentor,
    OnSwordHitMentor,
}

#[repr(u8)]
pub enum Ability {
    DiscardCardPlusBlock,
    DestroyGain1Resource,
    WeaponPlus1,
}

impl Ability {
    /// Resource points required to activate this ability.
    pub fn resource_cost(&self) -> u8 {
        match self {
            Ability::DiscardCardPlusBlock => 0,
            Ability::DestroyGain1Resource => 0,
            Ability::WeaponPlus1 => 1,
        }
    }
}


#[repr(u8)]
pub enum OnPlayConditionType {
    DrawDiscardHit6,
    HasIntimidated,
    HasLessLife,
}

#[repr(u8)]
pub enum OnPlayEffectType {
    ConditionalPower,
    ConditionalGoAgain,
    ConditionalIntimidate,
    GainLife,
    CreateQuicken,
}

pub struct OnPlayEffect {
    pub condition :OnPlayConditionType,
    pub effectType : OnPlayEffectType,
    pub magnitude : u8,
}

#[repr(u8)]
pub enum DefendEffect {
    Reveal6BottomOtherwise,
    PitchToBlock,
}

bitflags! {
    pub struct NextAttackType : u8 {
        const IsBrute = 0b1;
        const IsAttackAction = 0b10;
        const IsWeapon = 0b100;
        const IsDawnblade = 0b1000;
        const IsSword = 0b10000;
        const IsWarrior = 0b100000;
    }
}

#[repr(u8)]
pub enum NextAttackEffectType {
    GainPower,
    GainOnHitGoAgain,
    GoAgain,
}

pub struct NextAttackEffect {
    pub attackType : NextAttackType,
    pub effectType : NextAttackEffectType,
}

#[repr(u8)]
pub enum TurnEffectType {
    OnHitCounter,
    WeaponsGainPlus1,
}

#[repr(u8)]
pub enum AdditionalCostType {
    RevealCost1OrLess,
    DiscardCard,
}

#[repr(u8)]
pub enum TargetType {
    TargetSword,
    TargetWeapon,
}

#[repr(u8)]
pub enum PlayCondition {
    Played2WeaponAttacks,
    PlayedWeaponAttack,
}

#[repr(u8)]
pub enum TargetEffectType {
    GiveGoAgain,
    BoostPower,
}

pub struct TargetEffect {
    pub targetType : TargetType,
    pub effectType : TargetEffectType,
}


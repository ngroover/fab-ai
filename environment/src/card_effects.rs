use bitflags::bitflags;

use crate::cards::CardType;

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
    Attack2,
    Attack1,
}

impl Ability {
    /// Resource points required to activate this ability.
    pub fn resource_cost(&self) -> u8 {
        match self {
            Ability::DiscardCardPlusBlock => 0,
            Ability::DestroyGain1Resource => 0,
            Ability::WeaponPlus1 => 1,
            Ability::Attack2 => 2,
            Ability::Attack1 => 1,
        }
    }

    /// Whether activating this ability costs a card discarded from hand, on top
    /// of any `resource_cost`. Rally the Rearguard's "Discard a card:" is the
    /// only such cost today. It gates the activation as well as paying for it:
    /// an ability that discards is not offered to an empty hand.
    pub fn discards_a_card(&self) -> bool {
        matches!(self, Ability::DiscardCardPlusBlock)
    }

    /// Whether this ability may only be activated while its card is defending —
    /// that is, while it sits on its controller's combat chain as a declared
    /// blocker (Rally the Rearguard's "Activate this ability only while Rally
    /// the Rearguard is defending"). An ability that is not so restricted is
    /// activated from the zone its card lives in, which for every other ability
    /// in the catalog is an equipment or weapon slot.
    pub fn only_while_defending(&self) -> bool {
        matches!(self, Ability::DiscardCardPlusBlock)
    }

    /// Whether this ability may be used only once per turn, tracked per card on
    /// `CardState::ability_used_this_turn`.
    pub fn once_per_turn(&self) -> bool {
        matches!(self, Ability::DiscardCardPlusBlock)
    }

    /// The card type at which this ability is activated. Action-speed abilities
    /// cost an action point on your turn; instant-speed abilities can be used at
    /// any time you have priority (e.g. during the defend step).
    pub fn card_type(&self) -> CardType {
        match self {
            Ability::DiscardCardPlusBlock => CardType::Instant,
            Ability::DestroyGain1Resource => CardType::Action,
            Ability::WeaponPlus1 => CardType::Action,
            Ability::Attack2 => CardType::AttackAction,
            Ability::Attack1 => CardType::AttackAction,
        }
    }
}


#[repr(u8)]
pub enum OnPlayConditionType {
    DrawDiscardHit6,
    HasIntimidated,
    HasLessLife,
    /// No condition: the effect always applies when the card resolves.
    Always,
}

#[repr(u8)]
pub enum OnPlayEffectType {
    ConditionalPower,
    ConditionalGoAgain,
    ConditionalIntimidate,
    GainLife,
    CreateQuicken,
    /// Banks `magnitude` power for the next Brute attack the owner plays this
    /// turn (e.g. Awakening Bellow's "+3 power to the next attack with the brute
    /// type"). Unlike `ConditionalPower`, this only pumps a Brute attack.
    NextBrutePower,
    /// Banks `magnitude` power for the next attack action card the owner plays
    /// this turn, whatever its class (e.g. Come to Fight's "+1 power to your
    /// next attack action card"). The class-agnostic sibling of
    /// `NextBrutePower`; like it, a weapon swing neither takes nor spends it.
    NextAttackPower,
    /// Banks a *conditional* `magnitude` power for the next Brute attack the
    /// owner makes this turn (Barraging Beatdown's "your next Brute attack this
    /// turn gains 'While this attack is defended by less than 2 non-equipment
    /// cards it has +3 power'"). Two things set it apart from `NextBrutePower`:
    /// the bonus only pays out if the attack ends up defended by fewer than two
    /// non-equipment cards, and "Brute attack" is read to cover a brute weapon
    /// swing as well as a brute attack action card, so Bone Basher takes it.
    NextBruteConditionalPower,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ability_card_types() {
        // Rally the Rearguard's ability is an instant.
        assert_eq!(Ability::DiscardCardPlusBlock.card_type(), CardType::Instant);
        // Blossom of Spring and Gallantry Gold abilities are actions.
        assert_eq!(Ability::DestroyGain1Resource.card_type(), CardType::Action);
        assert_eq!(Ability::WeaponPlus1.card_type(), CardType::Action);
    }
}


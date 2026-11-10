use bitflags::bitflags;

use crate::cards::CardType;

#[repr(u8)]
pub enum ConstantEffect {
    OnDiscard6Intimidate,
    /// Dorinthea, Quicksilver Prodigy: "The first time Dawnblade, Resplendent
    /// gains go again each turn, you may attack an additional time with it
    /// this turn." Fired by `fab_step::attack_gains_go_again`; the extra attack
    /// is `PlayerFlags::EXTRA_WEAPON_ATTACK`.
    OnDawnbladeGoAgainExtraSwing,
    /// Dawnblade, Resplendent: "The second time you attack with Dawnblade each
    /// turn, it gains +1 power until the end of turn." Read when combat damage
    /// resolves (`fab_step::resolve_combat_damage`), from how many times the
    /// weapon has attacked this turn (`Player::abilities_used`).
    SecondAttackEachTurnPlus1Power,
    /// "Whenever you play a card with 6 or more power, intimidate." The play
    /// counterpart of `OnDiscard6Intimidate`. Carried by Chief Ruk'utan, whose
    /// constant effect only applies while he is face up in his owner's arsenal
    /// (see `fab_step::maybe_play6_intimidate`); his lesson counter and Alpha
    /// Rampage search ride the same trigger as his `MentorEffect`.
    OnPlay6Intimidate,
    OnSwordHitMentor,
    /// "When this is destroyed, reveal the top card of your deck. If it has 6
    /// or more power, put it on top of your deck. Otherwise, put it on the
    /// bottom." Carried by Bone Vizier; fired by `fab_step::destroy_equipment`.
    /// The reveal is the same one Pack Call's defend trigger makes.
    OnDestroyedReveal6BottomOtherwise,
    /// Quicken: "When you play an attack action card or attack with a weapon,
    /// destroy Quicken and the attack gains go again." Fired by
    /// `fab_step::trigger_quickens` from the owner's token zone.
    OnAttackDestroyForGoAgain,
}

/// What a mentor does with its lessons, on top of its constant effect. A
/// mentor collects a lesson counter each time its constant effect triggers
/// (counters live on the arsenal, `Player::arsenal_lesson_counters`), and its
/// mentor effect says what that lesson builds towards.
#[repr(u8)]
pub enum MentorEffect {
    /// Chief Ruk'utan: "put a lesson counter on Chief Ruk'utan. Then if there
    /// are 2 or more lesson counters on him, banish him, search your deck for
    /// Alpha Rampage, put it face up in arsenal and shuffle." See
    /// `fab_step::mentor_alpha_rampage_search`.
    AlphaRampageSearch,
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

    /// Whether this ability may be used only once per turn, tracked per copy
    /// in `Player::abilities_used`. Both weapon swings are "Once per
    /// Turn Action": the weapon returns to its slot whenever the combat chain
    /// closes, which the turn player can do mid-turn (`ActionType::CloseChain`),
    /// so its being back in the slot says nothing about whether it has swung.
    pub fn once_per_turn(&self) -> bool {
        matches!(self, Ability::DiscardCardPlusBlock | Ability::Attack2 | Ability::Attack1)
    }

    /// Resource points gained when this ability resolves. Blossom of Spring
    /// gains {r}; no other ability gains any.
    pub fn resources_gained(&self) -> u8 {
        match self {
            Ability::DestroyGain1Resource => 1,
            _ => 0,
        }
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
    /// Reprise: "If the defending hero has defended with a card from their
    /// hand this chain link" (Ironsong Response). Read as the card resolves,
    /// from the defender's `PlayerFlags::DEFENDED_FROM_HAND`.
    Reprise,
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
    /// Banks `magnitude` power for the next *weapon* attack the owner makes
    /// this turn (En Garde's "Your next weapon attack this turn gains +3
    /// power"). The weapon counterpart of `NextAttackPower`: only a weapon
    /// swing takes and spends it, so an attack action card in between leaves
    /// it banked.
    NextWeaponPower,
    /// "Your weapon attack gains `magnitude` power" (Ironsong Response): the
    /// owner's current attack gains the power, but only if it is a weapon
    /// attack. Not a target, so the card is playable into any attack; on an
    /// attack action card it does nothing.
    CurrentWeaponAttackPower,
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
    /// Ironhide Gauntlet / Ironhide Legs: "When you defend with this, you may
    /// pay {r}. If you do, it gains +2 block and 'When the combat chain closes,
    /// destroy this.'" The payment is a choice made as the trigger resolves
    /// (`Phase::DefendPayment`); see `fab_step::apply_defend_effect`.
    PayForPlus2BlockThenDestroy,
}

impl DefendEffect {
    /// Resources the trigger's optional payment costs; 0 for a trigger that
    /// asks for no payment.
    pub fn resource_cost(&self) -> u8 {
        match self {
            DefendEffect::PayForPlus2BlockThenDestroy => 1,
            DefendEffect::Reveal6BottomOtherwise | DefendEffect::PitchToBlock => 0,
        }
    }
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

/// What an attack reaction may target. Attack reactions only ever target the
/// attack on the current chain link, so the target type is a requirement on
/// that attack: with no such attack to target the card cannot be played (see
/// `fab_step::play_requirements_met`).
#[repr(u8)]
pub enum TargetType {
    TargetSword,
    /// "Target weapon attack" (In the Swing): the current attack is a weapon
    /// swing.
    TargetWeapon,
}

/// A "Play [this] only if ..." restriction, checked before the card is offered
/// (see `fab_step::play_requirements_met`).
#[repr(u8)]
pub enum PlayCondition {
    /// "Play only if you have attacked 2 or more times with weapons this turn"
    /// (In the Swing). The attack the card is played into counts: it was made
    /// when the weapon's swing was committed, which is when the swing is
    /// counted in `Player::abilities_used`.
    Played2WeaponAttacks,
    PlayedWeaponAttack,
}

#[repr(u8)]
pub enum TargetEffectType {
    GiveGoAgain,
    /// The target attack gains `magnitude` power, banked on the attacker's
    /// `attack_power_bonus` until combat damage.
    BoostPower,
}

pub struct TargetEffect {
    pub targetType : TargetType,
    pub effectType : TargetEffectType,
    pub magnitude : u8,
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


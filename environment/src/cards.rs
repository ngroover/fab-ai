use bitflags::bitflags;

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
    typ : CardType,
    cost: u8,
    pitch: u8,
    power: u8,
    defense: u8,
    color: Color,
    no_block: bool,
    slot: EquipmentSlot,
    card_class: CardClass,
    keyword: Keyword,
    hero_life: u8,
    hero_intellect: u8
   //effects 
   //conditions
}

#[repr(u8)]
pub enum Card {
    Rhinar,
    Dorinthea,
    AlphaRampage_R,
    Awakening_Bellow_R,
    Bare_Fangs_R,
    Beast_Mode_R,
    Pack_Hunt_R,
    Wild_Ride_R,
    Wrecking_Ball_R,
    Barraging_Beatdown_Y,
    Muscle_Mutt_Y,
    Pack_Call_Y,
    Raging_Onslaught_Y,
    Smash_Instinct_Y,
    Smash_With_Big_Tree_Y,
    Wounded_Bull_Y,
    Clearing_Bellow_B,
    Come_To_Fight_B,
    Dodge_B,
    Rally_The_Rearguard_B,
    Titanium_Bauble_B,
    Wrecker_Romp_B,
    Chief_Ruk_utan,
    Bone_Basher, 
    Blossom_Of_Spring,
    Bone_Vizier,
    Ironhide_Gauntlet,
    Ironhide_Legs,
    En_Garde_R,
    Flock_Of_The_Feather_Walkers_R,
    In_The_Swing_R,
    Ironsong_Response_R,
    Second_Swing_R,
    Sharpen_Steel_R,
    Thrust_R,
    Warriors_Valor_R,
    Driving_Blade_Y,
    Glistening_Steelblade_Y,
    On_A_Knife_Edge_Y,
    Out_For_Blood_Y,
    Run_Through_Y,
    Slice_And_Dice_Y,
    Blade_Flash_B,
    Hit_And_Run_B,
    Sigil_of_Solace_B,
    Toughen_Up_B,
    Visit_The_Blacksmith_B,
    Hala_Goldenhelm,
    Dawnblade,
    Gallantry_Gold,
    Ironrot_Helm,
    Ironrot_Legs,
}


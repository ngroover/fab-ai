
use crate::game_state::CardLocation;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ActionType {
    ChooseFirst,
    ChooseSecond,
    PlayCard,
    Activate,
    Pitch,
    Pass
}

#[derive(Debug)]
pub struct Action {
    pub typ : ActionType,
    pub index : usize,
    pub location : Option<CardLocation>
}

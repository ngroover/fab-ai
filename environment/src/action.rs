

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ActionType {
    ChooseFirst,
    ChooseSecond,
}

pub struct Action {
    pub typ : ActionType,
    pub index : usize,
}

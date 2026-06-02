
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

/// A chosen action. `index` is the slot into the active player's `cards` array
/// that the action refers to (0 for actions that reference no card, e.g.
/// `Pass` / `ChooseFirst`). The card's zone is not stored here — it is always
/// derivable from `cards[index].location`.
#[derive(Debug)]
pub struct Action {
    pub typ : ActionType,
    pub index : usize,
}

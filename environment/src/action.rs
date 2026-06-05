use crate::game_state::CardIdx;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ActionType {
    ChooseFirst,
    ChooseSecond,
    PlayCard,
    Activate,
    Pitch,
    Defend,
    Pass
}

/// A chosen action. `card` is the slot into the shared `cards` array the action
/// refers to, or `None` for actions that reference no card (e.g. `Pass`,
/// `ChooseFirst`) — making "no card" explicit rather than overloading slot 0,
/// which is itself a valid card. The card's zone is not stored here; it is
/// always derivable from `cards[card].location`.
#[derive(Clone, Copy, Debug)]
pub struct Action {
    pub typ : ActionType,
    pub card : Option<CardIdx>,
}

impl Action {
    /// The `cards` slot this action refers to, as a `usize` for indexing.
    /// Panics if the action carries no card, so only call it for
    /// card-referencing actions (`PlayCard`, `Activate`, `Pitch`, `Defend`).
    pub fn card_index(&self) -> usize {
        self.card.expect("action carries no card").get()
    }
}

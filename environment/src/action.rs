use crate::game_state::CardIdx;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ActionType {
    ChooseFirst,
    ChooseSecond,
    PlayCard,
    Activate,
    Pitch,
    /// Place a card from the pitch zone on the bottom of its owner's deck,
    /// during the PitchOrder phase at the end of the turn.
    BottomPitch,
    Defend,
    Arsenal,
    /// A "when you defend with this" trigger waiting on the stack, e.g. Pack
    /// Call's reveal. Never a choice a player makes and never returned by
    /// `legal_actions`: the engine pushes it once blockers are declared (see
    /// `push_defend_triggers`) and it exists only as a `PendingCard::typ`, to
    /// tell `resolve_top_of_stack` that what is resolving is the card's trigger
    /// rather than the card itself — which is already on the combat chain.
    DefendTrigger,
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
    /// card-referencing actions (`PlayCard`, `Activate`, `Pitch`,
    /// `BottomPitch`, `Defend`, `Arsenal`).
    pub fn card_index(&self) -> usize {
        self.card.expect("action carries no card").get()
    }
}

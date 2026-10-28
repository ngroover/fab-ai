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
    /// Turn the face-down mentor in the turn player's arsenal face up, in the
    /// `MentorFlip` phase at the start of their turn. `card` is the mentor.
    FlipFaceUp,
    /// Break the combat chain: in the Action phase, with combat damage dealt
    /// and the stack empty, the turn player may close the open chain before
    /// their next attack. Every card on it leaves — a weapon back to its slot,
    /// everything else to its owner's graveyard — and the Action phase carries
    /// on. Carries no card.
    CloseChain,
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

/// The most legal actions any single decision may offer. `LegalActions` stores
/// its actions inline in an array of this size, so generating them never
/// touches the heap; offering more panics (see `LegalActions::push`).
pub const MAX_LEGAL_ACTIONS: usize = 16;

/// A fixed-capacity list of legal actions, returned by `legal_actions`. The
/// actions live inline in an array of `MAX_LEGAL_ACTIONS` slots with a length
/// marking how many are filled, so building the list allocates nothing — the
/// legal-action generators run on every step, and a heap `Vec` there was the
/// engine's single largest cost. Derefs to `&[Action]`, so callers read it
/// exactly like a slice (`iter`, `len`, `is_empty`, indexing).
#[derive(Clone, Copy)]
pub struct LegalActions {
    actions: [Action; MAX_LEGAL_ACTIONS],
    len: u8,
}

impl LegalActions {
    /// An empty list.
    pub const fn new() -> Self {
        LegalActions {
            actions: [Action { typ: ActionType::Pass, card: None }; MAX_LEGAL_ACTIONS],
            len: 0,
        }
    }

    /// Append an action. Panics if the list already holds `MAX_LEGAL_ACTIONS`,
    /// which would mean a decision offers more choices than the engine sized
    /// for — raise the constant rather than silently dropping a legal action.
    #[inline]
    pub fn push(&mut self, action: Action) {
        let len = self.len as usize;
        if len == MAX_LEGAL_ACTIONS {
            panic!("legal actions exceeded MAX_LEGAL_ACTIONS ({})", MAX_LEGAL_ACTIONS);
        }
        self.actions[len] = action;
        self.len += 1;
    }
}

impl std::ops::Deref for LegalActions {
    type Target = [Action];

    #[inline]
    fn deref(&self) -> &[Action] {
        &self.actions[..self.len as usize]
    }
}

impl Extend<Action> for LegalActions {
    fn extend<I: IntoIterator<Item = Action>>(&mut self, iter: I) {
        for action in iter {
            self.push(action);
        }
    }
}

impl FromIterator<Action> for LegalActions {
    fn from_iter<I: IntoIterator<Item = Action>>(iter: I) -> Self {
        let mut actions = LegalActions::new();
        actions.extend(iter);
        actions
    }
}

impl IntoIterator for LegalActions {
    type Item = Action;
    type IntoIter = std::iter::Take<std::array::IntoIter<Action, MAX_LEGAL_ACTIONS>>;

    fn into_iter(self) -> Self::IntoIter {
        self.actions.into_iter().take(self.len as usize)
    }
}

impl<'a> IntoIterator for &'a LegalActions {
    type Item = &'a Action;
    type IntoIter = std::slice::Iter<'a, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::fmt::Debug for LegalActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass() -> Action {
        Action { typ: ActionType::Pass, card: None }
    }

    #[test]
    fn legal_actions_hold_up_to_the_maximum() {
        let actions: LegalActions = std::iter::repeat_n(pass(), MAX_LEGAL_ACTIONS).collect();
        assert_eq!(actions.len(), MAX_LEGAL_ACTIONS);
        assert_eq!(actions.into_iter().count(), MAX_LEGAL_ACTIONS);
    }

    #[test]
    #[should_panic(expected = "legal actions exceeded MAX_LEGAL_ACTIONS (16)")]
    fn pushing_past_the_maximum_panics() {
        let mut actions: LegalActions = std::iter::repeat_n(pass(), MAX_LEGAL_ACTIONS).collect();
        actions.push(pass());
    }
}

"""
Action space for the FaB gym environment.

An "action" is one of:
  - Play a card from hand (two-step: first choose the card, then choose which cards to pitch)
  - Activate weapon attack
  - Pass (end action phase, move to end phase)

We use a structured Action dataclass rather than a flat integer so agents can
reason clearly. The environment exposes `legal_actions(player, opponent)` which
returns the list of Action objects valid right now. The agent picks an index into
that list; the env decodes it.

ActionType enum:
  PLAY_CARD   — choose a card to play (from hand or from arsenal); no pitch yet
  PITCH       — (second step) choose which hand cards to pitch to cover the card's cost
  WEAPON      — activate weapon attack
  PASS        — end your action phase
  DEFEND      — during the defender's decision step: choose which cards/equip to use
  ARSENAL     — store a card in arsenal at end of turn
"""

from __future__ import annotations
from dataclasses import dataclass, field
from enum import Enum, auto
from typing import List, Optional, TYPE_CHECKING

if TYPE_CHECKING:
    from game_state import Player
    from cards import Card


class ActionType(Enum):
    PLAY_CARD          = auto()
    PITCH              = auto()   # second step: choose cards to pitch for the pending play
    WEAPON             = auto()
    PASS               = auto()
    DEFEND             = auto()   # used during the defend decision
    ARSENAL            = auto()   # used during the arsenal decision
    ACTIVATE_EQUIPMENT = auto()   # activate an equipment's once-per-turn ability
    GO_FIRST           = auto()   # CHOOSE_FIRST phase: choosing player elects to go first
    GO_SECOND          = auto()   # CHOOSE_FIRST phase: choosing player elects to go second
    PASS_PRIORITY      = auto()   # INSTANT phase: player passes priority without playing


@dataclass
class Action:
    action_type: ActionType

    # PLAY_CARD fields (step 1: choose which card to play)
    card: Optional[Card] = None
    from_arsenal: bool = False

    # PITCH fields (step 2: choose which hand cards to pitch to cover the cost)
    pitch_indices: List[int] = field(default_factory=list)  # indices into player.hand

    # DEFEND fields
    defend_hand_indices: List[int] = field(default_factory=list)
    defend_equip_slots: List[str] = field(default_factory=list)  # e.g. ["head", "legs"]

    # ARSENAL field
    arsenal_hand_index: int = -1  # -1 means "don't store anything"

    # ACTIVATE_EQUIPMENT field
    equip_slot: str = ""  # e.g. "head"

    def __repr__(self):
        if self.action_type == ActionType.PLAY_CARD:
            src = f"hand[{self.card.name}]"
            return f"Action(PLAY_CARD {src})"
        if self.action_type == ActionType.PITCH:
            return f"Action(PITCH indices={self.pitch_indices})"
        if self.action_type == ActionType.WEAPON:
            return "Action(WEAPON)"
        if self.action_type == ActionType.PASS:
            return "Action(PASS)"
        if self.action_type == ActionType.DEFEND:
            return f"Action(DEFEND hand={self.defend_hand_indices} equip={self.defend_equip_slots})"
        if self.action_type == ActionType.ARSENAL:
            return f"Action(ARSENAL store={self.arsenal_hand_index})"
        if self.action_type == ActionType.ACTIVATE_EQUIPMENT:
            return f"Action(ACTIVATE_EQUIPMENT slot={self.equip_slot})"
        if self.action_type == ActionType.GO_FIRST:
            return "Action(GO_FIRST)"
        if self.action_type == ActionType.GO_SECOND:
            return "Action(GO_SECOND)"
        if self.action_type == ActionType.PASS_PRIORITY:
            return "Action(PASS_PRIORITY)"
        return f"Action({self.action_type})"


# ──────────────────────────────────────────────────────────────
# Legal action generation
# ──────────────────────────────────────────────────────────────

def _pitch_combinations(hand: List['Card'], exclude_idx: int, needed: int):
    """
    Yield lists of hand indices that sum pitch value to >= needed,
    preferring fewest cards and highest-pitch-value cards first.
    Returns at most a handful of representative combos to keep the
    action space tractable.
    """
    from itertools import combinations

    pitchable = [(i, c) for i, c in enumerate(hand) if i != exclude_idx and c.pitch > 0]
    # Sort descending by pitch value so we find minimal combos first
    pitchable.sort(key=lambda x: x[1].pitch, reverse=True)

    found = set()
    for size in range(1, len(pitchable) + 1):
        for combo in combinations(pitchable, size):
            total = sum(c.pitch for _, c in combo)
            if total >= needed:
                key = tuple(sorted(i for i, _ in combo))
                if key not in found:
                    found.add(key)
                    yield list(key)
                    break  # one combo per size is enough for tractability
        if found:
            # Once we found a combo of this size, also look for the single
            # smallest-size combo (already done above). Stop at size 3 for tractability.
            if size >= 3:
                break


def legal_attack_actions(player: 'Player') -> List[Action]:
    """
    All legal PLAY_CARD and WEAPON actions during the action phase.
    PLAY_CARD actions only select which card to play; pitch choices come in a
    separate PITCH step if the card's cost exceeds current resource_points.
    Also includes PASS.
    """
    from cards import CardType

    actions: List[Action] = []

    if player.action_points < 1:
        return [Action(ActionType.PASS)]

    # ── Arsenal card ──
    if player.arsenal and player.arsenal.card_type not in (
        __import__('cards').CardType.MENTOR,
    ):
        card = player.arsenal
        needed = max(0, card.cost - player.resource_points)
        total_pitch = sum(c.pitch for c in player.hand if c.pitch > 0)
        if needed == 0 or total_pitch >= needed:
            actions.append(Action(ActionType.PLAY_CARD, card=player.arsenal, from_arsenal=True))

    # ── Hand cards ──
    seen_play_names: set = set()
    for i, card in enumerate(player.hand):
        if card.card_type in (CardType.DEFENSE_REACTION, CardType.ATTACK_REACTION):
            continue  # reactions are played in the reaction step, not freely
        if card.name in seen_play_names:
            continue  # duplicate card — same choice regardless of which copy is picked
        seen_play_names.add(card.name)
        needed = max(0, card.cost - player.resource_points)
        if needed == 0:
            actions.append(Action(ActionType.PLAY_CARD, card=card))
        else:
            # Card costs more than current resources — playable only if hand can cover with pitch
            pitchable_total = sum(c.pitch for j, c in enumerate(player.hand)
                                  if j != i and c.pitch > 0)
            if pitchable_total >= needed:
                actions.append(Action(ActionType.PLAY_CARD, card=card))

    # ── Weapon ──
    if player.weapon:
        from cards import CardType as CT
        is_dawnblade = "Dawnblade" in player.weapon.name
        weapon_cost = player.weapon.cost
        can_use = True
        if not is_dawnblade and player.weapon_used_this_turn:
            can_use = False
        if is_dawnblade and player.weapon_used_this_turn:
            # Dawnblade can attack again only if go again was granted AND the extra attack hasn't been used
            if not (player.next_weapon_go_again or player.weapon_additional_attack):
                can_use = False
        available_resources = player.resource_points + sum(c.pitch for c in player.hand)
        if can_use and available_resources >= weapon_cost:
            actions.append(Action(ActionType.WEAPON))

    # ── Equipment activations ──
    # Blossom of Spring: Once per Combat Chain — 0: gain 1 resource, then destroy
    blossom = player.equipment.get("chest")
    if (blossom and blossom.active and not blossom.destroyed
            and blossom.card.name == "Blossom of Spring"):
        actions.append(Action(ActionType.ACTIVATE_EQUIPMENT, equip_slot="chest"))

    # Always legal to pass
    actions.append(Action(ActionType.PASS))
    return actions


def legal_pitch_actions(player: 'Player', pending_card: 'Card') -> List[Action]:
    """
    Legal PITCH actions for the sequential pitch step.
    Returns one action per pitchable card in hand so the player selects cards
    one at a time.  The phase repeats until resource_points >= pending_card.cost.
    If cost is already covered, returns a single no-pitch action (safety net).
    """
    needed = max(0, pending_card.cost - player.resource_points)

    if needed == 0:
        return [Action(ActionType.PITCH, pitch_indices=[])]

    # Sort by descending pitch value so agents pitching greedily (legal[0]) pick
    # the highest-value card first, minimising the number of pitch steps needed.
    pitchable = sorted(
        ((i, c) for i, c in enumerate(player.hand) if c.pitch > 0),
        key=lambda x: (-x[1].pitch, x[0]),  # highest pitch first, then by index
    )
    seen_pitch_names: set = set()
    actions = []
    for i, c in pitchable:
        if c.name in seen_pitch_names:
            continue  # duplicate card — same pitch value regardless of which copy is picked
        seen_pitch_names.add(c.name)
        actions.append(Action(ActionType.PITCH, pitch_indices=[i]))
    return actions if actions else [Action(ActionType.PITCH, pitch_indices=[])]


def legal_defend_actions(player: 'Player', attack_power: int,
                         already_indices: List[int] = None,
                         already_equip: List[str] = None) -> List[Action]:
    """
    Legal DEFEND actions for one-card-at-a-time blocking.

    Returns a "done" action plus one action per remaining blockable card/equip
    that hasn't been committed yet this defend step.

    already_indices / already_equip track cards already accumulated this step.
    """
    from cards import CardType

    if already_indices is None:
        already_indices = []
    if already_equip is None:
        already_equip = []

    actions: List[Action] = []

    # Done — commit all accumulated block cards (or take full damage if none chosen)
    actions.append(Action(ActionType.DEFEND))

    # NOTE: instants are NOT offered here. They are played during the attack
    # reaction INSTANT window (opened before DEFEND begins) where they go on
    # the stack and resolve in LIFO order. Blocking cards do not use the stack.
    defenders = [(i, c) for i, c in enumerate(player.hand)
                 if c.defense > 0 and c.card_type != CardType.INSTANT and not c.no_block
                 and i not in already_indices]
    equip_slots = [slot for slot, eq in player.equipment.items()
                   if eq.active and eq.defense > 0 and slot not in already_equip]

    # One card at a time — deduplicate identical cards (same choice regardless of copy)
    seen_defend_names: set = set()
    for i, c in defenders:
        if c.name in seen_defend_names:
            continue
        seen_defend_names.add(c.name)
        actions.append(Action(ActionType.DEFEND, defend_hand_indices=[i]))

    # One equipment slot at a time
    for slot in equip_slots:
        actions.append(Action(ActionType.DEFEND, defend_equip_slots=[slot]))

    return actions


def legal_arsenal_actions(player: 'Player') -> List[Action]:
    """End-of-turn: store a card or store nothing."""
    actions = [Action(ActionType.ARSENAL, arsenal_hand_index=-1)]  # don't store
    if not player.arsenal:
        seen_arsenal_names: set = set()
        for i, card in enumerate(player.hand):
            if card.name in seen_arsenal_names:
                continue  # duplicate card — same choice regardless of which copy is stored
            seen_arsenal_names.add(card.name)
            actions.append(Action(ActionType.ARSENAL, arsenal_hand_index=i))
    return actions


def legal_choose_first_actions() -> List[Action]:
    """CHOOSE_FIRST phase: the randomly selected player picks whether to go first or second."""
    return [Action(ActionType.GO_FIRST), Action(ActionType.GO_SECOND)]


def legal_instant_actions(player: 'Player') -> List[Action]:
    """
    Legal actions during an INSTANT window. Either player may play an instant
    from hand (paying its cost), or pass priority. When both players pass
    consecutively, the top of the instant stack resolves; when the stack is
    empty and both pass, play returns to the phase that opened the window.

    PASS_PRIORITY is always listed first so agents that default to ``legal[0]``
    gracefully close the window.
    """
    from cards import CardType

    actions: List[Action] = [Action(ActionType.PASS_PRIORITY)]

    seen: set = set()
    for card in player.hand:
        if card.card_type != CardType.INSTANT:
            continue
        if card.name in seen:
            continue
        seen.add(card.name)
        needed = max(0, card.cost - player.resource_points)
        if needed == 0:
            actions.append(Action(ActionType.PLAY_CARD, card=card))
            continue
        # Does the rest of hand have enough pitch to cover the cost?
        pitchable_total = sum(c.pitch for c in player.hand
                              if c is not card and c.pitch > 0)
        if pitchable_total >= needed:
            actions.append(Action(ActionType.PLAY_CARD, card=card))
    return actions

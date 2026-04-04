"""
Action space for the FaB gym environment.

An "action" is one of:
  - Play a card from hand (+ pitch cards to cover cost)
  - Activate weapon attack
  - Pass (end action phase, move to end phase)

We use a structured Action dataclass rather than a flat integer so agents can
reason clearly. The environment exposes `legal_actions(player, opponent)` which
returns the list of Action objects valid right now. The agent picks an index into
that list; the env decodes it.

ActionType enum:
  PLAY_CARD   — play card at hand_index, pitching pitch_indices
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
    PLAY_CARD = auto()
    WEAPON    = auto()
    PASS      = auto()
    DEFEND    = auto()   # used during the defend decision
    ARSENAL   = auto()   # used during the arsenal decision


@dataclass
class Action:
    action_type: ActionType

    # PLAY_CARD fields
    card_index: int = -1          # index into player.hand (or -1 for arsenal card)
    from_arsenal: bool = False
    pitch_indices: List[int] = field(default_factory=list)  # indices into player.hand

    # DEFEND fields
    defend_hand_indices: List[int] = field(default_factory=list)
    defend_equip_slots: List[str] = field(default_factory=list)  # e.g. ["head", "legs"]

    # ARSENAL field
    arsenal_hand_index: int = -1  # -1 means "don't store anything"

    def __repr__(self):
        if self.action_type == ActionType.PLAY_CARD:
            src = "arsenal" if self.from_arsenal else f"hand[{self.card_index}]"
            return f"Action(PLAY_CARD {src}, pitch={self.pitch_indices})"
        if self.action_type == ActionType.WEAPON:
            return "Action(WEAPON)"
        if self.action_type == ActionType.PASS:
            return "Action(PASS)"
        if self.action_type == ActionType.DEFEND:
            return f"Action(DEFEND hand={self.defend_hand_indices} equip={self.defend_equip_slots})"
        if self.action_type == ActionType.ARSENAL:
            return f"Action(ARSENAL store={self.arsenal_hand_index})"
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
        if needed == 0:
            actions.append(Action(ActionType.PLAY_CARD, card_index=-1, from_arsenal=True))
        else:
            for pitch_combo in _pitch_combinations(player.hand, exclude_idx=-999, needed=needed):
                actions.append(Action(
                    ActionType.PLAY_CARD,
                    card_index=-1,
                    from_arsenal=True,
                    pitch_indices=pitch_combo,
                ))
                break  # one pitch combo per card keeps it tractable

    # ── Hand cards ──
    for i, card in enumerate(player.hand):
        if card.card_type in (CardType.DEFENSE_REACTION, CardType.ATTACK_REACTION):
            continue  # reactions are played in the reaction step, not freely
        needed = max(0, card.cost - player.resource_points)
        if needed == 0:
            actions.append(Action(ActionType.PLAY_CARD, card_index=i))
        else:
            for pitch_combo in _pitch_combinations(player.hand, exclude_idx=i, needed=needed):
                actions.append(Action(
                    ActionType.PLAY_CARD,
                    card_index=i,
                    pitch_indices=pitch_combo,
                ))
                break

    # ── Weapon ──
    if player.weapon:
        from cards import CardType as CT
        is_dawnblade = "Dawnblade" in player.weapon.name
        weapon_cost = 1
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

    # Always legal to pass
    actions.append(Action(ActionType.PASS))
    return actions


def legal_defend_actions(player: 'Player', attack_power: int) -> List[Action]:
    """
    All legal DEFEND actions for the defender.
    Includes no-block (empty defend) and all useful subsets.
    For tractability we generate: no block, each single card, pairs, and equipment combos.
    """
    from itertools import combinations
    from cards import CardType

    actions: List[Action] = []

    # No block
    actions.append(Action(ActionType.DEFEND))

    defenders = [(i, c) for i, c in enumerate(player.hand)
                 if c.defense > 0 and c.card_type != CardType.INSTANT]
    equip_slots = [slot for slot, eq in player.equipment.items() if eq.active and eq.defense > 0]

    # Single cards
    for i, c in defenders:
        actions.append(Action(ActionType.DEFEND, defend_hand_indices=[i]))

    # Pairs of hand cards
    for (i, ci), (j, cj) in combinations(defenders, 2):
        actions.append(Action(ActionType.DEFEND, defend_hand_indices=[i, j]))

    # Triples of hand cards
    for (i, ci), (j, cj), (k, ck) in combinations(defenders, 3):
        actions.append(Action(ActionType.DEFEND, defend_hand_indices=[i, j, k]))

    # All four hand cards
    if len(defenders) == 4:
        indices = [i for i, _ in defenders]
        actions.append(Action(ActionType.DEFEND, defend_hand_indices=indices))

    # Single equipment slots
    for slot in equip_slots:
        actions.append(Action(ActionType.DEFEND, defend_equip_slots=[slot]))

    # Card + equipment
    for i, c in defenders:
        for slot in equip_slots:
            actions.append(Action(ActionType.DEFEND,
                                  defend_hand_indices=[i],
                                  defend_equip_slots=[slot]))

    return actions


def legal_arsenal_actions(player: 'Player') -> List[Action]:
    """End-of-turn: store a card or store nothing."""
    actions = [Action(ActionType.ARSENAL, arsenal_hand_index=-1)]  # don't store
    if not player.arsenal:
        for i in range(len(player.hand)):
            actions.append(Action(ActionType.ARSENAL, arsenal_hand_index=i))
    return actions

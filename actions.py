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
from dataclasses import dataclass
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
    PITCH_ORDER        = auto()   # end-of-turn: place one card from pitch zone to deck bottom
    MENTOR_FLIP        = auto()   # start of turn: choose to flip face-down mentor face-up
    ACTIVATE_CARD_ABILITY = auto()  # activate a defending card's once-per-turn instant ability


@dataclass
class Action:
    action_type: ActionType

    # PLAY_CARD fields (step 1: choose which card to play)
    card: Optional[Card] = None
    from_arsenal: bool = False

    # PITCH fields (step 2: choose which hand cards to pitch to cover the cost)
    pitch_index: Optional[int] = None  # index into player.hand (None = no pitch needed)

    # DEFEND / ARSENAL shared field — index into player.hand (None = no card)
    hand_index: Optional[int] = None

    # DEFEND / ACTIVATE_EQUIPMENT shared field — equipment slot (None = no equipment)
    equip_slot: Optional[str] = None  # e.g. "head"

    # PITCH_ORDER field
    pitch_order_index: int = -1  # index into player.pitch_zone

    # MENTOR_FLIP field
    flip: bool = False  # True = flip mentor face-up, False = keep face-down

    def __repr__(self):
        if self.action_type == ActionType.PLAY_CARD:
            src = f"hand[{self.card.name}]"
            return f"Action(PLAY_CARD {src})"
        if self.action_type == ActionType.PITCH:
            return f"Action(PITCH index={self.pitch_index})"
        if self.action_type == ActionType.WEAPON:
            return "Action(WEAPON)"
        if self.action_type == ActionType.PASS:
            return "Action(PASS)"
        if self.action_type == ActionType.DEFEND:
            return f"Action(DEFEND hand={self.hand_index} equip={self.equip_slot})"
        if self.action_type == ActionType.ARSENAL:
            return f"Action(ARSENAL store={self.hand_index})"
        if self.action_type == ActionType.ACTIVATE_EQUIPMENT:
            return f"Action(ACTIVATE_EQUIPMENT slot={self.equip_slot})"
        if self.action_type == ActionType.GO_FIRST:
            return "Action(GO_FIRST)"
        if self.action_type == ActionType.GO_SECOND:
            return "Action(GO_SECOND)"
        if self.action_type == ActionType.PASS_PRIORITY:
            return "Action(PASS_PRIORITY)"
        if self.action_type == ActionType.PITCH_ORDER:
            return f"Action(PITCH_ORDER index={self.pitch_order_index})"
        if self.action_type == ActionType.MENTOR_FLIP:
            return f"Action(MENTOR_FLIP flip={self.flip})"
        if self.action_type == ActionType.ACTIVATE_CARD_ABILITY:
            card_name = self.card.name if self.card else None
            return f"Action(ACTIVATE_CARD_ABILITY card={card_name} discard_hand={self.hand_index})"
        return f"Action({self.action_type})"


# ──────────────────────────────────────────────────────────────
# Legal action generation
# ──────────────────────────────────────────────────────────────

def _card_has_discard_cost(card: 'Card') -> bool:
    """Return True if *card* requires discarding a card as an additional play cost."""
    from card_effects import EffectAction
    return any(e.action == EffectAction.DISCARD_CARD_COST for e in card.effects)


def _card_has_reveal_cost(card: 'Card') -> bool:
    """Return True if *card* requires revealing a cost ≤ 1 card as an additional play cost."""
    from card_effects import EffectAction
    return any(e.action == EffectAction.REVEAL_CARD_COST for e in card.effects)


def _has_reveal_available(available_cards: List['Card'], needed: int) -> bool:
    """Return True if *available_cards* can cover *needed* pitch AND still leave a cost ≤ 1 card.

    Reserves the cost ≤ 1 card with the lowest pitch value (maximising remaining pitch pool)
    then checks whether the rest covers *needed*.
    """
    low_cost = [c for c in available_cards if c.cost <= 1]
    if not low_cost:
        return False
    if needed == 0:
        return True
    # Reserve the low-cost card that wastes the least pitch potential.
    reserved = min(low_cost, key=lambda c: c.pitch)
    remaining = list(available_cards)
    remaining.remove(reserved)
    return sum(c.pitch for c in remaining if c.pitch > 0) >= needed


def _has_discard_available(available_cards: List['Card'], needed: int) -> bool:
    """Return True if *available_cards* can cover *needed* pitch resources AND
    still leave at least one card in hand for the discard additional cost.

    Uses a greedy (highest-pitch-first) strategy to minimise the number of
    cards pitched, which maximises the cards remaining for the discard.
    """
    if needed == 0:
        return len(available_cards) >= 1
    pitchable = sorted([c for c in available_cards if c.pitch > 0], key=lambda c: -c.pitch)
    accumulated = 0
    for i, c in enumerate(pitchable):
        accumulated += c.pitch
        if accumulated >= needed:
            return (len(available_cards) - (i + 1)) >= 1
    return False


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
    if player.arsenal and player.arsenal.card_type in (
        __import__('cards').CardType.ACTION_ATTACK,
        __import__('cards').CardType.INSTANT,
        __import__('cards').CardType.ACTION,
    ):
        card = player.arsenal
        needed = max(0, card.cost - player.resource_points)
        if _card_has_discard_cost(card):
            # Arsenal card not in hand, so all hand cards are available for pitch/discard
            if _has_discard_available(player.hand, needed):
                actions.append(Action(ActionType.PLAY_CARD, card=player.arsenal, from_arsenal=True))
        elif _card_has_reveal_cost(card):
            # Arsenal card not in hand; all hand cards available for pitch, must keep one cost ≤ 1
            if _has_reveal_available(player.hand, needed):
                actions.append(Action(ActionType.PLAY_CARD, card=player.arsenal, from_arsenal=True))
        else:
            total_pitch = sum(c.pitch for c in player.hand if c.pitch > 0)
            if needed == 0 or total_pitch >= needed:
                actions.append(Action(ActionType.PLAY_CARD, card=player.arsenal, from_arsenal=True))

    # ── Hand cards ──
    seen_play_names: set = set()
    for i, card in enumerate(player.hand):
        if card.card_type in (CardType.DEFENSE_REACTION, CardType.ATTACK_REACTION, CardType.MENTOR, CardType.RESOURCE):
            continue  # reactions are played in the reaction step, not freely; mentors/resources are not playable
        if card.name in seen_play_names:
            continue  # duplicate card — same choice regardless of which copy is picked
        seen_play_names.add(card.name)
        needed = max(0, card.cost - player.resource_points)
        other = [c for j, c in enumerate(player.hand) if j != i]
        if _card_has_discard_cost(card):
            # Must be able to cover pitch cost AND keep >= 1 card in hand for discard
            if _has_discard_available(other, needed):
                actions.append(Action(ActionType.PLAY_CARD, card=card))
        elif _card_has_reveal_cost(card):
            # Must be able to cover pitch cost AND keep >= 1 cost ≤ 1 card in hand to reveal
            if _has_reveal_available(other, needed):
                actions.append(Action(ActionType.PLAY_CARD, card=card))
        elif needed == 0:
            actions.append(Action(ActionType.PLAY_CARD, card=card))
        else:
            # Card costs more than current resources — playable only if hand can cover with pitch
            pitchable_total = sum(c.pitch for c in other if c.pitch > 0)
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
    # Blossom of Spring: Action — 0: gain 1 resource, go again, then destroy
    blossom = player.equipment.get("chest")
    if (blossom and blossom.active and not blossom.destroyed
            and blossom.card.name == "Blossom of Spring"):
        actions.append(Action(ActionType.ACTIVATE_EQUIPMENT, equip_slot="chest"))

    # Gallantry Gold: Action — 1: weapon attacks +1 power this turn, go again, then destroy
    gallantry = player.equipment.get("arms")
    if (gallantry and gallantry.active and not gallantry.destroyed
            and gallantry.card.name == "Gallantry Gold" and gallantry.card.effects):
        available = player.resource_points + sum(c.pitch for c in player.hand)
        if available >= gallantry.card.cost:
            actions.append(Action(ActionType.ACTIVATE_EQUIPMENT, equip_slot="arms"))

    # Always legal to pass
    actions.append(Action(ActionType.PASS))
    return actions


def legal_pitch_actions(player: 'Player', pending_card: 'Card') -> List[Action]:
    """
    Legal PITCH actions for the sequential pitch step.
    Returns one action per pitchable card in hand so the player selects cards
    one at a time.  The phase repeats until resource_points >= pending_card.cost.
    If cost is already covered, returns a single no-pitch action (safety net).

    For cards with a discard additional cost, a card may only be pitched if the
    remaining hand can still cover the outstanding cost (after this pitch) while
    preserving at least one card for the discard.  This prevents a suboptimal
    pitch sequence from stranding resources and bypassing the full cost.
    """
    needed = max(0, pending_card.cost - player.resource_points)

    if needed == 0:
        return [Action(ActionType.PITCH)]

    has_discard_cost = _card_has_discard_cost(pending_card)
    has_reveal_cost = _card_has_reveal_cost(pending_card)

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
        if has_discard_cost:
            remaining = [h for j, h in enumerate(player.hand) if j != i]
            remaining_needed = needed - c.pitch
            if remaining_needed <= 0:
                # This card alone covers the cost; need >= 1 left for discard
                if len(remaining) >= 1:
                    actions.append(Action(ActionType.PITCH, pitch_index=i))
            else:
                # Still need more after this pitch — remaining cards must be able to cover
                if _has_discard_available(remaining, remaining_needed):
                    actions.append(Action(ActionType.PITCH, pitch_index=i))
        else:
            if has_reveal_cost:
                # Don't pitch this card if it would leave no cost ≤ 1 card in hand to reveal
                hand_after = [card for j, card in enumerate(player.hand) if j != i]
                if not any(card.cost <= 1 for card in hand_after):
                    continue
            actions.append(Action(ActionType.PITCH, pitch_index=i))
    return actions if actions else [Action(ActionType.PITCH)]


def legal_defend_actions(player: 'Player', attack_power: int) -> List[Action]:
    """
    Legal DEFEND actions for one-card-at-a-time blocking.

    Returns a "done" action plus one action per remaining blockable card/equip.
    Cards already chosen are removed from hand immediately; equipment already
    chosen is removed from player.equipment and placed on the combat chain.
    """
    from cards import CardType, Keyword

    actions: List[Action] = []

    # Done — commit all accumulated block cards (or take full damage if none chosen)
    actions.append(Action(ActionType.DEFEND))

    defenders = [(i, c) for i, c in enumerate(player.hand)
                 if not c.no_block]
    equip_slots = [slot for slot, eq in player.equipment.items()
                   if eq.active]

    # One card at a time — deduplicate identical cards (same choice regardless of copy)
    seen_defend_names: set = set()
    for i, c in defenders:
        if c.name in seen_defend_names:
            continue
        seen_defend_names.add(c.name)
        actions.append(Action(ActionType.DEFEND, hand_index=i))

    # One equipment slot at a time
    for slot in equip_slots:
        actions.append(Action(ActionType.DEFEND, equip_slot=slot))

    return actions


def legal_arsenal_actions(player: 'Player') -> List[Action]:
    """End-of-turn: store a card or store nothing."""
    actions = [Action(ActionType.ARSENAL)]  # hand_index=None means don't store
    if not player.arsenal:
        seen_arsenal_names: set = set()
        for i, card in enumerate(player.hand):
            if card.name in seen_arsenal_names:
                continue  # duplicate card — same choice regardless of which copy is stored
            seen_arsenal_names.add(card.name)
            actions.append(Action(ActionType.ARSENAL, hand_index=i))
    return actions


def legal_choose_first_actions() -> List[Action]:
    """CHOOSE_FIRST phase: the randomly selected player picks whether to go first or second."""
    return [Action(ActionType.GO_FIRST), Action(ActionType.GO_SECOND)]


def legal_mentor_flip_actions() -> List[Action]:
    """MENTOR_FLIP phase: active player may flip their face-down mentor face-up or keep it down."""
    return [Action(ActionType.MENTOR_FLIP, flip=True),
            Action(ActionType.MENTOR_FLIP, flip=False)]


def legal_reaction_actions(player: 'Player', attacker_idx: int,
                           priority_idx: int,
                           pending_is_sword_attack: bool = False,
                           pending_is_weapon_attack: bool = False,
                           committed_defend_cards: Optional[List['Card']] = None,
                           rally_ability_used: bool = False) -> List[Action]:
    """
    Legal actions during the reaction phase (between defender committing blocks
    and combat damage resolution).

    Attacker may play ATTACK_REACTION or INSTANT cards.
    Defender may play DEFENSE_REACTION or INSTANT cards.
    Defender may also activate Rally the Rearguard's once-per-turn instant ability
    (discard a card: +3 block) if Rally is among the committed blocking cards.
    Either player may always pass priority.
    """
    from cards import CardType
    from card_effects import EffectTrigger, EffectAction

    actions: List[Action] = [Action(ActionType.PASS_PRIORITY)]
    is_attacker = priority_idx == attacker_idx
    seen: set = set()

    candidates = list(player.hand)
    if player.arsenal is not None:
        candidates.append(player.arsenal)

    for card in candidates:
        if card.name in seen:
            continue

        if card.card_type == CardType.INSTANT:
            pass  # either player may play instants
        elif card.card_type == CardType.ATTACK_REACTION and is_attacker:
            ctx = {"weapon_attack_count": player.weapon_attack_count,
                   "is_weapon_attack": pending_is_weapon_attack}
            if card.play_condition is not None and not card.play_condition(ctx):
                continue
            requires_sword = any(
                e.trigger == EffectTrigger.ON_ATTACK_REACTION
                and e.action == EffectAction.SWORD_ATTACK_GO_AGAIN
                for e in (card.effects or [])
            )
            if requires_sword and not pending_is_sword_attack:
                continue
        elif card.card_type == CardType.DEFENSE_REACTION and not is_attacker:
            pass
        else:
            continue

        seen.add(card.name)
        from_arsenal = card is player.arsenal
        needed = max(0, card.cost - player.resource_points)
        if needed == 0:
            actions.append(Action(ActionType.PLAY_CARD, card=card, from_arsenal=from_arsenal))
        else:
            pitchable_total = sum(c.pitch for c in player.hand
                                  if c is not card and c.pitch > 0)
            if pitchable_total >= needed:
                actions.append(Action(ActionType.PLAY_CARD, card=card, from_arsenal=from_arsenal))

    # Rally the Rearguard: "Once per turn Instant — Discard a card: +3 block.
    # Activate only while Rally the Rearguard is defending."
    if not is_attacker and not rally_ability_used and committed_defend_cards:
        rally_card = next(
            (c for c in committed_defend_cards if c.name == "Rally the Rearguard"),
            None,
        )
        if rally_card is not None and player.hand:
            seen_discard: set = set()
            for i, c in enumerate(player.hand):
                if c.name in seen_discard:
                    continue
                seen_discard.add(c.name)
                actions.append(Action(ActionType.ACTIVATE_CARD_ABILITY,
                                      card=rally_card, hand_index=i))

    return actions


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


def legal_pitch_order_actions(player: 'Player') -> List[Action]:
    """One PITCH_ORDER action per distinct card name remaining in the pitch zone.
    The player selects cards one at a time; each chosen card is placed at the
    bottom of the deck, so the last card selected ends up closest to the top."""
    seen: set = set()
    actions = []
    for i, c in enumerate(player.pitch_zone):
        if c.name not in seen:
            seen.add(c.name)
            actions.append(Action(ActionType.PITCH_ORDER, pitch_order_index=i))
    return actions

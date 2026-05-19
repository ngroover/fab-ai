"""
Rule-based agents for FaBEnv.

Each agent implements:
    select_action(obs, legal, player, opponent, context=None) -> Action

`context` is the dict returned by `FaBEnv.build_action_context()`; it
always carries a `"phase"` key (a `Phase` enum value) and may carry
phase-specific extras like `attack_power`, `pending_card`, or
`is_attacker`. Agents that don't care about phase ignore it.

HumanAgent prompts a human player via stdin for each decision.
"""

from __future__ import annotations
import random as _random_module
from typing import List, Optional, TYPE_CHECKING

from actions import Action, ActionType
from cards import CardType, Color, Keyword

if TYPE_CHECKING:
    from game_state import Player


# ──────────────────────────────────────────────────────────────
# Shared utilities
# ──────────────────────────────────────────────────────────────

def _can_afford(player: 'Player', cost: int, exclude=None) -> bool:
    available = player.resource_points
    for c in player.hand:
        if c is exclude:
            continue
        available += c.pitch
    return available >= cost


def _best_pitch_action_for(legal: List[Action], card_name: str,
                            from_arsenal: bool = False) -> Optional[Action]:
    """Find the PLAY_CARD action that plays a given card name."""
    for a in legal:
        if a.action_type != ActionType.PLAY_CARD:
            continue
        if a.from_arsenal != from_arsenal:
            continue
        return a  # caller matched card before calling this
    return None


def _find_play(legal: List[Action], card_names: List[str],
               player: 'Player') -> Optional[Action]:
    """
    Find a legal PLAY_CARD action for the first card in card_names that's in hand/arsenal.
    """
    for name in card_names:
        for a in legal:
            if a.action_type != ActionType.PLAY_CARD:
                continue
            if a.from_arsenal:
                card = player.arsenal
            elif a.card in player.hand:
                card = a.card
            else:
                continue
            if card and card.name == name:
                return a
    return None


def _find_weapon(legal: List[Action]) -> Optional[Action]:
    return next((a for a in legal if a.action_type == ActionType.WEAPON), None)


def _pass_action(legal: List[Action]) -> Action:
    return next(a for a in legal if a.action_type == ActionType.PASS)


# ──────────────────────────────────────────────────────────────
# Random agent (hero-agnostic, isolated RNG)
# ──────────────────────────────────────────────────────────────

class RandomAgent:
    """
    Selects uniformly at random from legal actions for every decision.

    Uses a private random.Random instance so its choices never disturb the
    game-state RNG (deck shuffles, coin flips, etc.).
    """

    def __init__(self, seed: Optional[int] = None):
        self._rng = _random_module.Random(seed)

    def select_action(self, obs: Optional[dict], legal: List[Action],
                      player: 'Player', opponent: Optional['Player'] = None,
                      context: Optional[dict] = None) -> Action:
        return self._rng.choice(legal)


# ──────────────────────────────────────────────────────────────
# Human agent (interactive stdin input)
# ──────────────────────────────────────────────────────────────

class HumanAgent:
    """
    Interactive agent that prompts the human player via stdin.

    Implements the unified `select_action(obs, legal, player, opponent,
    context)` interface and dispatches on `context["phase"]` to show the
    right prompt for each game phase.
    """

    # ── Display helpers ──────────────────────────────────────

    def _fmt_card(self, card) -> str:
        parts = [card.name]
        details = []
        if card.cost:
            details.append(f"cost:{card.cost}")
        if card.pitch:
            details.append(f"pitch:{card.pitch}")
        if card.power:
            details.append(f"pow:{card.power}")
        if card.defense:
            details.append(f"def:{card.defense}")
        if Keyword.GO_AGAIN in card.keywords:
            details.append("go-again")
        if Keyword.INTIMIDATE in card.keywords:
            details.append("intimidate")
        if details:
            parts.append(f"({', '.join(details)})")
        return " ".join(parts)

    def _show_hand(self, player: 'Player'):
        print("\n  Hand:")
        if player.hand:
            for i, c in enumerate(player.hand):
                print(f"    [{i}] {self._fmt_card(c)}")
        else:
            print("    (empty)")
        if player.arsenal:
            print(f"  Arsenal: {self._fmt_card(player.arsenal)}")

    def _show_equipment(self, player: 'Player'):
        active = {slot: eq for slot, eq in player.equipment.items()
                  if eq.active and eq.defense > 0}
        if active:
            print("  Equipment:")
            for slot, eq in active.items():
                print(f"    {slot}: {eq.card.name} (def:{eq.defense})")

    def _show_stats(self, player: 'Player', opponent: 'Player'):
        res_avail = player.resource_points + sum(c.pitch for c in player.hand)
        print(f"\n  You ({player.name} / {player.hero_name}): "
              f"{player.life} life | {player.action_points} action pt(s) | "
              f"{player.resource_points} resources (total available: {res_avail})")
        if player.weapon:
            wp = player.get_effective_weapon_power()
            print(f"  Weapon: {player.weapon.name} power {wp}"
                  + (" [used]" if player.weapon_used_this_turn else ""))
        print(f"  Opponent ({opponent.name} / {opponent.hero_name}): {opponent.life} life")

    def _action_label(self, action: Action, player: 'Player') -> str:
        if action.action_type == ActionType.PASS:
            return "PASS (end action phase)"
        if action.action_type == ActionType.PASS_PRIORITY:
            return "PASS PRIORITY (let the stack resolve)"
        if action.action_type == ActionType.WEAPON:
            wp = player.get_effective_weapon_power()
            return f"WEAPON — attack with {player.weapon.name} for {wp} power"
        if action.action_type == ActionType.ACTIVATE_EQUIPMENT:
            slot = action.equip_slot
            eq = player.equipment.get(slot)
            name = eq.card.name if eq else slot
            return f"ACTIVATE equipment — {name} ({slot})"
        if action.action_type == ActionType.PLAY_CARD:
            if action.from_arsenal:
                card = player.arsenal
                src = "arsenal"
            else:
                card = action.card
                src = f"hand"
            label = f"PLAY {self._fmt_card(card)} from {src}"
            if action.pitch_index is not None and action.pitch_index < len(player.hand):
                label += f" — pitching: {player.hand[action.pitch_index].name}"
            return label
        if action.action_type == ActionType.DEFEND:
            if action.hand_index is None and action.equip_slot is None:
                return "DONE — stop adding block cards"
            parts = []
            total = 0
            if action.hand_index is not None and 0 <= action.hand_index < len(player.hand):
                c = player.hand[action.hand_index]
                parts.append(f"{c.name} (def:{c.defense})")
                total += c.defense
            if action.equip_slot is not None and action.equip_slot in player.equipment:
                eq = player.equipment[action.equip_slot]
                parts.append(f"{eq.card.name}/{action.equip_slot} (def:{eq.defense})")
                total += eq.defense
            return f"ADD to defense — {', '.join(parts)} [+{total} def]"
        if action.action_type == ActionType.PITCH:
            if action.pitch_index is None:
                return "PITCH — no cards needed (cost already covered)"
            pitched = [player.hand[action.pitch_index]] if action.pitch_index < len(player.hand) else []
            names = [self._fmt_card(c) for c in pitched]
            total = sum(c.pitch for c in pitched)
            return f"PITCH — {', '.join(names)} (total: {total} resource{'s' if total != 1 else ''})"
        if action.action_type == ActionType.ARSENAL:
            if action.hand_index is None:
                return "DON'T store (no arsenal this turn)"
            card = player.hand[action.hand_index]
            return f"STORE in arsenal — {self._fmt_card(card)}"
        if action.action_type == ActionType.PITCH_ORDER:
            if 0 <= action.pitch_order_index < len(player.pitch_zone):
                card = player.pitch_zone[action.pitch_order_index]
                return f"PLACE at deck bottom — {self._fmt_card(card)}"
        if action.action_type == ActionType.PAY_FOR_BLOCK_BONUS:
            card_name = action.card.name if action.card else "equipment"
            return f"PAY 1 resource — {card_name} gains +2 block (destroyed when chain closes)"
        return str(action)

    def _choose(self, legal: List[Action], player: 'Player', prompt: str) -> Action:
        print(f"\n{prompt}")
        for i, action in enumerate(legal):
            print(f"  {i:>2}: {self._action_label(action, player)}")
        while True:
            try:
                raw = input(f"\nYour choice (0–{len(legal) - 1}): ").strip()
                idx = int(raw)
                if 0 <= idx < len(legal):
                    return legal[idx]
                print(f"  Please enter a number between 0 and {len(legal) - 1}.")
            except (ValueError, KeyboardInterrupt):
                print("  Invalid input — enter the number of your chosen action.")

    # ── Decision dispatch ────────────────────────────────────

    def select_action(self, obs: Optional[dict], legal: List[Action],
                      player: 'Player', opponent: Optional['Player'] = None,
                      context: Optional[dict] = None) -> Action:
        phase = context.get("phase") if context else None
        phase_name = phase.name if phase is not None else "ATTACK"

        print(f"\n{'═' * 60}")
        handler = self._PHASE_HANDLERS.get(phase_name, HumanAgent._render_attack)
        prompt = handler(self, player, opponent, context or {})
        return self._choose(legal, player, prompt)

    # ── Per-phase renderers (called by select_action) ─────────

    def _render_attack(self, player, opponent, context):
        print("  ATTACK PHASE — your turn")
        if opponent is not None:
            self._show_stats(player, opponent)
        self._show_hand(player)
        self._show_equipment(player)
        return "Choose an action:"

    def _render_defend(self, player, opponent, context):
        attack_power = context.get("attack_power", 0)
        already_defense = context.get("already_defense", 0)
        print(f"  DEFEND PHASE — incoming attack power: {attack_power}")
        if already_defense > 0:
            remaining = max(0, attack_power - already_defense)
            print(f"  Defending so far: {already_defense}  (damage if you stop now: {remaining})")
        else:
            print(f"  Your life: {player.life}  (damage if unblocked: {max(0, attack_power)})")
        self._show_hand(player)
        self._show_equipment(player)
        return "Add a card to your defense (or choose done):"

    def _render_arsenal(self, player, opponent, context):
        print("  ARSENAL PHASE — end of your turn")
        print(f"  Your life: {player.life}")
        self._show_hand(player)
        return "Store a card in arsenal?"

    def _render_pitch(self, player, opponent, context):
        pending_card = context.get("pending_card")
        card_str = f" for {pending_card.name}" if pending_card else ""
        needed = max(0, pending_card.cost - player.resource_points) if pending_card else "?"
        print(f"  PITCH PHASE — choose cards to pitch{card_str}")
        print(f"  Resources still needed: {needed}")
        print(f"  Current resources: {player.resource_points}")
        self._show_hand(player)
        return "Choose cards to pitch:"

    def _render_pitch_order(self, player, opponent, context):
        print("  PITCH ORDER — choose the order to place pitched cards at deck bottom")
        print("  The first card you choose will be deepest in the deck.")
        print(f"  Pitch zone ({len(player.pitch_zone)} card(s)):")
        for i, c in enumerate(player.pitch_zone):
            print(f"    [{i}] {self._fmt_card(c)}")
        return "Which card goes to deck bottom next?"

    def _render_instant(self, player, opponent, context):
        attack_power = context.get("attack_power", 0)
        print(f"  INSTANT WINDOW — {player.name}")
        if attack_power > 0:
            print(f"  ⚔ Incoming attack: {attack_power} power")
        print(f"  Your life: {player.life} | resources: {player.resource_points}")
        self._show_hand(player)
        return "Play an instant or pass priority:"

    def _render_reaction(self, player, opponent, context):
        attack_power = context.get("attack_power", 0)
        is_attacker = context.get("is_attacker", False)
        role = "ATTACKER" if is_attacker else "DEFENDER"
        print(f"  REACTION PHASE — {player.name} ({role})")
        print(f"  ⚔ Attack power: {attack_power}")
        print(f"  Your life: {player.life} | resources: {player.resource_points}")
        if is_attacker:
            print("  You may play attack reactions or instants.")
        else:
            print("  You may play defense reactions or instants.")
        self._show_hand(player)
        return "Play a reaction card or pass priority:"

    def _render_mentor_flip(self, player, opponent, context):
        arsenal_name = player.arsenal.name if player.arsenal else "mentor"
        print(f"  START OF TURN — {arsenal_name} is face-down in your arsenal.")
        return "Flip mentor face-up?"

    def _render_choose_first(self, player, opponent, context):
        print("  CHOOSE FIRST — you won the coin flip!")
        return "Go first or second?"

    _PHASE_HANDLERS = {
        "ATTACK":       _render_attack,
        "DEFEND":       _render_defend,
        "ARSENAL":      _render_arsenal,
        "PITCH":        _render_pitch,
        "PITCH_ORDER":  _render_pitch_order,
        "INSTANT":      _render_instant,
        "REACTION":     _render_reaction,
        "MENTOR_FLIP":  _render_mentor_flip,
        "CHOOSE_FIRST": _render_choose_first,
    }

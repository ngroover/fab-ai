"""
Rule-based agents for FaBEnv.

Each agent implements:
    select_action(obs, legal_actions, player, opponent) -> Action

These are the same heuristics from ai.py, now decoupled from the engine.
They can be used as baselines or opponents during RL training.

HumanAgent prompts a human player via stdin for each decision.
"""

from __future__ import annotations
from typing import List, Optional, TYPE_CHECKING

from actions import Action, ActionType
from cards import CardType, Color
from card_effects import EffectAction

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
# Rhinar agent
# ──────────────────────────────────────────────────────────────

class RhinarAgent:
    """
    Priority:
    1. Barraging Beatdown (0-cost, intimidate, +3 on next attack)
    2. Beast Mode (0-cost, +3 on next attack) if big attack in hand
    3. Best affordable attack action (highest power + bonus)
    4. Weapon (Bone Basher)
    5. Come to Fight
    6. Pass
    """

    def select_action(self, obs: dict, legal: List[Action], player: 'Player',
                      opponent: 'Player') -> Action:
        hand = player.hand

        # 1. Barraging Beatdown
        if player.next_brute_attack_bonus == 0:
            a = _find_play(legal, ["Barraging Beatdown"], player)
            if a:
                return a

        # 2. Beast Mode — only if we have a big attack to follow
        if player.next_brute_attack_bonus == 0:
            big_attacks = [c for c in hand if c.card_type == CardType.ACTION_ATTACK and c.power >= 6]
            if big_attacks:
                a = _find_play(legal, ["Beast Mode"], player)
                if a:
                    return a

        # 3. Best affordable attack action
        attack_actions = []
        for a in legal:
            if a.action_type != ActionType.PLAY_CARD:
                continue
            if a.from_arsenal:
                card = player.arsenal
            elif a.card in hand:
                card = a.card
            else:
                continue
            if card and card.card_type == CardType.ACTION_ATTACK:
                score = card.power + player.next_brute_attack_bonus
                if card.go_again:
                    score += 1
                if any(e.action == EffectAction.INTIMIDATE for e in card.effects):
                    score += 1
                attack_actions.append((score, a))

        if attack_actions:
            attack_actions.sort(key=lambda x: x[0], reverse=True)
            return attack_actions[0][1]

        # 4. Weapon
        a = _find_weapon(legal)
        if a:
            return a

        # 5. Come to Fight
        if not player.weapon_used_this_turn:
            a = _find_play(legal, ["Come to Fight"], player)
            if a:
                return a

        return _pass_action(legal)

    def select_defend(self, obs: dict, legal: List[Action], player: 'Player',
                      attack_power: int, already_defense: int = 0) -> Action:
        """
        Add one blocking card at a time until enough defense is accumulated,
        then choose done. Prefers the card that most tightly covers remaining needed defense.
        """
        effective_power = attack_power - already_defense
        if player.life - effective_power > 8:
            return legal[0]  # no block needed

        damage_we_can_take = max(0, player.life - 6)
        needed = max(0, effective_power - damage_we_can_take)

        if needed <= 0:
            return legal[0]  # accumulated defense already sufficient

        best: Optional[Action] = None
        best_excess = float('inf')

        for a in legal:
            if a.action_type != ActionType.DEFEND:
                continue
            if not a.defend_hand_indices and not a.defend_equip_slots:
                continue  # skip "done" option
            hand_def = sum(
                player.hand[i].defense
                for i in a.defend_hand_indices
                if 0 <= i < len(player.hand)
            )
            equip_def = sum(
                player.equipment[s].defense
                for s in a.defend_equip_slots
                if s in player.equipment and player.equipment[s].active
            )
            total = hand_def + equip_def
            if total > 0:
                excess = total - needed
                if excess < best_excess:
                    best_excess = excess
                    best = a

        return best if best else legal[0]

    def select_arsenal(self, obs: dict, legal: List[Action], player: 'Player') -> Action:
        """Store a blue or low-value card."""
        priority = ["Titanium Bauble", "Dodge", "Rally the Rearguard", "Clearing Bellow"]
        for name in priority:
            for a in legal:
                if a.action_type != ActionType.ARSENAL or a.arsenal_hand_index < 0:
                    continue
                if 0 <= a.arsenal_hand_index < len(player.hand):
                    if player.hand[a.arsenal_hand_index].name == name:
                        return a
        # Store any blue
        for a in legal:
            if a.action_type != ActionType.ARSENAL or a.arsenal_hand_index < 0:
                continue
            if 0 <= a.arsenal_hand_index < len(player.hand):
                if player.hand[a.arsenal_hand_index].color == Color.BLUE:
                    return a
        return next(a for a in legal if a.action_type == ActionType.ARSENAL
                    and a.arsenal_hand_index == -1)

    def select_pitch(self, obs: dict, legal: List[Action], player: 'Player',
                     pending_card=None) -> Action:
        return legal[0]

    def select_choose_first(self, legal: List[Action], player: 'Player') -> Action:
        return legal[0]


# ──────────────────────────────────────────────────────────────
# Dorinthea agent
# ──────────────────────────────────────────────────────────────

class DorintheiAgent:
    """
    Weapon-chain focus. Give Dawnblade go again, swing twice+ per turn.
    """

    _SETUP_CARDS = [
        "Glistening Steelblade", "En Garde", "Slice and Dice", "Warrior's Valor",
        "On a Knife Edge", "Blade Flash", "Hit and Run", "Visit the Blacksmith", "Sharpen Steel",
    ]
    _CHAIN_CARDS = [
        "Second Swing", "On a Knife Edge", "Blade Flash", "Hit and Run", "Glistening Steelblade", "En Garde",
    ]

    _ATTACK_CARDS = ["Run Through", "Out for Blood", "Driving Blade"]

    def select_action(self, obs: dict, legal: List[Action], player: 'Player',
                      opponent: 'Player') -> Action:
        # Activate Blossom of Spring before first weapon swing
        if not player.weapon_used_this_turn:
            for a in legal:
                if a.action_type == ActionType.ACTIVATE_EQUIPMENT:
                    return a

        # Before weapon: setup
        if not player.weapon_used_this_turn:
            a = _find_play(legal, self._SETUP_CARDS, player)
            if a:
                return a

        # Weapon swing
        a = _find_weapon(legal)
        if a:
            return a

        # After weapon: chain enablers
        if player.weapon_used_this_turn:
            a = _find_play(legal, self._CHAIN_CARDS, player)
            if a:
                return a

        # Attack actions
        a = _find_play(legal, self._ATTACK_CARDS, player)
        if a:
            return a

        # Titanium Bauble for resources
        if player.resource_points == 0:
            a = _find_play(legal, ["Titanium Bauble"], player)
            if a:
                return a

        return _pass_action(legal)

    def select_defend(self, obs: dict, legal: List[Action], player: 'Player',
                      attack_power: int, already_defense: int = 0) -> Action:
        """
        Add one blocking card at a time. Prefers defense reactions; stops once
        accumulated defense covers the needed threshold.
        """
        effective_power = attack_power - already_defense
        if player.life - effective_power > 8:
            return legal[0]

        damage_we_can_take = max(0, player.life - 6)
        needed = max(0, effective_power - damage_we_can_take)

        if needed <= 0:
            return legal[0]  # accumulated defense already sufficient

        # Prefer defense reactions, then tightest single-card cover
        best: Optional[Action] = None
        best_score = float('-inf')

        for a in legal:
            if a.action_type != ActionType.DEFEND:
                continue
            if not a.defend_hand_indices and not a.defend_equip_slots:
                continue  # skip "done" option
            hand_def = 0
            reaction_bonus = 0
            for i in a.defend_hand_indices:
                if 0 <= i < len(player.hand):
                    c = player.hand[i]
                    hand_def += c.defense
                    if c.card_type in (CardType.DEFENSE_REACTION,):
                        reaction_bonus += 1
            equip_def = sum(
                player.equipment[s].defense
                for s in a.defend_equip_slots
                if s in player.equipment and player.equipment[s].active
            )
            total = hand_def + equip_def
            if total >= needed:
                score = reaction_bonus * 10 - (total - needed)  # prefer tight blocks w/ reactions
                if score > best_score:
                    best_score = score
                    best = a

        if best:
            return best

        # No single card covers remaining needed — if lethal, use highest partial block
        if effective_power >= player.life:
            best_partial = None
            best_partial_def = 0
            for a in legal:
                if a.action_type != ActionType.DEFEND:
                    continue
                if not a.defend_hand_indices and not a.defend_equip_slots:
                    continue
                hand_def = sum(
                    player.hand[i].defense
                    for i in a.defend_hand_indices
                    if 0 <= i < len(player.hand)
                )
                equip_def = sum(
                    player.equipment[s].defense
                    for s in a.defend_equip_slots
                    if s in player.equipment and player.equipment[s].active
                )
                total = hand_def + equip_def
                if total > best_partial_def:
                    best_partial_def = total
                    best_partial = a
            if best_partial:
                return best_partial

        return legal[0]

    def select_arsenal(self, obs: dict, legal: List[Action], player: 'Player') -> Action:
        priority = ["Sigil of Solace", "On a Knife Edge", "Blade Flash", "Hit and Run",
                    "Toughen Up", "Flock of the Feather Walkers"]
        for name in priority:
            for a in legal:
                if a.action_type != ActionType.ARSENAL or a.arsenal_hand_index < 0:
                    continue
                if 0 <= a.arsenal_hand_index < len(player.hand):
                    if player.hand[a.arsenal_hand_index].name == name:
                        return a
        for a in legal:
            if a.action_type != ActionType.ARSENAL or a.arsenal_hand_index < 0:
                continue
            if 0 <= a.arsenal_hand_index < len(player.hand):
                if player.hand[a.arsenal_hand_index].color == Color.BLUE:
                    return a
        return next(a for a in legal if a.action_type == ActionType.ARSENAL
                    and a.arsenal_hand_index == -1)

    def select_pitch(self, obs: dict, legal: List[Action], player: 'Player',
                     pending_card=None) -> Action:
        return legal[0]

    def select_choose_first(self, legal: List[Action], player: 'Player') -> Action:
        return legal[0]


# ──────────────────────────────────────────────────────────────
# Human agent (interactive stdin input)
# ──────────────────────────────────────────────────────────────

class HumanAgent:
    """
    Interactive agent that prompts the human player via stdin.

    Supports all three decision phases:
        select_action  — attack phase
        select_defend  — defend phase
        select_arsenal — end-of-turn arsenal storage
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
        if card.go_again:
            details.append("go-again")
        if any(e.action == EffectAction.INTIMIDATE for e in card.effects):
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
            if action.pitch_indices:
                pitched = [player.hand[i].name for i in action.pitch_indices]
                label += f" — pitching: {', '.join(pitched)}"
            return label
        if action.action_type == ActionType.DEFEND:
            if not action.defend_hand_indices and not action.defend_equip_slots:
                return "DONE — stop adding block cards"
            parts = []
            total = 0
            for i in action.defend_hand_indices:
                if 0 <= i < len(player.hand):
                    c = player.hand[i]
                    parts.append(f"{c.name} (def:{c.defense})")
                    total += c.defense
            for slot in action.defend_equip_slots:
                if slot in player.equipment:
                    eq = player.equipment[slot]
                    parts.append(f"{eq.card.name}/{slot} (def:{eq.defense})")
                    total += eq.defense
            return f"ADD to defense — {', '.join(parts)} [+{total} def]"
        if action.action_type == ActionType.PITCH:
            if not action.pitch_indices:
                return "PITCH — no cards needed (cost already covered)"
            pitched = [player.hand[i] for i in action.pitch_indices if i < len(player.hand)]
            names = [self._fmt_card(c) for c in pitched]
            total = sum(c.pitch for c in pitched)
            return f"PITCH — {', '.join(names)} (total: {total} resource{'s' if total != 1 else ''})"
        if action.action_type == ActionType.ARSENAL:
            if action.arsenal_hand_index == -1:
                return "DON'T store (no arsenal this turn)"
            card = player.hand[action.arsenal_hand_index]
            return f"STORE in arsenal — {self._fmt_card(card)}"
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

    # ── Decision methods ─────────────────────────────────────

    def select_action(self, obs: dict, legal: List[Action], player: 'Player',
                      opponent: 'Player') -> Action:
        print(f"\n{'═' * 60}")
        print(f"  ATTACK PHASE — your turn")
        self._show_stats(player, opponent)
        self._show_hand(player)
        self._show_equipment(player)
        return self._choose(legal, player, "Choose an action:")

    def select_defend(self, obs: dict, legal: List[Action], player: 'Player',
                      attack_power: int, already_defense: int = 0) -> Action:
        print(f"\n{'═' * 60}")
        print(f"  DEFEND PHASE — incoming attack power: {attack_power}")
        if already_defense > 0:
            remaining = max(0, attack_power - already_defense)
            print(f"  Defending so far: {already_defense}  (damage if you stop now: {remaining})")
        else:
            print(f"  Your life: {player.life}  (damage if unblocked: {max(0, attack_power)})")
        self._show_hand(player)
        self._show_equipment(player)
        return self._choose(legal, player, "Add a card to your defense (or choose done):")

    def select_arsenal(self, obs: dict, legal: List[Action], player: 'Player') -> Action:
        print(f"\n{'═' * 60}")
        print(f"  ARSENAL PHASE — end of your turn")
        print(f"  Your life: {player.life}")
        self._show_hand(player)
        return self._choose(legal, player, "Store a card in arsenal?")

    def select_pitch(self, obs: dict, legal: List[Action], player: 'Player',
                     pending_card=None) -> Action:
        print(f"\n{'═' * 60}")
        card_str = f" for {pending_card.name}" if pending_card else ""
        needed = max(0, pending_card.cost - player.resource_points) if pending_card else "?"
        print(f"  PITCH PHASE — choose cards to pitch{card_str}")
        print(f"  Resources still needed: {needed}")
        print(f"  Current resources: {player.resource_points}")
        self._show_hand(player)
        return self._choose(legal, player, "Choose cards to pitch:")

    def select_choose_first(self, legal: List[Action], player: 'Player') -> Action:
        print(f"\n{'═' * 60}")
        print(f"  CHOOSE FIRST — you won the coin flip!")
        return self._choose(legal, player, "Go first or second?")

"""
Rule-based agents for FaBEnv.

Each agent implements:
    select_action(obs, legal_actions, player, opponent) -> Action

These are the same heuristics from ai.py, now decoupled from the engine.
They can be used as baselines or opponents during RL training.
"""

from __future__ import annotations
from typing import List, Optional, TYPE_CHECKING

from actions import Action, ActionType
from cards import CardType, Color

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
            elif 0 <= a.card_index < len(player.hand):
                card = player.hand[a.card_index]
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
            elif 0 <= a.card_index < len(hand):
                card = hand[a.card_index]
            else:
                continue
            if card and card.card_type == CardType.ACTION_ATTACK:
                score = card.power + player.next_brute_attack_bonus
                if card.go_again:
                    score += 1
                if card.intimidate:
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
                      attack_power: int) -> Action:
        """
        Block if attack would leave us below 6 life.
        Prefer blues / defense reactions. Use equipment as last resort.
        """
        if player.life - attack_power > 8:
            return legal[0]  # no block (first action is always no-block)

        damage_we_can_take = max(1, player.life - 6)
        needed = max(0, attack_power - damage_we_can_take)

        best: Optional[Action] = None
        best_excess = float('inf')

        for a in legal:
            if a.action_type != ActionType.DEFEND:
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
            if total >= needed:
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
        "On a Knife Edge", "Blade Flash", "Hit and Run", "Glistening Steelblade", "En Garde",
    ]
    _ATTACK_CARDS = ["Second Swing", "Run Through", "Out for Blood", "Driving Blade"]

    def select_action(self, obs: dict, legal: List[Action], player: 'Player',
                      opponent: 'Player') -> Action:
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
                      attack_power: int) -> Action:
        if player.life - attack_power > 8:
            return legal[0]

        damage_we_can_take = max(1, player.life - 6)
        needed = max(0, attack_power - damage_we_can_take)

        # Prefer defense reactions / instants, then blues
        best: Optional[Action] = None
        best_score = -1

        for a in legal:
            if a.action_type != ActionType.DEFEND:
                continue
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

        return best if best else legal[0]

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

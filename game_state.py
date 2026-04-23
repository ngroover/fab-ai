"""
Game state management for the Classic Battles: Rhinar vs Dorinthea simulator.
Both heroes are young heroes — 20 life, intellect 4, Blitz format.
"""

import random
from typing import List, Optional, Dict, Union
from cards import Card, CardType, EquipSlot, Color
from card_effects import CardEffect


class Equipment:
    def __init__(self, card: Card):
        self.card = card
        self.destroyed = False
        self.used_this_turn = False

    def reset_turn(self):
        self.used_this_turn = False

    @property
    def active(self):
        return not self.destroyed

    @property
    def defense(self):
        return self.card.defense if not self.destroyed else 0


class Player:
    def __init__(self, name: str, life: int, intellect: int,
                 deck: List[Card], equipment_list: List[Card], weapon: Card,
                 hero_name: str = "", rng: Optional[random.Random] = None):
        self.name = name
        self.life = life
        self.intellect = intellect
        # Use the provided isolated RNG, or fall back to the global random module.
        self._rng: Union[random.Random, object] = rng if rng is not None else random

        # Pull the hero card out of the deck before shuffling.
        self.hero_card: Optional[Card] = None
        play_deck: List[Card] = []
        for c in deck:
            if c.card_type == CardType.HERO:
                self.hero_card = c
            else:
                play_deck.append(c)

        self.hero_name = hero_name or (self.hero_card.name if self.hero_card else name)

        # Collect triggered effects from the hero card so they are active for the whole game.
        self.active_effects: List[CardEffect] = list(self.hero_card.effects) if self.hero_card else []

        self.deck: List[Card] = play_deck
        self._rng.shuffle(self.deck)

        self.hand: List[Card] = []
        self.graveyard: List[Card] = []
        self.combat_chain: List[Card] = []  # cards staying on chain until it closes
        self.banished: List[Card] = []
        self.pitch_zone: List[Card] = []
        self.arsenal: Optional[Card] = None

        # Equipment
        self.equipment: Dict[str, Equipment] = {}
        for eq in equipment_list:
            if eq.equip_slot:
                self.equipment[eq.equip_slot.value] = Equipment(eq)

        self.weapon: Optional[Card] = weapon
        self.weapon_used_this_turn = False

        # Turn resources
        self.action_points = 0
        self.resource_points = 0

        # Per-turn trackers
        self.weapon_attack_count = 0      # for In the Swing / Slice and Dice
        self.next_weapon_go_again = False  # from On a Knife Edge, Blade Flash, etc.
        self.next_weapon_power_bonus = 0   # from En Garde, Sharpen Steel, etc.
        self.next_sword_attack_power_bonus = 0  # from Run Through (set during reaction window, consumed on next attack)
        self.slice_and_dice_active = False # from Slice and Dice: +1 1st weapon, +2 2nd weapon
        self.next_attack_go_again = False  # from Come to Fight
        self.next_brute_attack_bonus = 0   # from Beast Mode / Barraging Beatdown setup
        self.attacks_this_turn = 0
        self.weapon_additional_attack = False  # Dawnblade: one extra attack when go again fires
        # Mentor state
        self.mentor_face_up = False
        self.mentor_lesson_counters = 0

    def draw(self, n=1):
        for _ in range(n):
            if not self.deck:
                return
            self.hand.append(self.deck.pop(0))

    def draw_to_intellect(self):
        need = self.intellect - len(self.hand)
        if need > 0:
            self.draw(need)

    def pitch(self, card: Card) -> int:
        if card in self.hand:
            self.hand.remove(card)
            self.pitch_zone.append(card)
            self.resource_points += card.pitch
            return card.pitch
        return 0

    def gain_life(self, amount: int):
        self.life = min(self.life + amount, 40)  # cap at starting life in Blitz

    def take_damage(self, amount: int, source_name: str = "") -> tuple:
        if amount <= 0:
            return (0, self.life, self.life)
        old_life = self.life
        self.life -= amount
        return (amount, old_life, self.life)

    def is_dead(self) -> bool:
        return self.life <= 0

    def reset_turn_resources(self):
        self.action_points = 1
        self.resource_points = 0
        self.weapon_used_this_turn = False
        self.weapon_attack_count = 0
        self.next_weapon_go_again = False
        self.next_weapon_power_bonus = 0
        self.next_sword_attack_power_bonus = 0
        self.slice_and_dice_active = False
        self.next_attack_go_again = False
        self.next_brute_attack_bonus = 0
        self.attacks_this_turn = 0
        self.weapon_additional_attack = False
        for eq in self.equipment.values():
            eq.reset_turn()

    def end_phase(self):
        self._rng.shuffle(self.pitch_zone)
        self.deck.extend(self.pitch_zone)
        self.pitch_zone.clear()
        self.action_points = 0
        self.resource_points = 0
        self.draw_to_intellect()

    def get_effective_weapon_power(self) -> int:
        base = self.weapon.power if self.weapon else 0
        if self.weapon and "Dawnblade" in self.weapon.name and self.weapon_attack_count >= 1:
            base += 1  # second attack gains +1 power until end of turn
        bonus = self.next_weapon_power_bonus + self.next_sword_attack_power_bonus
        if self.slice_and_dice_active:
            if self.weapon_attack_count == 0:
                bonus += 1
            elif self.weapon_attack_count == 1:
                bonus += 2
        return base + bonus


class GameState:
    def __init__(self, player1: Player, player2: Player,
                 rng: Optional[random.Random] = None):
        self.rng: random.Random = rng if rng is not None else random.Random()
        # Both players share the single game rng — overwrite any per-player rng
        # that was set during construction.
        for p in (player1, player2):
            p._rng = self.rng
        self.players = [player1, player2]
        self.turn_number = 1
        self.active_player_idx = 0
        self.is_first_turn = True

    @property
    def active(self) -> Player:
        return self.players[self.active_player_idx]

    @property
    def defending(self) -> Player:
        return self.players[1 - self.active_player_idx]

    def switch_turn(self):
        self.active_player_idx = 1 - self.active_player_idx
        self.turn_number += 1
        self.is_first_turn = False

    def is_over(self) -> bool:
        return any(p.is_dead() for p in self.players)

    def winner(self) -> Optional[Player]:
        for p in self.players:
            if not p.is_dead():
                return p
        return None

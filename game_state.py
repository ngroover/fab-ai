"""
Game state management for the Classic Battles: Rhinar vs Dorinthea simulator.
Both heroes are young heroes — 20 life, intellect 4, Blitz format.
"""

import random
from collections import deque
from typing import List, Optional, Dict, Union
from cards import Card, CardType, EquipSlot, Color
from card_effects import CardEffect


class Equipment:
    def __init__(self, card: Card):
        self.card = card
        self.destroyed = False
        self.used_this_turn = False
        self.block_counters = 0        # accumulated -1 block counters (Battleworn)
        # Marked True by PAY_FOR_BLOCK_BONUS-style effects; equipment is sent to
        # the graveyard when the current combat chain closes instead of returning
        # to the equipment zone.
        self.destroy_on_chain_close = False

    def reset_turn(self):
        self.used_this_turn = False

    @property
    def active(self):
        return not self.destroyed

    @property
    def defense(self):
        if self.destroyed:
            return 0
        return max(0, self.card.defense - self.block_counters)


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
            if CardType.HERO in c.card_type:
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
        self.banished: List[Card] = []          # intimidate — returns to hand at end of turn
        self.permanently_banished: List[Card] = []  # mentor fire, etc. — stays banished
        self.pitch_zone: List[Card] = []
        self.hand_revealed: List[Card] = []  # hand cards revealed to the opponent
        self.deck_bottom_known: List[Card] = []  # pitched cards at deck bottom, in order (oldest first)
        self.arsenal: Optional[Card] = None
        self.arena: List[Card] = []  # token cards and persistent cards in play

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
        self.next_weapon_go_again_if_hits = False  # from Warrior's Valor: go again only if weapon attack hits
        self.next_weapon_power_bonus = 0   # from En Garde, Sharpen Steel, etc.
        self.next_sword_attack_power_bonus = 0  # from Run Through (set during reaction window, consumed on next attack)
        self.weapon_attacks_power_bonus_all_turn = 0  # from Gallantry Gold: all weapon attacks this turn +N power
        self.weapon_swing_bonuses = []  # list of (swing_index, magnitude) scheduled by ON_PLAY effects
        self.next_attack_go_again = False  # from Come to Fight
        self.next_attack_power_bonus = 0             # from Out for Blood reprise, etc.
        self.next_brute_attack_bonus = 0             # Awakening Bellow: +N to next Brute attack (unconditional)
        self.next_brute_attack_conditional_bonus = 0  # from Barraging Beatdown (conditional on < 2 non-equip blockers)
        self.intimidated_this_turn = False            # True if player has triggered intimidate this turn
        self.attacks_this_turn = 0
        self.weapon_additional_attack = False  # Dawnblade: one extra attack when go again fires
        # Persistent Dawnblade counters (from Glistening Steelblade on-hit effect; never reset)
        self.dawnblade_counters = 0
        # Per-turn flag: True when Glistening Steelblade was played this turn
        self.glistening_steelblade_active = False
        # Mentor state
        self.mentor_face_up = False
        self.mentor_lesson_counters = 0

    def draw(self, n=1):
        for _ in range(n):
            if not self.deck:
                return
            self.hand.append(self.deck.pop(0))
            if len(self.deck_bottom_known) > len(self.deck):
                self.deck_bottom_known.pop(0)

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
        self.next_weapon_go_again_if_hits = False
        self.next_weapon_power_bonus = 0
        self.next_sword_attack_power_bonus = 0
        self.weapon_attacks_power_bonus_all_turn = 0
        self.weapon_swing_bonuses = []
        self.next_attack_go_again = False
        self.next_attack_power_bonus = 0
        self.next_brute_attack_bonus = 0
        self.next_brute_attack_conditional_bonus = 0
        self.intimidated_this_turn = False
        self.attacks_this_turn = 0
        self.weapon_additional_attack = False
        self.glistening_steelblade_active = False
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
        if self.weapon and "Dawnblade" in self.weapon.name:
            base += self.dawnblade_counters  # permanent +1 counters from Glistening Steelblade
            if self.weapon_attack_count >= 1:
                base += 1  # second attack gains +1 power until end of turn
        bonus = self.next_weapon_power_bonus + self.next_sword_attack_power_bonus + self.weapon_attacks_power_bonus_all_turn
        bonus += sum(mag for idx, mag in self.weapon_swing_bonuses if idx == self.weapon_attack_count)
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
        # Rolling buffer of the last N actions executed across both players,
        # consumed by the action-sequence observation field.
        self.action_history: deque = deque(maxlen=64)

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
        idx = self.winner_index()
        return self.players[idx] if idx is not None else None

    def winner_index(self) -> Optional[int]:
        """Index of the sole surviving player (0 or 1), or None when there is no
        decisive winner — i.e. both players still alive, or both dead at once.

        A player wins when their opponent is reduced to 0 or less life while they
        themselves are still alive.
        """
        alive = [i for i, p in enumerate(self.players) if not p.is_dead()]
        return alive[0] if len(alive) == 1 else None

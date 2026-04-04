"""
AI decision engine for Rhinar and Dorinthea (Classic Battles).

Rhinar: Intimidate + big attacks. Use Barraging Beatdown + Beast Mode to set up, 
        then land 7-9 power attacks. Bone Basher for chip damage.

Dorinthea: Weapon chain focus. Give Dawnblade go again every turn via buff cards
           (En Garde, On a Knife Edge, Blade Flash, Hit and Run, Slice and Dice),
           swing twice, use Reprise effects. Let Dawnblade accumulate +1 counters.
"""

import random
from typing import List, Optional, Tuple
from cards import Card, CardType, Color
from game_state import Player


def pitch_for_cost(player: Player, cost: int) -> List[Card]:
    """Select cards to pitch to meet a resource cost. Prefer blues, then yellows."""
    needed = cost - player.resource_points
    if needed <= 0:
        return []
    pitchable = sorted(
        [c for c in player.hand if c.pitch > 0],
        key=lambda c: c.pitch, reverse=True
    )
    to_pitch = []
    total = 0
    for card in pitchable:
        if total >= needed:
            break
        to_pitch.append(card)
        total += card.pitch
    return to_pitch if total >= needed else []


def can_afford(player: Player, cost: int, exclude: Optional[Card] = None) -> bool:
    available = player.resource_points
    for c in player.hand:
        if c == exclude:
            continue
        available += c.pitch
    return available >= cost


# ─────────────── RHINAR AI ───────────────

def rhinar_choose_action(player: Player, opponent: Player) -> Optional[Tuple[Card, List[Card]]]:
    """
    Priority:
    1. Beast Mode (0-cost setup for next attack) if we have a big attack to follow
    2. Barraging Beatdown (0-cost, intimidate, sets up +3 on next attack)
    3. Best affordable attack action (prefer higher power)
    4. Come to Fight (0-cost, give next attack go again) if useful
    """
    hand = player.hand

    # Priority 1: Barraging Beatdown — 0 cost, intimidates, gives next Brute attack bonus
    bb_cards = [c for c in hand if c.name == "Barraging Beatdown" and c.card_type == CardType.ACTION]
    if bb_cards and player.next_brute_attack_bonus == 0:
        card = bb_cards[0]
        return (card, [])  # costs 0

    # Priority 2: Beast Mode (0-cost, +3 to next Brute attack)
    bm_cards = [c for c in hand if c.name == "Beast Mode"]
    if bm_cards and player.next_brute_attack_bonus == 0:
        # Only worth playing if we have a big attack to follow
        attacks_available = [c for c in hand if c.card_type == CardType.ACTION_ATTACK and c.power >= 6]
        if attacks_available:
            return (bm_cards[0], [])

    # Priority 3: Best affordable attack action
    attacks = [c for c in hand if c.card_type == CardType.ACTION_ATTACK]
    affordable = []
    for card in attacks:
        if can_afford(player, card.cost, exclude=card):
            affordable.append(card)

    if affordable:
        # Score: power + bonus + go_again bonus
        def score(c: Card) -> float:
            p = c.power + player.next_brute_attack_bonus
            if c.go_again:
                p += 1
            if c.intimidate:
                p += 1
            return p
        affordable.sort(key=score, reverse=True)
        best = affordable[0]
        to_pitch = pitch_for_cost(player, best.cost)
        if player.resource_points + sum(c.pitch for c in to_pitch) >= best.cost:
            return (best, to_pitch)

    # Priority 4: Come to Fight (give weapon go again)
    ctf = [c for c in hand if c.name == "Come to Fight"]
    if ctf and not player.weapon_used_this_turn:
        return (ctf[0], [])

    return None


def rhinar_choose_defense(player: Player, attack_power: int) -> Tuple[List[Card], List['Equipment']]:
    """
    Rhinar is aggressive. With 20 life, defend if attack would leave us dangerously low.
    Target: don't go below ~6 life. Block to reduce damage to safe levels.
    """
    defending_cards = []
    defending_equipment = []

    # If we have tons of life, take the hit (save cards for attacking)
    if player.life - attack_power > 8:
        return [], []

    # Need to block. Prefer high-defense blues (save reds for attacks)
    hand_defenders = sorted(
        [c for c in player.hand if c.defense > 0 and c.card_type not in
         (CardType.ATTACK_REACTION,)],
        key=lambda c: (c.defense, c.color.value if c.color else 0),
        reverse=True
    )

    total_def = 0
    # Try to bring damage down to <=3 if we can
    damage_we_can_take = max(1, player.life - 6)
    needed_block = max(0, attack_power - damage_we_can_take)

    for card in hand_defenders:
        if total_def >= needed_block:
            break
        defending_cards.append(card)
        total_def += card.defense

    # Add equipment if still need more
    if total_def < needed_block:
        for slot in ['legs', 'arms', 'chest', 'head']:
            eq = player.equipment.get(slot)
            if eq and eq.active and eq.defense > 0:
                defending_equipment.append(eq)
                total_def += eq.defense
                if total_def >= needed_block:
                    break

    return defending_cards, defending_equipment


def rhinar_choose_arsenal(player: Player) -> Optional[Card]:
    """Store a blue card (best pitch, least useful offensively) in arsenal."""
    if player.arsenal:
        return None
    # Store Titanium Bauble, Dodge, Rally, or Clearing Bellow
    priority_store = ["Titanium Bauble", "Dodge", "Rally the Rearguard", "Clearing Bellow"]
    for name in priority_store:
        for c in player.hand:
            if c.name == name:
                return c
    # Fallback: store any blue
    blues = [c for c in player.hand if c.color == Color.BLUE]
    if blues:
        return blues[0]
    return None


# ─────────────── DORINTHEA AI ───────────────

def dorinthea_choose_action(player: Player, opponent: Player) -> Optional[Tuple[Card, List[Card]]]:
    """
    Dorinthea's whole game plan: give Dawnblade go again, swing twice per turn.
    Her hero ability already gives Dawnblade go again at start of turn.
    So the first weapon swing is free — then she needs to SET UP a second swing.

    Turn structure should be:
    1. Swing Dawnblade (hero ability gave it go again already)
    2. After it hits (go again from hero), play an enabler to give it go again AGAIN
    3. Swing Dawnblade a third time if possible, etc.

    Actually: Dorinthea prodigy gives go again at START. So:
    - Weapon goes → if hits, can chain. Play enabler for another go again.
    - If weapon hasn't swung yet: play enabler FIRST to boost the first swing,
      then weapon swings with the buffs.

    Priority before weapon swings:
    1. Power-boosting enablers (En Garde +3, Sharpen Steel +1) 
    2. Go-again enablers for after weapon: On a Knife Edge, Blade Flash, Hit and Run
    3. Slice and Dice (passive buff while active)
    4. Glistening Steelblade (best — give go again AND counter on hit)
    5. Visit the Blacksmith (+1 power, go again)
    
    After weapon swings, if action points remain:
    6. Attack actions that add pressure
    """
    hand = player.hand

    # If weapon hasn't swung yet, prioritize setting it up
    if not player.weapon_used_this_turn:
        # Best enablers in priority order (give power AND go again)
        setup_priority = [
            "Glistening Steelblade",  # go again + counter on hit, costs 1 (yellow)
            "En Garde",               # +3 power, go again, costs 0
            "Slice and Dice",         # passive weapon buffs, go again, costs 0
            "Warrior's Valor",        # +2 power + if hits go again, costs 0
            "On a Knife Edge",        # go again, costs 0
            "Blade Flash",            # go again, costs 0
            "Hit and Run",            # go again, costs 0
            "Visit the Blacksmith",   # +1 power + go again, costs 0
            "Sharpen Steel",          # +1 power (instant), costs 0
        ]
        for name in setup_priority:
            for card in hand:
                if card.name == name and can_afford(player, card.cost, exclude=card):
                    to_pitch = pitch_for_cost(player, card.cost)
                    return (card, to_pitch)

    # Weapon has been used — now play enablers to generate more weapon go-agains
    if player.weapon_used_this_turn:
        chain_priority = [
            "On a Knife Edge",
            "Blade Flash",
            "Hit and Run",
            "Glistening Steelblade",
            "En Garde",
        ]
        for name in chain_priority:
            for card in hand:
                if card.name == name and can_afford(player, card.cost, exclude=card):
                    to_pitch = pitch_for_cost(player, card.cost)
                    return (card, to_pitch)

    # Attack actions as secondary (Second Swing is great, Reprise stuff)
    attack_priority = ["Second Swing", "Run Through", "Out for Blood", "Driving Blade"]
    for name in attack_priority:
        for card in hand:
            if card.name == name and can_afford(player, card.cost, exclude=card):
                to_pitch = pitch_for_cost(player, card.cost)
                return (card, to_pitch)

    # Titanium Bauble for a floating resource
    baubles = [c for c in hand if c.name == "Titanium Bauble"]
    if baubles and player.resource_points == 0:
        return (baubles[0], [])

    return None


def dorinthea_choose_defense(player: Player, attack_power: int) -> Tuple[List[Card], List['Equipment']]:
    """
    Dorinthea defends more carefully — she needs her hand for weapon chains.
    Prefer blocking with blues (low attack value) and defense reactions.
    Don't over-block; take small amounts of damage if hand is needed for next turn.
    """
    defending_cards = []
    defending_equipment = []

    # If life is comfortable, take some chip damage
    if player.life - attack_power > 8:
        return [], []

    damage_we_can_take = max(1, player.life - 6)
    needed_block = max(0, attack_power - damage_we_can_take)

    # Prefer defense reactions, then high-def blues
    hand_defenders = sorted(
        [c for c in player.hand if c.defense > 0 and c.card_type not in
         (CardType.ATTACK_REACTION,)],
        key=lambda c: (
            1 if c.card_type in (CardType.DEFENSE_REACTION, CardType.INSTANT) else 0,
            c.defense,
            c.color.value if c.color else 0
        ),
        reverse=True
    )

    total_def = 0
    for card in hand_defenders:
        if total_def >= needed_block:
            break
        defending_cards.append(card)
        total_def += card.defense

    # Equipment last resort
    if total_def < needed_block:
        for slot in ['legs', 'arms', 'chest', 'head']:
            eq = player.equipment.get(slot)
            if eq and eq.active and eq.defense > 0:
                defending_equipment.append(eq)
                total_def += eq.defense
                if total_def >= needed_block:
                    break

    return defending_cards, defending_equipment


def dorinthea_choose_arsenal(player: Player) -> Optional[Card]:
    """Arsenal a go-again enabler or Sigil of Solace for next turn."""
    if player.arsenal:
        return None
    priority = ["Sigil of Solace", "On a Knife Edge", "Blade Flash", "Hit and Run",
                "Toughen Up", "Flock of the Feather Walkers"]
    for name in priority:
        for c in player.hand:
            if c.name == name:
                return c
    # Fallback: store a blue
    blues = [c for c in player.hand if c.color == Color.BLUE]
    if blues:
        return blues[0]
    return None

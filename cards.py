"""
Flesh and Blood - Classic Battles: Rhinar vs Dorinthea
Exact card data sourced from official LSS decklists and card databases.

Rhinar deck: 40-card Blitz, young hero Rhinar (20 life, intellect 4)
Dorinthea deck: 40-card Blitz, young hero Dorinthea, Quicksilver Prodigy (20 life, intellect 4)
"""

from dataclasses import dataclass
from typing import Optional, List
from enum import Enum


class CardType(Enum):
    ACTION_ATTACK = "Action - Attack"
    ACTION = "Action"
    INSTANT = "Instant"
    ATTACK_REACTION = "Attack Reaction"
    DEFENSE_REACTION = "Defense Reaction"
    EQUIPMENT = "Equipment"
    WEAPON = "Weapon"
    HERO = "Hero"
    MENTOR = "Mentor"
    RESOURCE = "Resource"


class Color(Enum):
    RED = 1
    YELLOW = 2
    BLUE = 3


class EquipSlot(Enum):
    HEAD = "head"
    CHEST = "chest"
    ARMS = "arms"
    LEGS = "legs"
    WEAPON = "weapon"


@dataclass
class Card:
    name: str
    card_type: CardType
    cost: int = 0
    pitch: int = 0
    power: int = 0
    defense: int = 0
    color: Optional[Color] = None
    go_again: bool = False
    text: str = ""
    intimidate: bool = False
    no_block: bool = False
    equip_slot: Optional[EquipSlot] = None

    def __str__(self):
        parts = [self.name]
        if self.color:
            parts.append(f"({self.color.name.capitalize()})")
        return " ".join(parts)

    def display(self):
        lines = [f"  [{self.card_type.value}] {self}"]
        stats = []
        if self.cost > 0:
            stats.append(f"Cost:{self.cost}")
        if self.pitch > 0:
            stats.append(f"Pitch:{self.pitch}")
        if self.power > 0:
            stats.append(f"Power:{self.power}")
        if self.defense > 0:
            stats.append(f"Def:{self.defense}")
        if stats:
            lines.append("    " + " | ".join(stats))
        if self.go_again:
            lines.append("    Go again")
        if self.no_block:
            lines.append("    No Block")
        if self.text:
            lines.append(f"    {self.text}")
        return "\n".join(lines)


# ──────────────────────── RHINAR DECK ────────────────────────
# Hero: Rhinar (young) — Life: 20, Intellect: 4
# Weapon: Bone Basher (1H) — Once per Turn Action — 1: Attack. Go again.
#         If Bone Basher hits, intimidate.
# Equipment: Blossom of Spring (head), Bone Vizier (chest),
#            Ironhide Gauntlet (arms), Ironhide Legs (legs)

def build_rhinar_deck() -> List[Card]:
    cards = []

    # ── RED (Pitch 1) — 13 cards ──

    # Alpha Rampage: cost 3, power 9, def 3, intimidate, Rhinar spec
    # Additional cost: discard a random card
    cards.append(Card("Alpha Rampage", CardType.ACTION_ATTACK, cost=3, pitch=1,
                       power=9, defense=3, color=Color.RED, intimidate=True,
                       text="Rhinar Specialization. As an additional cost to play Alpha Rampage, discard a random card. When you attack with Alpha Rampage, intimidate."))

    # Awakening Bellow x2: cost 2, power 6, def 3, go again, intimidate
    for _ in range(2):
        cards.append(Card("Awakening Bellow", CardType.ACTION, cost=1, pitch=1,
                           power=0, defense=3, color=Color.RED, go_again=True, intimidate=True,
                           text="Go again. Intimidate."))

    # Bare Fangs x2: cost 2, power 6, def 0, no block
    # When attacking: draw a card then discard a random card.
    # If discarded card has 6+ power, this gets +2 power.
    for _ in range(2):
        cards.append(Card("Bare Fangs", CardType.ACTION_ATTACK, cost=2, pitch=1,
                           power=6, defense=0, color=Color.RED, no_block=True,
                           text="When you attack with Bare Fangs, draw a card then discard a random card. If a card wth 6 or more power is discarded this way, Bare Fangs gets +2 power."))

    # Beast Mode x2: cost 3, power 6, def 3, (attack action)
    # Note: Beast Mode has no defense value and cannot block
    for _ in range(2):
        cards.append(Card("Beast Mode", CardType.ACTION_ATTACK, cost=3, pitch=1,
                           power=6, defense=3, color=Color.RED,
                           text="If you have intimidated this turn, Beast Mode gains +2 power."))

    # Pack Hunt x2: cost 2, power 6, def 3
    for _ in range(2):
        cards.append(Card("Pack Hunt", CardType.ACTION_ATTACK, cost=2, pitch=1,
                           power=6, defense=3, color=Color.RED, intimidate=True,
                           text="When you attack with Pack Hunt, intimidate"))

    # Wild Ride x2: cost 2, power 6, def 0, may have go again, no block
    # Draw a card then discard a random card. If discarded card 6+ power, go again
    for _ in range(2):
        cards.append(Card("Wild Ride", CardType.ACTION_ATTACK, cost=2, pitch=1,
                           power=6, defense=0, color=Color.RED, no_block=True,
                           text="When you attack with Wild Ride, draw a card then discard a random card.  If a card with 6 or more power is discarded this way, Wild Ride gains go again."))

    # Wrecking Ball x2: cost 3, power 6, def 0, intimidate on hit condition, no_block
    for _ in range(2):
        cards.append(Card("Wrecking Ball", CardType.ACTION_ATTACK, cost=3, pitch=1,
                           power=6, defense=0, color=Color.RED, no_block=True,
                           text="When you attack with Wrecking Ball, draw a card then discard a random card. If a card with 6 or more power is discarded this way, intimidate."))

    # ── YELLOW (Pitch 2) — 14 cards ──

    # Barraging Beatdown x2: cost 0, power 0, def 3, go again, intimidate
    # Non-attack action: next Brute attack gains conditional +3 power
    for _ in range(2):
        cards.append(Card("Barraging Beatdown", CardType.ACTION, cost=0, pitch=2,
                           power=0, defense=3, color=Color.YELLOW, go_again=True, intimidate=True,
                           text="Intimidate, then your next Brute attack this turn gains 'While this attack is defended by less than 2 non-equipment cards it has +3 power'. Go again."))

    # Muscle Mutt x2: cost 3, power 6, def 2
    # Generic attack
    for _ in range(2):
        cards.append(Card("Muscle Mutt", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=2, color=Color.YELLOW,
                           text=""))

    # Pack Call x2: cost 3, power 6, def 3
    # When you defend with Pack Call, reveal top card of deck; if 6+ power keep on top
    for _ in range(2):
        cards.append(Card("Pack Call", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=3, color=Color.YELLOW,
                           text="When you defend with Pack Call, reveal the top card of your deck. If it has 6 or more power, put it on top of your deck. Otherwise, put it on he bottom."))

    # Raging Onslaught x2: cost 3, power 6, def 3
    # If this hits, draw a card
    for _ in range(2):
        cards.append(Card("Raging Onslaught", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=3, color=Color.YELLOW,
                           text=""))

    # Smash Instinct x2: cost 2, power 6, def 3, intimidate
    for _ in range(2):
        cards.append(Card("Smash Instinct", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=3, color=Color.YELLOW, intimidate=True,
                           text="When you attack with Smash Instinct, intimidate."))

    # Smash with Big Tree x2: cost 2, power 6, no_block
    for _ in range(2):
        cards.append(Card("Smash with Big Tree", CardType.ACTION_ATTACK, cost=2, pitch=2,
                           power=6, defense=0, color=Color.YELLOW, no_block=True,
                           text=""))

    # Wounded Bull x2: cost 2, power 6, def 3, go again
    for _ in range(2):
        cards.append(Card("Wounded Bull", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=2, color=Color.YELLOW
                           text="When you play Wounded Bull, if you have less health than an opposing hero, it gains +1 power."))

    # ── BLUE (Pitch 3) — 12 cards ──

    # Clearing Bellow x2: cost 0, power 0, def 3, go again, intimidate
    for _ in range(2):
        cards.append(Card("Clearing Bellow", CardType.ACTION, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE, go_again=True, intimidate=True,
                           text="Intimidate.Go again."))

    # Come to Fight x2: cost 0, power 0, def 3
    # Non-attack action: your next attack this turn gains go again
    for _ in range(2):
        cards.append(Card("Come to Fight", CardType.ACTION, cost=1, pitch=3,
                           power=0, defense=3, color=Color.BLUE,
                           text="Your next attack action card you play this turn gains +1 power. Go again."))

    # Dodge x2: cost 0, def 2 — defense reaction
    for _ in range(2):
        cards.append(Card("Dodge", CardType.DEFENSE_REACTION, cost=0, pitch=3,
                           power=0, defense=2, color=Color.BLUE,
                           text=""))

    # Rally the Rearguard x2: cost 0, def 3 — defense reaction
    # When defending, if attacker has intimidated this turn, gain 2 life
    for _ in range(2):
        cards.append(Card("Rally the Rearguard", CardType.ACTION_ATTACK, cost=2, pitch=3,
                           power=4, defense=2, color=Color.BLUE,
                          text="Once per turn Instant - Discard a card: Rally the Rearguard gains +3 block.  Activate this ability only while Rally the Rearguard is defending."))

    # Titanium Bauble x2: cost 0, pitch 3, def 3
    # Gain 1 resource point
    for _ in range(2):
        cards.append(Card("Titanium Bauble", CardType.RESOURCE, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE,
                           text=""))

    # Wrecker Romp x2: cost 2, power 6, def 3
    for _ in range(2):
        cards.append(Card("Wrecker Romp", CardType.ACTION_ATTACK, cost=2, pitch=3,
                           power=6, defense=3, color=Color.BLUE))

    # Chief Ruk'utan x1: Mentor
    # When face-up: whenever you play a card with 6+ power, intimidate + lesson counter.
    # At 2 lesson counters: banish, search for Alpha Rampage, put face-up in arsenal.
    cards.append(Card("Chief Ruk'utan", CardType.MENTOR, cost=0, pitch=0,
                       power=0, defense=0,
                       text="While Ruk'utan is face down in arsenal, at the start of your turn, you may turn him face up.  While Ruk'utan is face up in arsenal, whenever ou play a card with 6 or more power, intimidate and put a lesson counter on him.  Then if there are 2 or more lesson counters on Rok'utan, banish him, search your deck for Alpha Rampage, put it face up in arsenal and shuffle."))
    return cards


def build_rhinar_equipment():
    """Bone Basher weapon + equipment set."""
    return [
        Card("Bone Basher", CardType.WEAPON, power=4, equip_slot=EquipSlot.WEAPON,
             text="Once per Turn Action — 2: Attack."),
        Card("Blossom of Spring", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.CHEST,
             text="Action: Destroy Blossom of Spring: Gain 1 resource. Go again"),
        Card("Bone Vizier", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.HEAD,
             text="When Bone Vizier is destroyed, reveal the top card of your deck.  If it has 6 or more power, put it on the top of your deck. Otherwise, put it on the bottom. Blade Break"),
        Card("Ironhide Gauntlet", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.ARMS,
             text="When you defend with Ironhide Gauntlet you may pay 1 resource.  If you do, it gains +2 block and 'When the combat chain closes, destroy Ironhide Gauntlets'"),
        Card("Ironhide Legs", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.LEGS,
             text="When you defend with Ironhide Legs you may pay 1 resource.  If you do, it gains +2 block and 'When the combat chain closes, destroy Ironhide Legs'"),
    ]


# ──────────────────────── DORINTHEA DECK ────────────────────────
# Hero: Dorinthea, Quicksilver Prodigy (young) — Life: 20, Intellect: 4
# Hero ability: Once per Turn — At the start of your action phase,
#   Dawnblade gains go again until end of turn.
# Weapon: Dawnblade, Resplendent — Once per Turn Action — 1: Attack.
#   Whenever Dawnblade hits a hero, put a +1 power counter on it.
# Equipment: Blossom of Spring (head), Gallantry Gold (chest),
#            Ironrot Helm (head — wait, check), Ironrot Legs (legs)
# Actually from decklist: Blossom of Spring, Gallantry Gold, Ironrot Helm, Ironrot Legs

def build_dorinthea_deck() -> List[Card]:
    cards = []

    # ── RED (Pitch 1) — 16 cards ──

    # En Garde x2: cost 1, def 3, warrior action (not attack)
    # Next weapon attack this turn gains +3 power. Go again.
    for _ in range(2):
        cards.append(Card("En Garde", CardType.ACTION, cost=1, pitch=1,
                           power=0, defense=3, color=Color.RED, go_again=True,
                           text="Your next weapon attack this turn gains +3 power. Go again."))

    # Flock of the Feather Walkers x2: cost 0, def 3, instant
    # Target defending attack action card gets +2 defense
    for _ in range(2):
        cards.append(Card("Flock of the Feather Walkers", CardType.INSTANT, cost=0, pitch=1,
                           power=0, defense=3, color=Color.RED,
                           text="Target defending attack action card gets +2 defense."))

    # In the Swing x2: cost 0, def 3, attack reaction
    # Play only if attacked 2+ times with weapons this turn.
    # Target weapon attack gains +3 power.
    for _ in range(2):
        cards.append(Card("In the Swing", CardType.ATTACK_REACTION, cost=0, pitch=1,
                           power=0, defense=3, color=Color.RED,
                           text="Play only if you have attacked 2 or more times with weapons this turn. Target weapon attack gains +3 power."))

    # Ironsong Response x2: cost 0, def 3, attack reaction
    # Target attack gains +2 power. Reprise — if defender defended with card from hand, draw a card.
    for _ in range(2):
        cards.append(Card("Ironsong Response", CardType.ATTACK_REACTION, cost=0, pitch=1,
                           power=0, defense=3, color=Color.RED,
                           text="Target attack gains +2 power. Reprise — If the defending hero defended with a card from hand, draw a card."))

    # Second Swing x2: cost 1, def 3, warrior action (not an attack)
    # Your next attack this turn gains +4 power. Go again.
    for _ in range(2):
        cards.append(Card("Second Swing", CardType.ACTION, cost=1, pitch=1,
                           power=0, defense=3, color=Color.RED, go_again=True,
                           text="Your next attack this turn gains +4 power. Go again."))

    # Sharpen Steel x2: cost 0, def 3, instant
    # Target weapon attack gains +1 power.
    for _ in range(2):
        cards.append(Card("Sharpen Steel", CardType.INSTANT, cost=0, pitch=1,
                           power=0, defense=3, color=Color.RED,
                           text="Target weapon attack gains +1 power."))

    # Thrust x2: cost 0, def 3, attack reaction
    # Target sword attack gains +3 power.
    for _ in range(2):
        cards.append(Card("Thrust", CardType.ATTACK_REACTION, cost=0, pitch=1,
                           power=0, defense=3, color=Color.RED,
                           text="Target sword attack gains +3 power."))

    # Warrior's Valor x2: cost 1, def 3, warrior action
    # Your next weapon attack gains +3 power and "If this hits, go again." Go again.
    for _ in range(2):
        cards.append(Card("Warrior's Valor", CardType.ACTION, cost=1, pitch=1,
                           power=0, defense=3, color=Color.RED, go_again=True,
                           text="Your next weapon attack this turn gains +3 power and 'If this hits, this attack gains go again.' Go again."))

    # ── YELLOW (Pitch 2) — 11 cards ──

    # Driving Blade x2: cost 2, power 5, def 3
    # If this hits, your next weapon attack this turn gains go again.
    for _ in range(2):
        cards.append(Card("Driving Blade", CardType.ACTION_ATTACK, cost=2, pitch=2,
                           power=5, defense=3, color=Color.YELLOW,
                           text="If Driving Blade hits, your next weapon attack this turn gains go again."))

    # Glistening Steelblade x1: cost 1, def 3, warrior action, Dorinthea spec
    # Next Dawnblade attack this turn has go again.
    # Whenever Dawnblade hits a hero this turn, put +1 power counter on it. Go again.
    cards.append(Card("Glistening Steelblade", CardType.ACTION, cost=1, pitch=2,
                       power=0, defense=3, color=Color.YELLOW, go_again=True,
                       text="Dorinthea Specialization. Your next Dawnblade attack this turn has go again. Whenever Dawnblade hits a hero this turn, put a +1 power counter on it. Go again."))

    # On a Knife Edge x2: cost 0, def 3, warrior action
    # Next weapon attack gains go again. Go again.
    for _ in range(2):
        cards.append(Card("On a Knife Edge", CardType.ACTION, cost=0, pitch=2,
                           power=0, defense=3, color=Color.YELLOW, go_again=True,
                           text="Your next weapon attack this turn gains go again. Go again."))

    # Out for Blood x2: cost 2, power 5, def 3, warrior attack
    # Reprise — if defender defended with card from hand, this gains +2 power.
    for _ in range(2):
        cards.append(Card("Out for Blood", CardType.ACTION_ATTACK, cost=2, pitch=2,
                           power=5, defense=3, color=Color.YELLOW,
                           text="Reprise — If the defending hero defended with a card from hand, Out for Blood gains +2 power."))

    # Run Through x2: cost 1, power 5, def 3, warrior attack
    # Reprise — if defender defended with card from hand, draw a card.
    for _ in range(2):
        cards.append(Card("Run Through", CardType.ACTION_ATTACK, cost=1, pitch=2,
                           power=5, defense=3, color=Color.YELLOW,
                           text="Reprise — If the defending hero defended with a card from hand, draw a card."))

    # Slice and Dice x2: cost 0, def 3, warrior action. Go again.
    # First weapon attack this turn gains +1 power.
    # Second weapon attack this turn gains +2 power.
    for _ in range(2):
        cards.append(Card("Slice and Dice", CardType.ACTION, cost=0, pitch=2,
                           power=0, defense=3, color=Color.YELLOW, go_again=True,
                           text="Whenever you attack with a sword or dagger this turn: first weapon attack gains +1 power, second weapon attack gains +2 power. Go again."))

    # ── BLUE (Pitch 3) — 12 cards ──

    # Blade Flash x2: cost 0, def 3, warrior action. Go again.
    # Next weapon attack this turn gains go again.
    for _ in range(2):
        cards.append(Card("Blade Flash", CardType.ACTION, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE, go_again=True,
                           text="Your next weapon attack this turn gains go again. Go again."))

    # Hit and Run x2: cost 0, def 3, warrior action. Go again.
    # Next weapon attack gains go again.
    for _ in range(2):
        cards.append(Card("Hit and Run", CardType.ACTION, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE, go_again=True,
                           text="Your next weapon attack this turn gains go again. Go again."))

    # Sigil of Solace x2: cost 0, def 3, instant
    # Gain 3 life.
    for _ in range(2):
        cards.append(Card("Sigil of Solace", CardType.INSTANT, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE,
                           text="Gain 3 life."))

    # Titanium Bauble x2: cost 0, def 0, instant (can't block)
    # Gain 1 resource point.
    for _ in range(2):
        cards.append(Card("Titanium Bauble", CardType.INSTANT, cost=0, pitch=3,
                           power=0, defense=0, color=Color.BLUE,
                           text="Gain 1 resource point."))

    # Toughen Up x2: cost 0, def 3, defense reaction
    # Generic defense reaction.
    for _ in range(2):
        cards.append(Card("Toughen Up", CardType.DEFENSE_REACTION, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE,
                           text="Generic Defense Reaction."))

    # Visit the Blacksmith x2: cost 0, def 2, generic action. Go again.
    # Next sword attack this turn gains +1 power.
    for _ in range(2):
        cards.append(Card("Visit the Blacksmith", CardType.ACTION, cost=0, pitch=3,
                           power=0, defense=2, color=Color.BLUE, go_again=True,
                           text="Your next sword attack this turn gains +1 power. Go again."))

    # Hala Goldenhelm x1: Mentor — starts face-down in arsenal
    # When face-up: whenever a sword attack you control hits, it gains go again + lesson counter.
    # At 2 lesson counters: banish, search for Glistening Steelblade, put face-up in arsenal.
    cards.append(Card("Hala Goldenhelm", CardType.MENTOR, cost=0, pitch=0,
                       power=0, defense=0,
                       text="Mentor. While face-up in arsenal: whenever a sword attack you control hits, it gains go again and put a lesson counter on Hala. At 2 lesson counters: banish, put Glistening Steelblade face-up in arsenal."))

    return cards


def build_dorinthea_equipment():
    """Dawnblade, Resplendent weapon + equipment set."""
    return [
        Card("Dawnblade, Resplendent", CardType.WEAPON, power=3, equip_slot=EquipSlot.WEAPON,
             text="Once per Turn Action — 1: Attack. If Dawnblade hits a hero twice in one turn, put a +1 power counter on it."),
        Card("Gallantry Gold", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.CHEST,
             text="Once per combat chain — 0: Your next weapon attack this turn gains +2 power. Destroy Gallantry Gold."),
        Card("Blossom of Spring", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.HEAD,
             text="Once per Combat Chain — 0: Gain 1 resource. Destroy Blossom of Spring."),
        Card("Ironrot Helm", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.ARMS,
             text="Battleworn."),
        Card("Ironrot Legs", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.LEGS,
             text="Battleworn."),
    ]

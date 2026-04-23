"""
Flesh and Blood - Classic Battles: Rhinar vs Dorinthea
Exact card data sourced from official LSS decklists and card databases.

Rhinar deck: 40-card Blitz, young hero Rhinar (20 life, intellect 4)
Dorinthea deck: 40-card Blitz, young hero Dorinthea, Quicksilver Prodigy (20 life, intellect 4)
"""

import re
from dataclasses import dataclass, field
from typing import Dict, Optional, List
from enum import Enum

from card_effects import CardEffect, EffectTrigger, EffectAction


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


class CardClass(Enum):
    GENERIC = "Generic"
    BRUTE = "Brute"
    WARRIOR = "Warrior"


class Keyword(Enum):
    GO_AGAIN = "Go Again"
    INTIMIDATE = "Intimidate"
    BLADE_BREAK = "Blade Break"
    BATTLEWORN = "Battleworn"
    REPRISE = "Reprise"


@dataclass
class Card:
    name: str
    card_type: CardType
    cost: int = 0
    pitch: int = 0
    power: int = 0
    defense: int = 0
    color: Optional[Color] = None
    text: str = ""
    no_block: bool = False
    equip_slot: Optional[EquipSlot] = None
    card_class: CardClass = CardClass.GENERIC
    keywords: List["Keyword"] = field(default_factory=list)
    effects: List[CardEffect] = field(default_factory=list)

    @property
    def card_id(self) -> str:
        """Stable identifier for this card template (name + color slug)."""
        name_slug = re.sub(r"[^a-z0-9]+", "-", self.name.lower()).strip("-")
        color_suffix = f"-{self.color.name.lower()}" if self.color else ""
        return f"{name_slug}{color_suffix}"

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
        for kw in self.keywords:
            lines.append(f"    {kw.value}")
        if self.text:
            lines.append(f"    {self.text}")
        return "\n".join(lines)


# ──────────────────────── RHINAR DECK ────────────────────────
# Hero: Rhinar (young) — Life: 20, Intellect: 4
# Weapon: Bone Basher (1H) — Once per Turn Action — 2: Attack.
# Equipment: Blossom of Spring (head), Bone Vizier (chest),
#            Ironhide Gauntlet (arms), Ironhide Legs (legs)

def build_rhinar_deck() -> List[Card]:
    cards = []

    # ── Hero card (extracted before shuffling) ──
    cards.append(Card("Rhinar (Young Brute)", CardType.HERO, card_class=CardClass.BRUTE,
                       text="Young Hero. Life 20, Intellect 4. Whenever you discard a card with 6 or more power during your action phase, intimidate.",
                       effects=[
                           CardEffect(
                               trigger=EffectTrigger.ON_DISCARD,
                               action=EffectAction.INTIMIDATE,
                               condition=lambda ctx: ctx.get("card") is not None and ctx["card"].power >= 6,
                           )
                       ]))

    # ── RED (Pitch 1) — 13 cards ──

    # Alpha Rampage: cost 3, power 9, def 3, intimidate, Rhinar spec
    # Additional cost: discard a random card
    cards.append(Card("Alpha Rampage", CardType.ACTION_ATTACK, cost=3, pitch=1,
                       power=9, defense=3, color=Color.RED,
                       card_class=CardClass.BRUTE,
                       text="Rhinar Specialization. As an additional cost to play Alpha Rampage, discard a random card. When you attack with Alpha Rampage, intimidate.",
                       effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.INTIMIDATE)]))

    # Awakening Bellow x2: cost 2, power 6, def 3, go again, intimidate
    for _ in range(2):
        cards.append(Card("Awakening Bellow", CardType.ACTION, cost=1, pitch=1,
                           power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.BRUTE,
                           text="Go again. Intimidate.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_PLAY, action=EffectAction.INTIMIDATE)]))

    # Bare Fangs x2: cost 2, power 6, def 0, no block
    # When attacking: draw a card then discard a random card.
    # If discarded card has 6+ power, this gets +2 power.
    for _ in range(2):
        cards.append(Card("Bare Fangs", CardType.ACTION_ATTACK, cost=2, pitch=1,
                           power=6, defense=0, color=Color.RED, no_block=True,
                           card_class=CardClass.BRUTE,
                           text="When you attack with Bare Fangs, draw a card then discard a random card. If a card wth 6 or more power is discarded this way, Bare Fangs gets +2 power.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.DRAW_DISCARD_POWER_BONUS)]))

    # Beast Mode x2: cost 3, power 6, def 3, (attack action)
    # Note: Beast Mode has no defense value and cannot block
    for _ in range(2):
        cards.append(Card("Beast Mode", CardType.ACTION_ATTACK, cost=3, pitch=1,
                           power=6, defense=3, color=Color.RED,
                           card_class=CardClass.BRUTE,
                           text="If you have intimidated this turn, Beast Mode gains +2 power."))

    # Pack Hunt x2: cost 2, power 6, def 3
    for _ in range(2):
        cards.append(Card("Pack Hunt", CardType.ACTION_ATTACK, cost=2, pitch=1,
                           power=6, defense=3, color=Color.RED,
                           card_class=CardClass.BRUTE,
                           text="When you attack with Pack Hunt, intimidate",
                           effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.INTIMIDATE)]))

    # Wild Ride x2: cost 2, power 6, def 0, may have go again, no block
    # Draw a card then discard a random card. If discarded card 6+ power, go again
    for _ in range(2):
        cards.append(Card("Wild Ride", CardType.ACTION_ATTACK, cost=2, pitch=1,
                           power=6, defense=0, color=Color.RED, no_block=True,
                           card_class=CardClass.BRUTE,
                           text="When you attack with Wild Ride, draw a card then discard a random card.  If a card with 6 or more power is discarded this way, Wild Ride gains go again.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.DRAW_DISCARD_GO_AGAIN)]))

    # Wrecking Ball x2: cost 3, power 6, def 0, intimidate on hit condition, no_block
    for _ in range(2):
        cards.append(Card("Wrecking Ball", CardType.ACTION_ATTACK, cost=3, pitch=1,
                           power=6, defense=0, color=Color.RED, no_block=True,
                           card_class=CardClass.BRUTE,
                           text="When you attack with Wrecking Ball, draw a card then discard a random card. If a card with 6 or more power is discarded this way, intimidate.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.DRAW_DISCARD_INTIMIDATE)]))

    # ── YELLOW (Pitch 2) — 14 cards ──

    # Barraging Beatdown x2: cost 0, power 0, def 3, go again, intimidate
    # Non-attack action: next Brute attack gains conditional +3 power
    for _ in range(2):
        cards.append(Card("Barraging Beatdown", CardType.ACTION, cost=0, pitch=2,
                           power=0, defense=3, color=Color.YELLOW, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.BRUTE,
                           text="Intimidate, then your next Brute attack this turn gains 'While this attack is defended by less than 2 non-equipment cards it has +3 power'. Go again.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_PLAY, action=EffectAction.INTIMIDATE)]))

    # Muscle Mutt x2: cost 3, power 6, def 2
    for _ in range(2):
        cards.append(Card("Muscle Mutt", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=2, color=Color.YELLOW,
                           card_class=CardClass.BRUTE,
                           text=""))

    # Pack Call x2: cost 3, power 6, def 3
    # When you defend with Pack Call, reveal top card of deck; if 6+ power keep on top
    for _ in range(2):
        cards.append(Card("Pack Call", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=3, color=Color.YELLOW,
                           card_class=CardClass.BRUTE,
                           text="When you defend with Pack Call, reveal the top card of your deck. If it has 6 or more power, put it on top of your deck. Otherwise, put it on he bottom."))

    # Raging Onslaught x2: cost 3, power 6, def 3
    for _ in range(2):
        cards.append(Card("Raging Onslaught", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=3, color=Color.YELLOW,
                           card_class=CardClass.BRUTE,
                           text=""))

    # Smash Instinct x2: cost 2, power 6, def 3, intimidate
    for _ in range(2):
        cards.append(Card("Smash Instinct", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=3, color=Color.YELLOW,
                           card_class=CardClass.BRUTE,
                           text="When you attack with Smash Instinct, intimidate.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.INTIMIDATE)]))

    # Smash with Big Tree x2: cost 2, power 6, no_block
    for _ in range(2):
        cards.append(Card("Smash with Big Tree", CardType.ACTION_ATTACK, cost=2, pitch=2,
                           power=6, defense=0, color=Color.YELLOW, no_block=True,
                           card_class=CardClass.BRUTE,
                           text=""))

    # Wounded Bull x2: cost 2, power 6, def 3, go again
    for _ in range(2):
        cards.append(Card("Wounded Bull", CardType.ACTION_ATTACK, cost=3, pitch=2,
                           power=6, defense=2, color=Color.YELLOW,
                           card_class=CardClass.BRUTE,
                           text="When you play Wounded Bull, if you have less health than an opposing hero, it gains +1 power."))

    # ── BLUE (Pitch 3) — 12 cards ──

    # Clearing Bellow x2: cost 0, power 0, def 3, go again, intimidate
    for _ in range(2):
        cards.append(Card("Clearing Bellow", CardType.ACTION, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.BRUTE,
                           text="Intimidate.Go again.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_PLAY, action=EffectAction.INTIMIDATE)]))

    # Come to Fight x2: cost 0, power 0, def 3 — Generic
    for _ in range(2):
        cards.append(Card("Come to Fight", CardType.ACTION, cost=1, pitch=3,
                           power=0, defense=3, color=Color.BLUE,
                           text="Your next attack action card you play this turn gains +1 power. Go again."))

    # Dodge x2: cost 0, def 2 — Generic defense reaction
    for _ in range(2):
        cards.append(Card("Dodge", CardType.DEFENSE_REACTION, cost=0, pitch=3,
                           power=0, defense=2, color=Color.BLUE,
                           text=""))

    # Rally the Rearguard x2 — Generic
    for _ in range(2):
        cards.append(Card("Rally the Rearguard", CardType.ACTION_ATTACK, cost=2, pitch=3,
                           power=4, defense=2, color=Color.BLUE,
                          text="Once per turn Instant - Discard a card: Rally the Rearguard gains +3 block.  Activate this ability only while Rally the Rearguard is defending."))

    # Titanium Bauble x2: cost 0, pitch 3, def 3 — Generic resource
    for _ in range(2):
        cards.append(Card("Titanium Bauble", CardType.RESOURCE, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE,
                           text=""))

    # Wrecker Romp x2: cost 2, power 6, def 3
    for _ in range(2):
        cards.append(Card("Wrecker Romp", CardType.ACTION_ATTACK, cost=2, pitch=3,
                           power=6, defense=3, color=Color.BLUE,
                           card_class=CardClass.BRUTE))

    # Chief Ruk'utan x1: Mentor
    # When face-up: whenever you play a card with 6+ power, intimidate + lesson counter.
    # At 2 lesson counters: banish, search for Alpha Rampage, put face-up in arsenal.
    cards.append(Card("Chief Ruk'utan", CardType.MENTOR, cost=0, pitch=0,
                       power=0, defense=0,
                       card_class=CardClass.BRUTE,
                       text="While Ruk'utan is face down in arsenal, at the start of your turn, you may turn him face up.  While Ruk'utan is face up in arsenal, whenever ou play a card with 6 or more power, intimidate and put a lesson counter on him.  Then if there are 2 or more lesson counters on Rok'utan, banish him, search your deck for Alpha Rampage, put it face up in arsenal and shuffle."))
    return cards


def build_rhinar_equipment():
    """Bone Basher weapon + equipment set."""
    return [
        Card("Bone Basher", CardType.WEAPON, cost=2, power=4, equip_slot=EquipSlot.WEAPON,
             card_class=CardClass.BRUTE,
             text="Once per Turn Action — 2: Attack."),
        Card("Blossom of Spring", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.CHEST,
             text="Action: Destroy Blossom of Spring: Gain 1 resource. Go again"),
        Card("Bone Vizier", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.HEAD,
             card_class=CardClass.BRUTE,
             text="When Bone Vizier is destroyed, reveal the top card of your deck.  If it has 6 or more power, put it on the top of your deck. Otherwise, put it on the bottom. Blade Break"),
        Card("Ironhide Gauntlet", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.ARMS,
             card_class=CardClass.BRUTE,
             text="When you defend with Ironhide Gauntlet you may pay 1 resource.  If you do, it gains +2 block and 'When the combat chain closes, destroy Ironhide Gauntlets'"),
        Card("Ironhide Legs", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.LEGS,
             card_class=CardClass.BRUTE,
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

    # ── Hero card (extracted before shuffling) ──
    cards.append(Card("Dorinthea, Quicksilver Prodigy", CardType.HERO, card_class=CardClass.WARRIOR,
                       text="Young Hero. Life 20, Intellect 4. The first time Dawnblade, Resplendent gains go again each turn, you may attack and additional time with it this turn."))

    # ── RED (Pitch 1) — 16 cards ──

    # En Garde x2: cost 1, def 3, warrior action (non-attack)
    for _ in range(2):
        cards.append(Card("En Garde", CardType.ACTION, cost=1, pitch=1,
                           power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.WARRIOR,
                           text="Your next weapon attack this turn gains +3 power. Go again."))

    # Flock of the Feather Walkers x2: cost 1, def 2, 5 power attack
    for _ in range(2):
        cards.append(Card("Flock of the Feather Walkers", CardType.ACTION_ATTACK, cost=1, pitch=1,
                           power=5, defense=2, color=Color.RED,
                           card_class=CardClass.GENERIC,
                           text="As an additional cost to play Flock of the Feather Walkers, reveal a card in your hand with cost 1 or less.  When you attack with Flock of the Feather Walkers, create a Quicken token."))

    # In the Swing x2: cost 0, def 3, attack reaction
    for _ in range(2):
        cards.append(Card("In the Swing", CardType.ATTACK_REACTION, cost=0, pitch=1,
                           power=0, defense=3, color=Color.RED,
                           card_class=CardClass.WARRIOR,
                           text="Play only if you have attacked 2 or more times with weapons this turn. Target weapon attack gains +3 power.",
                           effects=[CardEffect(
                               trigger=EffectTrigger.ON_ATTACK_REACTION,
                               action=EffectAction.ATTACK_POWER_BOOST,
                               magnitude=3,
                               condition=lambda ctx: ctx.get("weapon_attack_count", 0) >= 1,
                           )]))

    # Ironsong Response x2: cost 0, def 3, attack reaction
    for _ in range(2):
        cards.append(Card("Ironsong Response", CardType.ATTACK_REACTION, cost=0, pitch=1,
                           power=0, defense=3, color=Color.RED,
                           card_class=CardClass.WARRIOR,
                           text="Reprise - If the defending hero has defended with a card from their hand this chain link, your weapon attack gains +3 power."))

    # Second Swing x2: cost 1, def 3, warrior action
    for _ in range(2):
        cards.append(Card("Second Swing", CardType.ACTION, cost=1, pitch=1,
                           power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.WARRIOR,
                           text="If you have attacked with a weapon this turn, your next attack this turn gains +4 power. Go again.",
                           effects=[
                               CardEffect(
                                   trigger=EffectTrigger.ON_PLAY,
                                   action=EffectAction.WEAPON_ATTACK_POWER_BONUS,
                                   magnitude=4,
                                   condition=lambda ctx: ctx.get("weapon_attack_count", 0) >= 1,
                               )
                           ]))

    # Sharpen Steel x2: cost 0, def 3, action
    for _ in range(2):
        cards.append(Card("Sharpen Steel", CardType.ACTION, cost=0, pitch=1,
                           power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.WARRIOR,
                           text="Your next weapon attack this turn gains +3 power. Go again.",
                           effects=[CardEffect(
                               trigger=EffectTrigger.ON_PLAY,
                               action=EffectAction.WEAPON_ATTACK_POWER_BONUS,
                               magnitude=3,
                           )]))

    # Thrust x2: cost 0, def 2, attack reaction
    for _ in range(2):
        cards.append(Card("Thrust", CardType.ATTACK_REACTION, cost=1, pitch=1,
                           power=0, defense=2, color=Color.RED,
                           card_class=CardClass.WARRIOR,
                           text="Target sword attack gains +3 power.",
                           effects=[CardEffect(
                               trigger=EffectTrigger.ON_ATTACK_REACTION,
                               action=EffectAction.ATTACK_POWER_BOOST,
                               magnitude=3,
                           )]))

    # Warrior's Valor x2: cost 1, def 3, warrior action
    for _ in range(2):
        cards.append(Card("Warrior's Valor", CardType.ACTION, cost=1, pitch=1,
                           power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.WARRIOR,
                           text="Your next weapon attack this turn gains +3 power and 'When this attack hits, it gains go again.' Go again."))

    # ── YELLOW (Pitch 2) — 11 cards ──

    # Driving Blade x2: cost 2, def 3, warrior action
    for _ in range(2):
        cards.append(Card("Driving Blade", CardType.ACTION, cost=2, pitch=2,
                           power=0, defense=3, color=Color.YELLOW,
                           card_class=CardClass.WARRIOR,
                           text="Your next weapon attack this turn gains +2 power and go again. Go again."))

    # Glistening Steelblade x1: cost 1, def 3, Dorinthea spec
    cards.append(Card("Glistening Steelblade", CardType.ACTION, cost=1, pitch=2,
                       power=0, defense=3, color=Color.YELLOW, keywords=[Keyword.GO_AGAIN],
                       card_class=CardClass.WARRIOR,
                       text="Dorinthea Specialization. Your next Dawnblade attack this turn has go again.  Whenever Dawnblade hits a hero this turn, put a +1 counter on it. Go again."))

    # On a Knife Edge x2: cost 0, def 2, warrior action
    for _ in range(2):
        cards.append(Card("On a Knife Edge", CardType.ACTION, cost=0, pitch=2,
                           power=0, defense=2, color=Color.YELLOW, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.WARRIOR,
                           text="Your next sword attack this turn gains go again. Go again."))

    # Out for Blood x2: cost 1, def 3, warrior attack reaction
    for _ in range(2):
        cards.append(Card("Out for Blood", CardType.ATTACK_REACTION, cost=1, pitch=2,
                           power=0, defense=3, color=Color.YELLOW,
                           card_class=CardClass.WARRIOR,
                           text="Target weaopn attack gains +2 power.  Reprise - If the defending hero has defended with a card from their hand this chain link, your next attack th is turn gains +1 power."))

    # Run Through x2: cost 1, def 3, warrior attack reaction
    for _ in range(2):
        cards.append(Card("Run Through", CardType.ATTACK_REACTION, cost=1, pitch=2,
                           power=0, defense=3, color=Color.YELLOW,
                           card_class=CardClass.WARRIOR,
                           text="Target sword attack gains go again.  Your next sword attack this turn gains +2 power.",
                           effects=[
                               CardEffect(trigger=EffectTrigger.ON_ATTACK_REACTION, action=EffectAction.SWORD_ATTACK_GO_AGAIN),
                               CardEffect(trigger=EffectTrigger.ON_ATTACK_REACTION, action=EffectAction.NEXT_SWORD_ATTACK_POWER_BONUS, magnitude=2),
                           ]))

    # Slice and Dice x2: cost 0, def 3, warrior action
    for _ in range(2):
        cards.append(Card("Slice and Dice", CardType.ACTION, cost=0, pitch=2,
                           power=0, defense=3, color=Color.YELLOW, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.WARRIOR,
                           text="The first time you attack with a weapon this turn, if it's a sword or dagger it gains +1 power.  The second time you attack with a weapon this turn if it's a sword or dagger it gains +2 power.  Go again.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_PLAY,
                                               action=EffectAction.WEAPON_ATTACK_BONUS_PER_SWING)]))

    # ── BLUE (Pitch 3) — 12 cards ──

    # Blade Flash x2: cost 1, def 2, warrior attack reaction
    for _ in range(2):
        cards.append(Card("Blade Flash", CardType.ATTACK_REACTION, cost=1, pitch=3,
                           power=0, defense=2, color=Color.BLUE,
                           card_class=CardClass.WARRIOR,
                           text="Target sword attack gains go again.",
                           effects=[CardEffect(EffectTrigger.ON_ATTACK_REACTION,
                                               EffectAction.SWORD_ATTACK_GO_AGAIN)]))

    # Hit and Run x2: cost 0, def 3, warrior action
    for _ in range(2):
        cards.append(Card("Hit and Run", CardType.ACTION, cost=0, pitch=3,
                           power=0, defense=3, color=Color.BLUE, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.WARRIOR,
                           text="Your next weapon attack this turn gains go again.  If you have attacked with a weapon this turn your next attack this turn gains +1 power.  Go again."))

    # Sigil of Solace x2: cost 0, instant — Generic
    for _ in range(2):
        cards.append(Card("Sigil of Solace", CardType.INSTANT, cost=0, pitch=3,
                           power=0, defense=0, color=Color.BLUE, no_block=True,
                           text="Gain 1 life."))

    # Titanium Bauble x2: cost 0 — Generic resource
    for _ in range(2):
        cards.append(Card("Titanium Bauble", CardType.RESOURCE, cost=0, pitch=3,
                           power=0, defense=0, color=Color.BLUE,
                           text=""))

    # Toughen Up x2: cost 2, def 4 — Generic defense reaction
    for _ in range(2):
        cards.append(Card("Toughen Up", CardType.DEFENSE_REACTION, cost=2, pitch=3,
                           power=0, defense=4, color=Color.BLUE,
                           text=""))

    # Visit the Blacksmith x2: cost 0, def 2, warrior action
    for _ in range(2):
        cards.append(Card("Visit the Blacksmith", CardType.ACTION, cost=0, pitch=3,
                           power=0, defense=2, color=Color.BLUE, keywords=[Keyword.GO_AGAIN],
                           card_class=CardClass.WARRIOR,
                           text="Your next sword attack this turn gains +1 power."))

    # Hala Goldenhelm x1: Mentor — starts face-down in arsenal
    cards.append(Card("Hala Goldenhelm", CardType.MENTOR, cost=0, pitch=0,
                       power=0, defense=3,
                       card_class=CardClass.WARRIOR,
                       text="While Hala is face down in arsenal, at the start of your turn you may turn her face up.  While Hala is face up in arsenal, whenever a sword attack you control hits, it gains go again and put a lesson counter on Hala.  Then if there are 2 or more lesson counters on Hala banish her, search your deck for Glistening Steelblade, put it face up in arsenal, and shuffle."))

    return cards


def build_dorinthea_equipment():
    """Dawnblade, Resplendent weapon + equipment set."""
    return [
        Card("Dawnblade, Resplendent", CardType.WEAPON, cost=1, power=2, equip_slot=EquipSlot.WEAPON,
             card_class=CardClass.WARRIOR,
             text="Once per Turn Action — 1: Attack. The second time you attack with Dawnblade each turn, it gains +1 power until the end of turn."),
        Card("Gallantry Gold", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.ARMS,
             card_class=CardClass.WARRIOR,
             text="Action — 1: destroy Gallantry Gold: Your weapon attacks gain +1 power this turn. Go again. Battleworn."),
        Card("Blossom of Spring", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.CHEST,
             text="Action: Destroy Blossom of Spring: Gain 1 resource. Go again"),
        Card("Ironrot Helm", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.ARMS,
             text="Blade Break.", keywords=[Keyword.BLADE_BREAK]),
        Card("Ironrot Legs", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.LEGS,
             text="Blade Break.", keywords=[Keyword.BLADE_BREAK]),
    ]


# ──────────────────────── CARD CATALOG ────────────────────────

def _build_card_catalog() -> Dict[str, "Card"]:
    """
    Build a mapping of underscore-slugged card identifier -> Card for every
    unique card template across both classic battle decks (main deck +
    equipment).

    Keys use underscores, e.g. ``bare_fangs_red``, ``dawnblade_resplendent``.
    Duplicate copies of the same card (e.g. the two Bare Fangs in Rhinar's
    deck) are collapsed to a single entry.
    """
    all_cards = (
        build_rhinar_deck()
        + build_rhinar_equipment()
        + build_dorinthea_deck()
        + build_dorinthea_equipment()
    )
    catalog: Dict[str, Card] = {}
    for card in all_cards:
        key = card.card_id.replace("-", "_")
        if key not in catalog:
            catalog[key] = card
    return catalog


# Maps underscore-slugged card_id -> Card, e.g. CARD_CATALOG["bare_fangs_red"]
CARD_CATALOG: Dict[str, Card] = _build_card_catalog()

"""
Flesh and Blood - Classic Battles: Rhinar vs Dorinthea
Exact card data sourced from official LSS decklists and card databases.

Rhinar deck: 40-card Blitz, young hero Rhinar (20 life, intellect 4)
Dorinthea deck: 40-card Blitz, young hero Dorinthea, Quicksilver Prodigy (20 life, intellect 4)
"""

import re
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, Optional, List
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
    play_condition: Optional[Callable[[Dict[str, Any]], bool]] = field(default=None, compare=False)
    young: Optional[bool] = None  # Only relevant for hero cards
    hero_life: Optional[int] = None
    hero_intellect: Optional[int] = None

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
    c = CARD_CATALOG
    return [
        c["rhinar"],
        c["bone_basher"],
        c["blossom_of_spring"],
        c["bone_vizier"],
        c["ironhide_gauntlet"],
        c["ironhide_legs"],
        c["alpha_rampage_red"],
        *[c["awakening_bellow_red"]] * 2,
        *[c["bare_fangs_red"]] * 2,
        *[c["beast_mode_red"]] * 2,
        *[c["pack_hunt_red"]] * 2,
        *[c["wild_ride_red"]] * 2,
        *[c["wrecking_ball_red"]] * 2,
        *[c["barraging_beatdown_yellow"]] * 2,
        *[c["muscle_mutt_yellow"]] * 2,
        *[c["pack_call_yellow"]] * 2,
        *[c["raging_onslaught_yellow"]] * 2,
        *[c["smash_instinct_yellow"]] * 2,
        *[c["smash_with_big_tree_yellow"]] * 2,
        *[c["wounded_bull_yellow"]] * 2,
        *[c["clearing_bellow_blue"]] * 2,
        *[c["come_to_fight_blue"]] * 2,
        *[c["dodge_blue"]] * 2,
        *[c["rally_the_rearguard_blue"]] * 2,
        *[c["titanium_bauble_blue"]] * 2,
        *[c["wrecker_romp_blue"]] * 2,
        c["chief_ruk_utan"],
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
    c = CARD_CATALOG
    return [
        c["dorinthea_quicksilver_prodigy"],
        c["dawnblade_resplendent"],
        c["gallantry_gold"],
        c["blossom_of_spring"],
        c["ironrot_helm"],
        c["ironrot_legs"],
        *[c["en_garde_red"]] * 2,
        *[c["flock_of_the_feather_walkers_red"]] * 2,
        *[c["in_the_swing_red"]] * 2,
        *[c["ironsong_response_red"]] * 2,
        *[c["second_swing_red"]] * 2,
        *[c["sharpen_steel_red"]] * 2,
        *[c["thrust_red"]] * 2,
        *[c["warrior_s_valor_red"]] * 2,
        *[c["driving_blade_yellow"]] * 2,
        c["glistening_steelblade_yellow"],
        *[c["on_a_knife_edge_yellow"]] * 2,
        *[c["out_for_blood_yellow"]] * 2,
        *[c["run_through_yellow"]] * 2,
        *[c["slice_and_dice_yellow"]] * 2,
        *[c["blade_flash_blue"]] * 2,
        *[c["hit_and_run_blue"]] * 2,
        *[c["sigil_of_solace_blue"]] * 2,
        *[c["titanium_bauble_blue"]] * 2,
        *[c["toughen_up_blue"]] * 2,
        *[c["visit_the_blacksmith_blue"]] * 2,
        c["hala_goldenhelm"],
    ]


from classic_battles import CARD_CATALOG  # noqa: E402

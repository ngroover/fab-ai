"""
Observation builder for the FaB gym environment.

Observation is a Dict space with two keys: "agent" and "opponent".
Each is a fixed-size float array describing that player's visible state.

Card encoding uses a fixed vocabulary of all card names in both decks.
Each card slot is encoded as a one-hot over the vocabulary + numeric stats +
card_type one-hot + card_class one-hot + additional-cost flags.
"""

from __future__ import annotations
from typing import List, Dict, TYPE_CHECKING

from cards import Keyword, CardType, CardClass
from card_effects import EffectAction

if TYPE_CHECKING:
    from game_state import Player, GameState

# ── Card vocabulary (all unique card names across both decks + weapons/equipment) ──
CARD_VOCAB: List[str] = [
    # Rhinar cards
    "Alpha Rampage", "Awakening Bellow", "Bare Fangs", "Beast Mode",
    "Pack Hunt", "Wild Ride", "Wrecking Ball", "Barraging Beatdown",
    "Muscle Mutt", "Pack Call", "Raging Onslaught", "Smash Instinct",
    "Smash with Big Tree", "Wounded Bull", "Clearing Bellow", "Come to Fight",
    "Dodge", "Rally the Rearguard", "Titanium Bauble", "Wrecker Romp",
    "Chief Ruk'utan",
    # Dorinthea cards
    "En Garde", "Flock of the Feather Walkers", "In the Swing",
    "Ironsong Response", "Second Swing", "Sharpen Steel", "Thrust",
    "Warrior's Valor", "Driving Blade", "Glistening Steelblade",
    "On a Knife Edge", "Out for Blood", "Run Through", "Slice and Dice",
    "Blade Flash", "Hit and Run", "Sigil of Solace", "Toughen Up",
    "Visit the Blacksmith", "Hala Goldenhelm",
    # Weapons & equipment
    "Bone Basher", "Dawnblade, Resplendent",
    "Blossom of Spring", "Bone Vizier", "Ironhide Gauntlet", "Ironhide Legs",
    "Gallantry Gold", "Ironrot Helm", "Ironrot Legs",
    # Tokens
    "Quicken",
    # Unknown/padding
    "<PAD>",
]
VOCAB_SIZE = len(CARD_VOCAB)
CARD_IDX: Dict[str, int] = {name: i for i, name in enumerate(CARD_VOCAB)}

# Card type one-hot ordering (all CardType values)
_CARD_TYPE_ORDER = [
    CardType.ACTION_ATTACK, CardType.ACTION, CardType.INSTANT,
    CardType.ATTACK_REACTION, CardType.DEFENSE_REACTION, CardType.EQUIPMENT,
    CardType.WEAPON, CardType.HERO, CardType.MENTOR, CardType.RESOURCE, CardType.TOKEN,
]
_CARD_TYPE_IDX: Dict[CardType, int] = {ct: i for i, ct in enumerate(_CARD_TYPE_ORDER)}
CARD_TYPE_COUNT = len(_CARD_TYPE_ORDER)

# Card class one-hot ordering
_CARD_CLASS_ORDER = [CardClass.GENERIC, CardClass.BRUTE, CardClass.WARRIOR]
_CARD_CLASS_IDX: Dict[CardClass, int] = {cc: i for i, cc in enumerate(_CARD_CLASS_ORDER)}
CARD_CLASS_COUNT = len(_CARD_CLASS_ORDER)

# Cards in hand: up to 8 slots (intellect 4 + arsenal overage buffer)
MAX_HAND = 8
# Numeric features per card:
#   vocab one-hot (VOCAB_SIZE)
#   + [cost, pitch, power, defense, go_again, color]  (6)
#   + card_type one-hot (CARD_TYPE_COUNT)
#   + card_class one-hot (CARD_CLASS_COUNT)
#   + [has_discard_cost, has_reveal_cost]  (2)
CARD_FEATURES = VOCAB_SIZE + 6 + CARD_TYPE_COUNT + CARD_CLASS_COUNT + 2
# Total per-player feature vector size
PLAYER_OBS_SIZE = (
    MAX_HAND * CARD_FEATURES   # hand
    + CARD_FEATURES            # arsenal card (or PAD)
    + 4                        # equipment defense values [head, chest, arms, legs]
    + 1                        # weapon power (effective)
    + 9                        # turn state flags/values:
                               #   life, action_points, resource_points,
                               #   next_weapon_go_again, next_weapon_power_bonus,
                               #   next_brute_attack_bonus, weapon_used, attacks_this_turn,
                               #   arena_card_count
)


def _encode_card(card) -> List[float]:
    """Return a fixed-length float vector of length CARD_FEATURES for one card (or zeros for PAD)."""
    if card is None:
        return [0.0] * CARD_FEATURES
    one_hot = [0.0] * VOCAB_SIZE
    idx = CARD_IDX.get(card.name, CARD_IDX["<PAD>"])
    one_hot[idx] = 1.0
    color_val = card.color.value / 3.0 if card.color else 0.0
    numeric = [
        card.cost / 5.0,
        card.pitch / 3.0,
        card.power / 10.0,
        card.defense / 5.0,
        float(Keyword.GO_AGAIN in card.keywords),
        color_val,
    ]
    type_hot = [0.0] * CARD_TYPE_COUNT
    if card.card_type in _CARD_TYPE_IDX:
        type_hot[_CARD_TYPE_IDX[card.card_type]] = 1.0
    class_hot = [0.0] * CARD_CLASS_COUNT
    if card.card_class in _CARD_CLASS_IDX:
        class_hot[_CARD_CLASS_IDX[card.card_class]] = 1.0
    has_discard = float(any(e.action == EffectAction.DISCARD_CARD_COST for e in card.effects))
    has_reveal = float(any(e.action == EffectAction.REVEAL_CARD_COST for e in card.effects))
    return one_hot + numeric + type_hot + class_hot + [has_discard, has_reveal]


def encode_player(player: 'Player') -> List[float]:
    """
    Encode a player's full visible state into a flat float list of length PLAYER_OBS_SIZE.
    From the perspective of that player — hand is fully visible to themselves.
    """
    obs: List[float] = []

    # Hand (pad to MAX_HAND)
    hand_cards = player.hand[:MAX_HAND]
    for card in hand_cards:
        obs += _encode_card(card)
    for _ in range(MAX_HAND - len(hand_cards)):
        obs += _encode_card(None)

    # Arsenal card
    if player.arsenal and player.arsenal.card_type != CardType.MENTOR:
        obs += _encode_card(player.arsenal)
    else:
        obs += _encode_card(None)

    # Equipment defense values (normalized)
    for slot in ["head", "chest", "arms", "legs"]:
        eq = player.equipment.get(slot)
        obs.append(eq.defense / 3.0 if eq else 0.0)

    # Weapon effective power
    obs.append(player.get_effective_weapon_power() / 10.0)

    # Turn state
    obs += [
        player.life / 20.0,
        float(player.action_points),
        player.resource_points / 5.0,
        float(player.next_weapon_go_again),
        player.next_weapon_power_bonus / 5.0,
        player.next_brute_attack_bonus / 5.0,
        float(player.weapon_used_this_turn),
        player.attacks_this_turn / 5.0,
        len(player.arena) / 4.0,
    ]

    assert len(obs) == PLAYER_OBS_SIZE, (
        f"Obs size mismatch: got {len(obs)}, expected {PLAYER_OBS_SIZE}"
    )
    return obs


def encode_opponent_public(player: 'Player') -> List[float]:
    """
    Encode the opponent's *public* state — hand is hidden (only hand size visible).
    All card slots are PAD; we still expose life/equipment/weapon/counters.
    """
    obs: List[float] = []

    # Hand — hidden, pad all slots, but encode hand size as first element of last slot
    for _ in range(MAX_HAND):
        obs += _encode_card(None)

    # Arsenal — hidden
    obs += _encode_card(None)

    # Equipment
    for slot in ["head", "chest", "arms", "legs"]:
        eq = player.equipment.get(slot)
        obs.append(eq.defense / 3.0 if eq else 0.0)

    # Weapon power
    obs.append(player.get_effective_weapon_power() / 10.0)

    # Turn state (public info only — life, attacks, weapon used)
    obs += [
        player.life / 20.0,
        0.0,  # action_points hidden
        0.0,  # resource_points hidden
        0.0,  # next_weapon_go_again hidden
        0.0,  # next_weapon_power_bonus hidden
        0.0,  # next_brute_attack_bonus hidden
        float(player.weapon_used_this_turn),
        player.attacks_this_turn / 5.0,
        len(player.arena) / 4.0,  # arena is public
    ]

    assert len(obs) == PLAYER_OBS_SIZE
    return obs


def build_observation(player: 'Player', opponent: 'Player', game,
                      pending_card=None) -> dict:
    """
    Build the full observation dict for the active agent.

    Returns:
        {
            "agent":        float list [PLAYER_OBS_SIZE],   # full self info
            "opponent":     float list [PLAYER_OBS_SIZE],   # public opponent info
            "global":       float list [2],                 # [turn_number/80, is_first_turn]
            "pending_card": float list [CARD_FEATURES],     # card committed to play (PITCH phase only)
        }
    """
    return {
        "agent":        encode_player(player),
        "opponent":     encode_opponent_public(opponent),
        "global":       [game.turn_number / 80.0, float(game.is_first_turn)],
        "pending_card": _encode_card(pending_card),  # all zeros when not in PITCH phase
    }

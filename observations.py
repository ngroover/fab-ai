"""
Observation builder for the FaB gym environment.

Observation is a Dict space with two keys: "agent" and "opponent".
Each is a fixed-size float array describing that player's visible state.

Card slots use the learned embeddings produced by `card_embeddings.py`.
The first import auto-trains the embeddings if no artifacts are on disk;
subsequent imports load them from `card_embeddings_out/`.
"""

from __future__ import annotations
from typing import List, TYPE_CHECKING

from card_embeddings import ensure_embeddings, get_embed_dim, DEFAULT_OUT_DIR
from actions import ActionType

if TYPE_CHECKING:
    from game_state import Player
    from actions import RecordedAction

# ── Card embeddings (loaded once at import time) ─────────────────────────
_EMBEDDINGS = ensure_embeddings()
CARD_FEATURES = get_embed_dim(DEFAULT_OUT_DIR)
_ZERO_FEATURES: List[float] = [0.0] * CARD_FEATURES

# Cards in hand: up to 8 slots (intellect 4 + arsenal overage buffer)
MAX_HAND = 8
# Pitch zone: at most intellect-many cards per turn
MAX_PITCH = 4
# Combat chain: cards that stay on the chain until it closes
MAX_CHAIN = 4
# Total per-player feature vector size
PLAYER_OBS_SIZE = (
    MAX_HAND * CARD_FEATURES   # hand
    + CARD_FEATURES            # arsenal card (or zeros)
    + 4 * CARD_FEATURES        # equipment card embeddings [head, chest, arms, legs]
    + CARD_FEATURES            # weapon card embedding
    + CARD_FEATURES            # hero card embedding
    + MAX_PITCH * CARD_FEATURES  # pitch zone (cards pitched this turn)
    + MAX_CHAIN * CARD_FEATURES  # combat chain (cards currently on chain)
    + CARD_FEATURES + 1          # graveyard: summed embeddings + count
    + CARD_FEATURES + 1          # banish zone: summed embeddings + count
    + CARD_FEATURES + 1          # deck remaining: summed embeddings + count
    + 4                        # turn state values:
                               #   life, action_points, resource_points, arena_card_count
)

# ── Action sequence (top-level rolling history) ──────────────────────────
# One-hot ActionType ordering follows the enum's natural definition order.
ACTION_TYPE_VOCAB = list(ActionType)
ACTION_TYPE_DIM = len(ACTION_TYPE_VOCAB)
# Per-slot features: action_type one-hot + card embedding + actor bit.
ACTION_FEATURES = ACTION_TYPE_DIM + CARD_FEATURES + 1
# Rolling window length — must match GameState.action_history's maxlen.
ACTION_SEQ_LEN = 64
ACTION_SEQ_SIZE = ACTION_SEQ_LEN * ACTION_FEATURES
_ACTION_TYPE_TO_INDEX = {at: i for i, at in enumerate(ACTION_TYPE_VOCAB)}


def _encode_card(card) -> List[float]:
    """Return the learned embedding vector for a card (zeros for None / unknown)."""
    if card is None:
        return list(_ZERO_FEATURES)
    emb = _EMBEDDINGS.get(card.name)
    if emb is None:
        return list(_ZERO_FEATURES)
    return list(emb)


def _sum_embeddings(cards) -> List[float]:
    """Sum embeddings of a card list (order-invariant zone representation)."""
    result = [0.0] * CARD_FEATURES
    for card in cards:
        emb = _EMBEDDINGS.get(card.name)
        if emb is not None:
            for i, v in enumerate(emb):
                result[i] += v
    return result


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
    from cards import CardType
    if player.arsenal and player.arsenal.card_type != CardType.MENTOR:
        obs += _encode_card(player.arsenal)
    else:
        obs += _encode_card(None)

    # Equipment card embeddings (zeros if slot empty or piece destroyed)
    for slot in ["head", "chest", "arms", "legs"]:
        eq = player.equipment.get(slot)
        obs += _encode_card(eq.card if eq and not eq.destroyed else None)

    # Weapon card embedding
    obs += _encode_card(player.weapon)

    # Hero card embedding
    obs += _encode_card(player.hero_card)

    # Turn state. Per-turn buff flags (next_weapon_*, next_brute_attack_bonus,
    # weapon_used_this_turn, attacks_this_turn) are derivable from the
    # action_sequence field and intentionally omitted here.
    obs += [
        player.life / 20.0,
        float(player.action_points),
        player.resource_points / 5.0,
        len(player.arena) / 4.0,
    ]

    # Pitch zone (cards pitched this turn — visible to self)
    pitch_cards = player.pitch_zone[:MAX_PITCH]
    for card in pitch_cards:
        obs += _encode_card(card)
    for _ in range(MAX_PITCH - len(pitch_cards)):
        obs += _encode_card(None)

    # Combat chain (cards on the chain — public info)
    chain_cards = player.combat_chain[:MAX_CHAIN]
    for card in chain_cards:
        obs += _encode_card(card)
    for _ in range(MAX_CHAIN - len(chain_cards)):
        obs += _encode_card(None)

    # Graveyard: summed embeddings + count
    obs += _sum_embeddings(player.graveyard)
    obs.append(len(player.graveyard) / 20.0)

    # Banish zone: summed embeddings + count
    all_banished = player.banished + player.permanently_banished
    obs += _sum_embeddings(all_banished)
    obs.append(len(all_banished) / 4.0)

    # Deck remaining: summed embeddings + count
    obs += _sum_embeddings(player.deck)
    obs.append(len(player.deck) / 20.0)

    assert len(obs) == PLAYER_OBS_SIZE, (
        f"Obs size mismatch: got {len(obs)}, expected {PLAYER_OBS_SIZE}"
    )
    return obs


def encode_opponent_public(player: 'Player') -> List[float]:
    """
    Encode the opponent's *public* state — hand is hidden (only hand size visible).
    All card slots are zeros; we still expose life/equipment/weapon/counters.
    """
    obs: List[float] = []

    # Hand — hidden, pad all slots
    for _ in range(MAX_HAND):
        obs += _encode_card(None)

    # Arsenal — hidden
    obs += _encode_card(None)

    # Equipment card embeddings (public — opponent can see what you have equipped)
    for slot in ["head", "chest", "arms", "legs"]:
        eq = player.equipment.get(slot)
        obs += _encode_card(eq.card if eq and not eq.destroyed else None)

    # Weapon card embedding (public)
    obs += _encode_card(player.weapon)

    # Hero card embedding (public)
    obs += _encode_card(player.hero_card)

    # Turn state (public info only). Per-turn buff/attack flags are derivable
    # from the action_sequence field and intentionally omitted here.
    obs += [
        player.life / 20.0,
        0.0,  # action_points hidden
        0.0,  # resource_points hidden
        len(player.arena) / 4.0,  # arena is public
    ]

    # Pitch zone — public (opponent can see what was pitched)
    pitch_cards = player.pitch_zone[:MAX_PITCH]
    for card in pitch_cards:
        obs += _encode_card(card)
    for _ in range(MAX_PITCH - len(pitch_cards)):
        obs += _encode_card(None)

    # Combat chain — public
    chain_cards = player.combat_chain[:MAX_CHAIN]
    for card in chain_cards:
        obs += _encode_card(card)
    for _ in range(MAX_CHAIN - len(chain_cards)):
        obs += _encode_card(None)

    # Graveyard: summed embeddings + count (public)
    obs += _sum_embeddings(player.graveyard)
    obs.append(len(player.graveyard) / 20.0)

    # Banish zone: summed embeddings + count (public)
    all_banished = player.banished + player.permanently_banished
    obs += _sum_embeddings(all_banished)
    obs.append(len(all_banished) / 4.0)

    # Deck remaining: summed embeddings + count (opponent deck is trackable from public info)
    obs += _sum_embeddings(player.deck)
    obs.append(len(player.deck) / 20.0)

    assert len(obs) == PLAYER_OBS_SIZE
    return obs


def encode_action(rec: 'RecordedAction', viewer_idx: int) -> List[float]:
    """Encode a single RecordedAction relative to the viewing player."""
    one_hot = [0.0] * ACTION_TYPE_DIM
    one_hot[_ACTION_TYPE_TO_INDEX[rec.action_type]] = 1.0
    actor_bit = 0.0 if rec.actor_idx == viewer_idx else 1.0
    return one_hot + _encode_card(rec.card) + [actor_bit]


def encode_action_sequence(history, viewer_idx: int) -> List[float]:
    """Encode the rolling action history into a flat float list of length ACTION_SEQ_SIZE.

    Left-padded with zeros: oldest entries first, most recent at the end.
    """
    pad = ACTION_SEQ_LEN - len(history)
    out: List[float] = [0.0] * (pad * ACTION_FEATURES) if pad > 0 else []
    for rec in history:  # deque iterates oldest -> newest
        out.extend(encode_action(rec, viewer_idx))
    return out


def build_observation(player: 'Player', opponent: 'Player', game,
                      viewer_idx: int, pending_card=None) -> dict:
    """
    Build the full observation dict for the active agent.

    Returns:
        {
            "agent":           float list [PLAYER_OBS_SIZE],   # full self info
            "opponent":        float list [PLAYER_OBS_SIZE],   # public opponent info
            "global":          float list [2],                 # [turn_number/80, is_first_turn]
            "pending_card":    float list [CARD_FEATURES],     # card committed to play (PITCH only)
            "action_sequence": float list [ACTION_SEQ_SIZE],   # rolling action history
        }
    """
    return {
        "agent":           encode_player(player),
        "opponent":        encode_opponent_public(opponent),
        "global":          [game.turn_number / 80.0, float(game.is_first_turn)],
        "pending_card":    _encode_card(pending_card),  # zeros when not in PITCH phase
        "action_sequence": encode_action_sequence(game.action_history, viewer_idx),
    }

"""
Neural-network-backed agent for FaBEnv.

`NeuralAgent` implements the same selector interface as the rule-based
agents in `agents.py`, but delegates every decision to a pluggable model.
The default model is `PolicyValueNetwork`, a small PyTorch MLP with
random weights that produces:
  - a softmax distribution over the *current* legal-action list
  - a scalar value estimate of the state

The model is not trained yet; this module just wires the architecture in
so games can be played against / between neural agents.
"""

from __future__ import annotations

from typing import List, Optional, Tuple, TYPE_CHECKING

import torch
import torch.nn as nn
import torch.nn.functional as F

from actions import Action, ActionType
from observations import (
    PLAYER_OBS_SIZE,
    CARD_FEATURES,
    ACTION_SEQ_SIZE,
    ACTION_TYPE_DIM,
    _ACTION_TYPE_TO_INDEX,
    _encode_card,
)

if TYPE_CHECKING:
    from game_state import Player


# ── Sizes ────────────────────────────────────────────────────────────────
# Trunk input = both player views + global + pending_card + action_sequence.
OBS_FLAT_SIZE = 2 * PLAYER_OBS_SIZE + 2 + CARD_FEATURES + ACTION_SEQ_SIZE
# Per-action features: action-type one-hot + card embedding + 4 scalar fields.
ACTION_FEAT_SIZE = ACTION_TYPE_DIM + CARD_FEATURES + 4
TRUNK_HIDDEN = 256
LATENT_DIM = 64


# ── Featurization helpers ────────────────────────────────────────────────
def flatten_obs(obs: dict) -> torch.Tensor:
    """Concatenate the observation dict into a single flat float tensor.

    Tolerates missing keys (e.g. when called from `select_choose_first`,
    where the env hasn't built an obs dict yet) by zero-padding.
    """
    agent = obs.get("agent") or [0.0] * PLAYER_OBS_SIZE
    opponent = obs.get("opponent") or [0.0] * PLAYER_OBS_SIZE
    global_v = obs.get("global") or [0.0, 0.0]
    pending = obs.get("pending_card") or [0.0] * CARD_FEATURES
    action_seq = obs.get("action_sequence") or [0.0] * ACTION_SEQ_SIZE
    flat = list(agent) + list(opponent) + list(global_v) + list(pending) + list(action_seq)
    if len(flat) != OBS_FLAT_SIZE:
        # Pad or truncate defensively so a stale observation can't crash inference.
        if len(flat) < OBS_FLAT_SIZE:
            flat += [0.0] * (OBS_FLAT_SIZE - len(flat))
        else:
            flat = flat[:OBS_FLAT_SIZE]
    return torch.tensor(flat, dtype=torch.float32)


def encode_action_features(action: Action) -> List[float]:
    """Encode a single legal Action as a fixed-size float vector."""
    one_hot = [0.0] * ACTION_TYPE_DIM
    one_hot[_ACTION_TYPE_TO_INDEX[action.action_type]] = 1.0
    card_emb = _encode_card(action.card)
    pitch_index = -1 if action.pitch_index is None else action.pitch_index
    hand_index = -1 if action.hand_index is None else action.hand_index
    scalars = [
        float(action.from_arsenal),
        pitch_index / 8.0,
        hand_index / 8.0,
        action.pitch_order_index / 4.0,
    ]
    return one_hot + card_emb + scalars


def stack_action_features(legal: List[Action]) -> torch.Tensor:
    """Build a (N_legal, ACTION_FEAT_SIZE) tensor for the current legal list."""
    rows = [encode_action_features(a) for a in legal]
    return torch.tensor(rows, dtype=torch.float32)


# ── Model ────────────────────────────────────────────────────────────────
class PolicyValueNetwork(nn.Module):
    """Small MLP with a bilinear policy head and a scalar value head.

    The bilinear policy head produces one logit per legal action by dotting
    a state latent with an action latent, so it naturally handles the
    variable-length legal-action lists returned by `FaBEnv.legal_actions()`.
    """

    def __init__(self, seed: Optional[int] = None):
        if seed is not None:
            torch.manual_seed(seed)
        super().__init__()
        self.trunk = nn.Sequential(
            nn.Linear(OBS_FLAT_SIZE, TRUNK_HIDDEN),
            nn.ReLU(),
            nn.Linear(TRUNK_HIDDEN, LATENT_DIM),
            nn.ReLU(),
        )
        self.action_encoder = nn.Linear(ACTION_FEAT_SIZE, LATENT_DIM)
        self.value_head = nn.Linear(LATENT_DIM, 1)
        self.eval()

    def forward(
        self, obs_vec: torch.Tensor, action_feats: torch.Tensor
    ) -> Tuple[torch.Tensor, torch.Tensor]:
        state_latent = self.trunk(obs_vec)                       # (LATENT_DIM,)
        action_latents = self.action_encoder(action_feats)       # (N, LATENT_DIM)
        logits = action_latents @ state_latent                   # (N,)
        value = torch.tanh(self.value_head(state_latent)).squeeze(-1)  # scalar
        return logits, value

    def predict(
        self, obs: dict, legal: List[Action]
    ) -> Tuple[List[float], float]:
        """Run a forward pass and return (softmax probs, scalar value)."""
        obs_vec = flatten_obs(obs)
        action_feats = stack_action_features(legal)
        with torch.no_grad():
            logits, value = self.forward(obs_vec, action_feats)
            probs = F.softmax(logits, dim=-1)
        return probs.tolist(), float(value.item())


# ── Agent ────────────────────────────────────────────────────────────────
class NeuralAgent:
    """Agent that picks the argmax action from a policy/value model.

    Mirrors the interface of `RandomAgent` (agents.py) so it slots into
    `run_env.py` and `FaBEnv`'s phase-dispatch loop unchanged.
    """

    def __init__(
        self,
        model: Optional[PolicyValueNetwork] = None,
        seed: Optional[int] = None,
    ):
        self.model = model if model is not None else PolicyValueNetwork(seed=seed)

    def _choose(self, obs: Optional[dict], legal: List[Action]) -> Action:
        if len(legal) == 1:
            return legal[0]
        probs, _ = self.model.predict(obs or {}, legal)
        best = 0
        best_p = probs[0]
        for i in range(1, len(probs)):
            if probs[i] > best_p:
                best_p = probs[i]
                best = i
        return legal[best]

    # ── Selector methods (signatures match RandomAgent in agents.py) ─────
    def select_action(self, obs: dict, legal: List[Action], player: 'Player',
                      opponent: 'Player') -> Action:
        return self._choose(obs, legal)

    def select_defend(self, obs: dict, legal: List[Action], player: 'Player',
                      attack_power: int, already_defense: int = 0) -> Action:
        return self._choose(obs, legal)

    def select_arsenal(self, obs: dict, legal: List[Action],
                       player: 'Player') -> Action:
        return self._choose(obs, legal)

    def select_pitch(self, obs: dict, legal: List[Action], player: 'Player',
                     pending_card=None) -> Action:
        return self._choose(obs, legal)

    def select_pitch_order(self, obs: dict, legal: List[Action], player: 'Player') -> Action:
        return self._choose(obs, legal)

    def select_instant(self, obs: dict, legal: List[Action], player: 'Player',
                       attack_power: int = 0) -> Action:
        return self._choose(obs, legal)

    def select_reaction(self, obs: dict, legal: List[Action], player: 'Player',
                        attack_power: int = 0, is_attacker: bool = False) -> Action:
        return self._choose(obs, legal)

    def select_choose_first(self, legal: List[Action], player: 'Player') -> Action:
        return self._choose(None, legal)

"""
dist_protocol.py — wire format for distributed self-play.

The coordinator and workers exchange three kinds of messages:

  • HANDSHAKE_HELLO / HANDSHAKE_OK   (REQ ↔ REP) — initial auth + config + weights
  • MSG_TRANSITIONS                  (PUSH → PULL) — batch of game transitions
  • MSG_HEARTBEAT                    (REQ ↔ REP) — liveness ping
  • TOPIC_WEIGHTS                    (PUB → SUB)  — broadcast weight updates

All payloads are msgpack-packed dicts wrapped in an envelope that carries the
shared auth token. msgpack is used (instead of pickle) so a malicious client
cannot load arbitrary Python objects on the coordinator.

Transitions are sent in their already-flattened `Transition` form (from
self_play_trainer.py): tensors of floats, no Card/Action references. This
sidesteps the lambda-in-Card pickling problem and keeps wire sizes small.
"""

from __future__ import annotations

import hmac
import io
from typing import Any, Dict, List, Optional, Tuple

import msgpack
import torch


# ── Topic / message kinds ────────────────────────────────────────────────
TOPIC_WEIGHTS = b"weights"

KIND_HANDSHAKE_HELLO = "hello"
KIND_HANDSHAKE_OK    = "ok"
KIND_HANDSHAKE_FAIL  = "fail"
KIND_HEARTBEAT       = "heartbeat"
KIND_HEARTBEAT_OK    = "heartbeat_ok"
KIND_TRANSITIONS     = "transitions"


# ── Envelope ─────────────────────────────────────────────────────────────

def make_envelope(payload: Dict[str, Any], token: str, kind: str) -> bytes:
    """Pack a message with its auth token and a kind tag."""
    env = {"token": token, "kind": kind, "payload": payload}
    return msgpack.packb(env, use_bin_type=True)


def decode_envelope(data: bytes) -> Tuple[Optional[str], Optional[str], Optional[dict]]:
    """Return (kind, token, payload). Returns (None, None, None) on malformed input."""
    try:
        env = msgpack.unpackb(data, raw=False)
    except Exception:
        return None, None, None
    if not isinstance(env, dict):
        return None, None, None
    return env.get("kind"), env.get("token"), env.get("payload")


def check_token(received: Optional[str], expected: str) -> bool:
    """Constant-time token equality check."""
    if not received or not expected:
        return False
    return hmac.compare_digest(str(received), str(expected))


# ── Weight (de)serialization ─────────────────────────────────────────────

def pack_weights(state_dict: Dict[str, torch.Tensor], version: int, run_name: str) -> bytes:
    """Pack a network state_dict + metadata into a single msgpack blob."""
    buf = io.BytesIO()
    torch.save(state_dict, buf)
    payload = {
        "version": int(version),
        "run_name": str(run_name or ""),
        "state_dict": buf.getvalue(),
    }
    return msgpack.packb(payload, use_bin_type=True)


def unpack_weights(blob: bytes) -> Tuple[Dict[str, torch.Tensor], int, str]:
    """Reverse of `pack_weights`."""
    payload = msgpack.unpackb(blob, raw=False)
    buf = io.BytesIO(payload["state_dict"])
    state_dict = torch.load(buf, map_location="cpu", weights_only=True)
    return state_dict, int(payload["version"]), str(payload.get("run_name") or "")


# ── Transition (de)serialization ─────────────────────────────────────────

def transitions_to_wire(transitions: List["Transition"]) -> List[dict]:
    """Convert in-memory Transitions into plain dicts of primitive types."""
    out: List[dict] = []
    for t in transitions:
        out.append({
            "obs_vec": list(t.obs_vec),
            "action_feats": [list(row) for row in t.action_feats],
            "pi": list(t.pi),
            "to_play": int(t.to_play),
            "z": float(t.z),
            "weight_version": int(t.weight_version),
        })
    return out


def wire_to_transitions(items: List[dict]) -> List["Transition"]:
    """Reverse of `transitions_to_wire` — built lazily to avoid an import cycle."""
    from self_play_trainer import Transition

    out: List["Transition"] = []
    for d in items:
        out.append(Transition(
            obs_vec=list(d["obs_vec"]),
            action_feats=[list(r) for r in d["action_feats"]],
            pi=list(d["pi"]),
            to_play=int(d["to_play"]),
            z=float(d.get("z", 0.0)),
            weight_version=int(d.get("weight_version", 0)),
        ))
    return out


# ── TrainerConfig over the wire ──────────────────────────────────────────
# Only the fields workers actually need to play games. Keeping this explicit
# means the coordinator can run a newer TrainerConfig schema without breaking
# older workers.

CONFIG_FIELDS_FOR_WORKER = (
    "games_per_iter", "steps_per_iter",
    "n_simulations", "c_puct", "determinize",
    "dirichlet_alpha", "dirichlet_frac",
    "temp_start", "temp_end", "temp_drop_step",
    "opponent_pool", "deck_pool",
    "run_name",
)


def config_to_wire(cfg) -> dict:
    """Subset of TrainerConfig fields the worker actually needs."""
    out: Dict[str, Any] = {}
    for k in CONFIG_FIELDS_FOR_WORKER:
        v = getattr(cfg, k, None)
        if isinstance(v, tuple):
            v = list(v)
        out[k] = v
    return out


def wire_to_config(d: dict):
    """Reverse of `config_to_wire` — returns a real TrainerConfig instance."""
    from self_play_trainer import TrainerConfig

    kwargs: Dict[str, Any] = {}
    for k in CONFIG_FIELDS_FOR_WORKER:
        if k not in d or d[k] is None:
            continue
        v = d[k]
        # tuple-typed config fields need to be tuples for downstream code.
        if k in ("opponent_pool", "deck_pool") and isinstance(v, list):
            v = tuple(v)
        kwargs[k] = v
    return TrainerConfig(**kwargs)

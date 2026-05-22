"""
Card embedding generator for fab-ai.

Builds a hand-crafted feature vector per card covering all structured
fields on `Card` (card_type, color, card_class, equip_slot, keywords,
effect triggers/actions/magnitudes, stats, hero info, no_block, young),
then trains a small autoencoder to compress them into a dense learned
embedding suitable as a fixed input for downstream NN policies/value-nets.

Vocabulary is derived from CARD_CATALOG keys at build time, so adding a
card to the catalog and re-running this script automatically extends the
embedding table — no hand-edits to other files required.

Run as a script to (re)generate embeddings:
    python card_embeddings.py [--embed-dim 32] [--epochs 2000] [--out-dir DIR]

Use as a library:
    from card_embeddings import ensure_embeddings, get_embed_dim
    emb = ensure_embeddings()         # {card_name: np.ndarray of shape (embed_dim,)}
    dim = get_embed_dim()
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import warnings
from typing import Dict, List, Optional, Tuple

from card_effects import EffectAction, EffectTrigger
from cards import Card, CardClass, CardType, Color, EquipSlot, Keyword
from classic_battles import CARD_CATALOG


DEFAULT_OUT_DIR = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "card_embeddings_out"
)
DEFAULT_EMBED_DIM = 32
DEFAULT_EPOCHS = 2000


# Stable orderings recorded in meta.json for drift detection.
CARD_TYPE_VALUES = list(CardType)
COLOR_VALUES     = list(Color)
CLASS_VALUES     = list(CardClass)
SLOT_VALUES      = list(EquipSlot)
KEYWORD_VALUES   = list(Keyword)
TRIGGER_VALUES   = list(EffectTrigger)
ACTION_VALUES    = list(EffectAction)

# Dimension of the pure-Python hand-crafted feature vector (no autoencoder, no name one-hot).
PURE_EMBED_DIM = (
    len(CARD_TYPE_VALUES) + len(COLOR_VALUES) + len(CLASS_VALUES) +
    len(SLOT_VALUES) + len(KEYWORD_VALUES) + 2 + 6 +
    len(TRIGGER_VALUES) + len(ACTION_VALUES) + 2
)


# ─────────────────────────────────────────────────────────────────────────────
# Feature encoder
# ─────────────────────────────────────────────────────────────────────────────

def _unique_cards_in_order() -> List[Card]:
    """Cards from CARD_CATALOG, deduplicated by card_id (name + color), in insertion order.

    A "Pack Hunt" red and a hypothetical "Pack Hunt" blue have different
    stats, so they must be encoded as distinct entries.
    """
    seen, out = set(), []
    for card in CARD_CATALOG.values():
        if card.card_id in seen:
            continue
        seen.add(card.card_id)
        out.append(card)
    return out


def _one_hot(value, options: List) -> List[float]:
    vec = [0.0] * len(options)
    if value is None:
        return vec
    for i, opt in enumerate(options):
        if opt == value:
            vec[i] = 1.0
            break
    return vec


def encode_card(card: Card, vocab: List[str]) -> List[float]:
    """Return the hand-crafted feature vector for a single card.

    `vocab` is a list of card_ids (not raw names) so that different-colored
    versions of the same card get distinct one-hots.

    Layout (concatenated):
      - card_id one-hot             (len(vocab))
      - card_type multi-hot         (len(CARD_TYPE_VALUES))
      - color one-hot               (len(COLOR_VALUES))
      - card_class one-hot          (len(CLASS_VALUES))
      - equip_slot one-hot          (len(SLOT_VALUES))
      - keyword multi-hot           (len(KEYWORD_VALUES))
      - no_block, young             (2)
      - cost, pitch, power, defense, hero_life, hero_intellect (6, normalized)
      - effect-trigger multi-hot    (len(TRIGGER_VALUES))
      - effect-action multi-hot     (len(ACTION_VALUES))
      - effect count, magnitude sum (2, normalized)
    """
    parts: List[float] = []

    # card_id one-hot
    id_oh = [0.0] * len(vocab)
    if card.card_id in vocab:
        id_oh[vocab.index(card.card_id)] = 1.0
    parts += id_oh

    # card_type multi-hot (cards can have multiple types, e.g. Attack + Action)
    type_vec = [0.0] * len(CARD_TYPE_VALUES)
    for ct in card.card_type:
        for i, opt in enumerate(CARD_TYPE_VALUES):
            if opt == ct:
                type_vec[i] = 1.0
                break
    parts += type_vec

    # Categorical one-hots
    parts += _one_hot(card.color, COLOR_VALUES)
    parts += _one_hot(card.card_class, CLASS_VALUES)
    parts += _one_hot(card.equip_slot, SLOT_VALUES)

    # Keyword multi-hot
    kw_vec = [0.0] * len(KEYWORD_VALUES)
    for kw in card.keywords:
        for i, opt in enumerate(KEYWORD_VALUES):
            if opt == kw:
                kw_vec[i] = 1.0
                break
    parts += kw_vec

    # Boolean flags
    parts.append(1.0 if card.no_block else 0.0)
    parts.append(1.0 if card.young else 0.0)

    # Normalized scalars
    parts.append(card.cost / 5.0)
    parts.append(card.pitch / 3.0)
    parts.append(card.power / 10.0)
    parts.append(card.defense / 5.0)
    parts.append((card.hero_life or 0) / 20.0)
    parts.append((card.hero_intellect or 0) / 4.0)

    # Effect-trigger / -action multi-hots + aggregates
    trig_vec = [0.0] * len(TRIGGER_VALUES)
    act_vec  = [0.0] * len(ACTION_VALUES)
    total_mag = 0
    for eff in card.effects:
        for i, opt in enumerate(TRIGGER_VALUES):
            if opt == eff.trigger:
                trig_vec[i] = 1.0
                break
        for i, opt in enumerate(ACTION_VALUES):
            if opt == eff.action:
                act_vec[i] = 1.0
                break
        total_mag += eff.magnitude
    parts += trig_vec
    parts += act_vec
    parts.append(len(card.effects) / 4.0)
    parts.append(total_mag / 10.0)

    return parts


def _pure_embeddings() -> Dict[str, List[float]]:
    """Pure-Python fallback: hand-crafted features without id one-hot, no torch/numpy required.

    Keyed by `card_id` so red/yellow/blue versions of the same card get
    distinct entries.
    """
    result: Dict[str, List[float]] = {}
    seen: set = set()
    for card in CARD_CATALOG.values():
        if card.card_id not in seen:
            result[card.card_id] = encode_card(card, [])
            seen.add(card.card_id)
    return result


def build_feature_matrix() -> Tuple[List[str], List[List[float]]]:
    """Encode every unique card in CARD_CATALOG → (card_ids, feature matrix)."""
    cards = _unique_cards_in_order()
    ids = [c.card_id for c in cards]
    feats = [encode_card(c, ids) for c in cards]
    return ids, feats


# ─────────────────────────────────────────────────────────────────────────────
# Autoencoder
# ─────────────────────────────────────────────────────────────────────────────

class CardAutoencoder:
    """Placeholder definition; real class uses torch.nn — see train_autoencoder."""
    pass


def train_autoencoder(
    features: List[List[float]],
    embed_dim: int = DEFAULT_EMBED_DIM,
    epochs: int = DEFAULT_EPOCHS,
    lr: float = 1e-3,
    seed: int = 42,
    verbose: bool = True,
):
    import numpy as np
    import torch
    import torch.nn as nn

    class _Autoencoder(nn.Module):
        def __init__(self, input_dim: int, embed_dim: int, hidden: int = 64):
            super().__init__()
            self.encoder = nn.Sequential(
                nn.Linear(input_dim, hidden), nn.ReLU(), nn.Linear(hidden, embed_dim))
            self.decoder = nn.Sequential(
                nn.Linear(embed_dim, hidden), nn.ReLU(), nn.Linear(hidden, input_dim))

        def forward(self, x):
            z = self.encoder(x)
            return z, self.decoder(z)

    torch.manual_seed(seed)
    np.random.seed(seed)

    feat_array = np.array(features, dtype=np.float32)
    X = torch.from_numpy(feat_array).float()
    model = _Autoencoder(feat_array.shape[1], embed_dim)
    opt = torch.optim.Adam(model.parameters(), lr=lr)
    loss_fn = nn.MSELoss()

    losses: List[float] = []
    log_every = max(1, epochs // 10)
    for epoch in range(epochs):
        opt.zero_grad()
        _, x_hat = model(X)
        loss = loss_fn(x_hat, X)
        loss.backward()
        opt.step()
        losses.append(float(loss.item()))
        if verbose and (epoch == 0 or (epoch + 1) % log_every == 0):
            print(f"  epoch {epoch+1:>5}/{epochs}  loss={loss.item():.6f}")

    model.eval()
    with torch.no_grad():
        z, _ = model(X)
    return model, z.numpy(), losses


# ─────────────────────────────────────────────────────────────────────────────
# Persistence + drift detection
# ─────────────────────────────────────────────────────────────────────────────

def _build_meta(card_ids: List[str], feature_dim: int, embed_dim: int,
                losses: List[float]) -> Dict:
    return {
        "embed_dim":   embed_dim,
        "feature_dim": feature_dim,
        "n_cards":     len(card_ids),
        "card_type_values":   [v.name for v in CARD_TYPE_VALUES],
        "color_values":       [v.name for v in COLOR_VALUES],
        "class_values":       [v.name for v in CLASS_VALUES],
        "equip_slot_values":  [v.name for v in SLOT_VALUES],
        "keyword_values":     [v.name for v in KEYWORD_VALUES],
        "trigger_values":     [v.name for v in TRIGGER_VALUES],
        "action_values":      [v.name for v in ACTION_VALUES],
        "loss_initial": losses[0]  if losses else None,
        "loss_final":   losses[-1] if losses else None,
    }


def save_embeddings(out_dir: str, card_ids: List[str], features,
                    embeddings, model,
                    meta: Dict) -> None:
    import numpy as np
    import torch
    os.makedirs(out_dir, exist_ok=True)
    feat_arr = np.array(features, dtype=np.float32) if not hasattr(features, 'shape') else features
    emb_arr  = np.array(embeddings, dtype=np.float32) if not hasattr(embeddings, 'shape') else embeddings
    np.save(os.path.join(out_dir, "card_features.npy"),   feat_arr)
    np.save(os.path.join(out_dir, "card_embeddings.npy"), emb_arr)
    with open(os.path.join(out_dir, "card_ids.json"), "w") as f:
        json.dump(card_ids, f, indent=2)
    with open(os.path.join(out_dir, "meta.json"), "w") as f:
        json.dump(meta, f, indent=2)
    if model is not None and hasattr(model, 'state_dict'):
        torch.save(model.state_dict(), os.path.join(out_dir, "autoencoder.pt"))


def _check_schema_drift(meta: Dict) -> None:
    checks = [
        ("card_type_values",  [v.name for v in CARD_TYPE_VALUES]),
        ("color_values",      [v.name for v in COLOR_VALUES]),
        ("class_values",      [v.name for v in CLASS_VALUES]),
        ("equip_slot_values", [v.name for v in SLOT_VALUES]),
        ("keyword_values",    [v.name for v in KEYWORD_VALUES]),
        ("trigger_values",    [v.name for v in TRIGGER_VALUES]),
        ("action_values",     [v.name for v in ACTION_VALUES]),
    ]
    drift = [name for name, current in checks if meta.get(name) != current]
    cached_ids = meta.get("_card_ids_cache")
    if cached_ids is not None:
        catalog_ids = [c.card_id for c in _unique_cards_in_order()]
        if catalog_ids != cached_ids:
            drift.append("card_ids")
    if drift:
        warnings.warn(
            f"Card embedding schema drift in: {', '.join(drift)}. "
            f"Re-run `python card_embeddings.py` to regenerate embeddings.",
            stacklevel=2,
        )


def load_embeddings(out_dir: str = DEFAULT_OUT_DIR) -> Dict[str, List[float]]:
    """Load embeddings from `out_dir`. Raises FileNotFoundError if absent."""
    emb_path = os.path.join(out_dir, "card_embeddings.npy")
    if not os.path.exists(emb_path):
        raise FileNotFoundError(
            f"Embeddings not found at {emb_path}. "
            f"Run `python card_embeddings.py` to generate them."
        )
    import numpy as np
    with open(os.path.join(out_dir, "card_ids.json")) as f:
        card_ids = json.load(f)
    with open(os.path.join(out_dir, "meta.json")) as f:
        meta = json.load(f)
    meta["_card_ids_cache"] = card_ids
    _check_schema_drift(meta)
    embeddings = np.load(emb_path)
    return {cid: embeddings[i].tolist() for i, cid in enumerate(card_ids)}


def get_embed_dim(out_dir: str = DEFAULT_OUT_DIR) -> int:
    meta_path = os.path.join(out_dir, "meta.json")
    if not os.path.exists(meta_path):
        return PURE_EMBED_DIM
    with open(meta_path) as f:
        return int(json.load(f)["embed_dim"])


def ensure_embeddings(
    out_dir: str = DEFAULT_OUT_DIR,
    embed_dim: int = DEFAULT_EMBED_DIM,
    epochs: int = DEFAULT_EPOCHS,
) -> Dict[str, List[float]]:
    """Load embeddings, training them on the fly if artifacts are missing.

    Falls back to pure hand-crafted feature vectors when torch/numpy are not
    installed — PURE_EMBED_DIM floats per card, no autoencoder compression.
    """
    emb_path = os.path.join(out_dir, "card_embeddings.npy")
    if not os.path.exists(emb_path):
        try:
            import torch  # noqa: F401
            import numpy  # noqa: F401
        except ImportError:
            return _pure_embeddings()

        print(f"[card_embeddings] No artifacts at {out_dir!r}; generating...")
        card_ids, features = build_feature_matrix()
        model, embeddings, losses = train_autoencoder(
            features, embed_dim=embed_dim, epochs=epochs, verbose=False,
        )
        meta = _build_meta(card_ids, len(features[0]), embed_dim, losses)
        save_embeddings(out_dir, card_ids, features, embeddings, model, meta)
        print(f"[card_embeddings] Generated {len(card_ids)} embeddings "
              f"(dim={embed_dim}, final loss={losses[-1]:.6f}).")
    return load_embeddings(out_dir)


# ─────────────────────────────────────────────────────────────────────────────
# Content hash (for distributed-worker compatibility checks)
# ─────────────────────────────────────────────────────────────────────────────

def embeddings_hash(embeddings: Optional[Dict[str, List[float]]] = None) -> str:
    """Stable 16-char content hash of an embedding table.

    Independent of file format and float repr (values rounded to 8 decimals).
    Distributed workers compare this against the coordinator's hash during
    the handshake — a mismatch means the workers and coordinator are encoding
    cards into different vector spaces, which silently corrupts training.

    Loads the default artifacts via `ensure_embeddings()` when `embeddings`
    is omitted.
    """
    if embeddings is None:
        embeddings = ensure_embeddings()
    dim = len(next(iter(embeddings.values()))) if embeddings else 0
    h = hashlib.sha256()
    h.update(f"dim={dim}\n".encode("ascii"))
    for cid in sorted(embeddings.keys()):
        h.update(cid.encode("utf-8"))
        h.update(b"|")
        for v in embeddings[cid]:
            h.update(f"{v:.8f}".encode("ascii"))
            h.update(b",")
        h.update(b"\n")
    return h.hexdigest()[:16]


# ─────────────────────────────────────────────────────────────────────────────
# Similarity helpers
# ─────────────────────────────────────────────────────────────────────────────

def similar_cards(
    card_id: str,
    top_k: int = 10,
    metric: str = "dot",
    embeddings: Dict[str, List[float]] = None,
) -> List[Tuple[str, float]]:
    """Return the top_k cards most similar to `card_id`, scored by `metric`.

    `card_id` is the slug form (e.g. "alpha-rampage-red"). metric is "dot"
    (raw dot product) or "cosine" (length-normalized). The query card itself
    is excluded from the result.
    """
    import numpy as np
    if embeddings is None:
        embeddings = load_embeddings()
    if card_id not in embeddings:
        matches = [k for k in embeddings if card_id.lower() in k.lower()]
        hint = f" Did you mean: {matches}?" if matches else ""
        raise KeyError(f"Card {card_id!r} not in embeddings.{hint}")

    query = np.array(embeddings[card_id], dtype=np.float32)
    names = [k for k in embeddings if k != card_id]
    mat = np.array([embeddings[k] for k in names], dtype=np.float32)

    if metric == "dot":
        scores = mat @ query
    elif metric == "cosine":
        q_norm = query / (np.linalg.norm(query) + 1e-12)
        m_norm = mat / (np.linalg.norm(mat, axis=1, keepdims=True) + 1e-12)
        scores = m_norm @ q_norm
    else:
        raise ValueError(f"Unknown metric {metric!r}; use 'dot' or 'cosine'.")

    order = np.argsort(-scores)[:top_k]
    return [(names[i], float(scores[i])) for i in order]


def _print_similar(card_id: str, top_k: int, metric: str,
                   out_dir: str = DEFAULT_OUT_DIR) -> None:
    embeddings = load_embeddings(out_dir)
    results = similar_cards(card_id, top_k=top_k, metric=metric,
                            embeddings=embeddings)
    print(f"\nTop {top_k} cards similar to {card_id!r} (metric={metric}):\n")
    for i, (k, score) in enumerate(results, 1):
        print(f"  {i:>2}. {score:+.4f}  {k}")


# ─────────────────────────────────────────────────────────────────────────────
# CLI
# ─────────────────────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(description="Generate card embeddings.")
    parser.add_argument("--embed-dim", type=int, default=DEFAULT_EMBED_DIM)
    parser.add_argument("--epochs",    type=int, default=DEFAULT_EPOCHS)
    parser.add_argument("--lr",        type=float, default=1e-3)
    parser.add_argument("--seed",      type=int, default=42)
    parser.add_argument("--out-dir",   type=str, default=DEFAULT_OUT_DIR)
    parser.add_argument("--similar",   type=str, default=None,
                        help="Card id (e.g. 'alpha-rampage-red') to query — "
                             "prints most-similar cards instead of training.")
    parser.add_argument("--top",       type=int, default=10,
                        help="How many similar cards to show.")
    parser.add_argument("--metric",    type=str, default="dot",
                        choices=["dot", "cosine"])
    args = parser.parse_args()

    if args.similar is not None:
        _print_similar(args.similar, args.top, args.metric, args.out_dir)
        return

    print("Building feature matrix...")
    card_ids, features = build_feature_matrix()
    print(f"  cards: {len(card_ids)},  feature dim: {len(features[0])}")

    print(f"Training autoencoder "
          f"(embed_dim={args.embed_dim}, epochs={args.epochs})...")
    model, embeddings, losses = train_autoencoder(
        features,
        embed_dim=args.embed_dim,
        epochs=args.epochs,
        lr=args.lr,
        seed=args.seed,
    )
    print(f"  initial loss: {losses[0]:.6f}")
    print(f"  final loss:   {losses[-1]:.6f}")

    meta = _build_meta(card_ids, len(features[0]), args.embed_dim, losses)
    save_embeddings(args.out_dir, card_ids, features, embeddings, model, meta)
    print(f"Saved to {args.out_dir}/")


if __name__ == "__main__":
    main()

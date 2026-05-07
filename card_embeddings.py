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
import json
import os
import warnings
from typing import Dict, List, Tuple

import numpy as np
import torch
import torch.nn as nn

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


# ─────────────────────────────────────────────────────────────────────────────
# Feature encoder
# ─────────────────────────────────────────────────────────────────────────────

def _unique_cards_in_order() -> List[Card]:
    """Cards from CARD_CATALOG, deduplicated by name, in insertion order."""
    seen, out = set(), []
    for card in CARD_CATALOG.values():
        if card.name in seen:
            continue
        seen.add(card.name)
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


def encode_card(card: Card, vocab: List[str]) -> np.ndarray:
    """Return the hand-crafted feature vector for a single card.

    Layout (concatenated):
      - name one-hot                (len(vocab))
      - card_type one-hot           (len(CARD_TYPE_VALUES))
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

    # Name one-hot
    name_oh = [0.0] * len(vocab)
    if card.name in vocab:
        name_oh[vocab.index(card.name)] = 1.0
    parts += name_oh

    # Categorical one-hots
    parts += _one_hot(card.card_type, CARD_TYPE_VALUES)
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

    return np.asarray(parts, dtype=np.float32)


def build_feature_matrix() -> Tuple[List[str], np.ndarray]:
    """Encode every unique card in CARD_CATALOG → (names, feature matrix)."""
    cards = _unique_cards_in_order()
    names = [c.name for c in cards]
    feats = np.stack([encode_card(c, names) for c in cards])
    return names, feats


# ─────────────────────────────────────────────────────────────────────────────
# Autoencoder
# ─────────────────────────────────────────────────────────────────────────────

class CardAutoencoder(nn.Module):
    def __init__(self, input_dim: int, embed_dim: int, hidden: int = 64):
        super().__init__()
        self.encoder = nn.Sequential(
            nn.Linear(input_dim, hidden),
            nn.ReLU(),
            nn.Linear(hidden, embed_dim),
        )
        self.decoder = nn.Sequential(
            nn.Linear(embed_dim, hidden),
            nn.ReLU(),
            nn.Linear(hidden, input_dim),
        )

    def forward(self, x):
        z = self.encoder(x)
        return z, self.decoder(z)


def train_autoencoder(
    features: np.ndarray,
    embed_dim: int = DEFAULT_EMBED_DIM,
    epochs: int = DEFAULT_EPOCHS,
    lr: float = 1e-3,
    seed: int = 42,
    verbose: bool = True,
) -> Tuple[CardAutoencoder, np.ndarray, List[float]]:
    torch.manual_seed(seed)
    np.random.seed(seed)

    X = torch.from_numpy(features).float()
    model = CardAutoencoder(features.shape[1], embed_dim)
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

def _build_meta(names: List[str], feature_dim: int, embed_dim: int,
                losses: List[float]) -> Dict:
    return {
        "embed_dim":   embed_dim,
        "feature_dim": feature_dim,
        "n_cards":     len(names),
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


def save_embeddings(out_dir: str, names: List[str], features: np.ndarray,
                    embeddings: np.ndarray, model: CardAutoencoder,
                    meta: Dict) -> None:
    os.makedirs(out_dir, exist_ok=True)
    np.save(os.path.join(out_dir, "card_features.npy"),   features)
    np.save(os.path.join(out_dir, "card_embeddings.npy"), embeddings)
    with open(os.path.join(out_dir, "card_names.json"), "w") as f:
        json.dump(names, f, indent=2)
    with open(os.path.join(out_dir, "meta.json"), "w") as f:
        json.dump(meta, f, indent=2)
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
    cached_names = meta.get("_card_names_cache")
    if cached_names is not None:
        catalog_names = [c.name for c in _unique_cards_in_order()]
        if catalog_names != cached_names:
            drift.append("card_names")
    if drift:
        warnings.warn(
            f"Card embedding schema drift in: {', '.join(drift)}. "
            f"Re-run `python card_embeddings.py` to regenerate embeddings.",
            stacklevel=2,
        )


def load_embeddings(out_dir: str = DEFAULT_OUT_DIR) -> Dict[str, np.ndarray]:
    """Load embeddings from `out_dir`. Raises FileNotFoundError if absent."""
    emb_path = os.path.join(out_dir, "card_embeddings.npy")
    if not os.path.exists(emb_path):
        raise FileNotFoundError(
            f"Embeddings not found at {emb_path}. "
            f"Run `python card_embeddings.py` to generate them."
        )
    with open(os.path.join(out_dir, "card_names.json")) as f:
        names = json.load(f)
    with open(os.path.join(out_dir, "meta.json")) as f:
        meta = json.load(f)
    meta["_card_names_cache"] = names
    _check_schema_drift(meta)
    embeddings = np.load(emb_path)
    return {name: embeddings[i] for i, name in enumerate(names)}


def get_embed_dim(out_dir: str = DEFAULT_OUT_DIR) -> int:
    with open(os.path.join(out_dir, "meta.json")) as f:
        return int(json.load(f)["embed_dim"])


def ensure_embeddings(
    out_dir: str = DEFAULT_OUT_DIR,
    embed_dim: int = DEFAULT_EMBED_DIM,
    epochs: int = DEFAULT_EPOCHS,
) -> Dict[str, np.ndarray]:
    """Load embeddings, training them on the fly if artifacts are missing.

    Generation takes a few seconds for the current 46-card catalog, so this
    keeps a fresh checkout working without an explicit setup step.
    """
    emb_path = os.path.join(out_dir, "card_embeddings.npy")
    if not os.path.exists(emb_path):
        print(f"[card_embeddings] No artifacts at {out_dir!r}; generating...")
        names, features = build_feature_matrix()
        model, embeddings, losses = train_autoencoder(
            features, embed_dim=embed_dim, epochs=epochs, verbose=False,
        )
        meta = _build_meta(names, features.shape[1], embed_dim, losses)
        save_embeddings(out_dir, names, features, embeddings, model, meta)
        print(f"[card_embeddings] Generated {len(names)} embeddings "
              f"(dim={embed_dim}, final loss={losses[-1]:.6f}).")
    return load_embeddings(out_dir)


# ─────────────────────────────────────────────────────────────────────────────
# Similarity helpers
# ─────────────────────────────────────────────────────────────────────────────

def similar_cards(
    name: str,
    top_k: int = 10,
    metric: str = "dot",
    embeddings: Dict[str, np.ndarray] = None,
) -> List[Tuple[str, float]]:
    """Return the top_k cards most similar to `name`, scored by `metric`.

    metric: "dot" (raw dot product) or "cosine" (length-normalized).
    The query card itself is excluded from the result.
    """
    if embeddings is None:
        embeddings = load_embeddings()
    if name not in embeddings:
        matches = [n for n in embeddings if name.lower() in n.lower()]
        hint = f" Did you mean: {matches}?" if matches else ""
        raise KeyError(f"Card {name!r} not in embeddings.{hint}")

    query = embeddings[name]
    names = [n for n in embeddings if n != name]
    mat = np.stack([embeddings[n] for n in names])

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


def _print_similar(name: str, top_k: int, metric: str,
                   out_dir: str = DEFAULT_OUT_DIR) -> None:
    embeddings = load_embeddings(out_dir)
    results = similar_cards(name, top_k=top_k, metric=metric,
                            embeddings=embeddings)
    print(f"\nTop {top_k} cards similar to {name!r} (metric={metric}):\n")
    for i, (n, score) in enumerate(results, 1):
        print(f"  {i:>2}. {score:+.4f}  {n}")


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
                        help="Card name to query — prints most-similar cards "
                             "instead of training.")
    parser.add_argument("--top",       type=int, default=10,
                        help="How many similar cards to show.")
    parser.add_argument("--metric",    type=str, default="dot",
                        choices=["dot", "cosine"])
    args = parser.parse_args()

    if args.similar is not None:
        _print_similar(args.similar, args.top, args.metric, args.out_dir)
        return

    print("Building feature matrix...")
    names, features = build_feature_matrix()
    print(f"  cards: {len(names)},  feature dim: {features.shape[1]}")

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

    meta = _build_meta(names, features.shape[1], args.embed_dim, losses)
    save_embeddings(args.out_dir, names, features, embeddings, model, meta)
    print(f"Saved to {args.out_dir}/")


if __name__ == "__main__":
    main()

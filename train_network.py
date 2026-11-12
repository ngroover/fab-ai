"""
train_network.py — train the policy/value network on the Rust environment's
self-play records.

The Rust environment plays the games and runs the search
(`environment selfplay`, see environment/src/self_play.rs); this script only
fits a network to what it recorded and writes the weights back out for the
Rust side to load:

    python3 train_network.py selfplay.bin --out network.bin

For every recorded decision the network is shown the mover's observation and
trained to answer with

- a **policy**: the share of the search's visits each legal action got
  (cross-entropy, over the legal actions only), and
- a **value**: how the game ended for the mover, 1 / -1 / 0 (squared error).

The network is a plain MLP: the observation through a stack of ReLU layers,
then a linear policy head with one logit per action slot and a linear value
head squashed by tanh.

Weights file (little-endian), read by `load_network` and by the Rust
evaluator:

    [u8; 8]  magic "FABNN001"
    u32      observation size
    u32      action size
    u32      number of hidden layers, H
    [u32; H] their widths
    then for each of the H hidden layers, the policy head and the value head,
    in that order:
    [f32; out * in]  weight, row-major: weight[o][i] is at o * in + i
    [f32; out]       bias

Nothing here imports the Python game environment; the two file formats are the
whole interface.
"""

from __future__ import annotations

import argparse
import struct
import time
from dataclasses import dataclass
from typing import List, Optional, Sequence, Tuple

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

RECORDS_MAGIC = b"FABSP001"
NETWORK_MAGIC = b"FABNN001"

# A record's fixed-size part: game, ply, player, legal, outcome, root_value,
# nonzero. See environment/src/self_play.rs for the whole layout.
_RECORD_HEAD = struct.Struct("<IHBBffH")


# ── Self-play records ────────────────────────────────────────────────────────

@dataclass
class Records:
    """Self-play records, held column by column.

    The observation and the legal actions vary in length from record to
    record, so each is stored flat with a pointer array: record `r`'s
    observation entries are `obs_index[obs_ptr[r]:obs_ptr[r + 1]]` (and
    `obs_value` likewise), its legal actions
    `action_index[action_ptr[r]:action_ptr[r + 1]]` with `action_visits`.
    """

    obs_size: int
    action_size: int
    game: np.ndarray          # (n,) int64 — unique per game, across files
    player: np.ndarray        # (n,) uint8 — the mover's seat
    outcome: np.ndarray       # (n,) float32 — the game's result for the mover
    root_value: np.ndarray    # (n,) float32 — the search's own estimate
    obs_ptr: np.ndarray       # (n + 1,) int64
    obs_index: np.ndarray     # int64
    obs_value: np.ndarray     # float32
    action_ptr: np.ndarray    # (n + 1,) int64
    action_index: np.ndarray  # int64
    action_visits: np.ndarray  # float32

    def __len__(self) -> int:
        return len(self.outcome)

    def batch(self, rows: np.ndarray) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor, torch.Tensor]:
        """Records `rows` as dense tensors: observations `(B, obs_size)`, the
        legal-action mask and the policy target `(B, action_size)`, and the
        value target `(B,)`."""
        rows = np.asarray(rows, dtype=np.int64)
        obs = np.zeros((len(rows), self.obs_size), dtype=np.float32)
        r, flat = _gather(self.obs_ptr, rows)
        obs[r, self.obs_index[flat]] = self.obs_value[flat]

        policy = np.zeros((len(rows), self.action_size), dtype=np.float32)
        mask = np.zeros((len(rows), self.action_size), dtype=bool)
        r, flat = _gather(self.action_ptr, rows)
        policy[r, self.action_index[flat]] = self.action_visits[flat]
        mask[r, self.action_index[flat]] = True
        policy /= policy.sum(axis=1, keepdims=True)

        return (
            torch.from_numpy(obs),
            torch.from_numpy(mask),
            torch.from_numpy(policy),
            torch.from_numpy(self.outcome[rows]),
        )


def _gather(ptr: np.ndarray, rows: np.ndarray) -> Tuple[np.ndarray, np.ndarray]:
    """For the variable-length entries of `rows`: which row of the batch each
    entry belongs to, and where it is in the flat arrays."""
    starts = ptr[rows]
    lengths = ptr[rows + 1] - starts
    batch_row = np.repeat(np.arange(len(rows)), lengths)
    # Each entry's offset within its own record, then shifted to its start.
    within = np.arange(lengths.sum()) - np.repeat(np.cumsum(lengths) - lengths, lengths)
    return batch_row, np.repeat(starts, lengths) + within


def load_records(paths: Sequence[str]) -> Records:
    """Read one or more self-play files. They must agree on the observation
    and action sizes. Game numbers are made unique across files."""
    sizes: Optional[Tuple[int, int]] = None
    game, player, outcome, root_value = [], [], [], []
    obs_len, obs_index, obs_value = [], [], []
    action_len, action_index, action_visits = [], [], []
    game_base = 0

    for path in paths:
        with open(path, "rb") as f:
            data = f.read()
        if data[:8] != RECORDS_MAGIC or len(data) < 16:
            raise ValueError(f"{path} is not a self-play file")
        file_sizes = struct.unpack_from("<II", data, 8)
        if sizes is None:
            sizes = file_sizes
        elif sizes != file_sizes:
            raise ValueError(f"{path} has observation/action sizes {file_sizes}, not {sizes}")

        offset = 16
        last_game = -1
        try:
            while offset < len(data):
                g, _ply, seat, legal, z, q, nonzero = _RECORD_HEAD.unpack_from(data, offset)
                offset += _RECORD_HEAD.size
                obs_index.append(np.frombuffer(data, "<u2", nonzero, offset))
                offset += 2 * nonzero
                obs_value.append(np.frombuffer(data, "<f4", nonzero, offset))
                offset += 4 * nonzero
                action_index.append(np.frombuffer(data, "<u2", legal, offset))
                offset += 2 * legal
                action_visits.append(np.frombuffer(data, "<u4", legal, offset))
                offset += 4 * legal
                game.append(game_base + g)
                player.append(seat)
                outcome.append(z)
                root_value.append(q)
                obs_len.append(nonzero)
                action_len.append(legal)
                last_game = max(last_game, g)
        except (struct.error, ValueError) as error:
            raise ValueError(f"{path} is cut short at byte {offset}") from error
        game_base += last_game + 1

    if sizes is None:
        raise ValueError("no self-play files given")

    def flat(parts, dtype):
        return np.concatenate(parts).astype(dtype) if parts else np.zeros(0, dtype)

    def pointers(lengths):
        return np.concatenate([[0], np.cumsum(lengths, dtype=np.int64)]).astype(np.int64)

    records = Records(
        obs_size=sizes[0],
        action_size=sizes[1],
        game=np.asarray(game, dtype=np.int64),
        player=np.asarray(player, dtype=np.uint8),
        outcome=np.asarray(outcome, dtype=np.float32),
        root_value=np.asarray(root_value, dtype=np.float32),
        obs_ptr=pointers(obs_len),
        obs_index=flat(obs_index, np.int64),
        obs_value=flat(obs_value, np.float32),
        action_ptr=pointers(action_len),
        action_index=flat(action_index, np.int64),
        action_visits=flat(action_visits, np.float32),
    )
    if len(records.obs_index) and records.obs_index.max() >= records.obs_size:
        raise ValueError("an observation index is out of range")
    if len(records.action_index) and records.action_index.max() >= records.action_size:
        raise ValueError("an action index is out of range")
    return records


def split_by_game(records: Records, validation_every: int) -> Tuple[np.ndarray, np.ndarray]:
    """Row numbers for training and for validation. Whole games go one way or
    the other — every `validation_every`th game is held out — since the
    positions of one game are far too alike to be split fairly. 0 holds out
    nothing."""
    rows = np.arange(len(records))
    if validation_every <= 0:
        return rows, rows[:0]
    held_out = records.game % validation_every == validation_every - 1
    return rows[~held_out], rows[held_out]


# ── Network ──────────────────────────────────────────────────────────────────

class PolicyValueNet(nn.Module):
    """Observation → (one policy logit per action slot, value in -1..1)."""

    def __init__(self, obs_size: int, action_size: int, hidden: Sequence[int] = (256, 256)):
        super().__init__()
        if not hidden:
            raise ValueError("the network needs at least one hidden layer")
        self.obs_size = obs_size
        self.action_size = action_size
        self.hidden = tuple(hidden)
        widths = [obs_size, *hidden]
        self.trunk = nn.ModuleList(nn.Linear(a, b) for a, b in zip(widths, widths[1:]))
        self.policy_head = nn.Linear(widths[-1], action_size)
        self.value_head = nn.Linear(widths[-1], 1)

    def forward(self, obs: torch.Tensor) -> Tuple[torch.Tensor, torch.Tensor]:
        x = obs
        for layer in self.trunk:
            x = F.relu(layer(x))
        return self.policy_head(x), torch.tanh(self.value_head(x)).squeeze(-1)

    def layers(self) -> List[nn.Linear]:
        """Every linear layer, in the weights file's order."""
        return [*self.trunk, self.policy_head, self.value_head]


def save_network(net: PolicyValueNet, path: str) -> None:
    """Write `net`'s weights in the format the Rust evaluator loads."""
    with open(path, "wb") as f:
        f.write(NETWORK_MAGIC)
        f.write(struct.pack("<III", net.obs_size, net.action_size, len(net.hidden)))
        f.write(struct.pack(f"<{len(net.hidden)}I", *net.hidden))
        for layer in net.layers():
            # nn.Linear keeps its weight as (out, in): already row-major.
            f.write(layer.weight.detach().cpu().numpy().astype("<f4").tobytes())
            f.write(layer.bias.detach().cpu().numpy().astype("<f4").tobytes())


def load_network(path: str) -> PolicyValueNet:
    """Read a weights file back into a network."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != NETWORK_MAGIC or len(data) < 20:
        raise ValueError(f"{path} is not a network weights file")
    obs_size, action_size, layers = struct.unpack_from("<III", data, 8)
    hidden = struct.unpack_from(f"<{layers}I", data, 20)
    net = PolicyValueNet(obs_size, action_size, hidden)
    offset = 20 + 4 * layers
    expected = offset + 4 * sum(l.weight.numel() + l.bias.numel() for l in net.layers())
    if len(data) != expected:
        raise ValueError(f"{path} is {len(data)} bytes; its header calls for {expected}")
    with torch.no_grad():
        for layer in net.layers():
            for tensor in (layer.weight, layer.bias):
                values = np.frombuffer(data, "<f4", tensor.numel(), offset)
                tensor.copy_(torch.from_numpy(values.astype(np.float32)).reshape(tensor.shape))
                offset += 4 * tensor.numel()
    return net


# ── Training ─────────────────────────────────────────────────────────────────

def losses(
    net: PolicyValueNet,
    obs: torch.Tensor,
    mask: torch.Tensor,
    policy: torch.Tensor,
    outcome: torch.Tensor,
) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
    """Per-record policy loss, value loss, and whether the network's favourite
    legal action is the search's.

    The policy loss is the cross-entropy between the visit shares and the
    network's distribution over the *legal* actions: an illegal slot's logit
    is left out of the softmax, as it is when the search uses the network.
    """
    logits, value = net(obs)
    logits = logits.masked_fill(~mask, float("-inf"))
    log_probs = F.log_softmax(logits, dim=1)
    # Illegal slots are -inf with a target of 0; keep 0 * -inf out of the sum.
    policy_loss = -(policy * torch.where(mask, log_probs, torch.zeros_like(log_probs))).sum(dim=1)
    value_loss = (value - outcome) ** 2
    agrees = logits.argmax(dim=1) == policy.argmax(dim=1)
    return policy_loss, value_loss, agrees


@dataclass
class Metrics:
    """Averages over a set of records."""

    policy_loss: float
    value_loss: float
    agreement: float  # how often the network's top action is the search's

    def __str__(self) -> str:
        return (f"policy {self.policy_loss:.4f}  value {self.value_loss:.4f}  "
                f"top-1 {100 * self.agreement:.1f}%")


def evaluate(net: PolicyValueNet, records: Records, rows: np.ndarray,
             batch_size: int = 4096, device: str = "cpu") -> Metrics:
    """The network's average losses over records `rows`."""
    net.eval()
    totals = np.zeros(3)
    with torch.no_grad():
        for start in range(0, len(rows), batch_size):
            batch = [t.to(device) for t in records.batch(rows[start:start + batch_size])]
            totals += [t.float().sum().item() for t in losses(net, *batch)]
    return Metrics(*(totals / max(len(rows), 1)))


def baselines(records: Records, rows: np.ndarray) -> Metrics:
    """What a network that has learned nothing scores on `rows`: every legal
    action equally likely, and the average outcome as its value. The policy
    loss cannot go below the visit shares' own entropy, which is printed
    beside it."""
    if len(rows) == 0:
        return Metrics(0.0, 0.0, 0.0)
    legal = (records.action_ptr[rows + 1] - records.action_ptr[rows]).astype(np.float64)
    outcome = records.outcome[rows]
    return Metrics(
        policy_loss=float(np.log(legal).mean()),
        value_loss=float(outcome.var()),
        agreement=float((1.0 / legal).mean()),
    )


def target_entropy(records: Records, rows: np.ndarray, batch_size: int = 4096) -> float:
    """The average entropy of the visit shares over `rows`: the lowest policy
    loss any network could reach."""
    total = 0.0
    for start in range(0, len(rows), batch_size):
        _, _, policy, _ = records.batch(rows[start:start + batch_size])
        total += -(policy * torch.log(policy.clamp_min(1e-12))).sum().item()
    return total / max(len(rows), 1)


def train(
    net: PolicyValueNet,
    records: Records,
    train_rows: np.ndarray,
    validation_rows: np.ndarray,
    epochs: int = 10,
    batch_size: int = 256,
    lr: float = 1e-3,
    weight_decay: float = 1e-4,
    value_weight: float = 1.0,
    seed: int = 0,
    device: str = "cpu",
    log=print,
) -> List[Tuple[Metrics, Metrics]]:
    """Fit `net` to `train_rows` for `epochs` passes, logging the training and
    validation metrics after each. Returns them, one pair per epoch."""
    net.to(device)
    optimizer = torch.optim.AdamW(net.parameters(), lr=lr, weight_decay=weight_decay)
    rng = np.random.default_rng(seed)
    history = []
    for epoch in range(1, epochs + 1):
        net.train()
        started = time.time()
        order = rng.permutation(train_rows)
        for start in range(0, len(order), batch_size):
            batch = [t.to(device) for t in records.batch(order[start:start + batch_size])]
            policy_loss, value_loss, _ = losses(net, *batch)
            loss = policy_loss.mean() + value_weight * value_loss.mean()
            optimizer.zero_grad()
            loss.backward()
            optimizer.step()
        trained = evaluate(net, records, train_rows, device=device)
        held_out = evaluate(net, records, validation_rows, device=device)
        history.append((trained, held_out))
        log(f"epoch {epoch:3d}  train: {trained}")
        if len(validation_rows):
            log(f"           valid: {held_out}   ({time.time() - started:.1f}s)")
    return history


def main(argv: Optional[Sequence[str]] = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("records", nargs="+", help="self-play files from `environment selfplay`")
    parser.add_argument("--out", required=True, help="where to write the trained weights")
    parser.add_argument("--init", help="weights file to continue training from")
    parser.add_argument("--hidden", type=int, nargs="+", default=[256, 256],
                        help="hidden layer widths (ignored with --init)")
    parser.add_argument("--epochs", type=int, default=10)
    parser.add_argument("--batch", type=int, default=256)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--weight-decay", type=float, default=1e-4)
    parser.add_argument("--value-weight", type=float, default=1.0,
                        help="weight of the value loss beside the policy loss")
    parser.add_argument("--validation-every", type=int, default=10,
                        help="hold out every Nth game for validation; 0 for none")
    parser.add_argument("--seed", type=int, default=0)
    args = parser.parse_args(argv)

    torch.manual_seed(args.seed)
    device = "cuda" if torch.cuda.is_available() else "cpu"

    records = load_records(args.records)
    train_rows, validation_rows = split_by_game(records, args.validation_every)
    games = len(np.unique(records.game))
    print(f"{len(records)} records from {games} games: "
          f"{len(train_rows)} to train on, {len(validation_rows)} held out")

    if args.init:
        net = load_network(args.init)
        if (net.obs_size, net.action_size) != (records.obs_size, records.action_size):
            raise SystemExit(f"{args.init} is for other observation/action sizes than the records")
    else:
        net = PolicyValueNet(records.obs_size, records.action_size, args.hidden)
    print(f"network: {records.obs_size} -> {' -> '.join(map(str, net.hidden))} -> "
          f"{records.action_size} + 1   ({sum(p.numel() for p in net.parameters())} weights, {device})")

    report_rows = validation_rows if len(validation_rows) else train_rows
    print(f"untrained baseline: {baselines(records, report_rows)}")
    print(f"lowest possible policy loss: {target_entropy(records, report_rows):.4f}")

    train(net, records, train_rows, validation_rows, epochs=args.epochs, batch_size=args.batch,
          lr=args.lr, weight_decay=args.weight_decay, value_weight=args.value_weight,
          seed=args.seed, device=device)

    save_network(net.cpu(), args.out)
    print(f"weights written to {args.out}")


if __name__ == "__main__":
    main()

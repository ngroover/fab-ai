"""
self_play_trainer.py — AlphaZero-style self-play trainer for FaBEnv.

A single iteration is:

    1. SELF-PLAY: play `games_per_iter` games. At every decision point use
       `PUCTSearch` (alpha_zero_mcts) seeded by the current PolicyValueNetwork
       to produce a visit distribution π over legal actions. Sample the chosen
       action from π with a temperature schedule. Append the transition
       (obs, legal_actions, π, to_play) to a replay buffer.

    2. TRAIN: run `steps_per_iter` gradient steps. Each step samples a batch
       from the buffer and minimizes
            L = cross_entropy(softmax(logits), π)  +  c_value · MSE(value, z)
       where z is the game outcome from each transition's `to_play` view.

    3. EVAL (every `eval_every` iters): play `eval_games` games against each
       fixed opponent (RandomAgent) with greedy
       net inference (no MCTS) and record win rates.

    4. CHECKPOINT: write `state_dict` to `./checkpoints/<name>.pt` and update
       `./checkpoints/index.json`.

The class accepts a `callbacks` dict so the web UI can render live metrics
without coupling the trainer to Flask.

CLI usage:
    python self_play_trainer.py --iters 1 --games 2 --steps 4 --sims 8
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import random
import time
from collections import deque
from dataclasses import asdict, dataclass, field
from datetime import datetime
from typing import Callable, Dict, List, Optional, Sequence, Tuple

import torch
import torch.nn.functional as F

from actions import Action
from agents import RandomAgent
from cards import build_dorinthea_deck, build_rhinar_deck
from fab_env import FaBEnv
from neural_agent import (
    NeuralAgent,
    PolicyValueNetwork,
    flatten_obs,
    stack_action_features,
)
from alpha_zero_mcts import PUCTSearch, sample_action_index


# ─────────────────────────────────────────────────────────────────────────────
# Deck pool
# ─────────────────────────────────────────────────────────────────────────────

# Map of pool name → zero-arg builder. Each game samples one entry per side
# independently, so mirror matchups (e.g. rhinar vs rhinar) can occur.
DECK_BUILDERS: Dict[str, Callable[[], list]] = {
    "rhinar": build_rhinar_deck,
    "dorinthea": build_dorinthea_deck,
}


# ─────────────────────────────────────────────────────────────────────────────
# Config
# ─────────────────────────────────────────────────────────────────────────────

CHECKPOINT_DIR = os.path.join(os.path.dirname(__file__), "checkpoints")
INDEX_PATH = os.path.join(CHECKPOINT_DIR, "index.json")


@dataclass
class TrainerConfig:
    # Loop sizes
    games_per_iter: int = 8
    steps_per_iter: int = 32
    total_iters: int = 10            # 0 → run until stopped

    # MCTS
    n_simulations: int = 32
    c_puct: float = 1.5
    determinize: bool = True
    dirichlet_alpha: float = 0.3
    dirichlet_frac: float = 0.25

    # Temperature schedule (greedy after `temp_drop_step` decisions per game)
    temp_start: float = 1.0
    temp_end: float = 0.1
    temp_drop_step: int = 12

    # PyTorch optimization
    lr: float = 3e-4
    batch_size: int = 64
    weight_decay: float = 1e-4
    c_value: float = 0.5
    grad_clip: float = 5.0
    buffer_size: int = 20_000

    # Evaluation
    eval_games: int = 4
    eval_every: int = 1              # evaluate after every N iterations
    eval_opponents: Tuple[str, ...] = ("random",)
    # When True, also play eval_games per (net_deck, opp_deck) pair against
    # every saved checkpoint and record the matchup win rates. This populates
    # the "Matchups" tab in the web viewer.
    eval_vs_checkpoints: bool = False

    # Bookkeeping
    opponent_pool: Tuple[str, ...] = ("self",)
    # Pool of decks to draw from per side, per game. Each agent's deck is
    # sampled independently, so e.g. rhinar-vs-rhinar mirrors can occur.
    # Names must be keys in DECK_BUILDERS.
    deck_pool: Tuple[str, ...] = ("rhinar", "dorinthea")
    run_name: str = ""               # auto-set if blank

    # Distributed self-play (workers stream transitions; coordinator trains).
    # When False the standard single-process SelfPlayTrainer is used.
    distributed: bool = False
    dist_bind_host: str = "0.0.0.0"
    dist_pull_port: int = 5556
    dist_pub_port: int = 5557
    dist_rep_port: int = 5558
    dist_broadcast_secs: float = 5.0
    dist_max_staleness: int = 10
    dist_min_buffer: int = 64
    base_checkpoint: Optional[str] = None  # name in ./checkpoints/, or None
    seed: Optional[int] = None


# ─────────────────────────────────────────────────────────────────────────────
# Replay buffer
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class Transition:
    """A decision sample ready for gradient updates.

    Stores already-flattened features (`obs_vec`, `action_feats`) so the
    training loop never has to re-walk `Action`/`Card` objects. This makes
    transitions safely transportable across machines (no lambdas in the
    graph) and avoids re-running `flatten_obs`/`stack_action_features` on
    every grad step.
    """

    obs_vec: List[float]              # length OBS_FLAT_SIZE
    action_feats: List[List[float]]   # (N_legal, ACTION_FEAT_SIZE)
    pi: List[float]                   # MCTS visit distribution (length N_legal)
    to_play: int                      # 0 or 1
    z: float = 0.0                    # game outcome (filled after game ends)
    weight_version: int = 0           # net version that produced this sample


class ReplayBuffer:
    """Bounded ring buffer of completed transitions."""

    def __init__(self, capacity: int):
        self._buf: deque[Transition] = deque(maxlen=capacity)

    def __len__(self) -> int:
        return len(self._buf)

    def push_many(self, items: Sequence[Transition]) -> None:
        for t in items:
            self._buf.append(t)

    def sample(self, n: int, rng: random.Random) -> List[Transition]:
        n = min(n, len(self._buf))
        if n <= 0:
            return []
        return rng.sample(list(self._buf), n)


# ─────────────────────────────────────────────────────────────────────────────
# Agent action dispatch
# ─────────────────────────────────────────────────────────────────────────────

def _dispatch_agent_action(env: FaBEnv, agent, obs: dict) -> Action:
    """Ask `agent` for an action in the env's current phase."""
    agent_id = env.agent_selection
    player_idx = int(agent_id[-1])
    player = env._game.players[player_idx]
    opponent = env._game.players[1 - player_idx]
    legal = env.legal_actions()
    agent_obs = obs.get(agent_id) if isinstance(obs, dict) else None
    return agent.select_action(
        agent_obs, legal, player, opponent, env.build_action_context()
    )


# ─────────────────────────────────────────────────────────────────────────────
# Module-level helpers (also used by the distributed worker)
# ─────────────────────────────────────────────────────────────────────────────

def temperature_for(cfg: "TrainerConfig", decision_idx: int) -> float:
    if decision_idx < cfg.temp_drop_step:
        return cfg.temp_start
    return cfg.temp_end


def sample_deck_pair(cfg: "TrainerConfig", rng: random.Random) -> Tuple[str, str]:
    """Pick a deck name for each side independently from `cfg.deck_pool`."""
    pool = [d for d in (cfg.deck_pool or ()) if d in DECK_BUILDERS]
    if not pool:
        pool = list(DECK_BUILDERS.keys())
    return rng.choice(pool), rng.choice(pool)


def make_opponent(
    kind: str,
    rng: random.Random,
    opponent_idx: Optional[int] = None,
):
    if kind == "self":
        return None  # PUCTSearch handles both sides
    if kind == "random":
        return RandomAgent(seed=rng.randrange(2**31))
    if kind == "mcts":
        from mcts_agent import MCTSAgent
        # MCTSAgent's player_idx must match the seat it will play. Caller
        # passes `opponent_idx`; default to seat 1 if it wasn't threaded.
        return MCTSAgent(
            player_idx=opponent_idx if opponent_idx is not None else 1,
            n_simulations=200,
            seed=rng.randrange(2**31),
        )
    if kind == "past":
        index = _load_index()
        names = [c["name"] for c in index.get("checkpoints", [])]
        if names:
            ckpt_name = rng.choice(names)
            opp_net = _load_network_from_checkpoint(ckpt_name)
            if opp_net is not None:
                return NeuralAgent(model=opp_net)
        # No checkpoints available — fall through to RandomAgent so the
        # worker still produces useful samples instead of crashing.
        print(
            f"[make_opponent] 'past' requested but no checkpoints available; "
            f"using RandomAgent",
            flush=True,
        )
        return RandomAgent(seed=rng.randrange(2**31))
    # Unknown kind: never return None — callers will try `agent.select_action`
    # and hit AttributeError. Fall back to RandomAgent and log so config
    # mistakes are noisy.
    print(
        f"[make_opponent] unsupported opponent kind {kind!r}; "
        f"falling back to RandomAgent",
        flush=True,
    )
    return RandomAgent(seed=rng.randrange(2**31))


def play_one_self_play_game(
    net: PolicyValueNetwork,
    cfg: "TrainerConfig",
    rng: random.Random,
    opp_kind: str,
    deck0_name: str,
    deck1_name: str,
    weight_version: int = 0,
    should_stop: Optional[Callable[[], bool]] = None,
) -> Tuple[List[Transition], float, int, float]:
    """Play one self-play game and return (transitions, p0_outcome, length, avg_pred_v_p0).

    Pure function — does NOT touch any `SelfPlayTrainer` state. The worker
    process and the single-machine trainer both call this.
    """
    env = FaBEnv(verbose=False)
    env.reset(
        _build_deck(deck0_name),
        _build_deck(deck1_name),
        seed=rng.randrange(2**31),
    )

    # If the opponent is fixed, randomize side per game so the net learns both heroes.
    learning_idx = rng.choice((0, 1)) if opp_kind != "self" else None
    opponent_idx = 1 - learning_idx if learning_idx is not None else None
    opp_agent = make_opponent(opp_kind, rng, opponent_idx=opponent_idx)
    if opp_agent is not None and hasattr(opp_agent, "set_env"):
        opp_agent.set_env(env)

    search = PUCTSearch(
        net,
        n_simulations=cfg.n_simulations,
        c_puct=cfg.c_puct,
        determinize=cfg.determinize,
        dirichlet_alpha=cfg.dirichlet_alpha,
        dirichlet_frac=cfg.dirichlet_frac,
        seed=rng.randrange(2**31),
    )

    _should_stop = should_stop or (lambda: False)
    decisions: List[Transition] = []
    per_side = [0, 0]
    net_root_values: List[float] = []
    length = 0

    while not env.done:
        length += 1
        if _should_stop():
            break
        agent_id = env.agent_selection
        to_play = int(agent_id[-1])

        legal = env.legal_actions()
        if not legal:
            break

        if len(legal) == 1:
            env.step(legal[0])
            continue

        mcts_owns_this_decision = (
            opp_kind == "self" or to_play == learning_idx
        )

        if mcts_owns_this_decision:
            obs_dict = env._get_obs()[agent_id]
            root_legal, pi, root_q = search.run(env)
            if not root_legal:
                break
            net_root_values.append(root_q if to_play == 0 else -root_q)
            tau = temperature_for(cfg, per_side[to_play])
            idx = sample_action_index(pi, tau, rng)
            action = root_legal[idx]
            # Pre-flatten so the transition no longer carries Action/Card
            # references — safe to ship over the wire and faster to train on.
            obs_vec = flatten_obs(obs_dict).tolist()
            action_feats = stack_action_features(root_legal).tolist()
            decisions.append(Transition(
                obs_vec=obs_vec,
                action_feats=action_feats,
                pi=list(pi),
                to_play=to_play,
                weight_version=weight_version,
            ))
            per_side[to_play] += 1
            env.step(action)
        else:
            obs_full = env._get_obs()
            action = _dispatch_agent_action(env, opp_agent, obs_full)
            env.step(action)

    r0 = env._rewards.get("agent_0", 0.0)
    if r0 > 0.5:
        z0, z1 = 1.0, -1.0
    elif r0 < -0.5:
        z0, z1 = -1.0, 1.0
    else:
        z0, z1 = 0.0, 0.0

    for t in decisions:
        t.z = z0 if t.to_play == 0 else z1

    avg_root_v = (sum(net_root_values) / len(net_root_values)
                  if net_root_values else 0.0)
    return decisions, z0, length, avg_root_v


# ─────────────────────────────────────────────────────────────────────────────
# Trainer
# ─────────────────────────────────────────────────────────────────────────────

class SelfPlayTrainer:
    """AlphaZero loop wrapped in a single class.

    The web viewer drives this through `callbacks` (`on_log`, `on_metrics`,
    `on_status`, `on_checkpoint`, `should_stop`, `wait_if_paused`); the
    standalone CLI uses sensible defaults.
    """

    def __init__(
        self,
        config: TrainerConfig,
        callbacks: Optional[Dict[str, Callable]] = None,
    ) -> None:
        self.config = config
        callbacks = callbacks or {}
        self._on_log: Callable[[str], None] = callbacks.get(
            "on_log", lambda msg: print(msg))
        self._on_metrics: Callable[[dict], None] = callbacks.get(
            "on_metrics", lambda m: None)
        self._on_status: Callable[[str], None] = callbacks.get(
            "on_status", lambda s: None)
        self._on_checkpoint: Callable[[dict], None] = callbacks.get(
            "on_checkpoint", lambda c: None)
        self._on_progress: Callable[[dict], None] = callbacks.get(
            "on_progress", lambda p: None)
        self._should_stop: Callable[[], bool] = callbacks.get(
            "should_stop", lambda: False)
        self._wait_if_paused: Callable[[], None] = callbacks.get(
            "wait_if_paused", lambda: None)

        seed = config.seed if config.seed is not None else random.randrange(2**31)
        self._rng = random.Random(seed)
        torch.manual_seed(seed)

        self.net = PolicyValueNetwork()
        if config.base_checkpoint:
            self._load_checkpoint(config.base_checkpoint)
        self.optimizer = torch.optim.Adam(
            self.net.parameters(),
            lr=config.lr,
            weight_decay=config.weight_decay,
        )
        self.buffer = ReplayBuffer(config.buffer_size)

        self._iter = 0
        self._games_done = 0
        self._grad_steps = 0
        self._start_time = time.time()

        if not config.run_name:
            ts = datetime.now().strftime("%Y%m%d-%H%M%S")
            self.config.run_name = f"run-{ts}"

    # ──────────────────────────────────────────────────────────
    # Main entry point
    # ──────────────────────────────────────────────────────────

    def run(self) -> None:
        self._start_time = time.time()
        self._log(f"  ▶  Trainer starting: run={self.config.run_name} "
                  f"seed={self.config.seed}")
        os.makedirs(CHECKPOINT_DIR, exist_ok=True)

        try:
            while True:
                if self._should_stop():
                    self._log("  ⏹  Stop requested — exiting before iteration.")
                    break
                if self.config.total_iters > 0 and self._iter >= self.config.total_iters:
                    self._log(f"  ✓  Reached total_iters={self.config.total_iters}.")
                    break

                self._iter += 1
                self._log(f"\n══ Iteration {self._iter} ══")

                # ── 1. Self-play ──────────────────────────────
                self._on_status("self-play")
                ep_lens, value_pred_avg, value_real_avg = \
                    self._run_self_play_phase()
                if self._should_stop():
                    break

                # ── 2. Training ────────────────────────────────
                self._on_status("training")
                ploss, vloss = self._run_training_phase()
                if self._should_stop():
                    break

                # ── 3. Eval ────────────────────────────────────
                eval_wr: Dict[str, float] = {}
                matchups: Dict[str, Dict[str, float]] = {}
                if (self.config.eval_every > 0
                        and self._iter % self.config.eval_every == 0):
                    self._on_status("evaluating")
                    eval_wr = self._run_eval_phase()
                    if self._should_stop():
                        break
                    matchups = self._run_checkpoint_matchups()
                    if self._should_stop():
                        break

                # ── 4. Checkpoint ──────────────────────────────
                ckpt = self._save_checkpoint(self._iter, eval_wr, matchups)

                # ── 5. Publish metrics ─────────────────────────
                self._on_metrics({
                    "iter": self._iter,
                    "games": self._games_done,
                    "grad_steps": self._grad_steps,
                    "policy_loss": ploss,
                    "value_loss": vloss,
                    "wr_random": eval_wr.get("random"),
                    "mean_len": (sum(ep_lens) / len(ep_lens)) if ep_lens else 0.0,
                    "value_pred": value_pred_avg,
                    "value_real": value_real_avg,
                    "checkpoint": ckpt["name"] if ckpt else None,
                })
                self._wait_if_paused()
        except Exception as exc:  # surface errors to the UI
            import traceback
            self._on_status("error")
            self._log(f"  ⚠  Trainer error: {exc}")
            self._log(traceback.format_exc())
            raise
        finally:
            self._on_status("idle")

    # ──────────────────────────────────────────────────────────
    # Self-play phase
    # ──────────────────────────────────────────────────────────

    def _run_self_play_phase(self) -> Tuple[List[int], float, float]:
        ep_lens: List[int] = []
        val_preds: List[float] = []
        val_reals: List[float] = []
        for game_i in range(self.config.games_per_iter):
            if self._should_stop():
                break
            self._wait_if_paused()
            opp_kind = self._rng.choice(self.config.opponent_pool or ("self",))
            deck0_name, deck1_name = self._sample_deck_pair()
            transitions, outcome, length, pred_avg = \
                self._play_one_self_play_game(opp_kind, deck0_name, deck1_name)
            ep_lens.append(length)
            val_preds.append(pred_avg)
            val_reals.append(outcome)
            self.buffer.push_many(transitions)
            self._games_done += 1
            self._on_progress({"games": self._games_done})
            self._log(
                f"  ⚔  game {game_i+1}/{self.config.games_per_iter} "
                f"(opp={opp_kind}, decks={deck0_name} vs {deck1_name}) "
                f"length={length} "
                f"outcome(p0)={outcome:+.0f} "
                f"v̂(p0)={pred_avg:+.3f} "
                f"buffer={len(self.buffer)}"
            )
        avg_pred = sum(val_preds) / len(val_preds) if val_preds else 0.0
        avg_real = sum(val_reals) / len(val_reals) if val_reals else 0.0
        return ep_lens, avg_pred, avg_real

    def _play_one_self_play_game(
        self, opp_kind: str, deck0_name: str, deck1_name: str,
    ) -> Tuple[List[Transition], float, int, float]:
        return play_one_self_play_game(
            self.net, self.config, self._rng,
            opp_kind, deck0_name, deck1_name,
            weight_version=0,
            should_stop=self._should_stop,
        )

    def _temperature_for(self, decision_idx: int) -> float:
        return temperature_for(self.config, decision_idx)

    def _sample_deck_pair(self) -> Tuple[str, str]:
        return sample_deck_pair(self.config, self._rng)

    def _make_opponent(self, kind: str):
        return make_opponent(kind, self._rng)

    # ──────────────────────────────────────────────────────────
    # Training phase
    # ──────────────────────────────────────────────────────────

    def _run_training_phase(self) -> Tuple[float, float]:
        """Run `steps_per_iter` grad steps and return (avg_p_loss, avg_v_loss)."""
        if len(self.buffer) == 0:
            self._log("  ⚠  Skipping training: buffer is empty.")
            return 0.0, 0.0

        self.net.train()
        total_p = 0.0
        total_v = 0.0
        steps = 0
        for step in range(self.config.steps_per_iter):
            if self._should_stop():
                break
            self._wait_if_paused()
            batch = self.buffer.sample(self.config.batch_size, self._rng)
            if not batch:
                break
            p_loss, v_loss = self._train_step(batch)
            total_p += p_loss
            total_v += v_loss
            steps += 1
            self._grad_steps += 1
            self._on_progress({"grad_steps": self._grad_steps})
        self.net.eval()

        avg_p = total_p / steps if steps else 0.0
        avg_v = total_v / steps if steps else 0.0
        self._log(
            f"  ∇  {steps} grad steps · "
            f"policy_loss={avg_p:.4f} value_loss={avg_v:.4f}"
        )
        return avg_p, avg_v

    def _train_step(self, batch: List[Transition]) -> Tuple[float, float]:
        """One gradient step over a list of transitions.

        Because legal-action list length varies per state we accumulate the
        per-example losses one at a time then divide by batch size. This is
        slower than a fully batched forward but keeps the code simple and
        correct.
        """
        self.optimizer.zero_grad(set_to_none=True)

        policy_loss = torch.zeros((), dtype=torch.float32)
        value_loss = torch.zeros((), dtype=torch.float32)

        for t in batch:
            obs_vec = torch.tensor(t.obs_vec, dtype=torch.float32)
            action_feats = torch.tensor(t.action_feats, dtype=torch.float32)
            logits, value = self.net.forward(obs_vec, action_feats)
            log_probs = F.log_softmax(logits, dim=-1)
            pi = torch.tensor(t.pi, dtype=torch.float32)
            policy_loss = policy_loss + (-(pi * log_probs).sum())
            z = torch.tensor(t.z, dtype=torch.float32)
            value_loss = value_loss + (value - z) ** 2

        n = float(len(batch))
        policy_loss = policy_loss / n
        value_loss = value_loss / n
        total = policy_loss + self.config.c_value * value_loss
        total.backward()
        torch.nn.utils.clip_grad_norm_(
            self.net.parameters(), max_norm=self.config.grad_clip
        )
        self.optimizer.step()
        return float(policy_loss.item()), float(value_loss.item())

    # ──────────────────────────────────────────────────────────
    # Eval phase
    # ──────────────────────────────────────────────────────────

    def _run_eval_phase(self) -> Dict[str, float]:
        """Play eval_games per (net_deck, opp_deck) pair for each eval opponent.

        Returns a flat dict with keys:
          - "{opp_type}" → overall win rate
          - "{opp_type}:{net_deck}_vs_{opp_deck}" → per-matchup win rate
        where net_deck is the deck the neural net played and opp_deck is the
        deck the opponent played.
        """
        results: Dict[str, float] = {}
        self.net.eval()
        pool = [d for d in (self.config.deck_pool or ()) if d in DECK_BUILDERS]
        if not pool:
            pool = list(DECK_BUILDERS.keys())

        for opp_name in self.config.eval_opponents:
            if self._should_stop():
                break
            all_wins = 0
            all_total = 0
            for net_deck in pool:
                for opp_deck in pool:
                    wins = 0
                    total = 0
                    for _ in range(self.config.eval_games):
                        if self._should_stop():
                            break
                        # Randomly assign net to a seat so first-player advantage
                        # is averaged out, but always use the intended decks.
                        net_seat = self._rng.choice((0, 1))
                        deck0 = net_deck if net_seat == 0 else opp_deck
                        deck1 = opp_deck if net_seat == 0 else net_deck
                        outcome = self._play_one_eval_game(opp_name, net_seat, deck0, deck1)
                        total += 1
                        all_total += 1
                        if outcome > 0:
                            wins += 1
                            all_wins += 1
                    if total:
                        wr = wins / total
                        results[f"{opp_name}:{net_deck}_vs_{opp_deck}"] = wr
                        self._log(
                            f"    as {net_deck} / opp {opp_deck} vs {opp_name}: "
                            f"{wins}/{total} = {wr*100:.0f}%"
                        )
            if all_total:
                overall = all_wins / all_total
                results[opp_name] = overall
                self._log(f"  🎯  vs {opp_name} overall: {all_wins}/{all_total} = {overall*100:.0f}%")

        return results

    def _play_one_eval_game(
        self, opp_name: str, net_seat: int,
        deck0_name: str, deck1_name: str,
        opp_agent_override=None,
    ) -> float:
        """Return +1/0/-1 from the *network's* perspective.

        If `opp_agent_override` is provided it is used directly; otherwise the
        opponent is built from `opp_name` via `_make_opponent`.
        """
        env = FaBEnv(verbose=False)
        env.reset(
            _build_deck(deck0_name),
            _build_deck(deck1_name),
            seed=self._rng.randrange(2**31),
        )
        net_agent = NeuralAgent(model=self.net)
        if opp_agent_override is not None:
            opp_agent = opp_agent_override
        else:
            opponent_idx = 1 - net_seat
            opp_agent = (
                make_opponent(opp_name, self._rng, opponent_idx=opponent_idx)
                or RandomAgent()
            )
        if hasattr(opp_agent, "set_env"):
            opp_agent.set_env(env)

        while not env.done:
            if self._should_stop():
                return 0.0
            agent_id = env.agent_selection
            to_play = int(agent_id[-1])
            legal = env.legal_actions()
            if not legal:
                break
            if len(legal) == 1:
                env.step(legal[0])
                continue
            obs_full = env._get_obs()
            agent = net_agent if to_play == net_seat else opp_agent
            action = _dispatch_agent_action(env, agent, obs_full)
            env.step(action)

        r = env._rewards.get(f"agent_{net_seat}", 0.0)
        if r > 0.5:
            return 1.0
        if r < -0.5:
            return -1.0
        return 0.0

    def _run_checkpoint_matchups(self) -> Dict[str, Dict[str, float]]:
        """Play eval_games per (net_deck, opp_deck) pair against every saved
        checkpoint. Returns a nested dict:

            {opp_ckpt_name: {f"{net_deck}_vs_{opp_deck}": win_rate}}

        Only runs when `config.eval_vs_checkpoints` is True. Skips checkpoints
        whose `.pt` file is missing.
        """
        out: Dict[str, Dict[str, float]] = {}
        if not self.config.eval_vs_checkpoints:
            return out

        pool = [d for d in (self.config.deck_pool or ()) if d in DECK_BUILDERS]
        if not pool:
            pool = list(DECK_BUILDERS.keys())

        index = _load_index()
        names = [c["name"] for c in index.get("checkpoints", [])]
        if not names:
            return out

        self.net.eval()
        for ckpt_name in names:
            if self._should_stop():
                break
            opp_net = _load_network_from_checkpoint(ckpt_name)
            if opp_net is None:
                self._log(f"    skip matchup vs {ckpt_name}: weights missing")
                continue
            opp_agent = NeuralAgent(model=opp_net)
            pair_results: Dict[str, float] = {}
            for net_deck in pool:
                for opp_deck in pool:
                    wins = 0
                    total = 0
                    for _ in range(self.config.eval_games):
                        if self._should_stop():
                            break
                        net_seat = self._rng.choice((0, 1))
                        deck0 = net_deck if net_seat == 0 else opp_deck
                        deck1 = opp_deck if net_seat == 0 else net_deck
                        outcome = self._play_one_eval_game(
                            "", net_seat, deck0, deck1,
                            opp_agent_override=opp_agent,
                        )
                        total += 1
                        if outcome > 0:
                            wins += 1
                    if total:
                        pair_results[f"{net_deck}_vs_{opp_deck}"] = wins / total
            if pair_results:
                out[ckpt_name] = pair_results
                summary = ", ".join(
                    f"{k}={v*100:.0f}%" for k, v in pair_results.items()
                )
                self._log(f"  ⚔  matchup vs {ckpt_name}: {summary}")

        return out

    # ──────────────────────────────────────────────────────────
    # Checkpoint I/O
    # ──────────────────────────────────────────────────────────

    def _save_checkpoint(
        self, iter_: int, eval_wr: Dict[str, float],
        matchups: Optional[Dict[str, Dict[str, float]]] = None,
    ) -> Optional[dict]:
        os.makedirs(CHECKPOINT_DIR, exist_ok=True)
        name = f"{self.config.run_name}-iter-{iter_:04d}"
        path = os.path.join(CHECKPOINT_DIR, f"{name}.pt")
        state_dict = self.net.state_dict()
        torch.save({
            "state_dict": state_dict,
            "iter": iter_,
            "config": asdict(self.config),
        }, path)
        size = os.path.getsize(path)
        weight_hash = _hash_state_dict(state_dict)
        meta = {
            "name": name,
            "iter": iter_,
            "created": datetime.now().isoformat(timespec="seconds"),
            "size_bytes": size,
            "metrics": dict(eval_wr),
            "matchups": {k: dict(v) for k, v in (matchups or {}).items()},
            "run_name": self.config.run_name,
            "hash": weight_hash,
        }
        self._append_to_index(meta)
        self._log(
            f"  💾  saved checkpoint {name} ({size/1024:.1f} KB)"
        )
        self._on_checkpoint(meta)
        return meta

    def _append_to_index(self, meta: dict) -> None:
        idx = _load_index()
        idx["checkpoints"].append(meta)
        with open(INDEX_PATH, "w") as f:
            json.dump(idx, f, indent=2)

    def _load_checkpoint(self, name: str) -> None:
        path = os.path.join(CHECKPOINT_DIR, f"{name}.pt")
        if not os.path.isfile(path):
            self._log(f"  ⚠  base_checkpoint {name!r} not found at {path}")
            return
        data = torch.load(path, map_location="cpu", weights_only=False)
        self.net.load_state_dict(data["state_dict"])
        self._log(f"  📂  loaded base checkpoint {name}")

    # ──────────────────────────────────────────────────────────
    # Small helpers
    # ──────────────────────────────────────────────────────────

    def _log(self, msg: str) -> None:
        self._on_log(msg)


# ─────────────────────────────────────────────────────────────────────────────
# Deck builder dispatch
# ─────────────────────────────────────────────────────────────────────────────

def _build_deck(name: str) -> list:
    builder = DECK_BUILDERS.get(name)
    if builder is None:
        raise KeyError(
            f"Unknown deck {name!r}; known: {sorted(DECK_BUILDERS)}"
        )
    return builder()


# ─────────────────────────────────────────────────────────────────────────────
# Load a checkpoint's weights into a fresh PolicyValueNetwork
# ─────────────────────────────────────────────────────────────────────────────

# Optional hook for distributed workers: a callable that ensures a checkpoint
# file exists locally, downloading it from the coordinator if missing. Single-
# machine training leaves this as None, so behavior is unchanged there.
_checkpoint_provider: Optional[Callable[[str], bool]] = None


def set_checkpoint_provider(fn: Optional[Callable[[str], bool]]) -> None:
    """Register (or clear) the on-demand checkpoint fetcher used by 'past'."""
    global _checkpoint_provider
    _checkpoint_provider = fn


def _load_network_from_checkpoint(name: str) -> Optional[PolicyValueNetwork]:
    path = os.path.join(CHECKPOINT_DIR, f"{name}.pt")
    if not os.path.isfile(path) and _checkpoint_provider is not None:
        try:
            _checkpoint_provider(name)
        except Exception as exc:
            print(f"[checkpoint] provider failed for {name!r}: {exc!r}", flush=True)
    if not os.path.isfile(path):
        return None
    try:
        data = torch.load(path, map_location="cpu", weights_only=False)
    except Exception as exc:
        # A truncated/corrupt download must not poison the cache: drop it so a
        # later game re-fetches instead of failing forever.
        print(f"[checkpoint] failed to load {name!r}: {exc!r}", flush=True)
        try:
            os.remove(path)
        except OSError:
            pass
        return None
    net = PolicyValueNetwork()
    net.load_state_dict(data["state_dict"])
    net.eval()
    return net


# ─────────────────────────────────────────────────────────────────────────────
# Weight-hash helper (short content hash of the model parameters)
# ─────────────────────────────────────────────────────────────────────────────

def _hash_state_dict(state_dict: Dict[str, torch.Tensor]) -> str:
    h = hashlib.sha1()
    for k in sorted(state_dict.keys()):
        h.update(k.encode("utf-8"))
        h.update(state_dict[k].detach().cpu().contiguous().numpy().tobytes())
    return h.hexdigest()[:12]


# ─────────────────────────────────────────────────────────────────────────────
# Index file helpers (used by web_viewer to render the Models tab)
# ─────────────────────────────────────────────────────────────────────────────

def _load_index() -> dict:
    if os.path.isfile(INDEX_PATH):
        try:
            with open(INDEX_PATH) as f:
                data = json.load(f)
            if isinstance(data, dict) and "checkpoints" in data:
                return data
        except (OSError, json.JSONDecodeError):
            pass
    return {"checkpoints": []}


def list_checkpoints() -> List[dict]:
    return _load_index()["checkpoints"]


def delete_checkpoint(name: str) -> bool:
    idx = _load_index()
    new = [c for c in idx["checkpoints"] if c["name"] != name]
    if len(new) == len(idx["checkpoints"]):
        return False
    idx["checkpoints"] = new
    with open(INDEX_PATH, "w") as f:
        json.dump(idx, f, indent=2)
    path = os.path.join(CHECKPOINT_DIR, f"{name}.pt")
    if os.path.isfile(path):
        os.remove(path)
    return True


def rename_checkpoint(old: str, new: str) -> bool:
    if not new or "/" in new or "\\" in new:
        return False
    idx = _load_index()
    found = False
    for c in idx["checkpoints"]:
        if c["name"] == old:
            c["name"] = new
            found = True
            break
    if not found:
        return False
    old_path = os.path.join(CHECKPOINT_DIR, f"{old}.pt")
    new_path = os.path.join(CHECKPOINT_DIR, f"{new}.pt")
    if os.path.isfile(old_path):
        os.rename(old_path, new_path)
    with open(INDEX_PATH, "w") as f:
        json.dump(idx, f, indent=2)
    return True


# ─────────────────────────────────────────────────────────────────────────────
# CLI
# ─────────────────────────────────────────────────────────────────────────────

def _build_cli() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="AlphaZero-style self-play trainer for FaBEnv"
    )
    p.add_argument("--iters", type=int, default=1, help="Total iterations (0 = until stopped)")
    p.add_argument("--games", type=int, default=2, help="Self-play games per iteration")
    p.add_argument("--steps", type=int, default=4, help="Gradient steps per iteration")
    p.add_argument("--sims", type=int, default=8, help="MCTS simulations per decision")
    p.add_argument("--batch", type=int, default=32, help="Batch size for grad steps")
    p.add_argument("--lr", type=float, default=3e-4, help="Adam learning rate")
    p.add_argument("--eval-games", type=int, default=2, help="Eval games per opponent")
    p.add_argument("--eval-every", type=int, default=1, help="Eval every N iterations")
    p.add_argument("--seed", type=int, default=None, help="Random seed")
    p.add_argument("--run-name", type=str, default="", help="Checkpoint prefix")
    p.add_argument("--base", type=str, default=None,
                   help="Base checkpoint name (under ./checkpoints/)")
    p.add_argument("--no-pimc", action="store_true",
                   help="Disable PIMC determinization")
    p.add_argument("--eval-vs-checkpoints", action="store_true",
                   help="Also play eval games against every saved checkpoint "
                        "and record matchup win rates per deck pair.")
    p.add_argument(
        "--deck-pool", type=str, default="",
        help=(
            "Comma-separated deck pool for both sides "
            f"(choices: {','.join(sorted(DECK_BUILDERS))}). "
            "Each side's deck is sampled independently per game."
        ),
    )
    return p


def main() -> None:
    args = _build_cli().parse_args()
    deck_pool: Tuple[str, ...] = TrainerConfig.deck_pool
    if args.deck_pool:
        names = [n.strip() for n in args.deck_pool.split(",") if n.strip()]
        unknown = [n for n in names if n not in DECK_BUILDERS]
        if unknown:
            raise SystemExit(
                f"Unknown deck(s) in --deck-pool: {unknown}; "
                f"choices: {sorted(DECK_BUILDERS)}"
            )
        if names:
            deck_pool = tuple(names)
    cfg = TrainerConfig(
        games_per_iter=args.games,
        steps_per_iter=args.steps,
        total_iters=args.iters,
        n_simulations=args.sims,
        batch_size=args.batch,
        lr=args.lr,
        eval_games=args.eval_games,
        eval_every=args.eval_every,
        determinize=not args.no_pimc,
        seed=args.seed,
        run_name=args.run_name,
        base_checkpoint=args.base,
        deck_pool=deck_pool,
        eval_vs_checkpoints=args.eval_vs_checkpoints,
    )
    SelfPlayTrainer(cfg).run()


if __name__ == "__main__":
    main()

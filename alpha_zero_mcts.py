"""
alpha_zero_mcts.py — PUCT-based MCTS for AlphaZero-style self-play training.

Differences from the plain `mcts_agent.MCTSAgent`:

- Leaf evaluation uses the policy/value network from `neural_agent` instead of
  random rollouts. The value head bootstraps the leaf value; the policy head
  provides the per-action priors P(a|s) used by the PUCT selection rule.
- After search, `PUCTSearch.run()` returns the visit-count distribution at the
  root (the AlphaZero policy training target π).
- Backup follows the minimax convention: the leaf value is negated as we
  ascend across "to-play" boundaries.
- Hidden information is handled the same way as `mcts_agent`: PIMC
  determinization of the opponent's hand+deck at the start of every
  simulation. `_determinize` is reused directly.
"""

from __future__ import annotations

import copy
import math
import random
from typing import Dict, List, Optional, Tuple

import torch

from actions import Action
from fab_env import FaBEnv
from mcts_agent import _determinize
from neural_agent import (
    PolicyValueNetwork,
    encode_action_features,
    flatten_obs,
)


# ─────────────────────────────────────────────────────────────────────────────
# Node
# ─────────────────────────────────────────────────────────────────────────────

class AZNode:
    """One node in the PUCT search tree.

    A node represents the post-step state after the parent's chosen action.
    `to_play` is the player who must choose next from this state (0 or 1).
    `legal_actions` and `priors` are aligned by index and are filled in at
    expansion time (when the network first evaluates this state).
    """

    __slots__ = (
        "parent", "action_key", "to_play",
        "children", "visits", "value_sum",
        "legal_actions", "priors", "expanded", "terminal_value",
    )

    def __init__(
        self,
        parent: Optional["AZNode"],
        action_key: Optional[str],
        to_play: int,
    ) -> None:
        self.parent = parent
        self.action_key = action_key       # str(action) edge from parent
        self.to_play = to_play
        self.children: Dict[str, AZNode] = {}
        self.visits: int = 0
        # value_sum is in this node's `to_play` perspective.
        self.value_sum: float = 0.0
        self.legal_actions: List[Action] = []
        self.priors: List[float] = []
        self.expanded: bool = False
        self.terminal_value: Optional[float] = None  # +1/0/-1 from to_play view

    def q_for_parent(self, parent_to_play: int) -> float:
        """Average value of this child node, expressed from `parent_to_play`'s
        perspective. Same player → +Q; opposing player → −Q."""
        if self.visits == 0:
            return 0.0
        q = self.value_sum / self.visits
        return q if self.to_play == parent_to_play else -q


# ─────────────────────────────────────────────────────────────────────────────
# PUCT search
# ─────────────────────────────────────────────────────────────────────────────

class PUCTSearch:
    """AlphaZero-style PUCT tree search guided by a policy/value network.

    Parameters
    ----------
    net:
        The shared `PolicyValueNetwork`. Must be in `eval()` for search.
    n_simulations:
        Number of simulations (clone+descend+expand+backup) per call to `run`.
    c_puct:
        Exploration constant for PUCT.
        Default 1.5 (AlphaZero typically uses 1.0–4.0; gameplay varies).
    determinize:
        When True, opponent's hidden hand/deck are resampled at the start of
        each simulation. Required for any imperfect-information game.
    dirichlet_alpha, dirichlet_frac:
        Root-only Dirichlet exploration noise (AlphaZero recipe). Set
        `dirichlet_frac=0.0` to disable.
    seed:
        Seed for the internal RNG (determinization + Dirichlet sampling).
    """

    def __init__(
        self,
        net: PolicyValueNetwork,
        n_simulations: int = 32,
        c_puct: float = 1.5,
        determinize: bool = True,
        dirichlet_alpha: float = 0.3,
        dirichlet_frac: float = 0.25,
        seed: Optional[int] = None,
    ) -> None:
        self.net = net
        self.n_simulations = max(1, int(n_simulations))
        self.c_puct = float(c_puct)
        self.determinize = bool(determinize)
        self.dirichlet_alpha = float(dirichlet_alpha)
        self.dirichlet_frac = float(dirichlet_frac)
        self._rng = random.Random(seed)

    # ──────────────────────────────────────────────────────────
    # Public entry point
    # ──────────────────────────────────────────────────────────

    def run(self, env: FaBEnv) -> Tuple[List[Action], List[float], float]:
        """Run search from `env`'s current state.

        Returns:
            legal_actions:        the legal action list at the root
            visit_distribution:   π(a|s) = visits(a) / Σ visits  (same length)
            root_value:           current Q-estimate of the root, in
                                  root.to_play's perspective.
        """
        assert not env.done, "PUCTSearch.run called on a finished env"

        root_to_play = int(env.agent_selection[-1])
        root = AZNode(parent=None, action_key=None, to_play=root_to_play)

        # Initial expansion on a non-determinized clone so the root's
        # `legal_actions` matches the live env (training data needs this).
        root_obs = env._get_obs()[env.agent_selection]
        root_legal = env.legal_actions()
        if not root_legal:
            return [], [], 0.0
        priors, root_value = self._evaluate(root_obs, root_legal)
        priors = self._add_root_noise(priors)
        root.legal_actions = root_legal
        root.priors = priors
        root.expanded = True

        # Run simulations
        for _ in range(self.n_simulations):
            sim_env = copy.deepcopy(env)
            if self.determinize:
                _determinize(sim_env, root_to_play, self._rng)
            self._simulate(root, sim_env)

        # Build visit distribution over root.legal_actions
        visits = []
        for action in root_legal:
            key = str(action)
            child = root.children.get(key)
            visits.append(child.visits if child is not None else 0)

        total = sum(visits)
        if total == 0:
            # Fall back to priors if no child was visited (very low sim count).
            pi = list(priors)
        else:
            pi = [v / total for v in visits]

        # Root Q for diagnostics / value-prediction logging.
        if total > 0:
            root_q = sum(
                root.children[str(a)].q_for_parent(root_to_play)
                * (root.children[str(a)].visits / total)
                for a in root_legal
                if str(a) in root.children
            )
        else:
            root_q = root_value

        return root_legal, pi, float(root_q)

    # ──────────────────────────────────────────────────────────
    # Internals
    # ──────────────────────────────────────────────────────────

    def _simulate(self, root: AZNode, sim_env: FaBEnv) -> None:
        """One PUCT simulation: descend, expand, backup."""
        node = root
        path: List[AZNode] = [node]

        # ── Descend through expanded nodes by PUCT until we hit a leaf
        #    (unexpanded or terminal).
        while True:
            if sim_env.done:
                # Pure terminal — value is from sim_env's rewards.
                v_leaf = self._terminal_value(sim_env, node.to_play)
                self._backup(path, v_leaf, node.to_play)
                return

            if not node.expanded:
                # Expand: evaluate with the network, record priors + legal.
                agent_id = sim_env.agent_selection
                to_play = int(agent_id[-1])
                # Sync to_play in case the parent's recorded to_play is stale
                # (env auto-executed forced states between turns).
                node.to_play = to_play
                obs_dict = sim_env._get_obs()[agent_id]
                legal = sim_env.legal_actions()
                if not legal:
                    # Stuck — treat as 0 value.
                    self._backup(path, 0.0, node.to_play)
                    return
                priors, v_leaf = self._evaluate(obs_dict, legal)
                node.legal_actions = legal
                node.priors = priors
                node.expanded = True
                self._backup(path, v_leaf, node.to_play)
                return

            # Select best child by PUCT
            best_key, best_action = self._select_child(node, sim_env)
            if best_action is None:
                # No legal continuation under this determinization — treat as
                # neutral and back up zero.
                self._backup(path, 0.0, node.to_play)
                return

            sim_env.step(best_action)

            if best_key not in node.children:
                # First time we step into this edge — create the child stub.
                # to_play will be re-synced when the child is expanded.
                child_to_play = (
                    int(sim_env.agent_selection[-1]) if not sim_env.done
                    else 1 - node.to_play
                )
                child = AZNode(parent=node, action_key=best_key,
                               to_play=child_to_play)
                node.children[best_key] = child

            node = node.children[best_key]
            path.append(node)

    def _select_child(
        self, node: AZNode, sim_env: FaBEnv
    ) -> Tuple[Optional[str], Optional[Action]]:
        """Pick the legal child with the highest PUCT score."""
        legal = sim_env.legal_actions() if not sim_env.done else []
        if not legal:
            return None, None
        legal_map = {str(a): a for a in legal}

        # Sum of visits across all of node's children — drives the exploration
        # bonus. Use 1 as a floor so the first child still gets evaluated.
        n_sum = max(1, sum(c.visits for c in node.children.values()))
        sqrt_n_sum = math.sqrt(n_sum)

        best_score = -float("inf")
        best_key = None
        best_action = None

        for prior, action in zip(node.priors, node.legal_actions):
            key = str(action)
            if key not in legal_map:
                # Action illegal under this determinization — skip it.
                continue
            child = node.children.get(key)
            q = child.q_for_parent(node.to_play) if child is not None else 0.0
            n_a = child.visits if child is not None else 0
            u = self.c_puct * prior * sqrt_n_sum / (1 + n_a)
            score = q + u
            if score > best_score:
                best_score = score
                best_key = key
                best_action = legal_map[key]

        return best_key, best_action

    def _backup(
        self, path: List[AZNode], v_leaf: float, leaf_to_play: int
    ) -> None:
        """Increment visits + accumulate value along the path.

        `value_sum` at each node is always kept in that node's `to_play`
        perspective, so on backup we sign-flip the leaf value as we cross
        to-play boundaries.
        """
        for node in path:
            sign = 1.0 if node.to_play == leaf_to_play else -1.0
            node.visits += 1
            node.value_sum += sign * v_leaf

    def _evaluate(
        self, obs_dict: dict, legal: List[Action]
    ) -> Tuple[List[float], float]:
        """Forward the network on (obs, legal) and return (priors, value).

        The network is kept in eval() mode and no gradients are tracked. The
        priors and value are returned as plain Python floats so we don't keep
        autograd state alive across simulations.
        """
        was_training = self.net.training
        self.net.eval()
        try:
            probs, value = self.net.predict(obs_dict, legal)
        finally:
            if was_training:
                self.net.train()
        return list(probs), float(value)

    def _terminal_value(self, sim_env: FaBEnv, to_play: int) -> float:
        """Return ±1 / 0 from `to_play`'s perspective at game end."""
        r = sim_env._rewards.get(f"agent_{to_play}", 0.0)
        # The env adds a damage-shaping term (+0.01 per damage) on top of
        # ±1 win/loss. Clip to {-1, 0, +1} for AZ training targets.
        if r > 0.5:
            return 1.0
        if r < -0.5:
            return -1.0
        return 0.0

    def _add_root_noise(self, priors: List[float]) -> List[float]:
        """Mix Dirichlet noise into the root priors (AlphaZero exploration)."""
        if self.dirichlet_frac <= 0.0 or not priors:
            return priors
        n = len(priors)
        # Sample n iid Gamma(α) and normalize → Dirichlet(α).
        noise = [self._rng.gammavariate(self.dirichlet_alpha, 1.0)
                 for _ in range(n)]
        s = sum(noise)
        if s <= 0.0:
            return priors
        noise = [x / s for x in noise]
        eps = self.dirichlet_frac
        return [(1.0 - eps) * p + eps * n_i for p, n_i in zip(priors, noise)]


# ─────────────────────────────────────────────────────────────────────────────
# Convenience: action sampling with temperature
# ─────────────────────────────────────────────────────────────────────────────

def sample_action_index(
    pi: List[float],
    temperature: float,
    rng: random.Random,
) -> int:
    """Pick an action index from the visit distribution π.

    `temperature = 0` → greedy argmax. Otherwise rescale π by 1/τ before
    sampling. Returns 0 for an empty input (caller should guard).
    """
    if not pi:
        return 0
    if temperature <= 1e-6:
        # Greedy with random tie-breaking.
        best = max(pi)
        winners = [i for i, p in enumerate(pi) if p == best]
        return rng.choice(winners)
    if abs(temperature - 1.0) < 1e-6:
        weighted = pi
    else:
        inv = 1.0 / temperature
        # Visit counts are already non-negative; raise to 1/τ then renormalize.
        weighted = [max(p, 0.0) ** inv for p in pi]
        s = sum(weighted)
        if s <= 0.0:
            return rng.randrange(len(pi))
        weighted = [w / s for w in weighted]
    r = rng.random()
    cum = 0.0
    for i, w in enumerate(weighted):
        cum += w
        if r < cum:
            return i
    return len(weighted) - 1


__all__ = [
    "AZNode",
    "PUCTSearch",
    "sample_action_index",
]

"""
mcts_agent.py — Monte Carlo Tree Search agent for FaBEnv.

Uses Perfect Information Monte Carlo (PIMC): at the start of each simulation
the opponent's hidden hand and deck are replaced with a random sample drawn
from the pool of cards not yet visible to us.  UCT (UCB1 applied to trees)
drives exploration; random rollouts estimate terminal value.

Usage
-----
    from mcts_agent import MCTSAgent
    from run_env import run_game

    mcts = MCTSAgent(player_idx=0, n_simulations=200)
    run_game(agent0=mcts, verbose=True)
"""

from __future__ import annotations

import copy
import math
import random
from collections import Counter
from typing import Dict, List, Optional

from cards import build_rhinar_deck, build_dorinthea_deck, CardType
from fab_env import FaBEnv, Phase
from actions import Action
from agents import RandomAgent


# ─────────────────────────────────────────────────────────────────────────────
# MCTS tree node
# ─────────────────────────────────────────────────────────────────────────────

class MCTSNode:
    """One node in the UCT search tree.

    Represents the game state reached after the MCTS agent took
    ``action_from_parent`` from the parent node.
    """

    __slots__ = ("parent", "action_from_parent", "children",
                 "visits", "total_reward", "untried_actions")

    def __init__(
        self,
        parent: Optional[MCTSNode] = None,
        action_from_parent: Optional[Action] = None,
        untried_actions: Optional[List[Action]] = None,
    ) -> None:
        self.parent = parent
        self.action_from_parent = action_from_parent
        self.children: Dict[str, MCTSNode] = {}
        self.visits: int = 0
        self.total_reward: float = 0.0
        # Copy so callers can't mutate the source list
        self.untried_actions: List[Action] = list(untried_actions or [])

    # ------------------------------------------------------------------
    def ucb1(self, c: float) -> float:
        if self.visits == 0:
            return float("inf")
        return (self.total_reward / self.visits
                + c * math.sqrt(math.log(self.parent.visits) / self.visits))

    def best_child(self, c: float) -> MCTSNode:
        return max(self.children.values(), key=lambda ch: ch.ucb1(c))

    def is_fully_expanded(self) -> bool:
        return len(self.untried_actions) == 0

    def is_terminal(self) -> bool:
        return self.is_fully_expanded() and len(self.children) == 0


# ─────────────────────────────────────────────────────────────────────────────
# Module-level helpers
# ─────────────────────────────────────────────────────────────────────────────

def _get_hero_full_deck(hero_name: str) -> List:
    """Return the hero's full card pool, excluding the hero card itself."""
    if "Rhinar" in hero_name:
        full = build_rhinar_deck()
    else:
        full = build_dorinthea_deck()
    return [c for c in full if c.card_type != CardType.HERO]


def _determinize(sim_env: FaBEnv, my_player_idx: int, rng: random.Random) -> None:
    """Replace the opponent's hidden hand/deck with a plausible random sample.

    Cards already in visible zones (graveyard, pitch_zone, combat_chain,
    banished, arsenal) are subtracted from the full deck by name-based
    counting, preserving multi-copy card handling.  The remaining unseen
    cards are shuffled and split: first ``hand_size`` go to opponent.hand,
    the rest to opponent.deck.
    """
    opp_idx = 1 - my_player_idx
    opp = sim_env._game.players[opp_idx]

    full_deck = _get_hero_full_deck(opp.hero_name)

    # Cards in any zone that is visible to us
    seen = (opp.graveyard
            + opp.pitch_zone
            + opp.combat_chain
            + opp.banished
            + ([opp.arsenal] if opp.arsenal else []))

    seen_remaining = Counter(c.name for c in seen)
    unseen: List = []
    for card in full_deck:
        if seen_remaining.get(card.name, 0) > 0:
            seen_remaining[card.name] -= 1
        else:
            unseen.append(card)

    hand_size = len(opp.hand)
    rng.shuffle(unseen)
    opp.hand = unseen[:hand_size]
    opp.deck = unseen[hand_size:]


def _dispatch_action(env: FaBEnv, agent, agent_id: str) -> None:
    """Ask *agent* for a decision and step *env* with it.

    Mirrors the dispatch logic in run_env.run_game() so that RandomAgent
    can drive the sim correctly through all phases.
    """
    player_idx = int(agent_id[-1])
    player = env._game.players[player_idx]
    opponent = env._game.players[1 - player_idx]
    obs = env._get_obs()
    legal = env.legal_actions()
    if not legal:
        return

    phase = env._phase
    if phase == Phase.ATTACK:
        action = agent.select_action(obs[agent_id], legal, player, opponent)
    elif phase == Phase.DEFEND:
        action = agent.select_defend(
            obs[agent_id], legal, player,
            env._pending_attack_power,
            env._pending_defend_total,
        )
    elif phase == Phase.PITCH:
        action = agent.select_pitch(
            obs[agent_id], legal, player, env._pending_play_card
        )
    elif phase == Phase.REACTION:
        is_attacker = player_idx == env._reaction_attacker_idx
        action = agent.select_reaction(
            obs[agent_id], legal, player,
            env._pending_attack_power, is_attacker,
        )
    elif phase == Phase.INSTANT:
        ap = env._pending_attack_power if env._pending_attack is not None else 0
        action = agent.select_instant(obs[agent_id], legal, player, ap)
    elif phase == Phase.ARSENAL:
        action = agent.select_arsenal(obs[agent_id], legal, player)
    else:
        action = legal[0]

    env.step(action)


def _advance_to_my_turn(
    env: FaBEnv, random_agent: RandomAgent, my_agent_id: str
) -> None:
    """Step the env with *random_agent* until it is *my_agent_id*'s turn."""
    while not env.done and env.agent_selection != my_agent_id:
        _dispatch_action(env, random_agent, env.agent_selection)


def _rollout(
    sim_env: FaBEnv, my_player_idx: int, random_agent: RandomAgent
) -> float:
    """Play the game to completion using *random_agent* for both sides.

    Returns the reward from the MCTS agent's perspective (+1 win, -1 loss,
    0 draw).
    """
    while not sim_env.done:
        _dispatch_action(sim_env, random_agent, sim_env.agent_selection)
    return sim_env._rewards.get(f"agent_{my_player_idx}", 0.0)


# ─────────────────────────────────────────────────────────────────────────────
# MCTS Agent
# ─────────────────────────────────────────────────────────────────────────────

class MCTSAgent:
    """MCTS agent with PIMC determinization for hidden-information handling.

    Parameters
    ----------
    player_idx:
        0 for Rhinar (agent_0), 1 for Dorinthea (agent_1).
    n_simulations:
        Number of MCTS simulations per decision.  More sims → stronger play
        but slower decisions.  200 is a reasonable starting point.
    exploration_c:
        UCB1 exploration constant.  1.41 (≈ √2) is the standard default.
    seed:
        Seed for the agent's private RNG (determinization sampling).
    """

    def __init__(
        self,
        player_idx: int,
        n_simulations: int = 200,
        exploration_c: float = 1.41,
        seed: Optional[int] = None,
    ) -> None:
        self._player_idx = player_idx
        self._agent_id = f"agent_{player_idx}"
        self._n_sims = n_simulations
        self._c = exploration_c
        self._rng = random.Random(seed)
        self._env: Optional[FaBEnv] = None
        self._rollout_agent = RandomAgent()

    def set_env(self, env: FaBEnv) -> None:
        """Provide a reference to the live env so simulations can deepcopy it."""
        self._env = env

    # ------------------------------------------------------------------
    # Core MCTS machinery
    # ------------------------------------------------------------------

    def _mcts_select(self, legal: List[Action]) -> Action:
        if len(legal) == 1:
            return legal[0]
        if self._env is None:
            raise RuntimeError(
                "MCTSAgent.set_env() must be called before the game loop."
            )
        return self._run_mcts(legal)

    def _run_mcts(self, legal: List[Action]) -> Action:
        root = MCTSNode(untried_actions=legal[:])

        for _ in range(self._n_sims):
            # ── 1. Clone + determinize ──────────────────────────────
            sim_env = copy.deepcopy(self._env)
            _determinize(sim_env, self._player_idx, self._rng)
            _advance_to_my_turn(sim_env, self._rollout_agent, self._agent_id)

            # ── 2. Selection ────────────────────────────────────────
            node = self._select(root, sim_env)

            # ── 3. Expansion ────────────────────────────────────────
            if not sim_env.done and not node.is_fully_expanded():
                action_key, action = self._pick_untried(node, sim_env)
                if action is not None:
                    sim_env.step(action)
                    _advance_to_my_turn(
                        sim_env, self._rollout_agent, self._agent_id
                    )
                    child_legal = (
                        sim_env.legal_actions() if not sim_env.done else []
                    )
                    child = MCTSNode(
                        parent=node,
                        action_from_parent=action,
                        untried_actions=child_legal,
                    )
                    node.children[action_key] = child
                    node = child

            # ── 4. Rollout ──────────────────────────────────────────
            reward = _rollout(sim_env, self._player_idx, self._rollout_agent)

            # ── 5. Backpropagation ──────────────────────────────────
            cur = node
            while cur is not None:
                cur.visits += 1
                cur.total_reward += reward
                cur = cur.parent

        if not root.children:
            # No simulation produced a child — fall back to first legal action
            return legal[0]
        return root.best_child(c=0.0).action_from_parent

    def _select(self, root: MCTSNode, sim_env: FaBEnv) -> MCTSNode:
        """Walk the tree using UCB1 until we find an unexpanded/terminal node."""
        node = root
        while not sim_env.done and node.is_fully_expanded() and node.children:
            legal_map = {str(a): a for a in sim_env.legal_actions()}
            # Only consider children whose action is legal in this determinization
            candidates = [
                (k, c) for k, c in node.children.items() if k in legal_map
            ]
            if not candidates:
                break
            best_key, best_node = max(candidates, key=lambda kc: kc[1].ucb1(self._c))
            sim_env.step(legal_map[best_key])
            _advance_to_my_turn(sim_env, self._rollout_agent, self._agent_id)
            node = best_node
        return node

    def _pick_untried(self, node: MCTSNode, sim_env: FaBEnv):
        """Pop an untried action that is legal in the current determinization."""
        legal_map = {str(a): a for a in sim_env.legal_actions()}
        while node.untried_actions:
            candidate = node.untried_actions.pop()
            key = str(candidate)
            if key in legal_map:
                return key, legal_map[key]
        return None, None

    # ------------------------------------------------------------------
    # Agent interface — all delegate to _mcts_select
    # ------------------------------------------------------------------

    def select_action(self, obs, legal, player, opponent) -> Action:
        return self._mcts_select(legal)

    def select_defend(self, obs, legal, player, attack_power,
                      already_defense=0) -> Action:
        return self._mcts_select(legal)

    def select_pitch(self, obs, legal, player, pending_card=None) -> Action:
        return self._mcts_select(legal)

    def select_arsenal(self, obs, legal, player) -> Action:
        return self._mcts_select(legal)

    def select_instant(self, obs, legal, player, attack_power=0) -> Action:
        return self._mcts_select(legal)

    def select_reaction(self, obs, legal, player,
                        attack_power=0, is_attacker=False) -> Action:
        return self._mcts_select(legal)

    def select_choose_first(self, legal, player) -> Action:
        return self._mcts_select(legal)

    def select_pitch_order(self, obs, legal, player) -> Action:
        return self._mcts_select(legal)

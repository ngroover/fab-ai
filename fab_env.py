"""
FaBEnv — Flesh and Blood Classic Battles as a two-agent gym environment.

Interface mirrors gymnasium's Env but exposes two agents (agent_0 = player 0,
agent_1 = player 1). Follows the PettingZoo AEC (Agent-Environment-Cycle) pattern:
one agent acts at a time, alternating, with the environment handling all resolution.

Key API
-------
env = FaBEnv()
obs, infos = env.reset()

while not env.done:
    agent = env.agent_selection          # whose turn to act
    legal  = env.legal_actions()         # list[Action] valid right now
    action = your_agent.select(obs[agent], legal)
    obs, rewards, terminations, truncations, infos = env.step(action)

Decision phases within a single "turn":
  1. ATTACK   — active player picks attack/weapon/pass actions until they pass
  2. DEFEND   — defending player reacts to each attack
  3. ARSENAL  — active player stores a card at end of turn
"""

from __future__ import annotations

import random
import copy
from enum import Enum, auto
from typing import Dict, List, Optional, Tuple, Any

from cards import (Card, CardType, Color,
                   build_rhinar_deck, build_rhinar_equipment,
                   build_dorinthea_deck, build_dorinthea_equipment)
from card_effects import EffectTrigger, EffectAction
from game_state import Player, GameState, Equipment
from actions import (
    Action, ActionType,
    legal_attack_actions, legal_pitch_actions,
    legal_defend_actions, legal_arsenal_actions, legal_choose_first_actions,
    legal_instant_actions, legal_reaction_actions,
)
from observations import build_observation, PLAYER_OBS_SIZE, CARD_FEATURES
from spaces import Discrete, Box, Dict as DictSpace


# ──────────────────────────────────────────────────────────────
# Phase enum
# ──────────────────────────────────────────────────────────────

class Phase(Enum):
    START        = auto()
    CHOOSE_FIRST = auto()  # pre-game: randomly selected player chooses who goes first
    ATTACK       = auto()
    PITCH        = auto()   # second step of playing a card: choose which cards to pitch
    DEFEND       = auto()
    REACTION     = auto()   # after defender commits blocks: attacker plays attack reactions, then defender plays defense reactions
    INSTANT      = auto()   # either player may play instants onto a stack; LIFO resolution
    ARSENAL      = auto()
    END          = auto()


# ──────────────────────────────────────────────────────────────
# FaBEnv
def _make_rhinar(rng: Optional[random.Random] = None) -> Player:
    equip = build_rhinar_equipment()
    return Player(
        name="Rhinar",
        life=20,
        intellect=4,
        deck=build_rhinar_deck(),
        equipment_list=equip[1:],
        weapon=equip[0],
        rng=rng,
    )


def _make_dorinthea(rng: Optional[random.Random] = None) -> Player:
    equip = build_dorinthea_equipment()
    return Player(
        name="Dorinthea",
        life=20,
        intellect=4,
        deck=build_dorinthea_deck(),
        equipment_list=equip[1:],
        weapon=equip[0],
        rng=rng,
    )


# ──────────────────────────────────────────────────────────────

class FaBEnv:
    """
    Two-agent AEC-style environment for FaB Classic Battles.

    Agents: ["agent_0", "agent_1"]
    agent_0 = player index 0, agent_1 = player index 1.
    """

    metadata = {"name": "fab_classic_battles_v0"}
    MAX_TURNS = 80

    def __init__(self, verbose: bool = False, log_file: Optional[str] = None,
                 log_callback=None):
        self.verbose = verbose
        self._log_file = log_file
        self._log_callback = log_callback
        self.agents = ["agent_0", "agent_1"]
        self._game: Optional[GameState] = None
        self._phase = Phase.START
        self._pending_attack: Optional[Card] = None  # card currently resolving
        self._pending_attack_power: int = 0
        self._pending_is_weapon: bool = False
        self._rewards: Dict[str, float] = {"agent_0": 0.0, "agent_1": 0.0}
        self._terminations: Dict[str, bool] = {"agent_0": False, "agent_1": False}
        self._truncations: Dict[str, bool] = {"agent_0": False, "agent_1": False}
        self.agent_selection: str = "agent_0"
        self.done: bool = False
        self._pending_play_card: Optional[Card] = None  # card chosen in PLAY_CARD step, awaiting pitch
        self._pitched_this_play: List[Card] = []         # cards pitched so far for the current pending card
        self._pending_weapon_attack: bool = False        # True when PITCH phase is for a weapon attack
        self._pending_instant_play: bool = False         # True when PITCH phase is for an instant played during the INSTANT window
        self._pending_instant_player_idx: int = 0        # which player is paying for / owns the pending instant
        self._pending_defend_indices: List[int] = []     # hand indices accumulated during defend step
        self._pending_defend_equip_slots: List[str] = [] # equip slots accumulated during defend step
        self._choosing_player_idx: int = 0               # player who won the coin flip in CHOOSE_FIRST
        # Instant stack & priority bookkeeping — only meaningful in Phase.INSTANT
        self._instant_stack: List[Tuple[int, Card]] = []  # LIFO of (owner_idx, card)
        self._instant_priority_idx: int = 0
        self._instant_passes: int = 0                     # consecutive passes since last stack change
        self._instant_return_phase: Optional[Phase] = None
        self._instant_return_agent_idx: int = 0
        # Reaction phase bookkeeping — only meaningful in Phase.REACTION
        self._reaction_priority_idx: int = 0
        self._reaction_passes: int = 0
        self._reaction_attacker_idx: int = 0
        self._reaction_defense_bonus: int = 0             # defense from resolved defense reactions
        self._committed_defend_action: Optional[Action] = None
        self._pending_reaction_play: bool = False         # True when PITCH phase is paying for a reaction card
        self._pending_reaction_player_idx: int = 0
        # Isolated RNG — seeded in reset() so game randomness is never shared with external code.
        self._rng: random.Random = random.Random()

        # Observation / action spaces (agent-specific but symmetric structure)
        obs_size = PLAYER_OBS_SIZE
        self.observation_spaces = {
            a: DictSpace({
                "agent":        Box(0.0, 1.0, shape=(obs_size,)),
                "opponent":     Box(0.0, 1.0, shape=(obs_size,)),
                "global":       Box(0.0, 1.0, shape=(2,)),
                # During the PITCH phase, encodes the card the agent has committed to play.
                # All zeros in every other phase.
                "pending_card": Box(0.0, 1.0, shape=(CARD_FEATURES,)),
            })
            for a in self.agents
        }
        # Action space upper bound; actual legal count varies. Agents MUST use legal_actions().
        self.action_spaces = {a: Discrete(256) for a in self.agents}

    # ──────────────────────────────────────────────────────────
    # reset
    # ──────────────────────────────────────────────────────────

    def reset(self, seed: Optional[int] = None,
              player0: Optional[Player] = None,
              player1: Optional[Player] = None) -> Tuple[Dict, Dict]:
        """
        Reset the environment and return initial observations.

        Parameters
        ----------
        player0, player1 : optional Player overrides.  When provided, these
            replace the default Rhinar / Dorinthea players so custom decks
            can be used without subclassing FaBEnv.

        Returns:
            obs   — {agent_id: observation_dict}
            infos — {agent_id: {}}
        """
        self._rng = random.Random(seed)

        p0 = _make_rhinar(self._rng)
        p1 = _make_dorinthea(self._rng)

        self._game = GameState(p0, p1, rng=self._rng)

        # Opening hands
        for p in self._game.players:
            p.draw_to_intellect()

        self._rewards = {"agent_0": 0.0, "agent_1": 0.0}
        self._terminations = {"agent_0": False, "agent_1": False}
        self._truncations = {"agent_0": False, "agent_1": False}
        self.done = False

        # Reset instant-window bookkeeping
        self._instant_stack = []
        self._instant_priority_idx = 0
        self._instant_passes = 0
        self._instant_return_phase = None
        self._instant_return_agent_idx = 0
        self._pending_instant_play = False
        self._pending_instant_player_idx = 0
        # Reset reaction-phase bookkeeping
        self._reaction_priority_idx = 0
        self._reaction_passes = 0
        self._reaction_attacker_idx = 0
        self._reaction_defense_bonus = 0
        self._committed_defend_action = None
        self._pending_reaction_play = False
        self._pending_reaction_player_idx = 0

        # Randomly select which player gets to choose who goes first
        self._choosing_player_idx = self._rng.randint(0, 1)
        self._phase = Phase.CHOOSE_FIRST
        self.agent_selection = f"agent_{self._choosing_player_idx}"
        self._log(f"  🪙  Coin flip — agent_{self._choosing_player_idx} "
                  f"({self._game.players[self._choosing_player_idx].name}) "
                  f"chooses who goes first.")

        obs = self._get_obs()
        infos = {a: {} for a in self.agents}
        return obs, infos

    # ──────────────────────────────────────────────────────────
    # step
    # ──────────────────────────────────────────────────────────

    def step(self, action: Action) -> Tuple[Dict, Dict, Dict, Dict, Dict]:
        """
        Apply `action` for the current agent (self.agent_selection).

        Returns:
            obs, rewards, terminations, truncations, infos
        """
        if self.done:
            raise RuntimeError("step() called on a finished environment. Call reset() first.")

        agent = self.agent_selection
        active_idx = int(agent[-1])
        active = self._game.players[active_idx]
        opponent = self._game.players[1 - active_idx]

        self._rewards = {"agent_0": 0.0, "agent_1": 0.0}  # zero out each step

        # ── Dispatch by phase ──
        if self._phase == Phase.CHOOSE_FIRST:
            self._handle_choose_first_action(action)
        elif self._phase == Phase.ATTACK:
            self._handle_attack_action(action, active, opponent)
        elif self._phase == Phase.PITCH:
            self._handle_pitch_action(action, active, opponent)
        elif self._phase == Phase.DEFEND:
            self._handle_defend_action(action, active, opponent)
        elif self._phase == Phase.REACTION:
            self._handle_reaction_action(action, active, opponent)
        elif self._phase == Phase.INSTANT:
            self._handle_instant_action(action, active, opponent)
        elif self._phase == Phase.ARSENAL:
            self._handle_arsenal_action(action, active, opponent)

        # ── Check game over ──
        if self._game.is_over() or self._game.turn_number > self.MAX_TURNS:
            self._finalize()

        # ── Auto-execute forced (single-legal-action) states ──
        while not self.done:
            forced = self.legal_actions()
            if len(forced) != 1:
                break
            auto_action = forced[0]
            auto_agent = self.agent_selection
            auto_active_idx = int(auto_agent[-1])
            auto_active = self._game.players[auto_active_idx]
            auto_opponent = self._game.players[1 - auto_active_idx]

            if self._phase == Phase.CHOOSE_FIRST:
                self._handle_choose_first_action(auto_action)
            elif self._phase == Phase.ATTACK:
                self._handle_attack_action(auto_action, auto_active, auto_opponent)
            elif self._phase == Phase.PITCH:
                self._handle_pitch_action(auto_action, auto_active, auto_opponent)
            elif self._phase == Phase.DEFEND:
                self._handle_defend_action(auto_action, auto_active, auto_opponent)
            elif self._phase == Phase.REACTION:
                self._handle_reaction_action(auto_action, auto_active, auto_opponent)
            elif self._phase == Phase.INSTANT:
                self._handle_instant_action(auto_action, auto_active, auto_opponent)
            elif self._phase == Phase.ARSENAL:
                self._handle_arsenal_action(auto_action, auto_active, auto_opponent)

            if self._game.is_over() or self._game.turn_number > self.MAX_TURNS:
                self._finalize()

        obs = self._get_obs()
        infos = {a: {"legal_actions": self.legal_actions()} for a in self.agents}
        return obs, dict(self._rewards), dict(self._terminations), dict(self._truncations), infos

    # ──────────────────────────────────────────────────────────
    # legal_actions
    # ──────────────────────────────────────────────────────────

    def legal_actions(self) -> List[Action]:
        """Return legal actions for the current agent in the current phase."""
        if self.done:
            return []
        agent = self.agent_selection
        active_idx = int(agent[-1])
        active = self._game.players[active_idx]

        if self._phase == Phase.CHOOSE_FIRST:
            return legal_choose_first_actions()
        elif self._phase == Phase.ATTACK:
            return legal_attack_actions(active)
        elif self._phase == Phase.PITCH:
            return legal_pitch_actions(active, self._pending_play_card)
        elif self._phase == Phase.DEFEND:
            return legal_defend_actions(active, self._pending_attack_power,
                                        self._pending_defend_indices,
                                        self._pending_defend_equip_slots)
        elif self._phase == Phase.REACTION:
            return legal_reaction_actions(active, self._reaction_attacker_idx,
                                          self._reaction_priority_idx)
        elif self._phase == Phase.INSTANT:
            return legal_instant_actions(active)
        elif self._phase == Phase.ARSENAL:
            return legal_arsenal_actions(active)
        return []

    # ──────────────────────────────────────────────────────────
    # Internal: action handlers
    # ──────────────────────────────────────────────────────────

    def _handle_choose_first_action(self, action: Action):
        """Pre-game: the coin-flip winner chooses who goes first."""
        if action.action_type == ActionType.GO_FIRST:
            self._game.active_player_idx = self._choosing_player_idx
        else:  # GO_SECOND
            self._game.active_player_idx = 1 - self._choosing_player_idx
        chooser = self._game.players[self._choosing_player_idx]
        first = self._game.active
        self._log(f"  ▶  {chooser.name} chooses: {first.name} goes first.")
        self._begin_turn()

    def _handle_attack_action(self, action: Action, active: Player, opponent: Player):
        """Step 1 of card play: agent selects which card to play (or weapon/pass)."""
        if action.action_type == ActionType.PASS:
            self._log(f"  ▶  {active.name} passes.")
            self._end_attack_phase(active, opponent)
            return

        if action.action_type == ActionType.WEAPON:
            weapon = active.weapon
            if weapon:
                needed = max(0, weapon.cost - active.resource_points)
                if needed > 0:
                    # Transition to PITCH phase so agent picks which cards to pitch
                    self._pending_play_card = weapon
                    self._pitched_this_play = []
                    self._pending_weapon_attack = True
                    self._phase = Phase.PITCH
                    self._log(f"\n  ▶  {active.name} chooses to attack with {weapon.name} "
                              f"(needs {needed} more resource{'s' if needed != 1 else ''})")
                    return
            self._resolve_weapon_attack(active, opponent)
            return

        if action.action_type == ActionType.ACTIVATE_EQUIPMENT:
            self._resolve_equipment_activation(action.equip_slot, active)
            return

        if action.action_type == ActionType.PLAY_CARD:
            # Remove the chosen card from hand / arsenal
            if action.from_arsenal and active.arsenal and active.arsenal.card_type != CardType.MENTOR:
                card = active.arsenal
                active.arsenal = None
            elif action.card in active.hand:
                card = action.card
                active.hand.remove(card)
            else:
                self._log(f"  ⚠  Invalid card {action.card}, passing.")
                self._end_attack_phase(active, opponent)
                return

            active.action_points -= 1

            needed = max(0, card.cost - active.resource_points)
            if needed > 0:
                # Cost not yet covered — transition to PITCH phase so agent picks pitches
                self._pending_play_card = card
                self._pitched_this_play = []
                self._phase = Phase.PITCH
                self._log(f"\n  ▶  {active.name} chooses to play {card} "
                          f"(needs {needed} more resource{'s' if needed != 1 else ''})")
                return  # PITCH handler will finish resolving the card

            # Card is free (or resources already cover it) — resolve immediately
            active.resource_points -= card.cost
            self._log(f"\n  ▶  {active.name} plays {card}")
            self._resolve_played_card(card, active, opponent)
            return

        # After instant/action: if we still have action points, stay in ATTACK phase;
        # otherwise end turn.
        if active.action_points <= 0:
            self._end_attack_phase(active, opponent)

    def _handle_pitch_action(self, action: Action, active: Player, opponent: Player):
        """Sequential pitch step: agent pitches one card at a time to cover the cost.
        Stays in PITCH phase until resource_points >= pending card's cost, then resolves."""
        card = self._pending_play_card

        if action.pitch_indices:
            # Pitch the single selected card immediately
            idx = action.pitch_indices[0]
            if 0 <= idx < len(active.hand):
                pc = active.hand[idx]
                active.pitch(pc)  # removes from hand, adds pitch value to resource_points
                self._pitched_this_play.append(pc)

            # If cost still not covered, stay in PITCH phase for another card
            if active.resource_points < card.cost:
                return

        # Cost is covered (or no pitchable cards remain) — resolve the card
        self._pending_play_card = None
        pitched = self._pitched_this_play[:]
        self._pitched_this_play = []

        if self._pending_instant_play:
            # Instant played during an INSTANT window — pay cost and push to stack,
            # then return to INSTANT phase with priority handed to the opponent.
            self._pending_instant_play = False
            owner_idx = self._pending_instant_player_idx
            active.resource_points -= card.cost
            self._log(f"\n    ▶  {active.name} plays {card}"
                      + (f" (pitched: {', '.join(c.name for c in pitched)})"
                         if pitched else ""))
            self._phase = Phase.INSTANT
            self._push_instant_to_stack(card, owner_idx)
            return

        if self._pending_reaction_play:
            # Card played during REACTION window — push to stack and return to REACTION.
            self._pending_reaction_play = False
            owner_idx = self._pending_reaction_player_idx
            active.resource_points -= card.cost
            self._log(f"\n    ▶  {active.name} plays {card}"
                      + (f" (pitched: {', '.join(c.name for c in pitched)})"
                         if pitched else ""))
            self._phase = Phase.REACTION
            self._push_reaction_to_stack(card, owner_idx)
            return

        self._phase = Phase.ATTACK

        if self._pending_weapon_attack:
            self._pending_weapon_attack = False
            self._resolve_weapon_attack(active, opponent, pitched)
            return

        active.resource_points -= card.cost
        self._log(f"\n  ▶  {active.name} plays {card}"
                  + (f" (pitched: {', '.join(c.name for c in pitched)})"
                     if pitched else ""))

        self._resolve_played_card(card, active, opponent)

    def _resolve_played_card(self, card: Card, active: Player, opponent: Player):
        """Dispatch card resolution after cost/pitch have been handled."""
        if card.card_type == CardType.INSTANT:
            self._resolve_instant(card, active, opponent)
        elif card.card_type == CardType.ACTION:
            self._resolve_action(card, active, opponent)
        elif card.card_type == CardType.ACTION_ATTACK:
            self._pending_attack = card
            self._pending_is_weapon = False
            self._trigger_defend_phase(active, opponent)
            return  # defend phase takes over; returns to ATTACK after defend resolves

        # After instant/action: end turn if no action points remain
        if active.action_points <= 0:
            self._end_attack_phase(active, opponent)

    @property
    def _pending_defend_total(self) -> int:
        """Total defense value accumulated so far this defend step."""
        agent = self.agent_selection
        player_idx = int(agent[-1])
        player = self._game.players[player_idx]
        total = sum(
            player.hand[i].defense
            for i in self._pending_defend_indices
            if 0 <= i < len(player.hand)
        )
        total += sum(
            player.equipment[slot].defense
            for slot in self._pending_defend_equip_slots
            if slot in player.equipment and player.equipment[slot].active
        )
        return total

    def _handle_defend_action(self, action: Action, defender: Player, attacker: Player):
        """Defender picks one card at a time; empty action commits the accumulated block.

        Instants are NOT played here — they go through the attack reaction
        INSTANT window (opened before this DEFEND phase begins) and resolve
        off the stack. Blocking cards are committed directly and do not use
        the stack at all.
        """
        # Single card/equipment addition — accumulate and stay in DEFEND phase
        if action.defend_hand_indices or action.defend_equip_slots:
            self._pending_defend_indices.extend(action.defend_hand_indices)
            self._pending_defend_equip_slots.extend(action.defend_equip_slots)
            return  # defender picks again next step

        # Done — commit blocks and open the reaction window before resolving combat
        full_action = Action(ActionType.DEFEND,
                             defend_hand_indices=list(self._pending_defend_indices),
                             defend_equip_slots=list(self._pending_defend_equip_slots))
        self._pending_defend_indices = []
        self._pending_defend_equip_slots = []
        self._committed_defend_action = full_action
        attacker_idx = self._game.active_player_idx
        defender_idx = 1 - attacker_idx
        self._enter_reaction_phase(attacker_idx, defender_idx)

    def _resolve_defend(self, action: Action, defender: Player, attacker: Player,
                        reaction_defense_bonus: int = 0):
        """Resolve the defend step once all blocking cards and reactions have been resolved."""
        card = self._pending_attack
        is_weapon = self._pending_is_weapon

        # Collect defending cards
        def_cards = self._snapshot_by_indices(defender.hand, action.defend_hand_indices)
        def_equip = []
        for slot in action.defend_equip_slots:
            eq = defender.equipment.get(slot)
            if eq and eq.active:
                def_equip.append(eq)

        # Remove defending hand cards
        for c in def_cards:
            if c in defender.hand:
                defender.hand.remove(c)

        # Use the power stored when the attack was declared. It already includes
        # weapon bonuses (set by _trigger_defend_phase) and any modifications from
        # reaction cards (e.g. Thrust +3, Out for Blood +2).
        power = self._pending_attack_power
        if is_weapon:
            attacker.next_weapon_power_bonus = 0
        else:
            attacker.next_brute_attack_bonus = 0

        total_def = (sum(c.defense for c in def_cards) + sum(e.defense for e in def_equip)
                     + reaction_defense_bonus)

        if def_cards or def_equip or reaction_defense_bonus:
            names = [c.name for c in def_cards] + [e.card.name for e in def_equip]
            bonus_str = f" +{reaction_defense_bonus} from reactions" if reaction_defense_bonus else ""
            self._log(f"    🛡  {defender.name} defends: {', '.join(names) if names else '(none)'} "
                      f"(def: {total_def}{bonus_str})")
        else:
            self._log(f"    🛡  {defender.name} does not defend.")

        # Damage
        damage = max(0, power - total_def)
        hit = damage > 0
        if hit:
            _, old_life, new_life = defender.take_damage(damage, card.name)
            self._log(f"    💥  {defender.name} takes {damage} damage from {card.name}! (Life: {old_life} → {new_life})")
            # Small shaping reward: damaging opponent
            atk_agent = f"agent_{self._game.active_player_idx}"
            self._rewards[atk_agent] += damage * 0.01
        else:
            self._log(f"    ✅  Fully blocked!")

        # Battleworn
        for eq in def_equip:
            if "Battleworn" in eq.card.text and not eq.destroyed:
                eq.destroyed = True
                self._log(f"    🔨 {eq.card.name} destroyed (Battleworn).")

        # Defending cards to graveyard
        for c in def_cards:
            defender.graveyard.append(c)

        # On-hit effects
        if hit:
            self._on_hit(card, attacker, defender, is_weapon)

        # Attack card to graveyard
        if not is_weapon:
            attacker.graveyard.append(card)

        attacker.attacks_this_turn += 1
        if is_weapon:
            attacker.weapon_attack_count += 1

        # Go again
        go = card.go_again
        if is_weapon and attacker.next_weapon_go_again:
            go = True
            attacker.next_weapon_go_again = False
        if go:
            attacker.action_points += 1
            self._log(f"    ↩  Go again! {attacker.name} gains 1 action point.")
            if is_weapon and not attacker.weapon_additional_attack and attacker.weapon_attack_count == 1:
                # Only the first weapon attack's go-again grants an additional attack slot
                attacker.weapon_additional_attack = True

        # Mentor check for Rhinar
        if not is_weapon and card.power >= 6 and attacker.mentor_face_up:
            self._mentor_lesson(attacker)

        self._pending_attack = None
        self._pending_is_weapon = False

        # Open an instant window before returning control to the attacker.
        self._enter_instant_phase(
            return_phase=Phase.ATTACK,
            return_agent_idx=self._game.active_player_idx,
        )

    def _handle_arsenal_action(self, action: Action, active: Player, opponent: Player):
        """Store a card (or nothing) in arsenal, then complete end phase."""
        if action.arsenal_hand_index >= 0 and not active.arsenal:
            if action.arsenal_hand_index < len(active.hand):
                card = active.hand[action.arsenal_hand_index]
                active.hand.remove(card)
                active.arsenal = card
                self._log(f"\n  📦  {active.name} stores {card.name} in arsenal.")
                # Mentors go face-up immediately when placed in arsenal
                if card.card_type == CardType.MENTOR:
                    active.mentor_face_up = True
                    self._log(f"    🎓 {card.name} is now face-up in arsenal.")

        # Return banished cards (intimidate cleanup)
        if opponent.banished:
            names = ", ".join(c.name for c in opponent.banished)
            opponent.hand.extend(opponent.banished)
            opponent.banished.clear()
            self._log(f"    ↩  Banished cards returned to {opponent.name}'s hand: {names}.")

        # Pitch zone to deck bottom
        self._rng.shuffle(active.pitch_zone)
        active.deck.extend(active.pitch_zone)
        active.pitch_zone.clear()
        active.resource_points = 0
        active.action_points = 0

        # Draw up
        hand_size_before = len(active.hand)
        active.draw_to_intellect()
        drawn = active.hand[hand_size_before:]
        drawn_str = f": {', '.join(c.name for c in drawn)}" if drawn else " (none)"
        self._log(f"\n  🔄  {active.name} draws to {active.intellect}{drawn_str}. "
                  f"Hand: {len(active.hand)}, Deck: {len(active.deck)}")

        # First turn: defender draws too
        if self._game.is_first_turn:
            opp_hand_size_before = len(opponent.hand)
            opponent.draw_to_intellect()
            opp_drawn = opponent.hand[opp_hand_size_before:]
            opp_drawn_str = f": {', '.join(c.name for c in opp_drawn)}" if opp_drawn else " (none)"
            self._log(f"  🔄  (First turn) {opponent.name} also draws to {opponent.intellect}{opp_drawn_str}.")

        # Switch turns
        self._game.is_first_turn = False
        self._game.switch_turn()
        self._begin_turn()

    # ──────────────────────────────────────────────────────────
    # Internal: instant-window handling
    # ──────────────────────────────────────────────────────────

    def _enter_instant_phase(self, return_phase: "Phase", return_agent_idx: int,
                              priority_idx: Optional[int] = None) -> None:
        """Open an instant window. Either player may play instants onto a LIFO
        stack, paying pitch costs as usual. When both players pass priority in
        succession with cards on the stack, the top resolves; when both pass
        with an empty stack, play returns to *return_phase* with
        *return_agent_idx* receiving control.

        By default the active turn player receives priority first. Callers can
        override via *priority_idx* — for attack reaction windows the defender
        reacts first, since the attack was declared against them.
        """
        self._instant_return_phase = return_phase
        self._instant_return_agent_idx = return_agent_idx
        self._instant_passes = 0
        if priority_idx is None:
            priority_idx = self._game.active_player_idx
        self._instant_priority_idx = priority_idx
        self._phase = Phase.INSTANT
        self.agent_selection = f"agent_{self._instant_priority_idx}"
        priority_name = self._game.players[self._instant_priority_idx].name

    def _exit_instant_phase(self) -> None:
        """Close the instant window and return to the phase that opened it.

        If the window was opened as an attack reaction (``return_phase ==
        Phase.DEFEND``) and an attack is still pending, the attack's
        ``ON_ATTACK`` triggered effects fire now — AFTER any instants that
        were played in reaction have already resolved off the stack. This is
        what makes Sigil of Solace (gain life) resolve before Wild Ride's
        draw-discard / intimidate effect.
        """
        return_phase = self._instant_return_phase
        return_agent_idx = self._instant_return_agent_idx
        self._instant_return_phase = None
        self._instant_passes = 0

        if return_phase == Phase.DEFEND and self._pending_attack is not None:
            attacker = self._game.active
            defender = self._game.players[1 - self._game.active_player_idx]
            # Fire the attack's ON_ATTACK triggered ability (non-weapon attacks
            # only — weapons don't have card-level ON_ATTACK effects here).
            if not self._pending_is_weapon:
                self._apply_card_effects(
                    self._pending_attack, EffectTrigger.ON_ATTACK, {},
                    attacker, defender,
                )
            self._phase = Phase.DEFEND
            self.agent_selection = f"agent_{return_agent_idx}"
            return

        self._phase = return_phase if return_phase is not None else Phase.ATTACK
        self.agent_selection = f"agent_{return_agent_idx}"

    def _push_instant_to_stack(self, card: "Card", owner_idx: int) -> None:
        """Place *card* onto the instant stack owned by *owner_idx*, then pass
        priority to the opponent so they may respond."""
        self._instant_stack.append((owner_idx, card))
        owner_name = self._game.players[owner_idx].name
        self._log(f"    📌  {owner_name} puts {card.name} on the stack "
                  f"(stack size: {len(self._instant_stack)}).")
        self._instant_passes = 0
        self._instant_priority_idx = 1 - owner_idx
        self.agent_selection = f"agent_{self._instant_priority_idx}"

    def _resolve_instant_from_stack(self, card: "Card", owner: "Player",
                                    opponent: "Player") -> None:
        """Resolve an instant that was previously placed on the stack.
        Unlike ``_resolve_instant``, this does not grant the owner an action
        point — the card is resolving outside their action phase."""
        n = card.name
        if n == "Sigil of Solace":
            owner.gain_life(1)
            self._log(f"    💚 Sigil of Solace resolves — {owner.name} gains 1 life "
                      f"({owner.life}).")
        elif n == "Titanium Bauble":
            owner.resource_points += 1
            self._log(f"    💰 Titanium Bauble resolves — {owner.name} gains 1 resource.")
        elif n == "Sharpen Steel":
            owner.next_weapon_power_bonus += 1
            self._log(f"    ⚡ Sharpen Steel resolves — next weapon attack gains +1 power.")
        elif n == "Flock of the Feather Walkers":
            self._log(f"    🦅 Flock of the Feather Walkers resolves.")
        else:
            self._log(f"    ✨ {n} resolves.")
        owner.graveyard.append(card)

    def _resolve_top_of_stack(self) -> None:
        """Pop and resolve the topmost instant on the stack (LIFO)."""
        owner_idx, card = self._instant_stack.pop()
        owner = self._game.players[owner_idx]
        opp = self._game.players[1 - owner_idx]
        self._resolve_instant_from_stack(card, owner, opp)

    def _handle_instant_action(self, action: Action, active: Player,
                                opponent: Player) -> None:
        """Priority-holder action inside an instant window."""
        if action.action_type == ActionType.PASS_PRIORITY:
            self._instant_passes += 1
            if self._instant_passes >= 2:
                if self._instant_stack:
                    # Both players passed with cards on the stack — resolve top.
                    self._resolve_top_of_stack()
                    self._instant_passes = 0
                    self._instant_priority_idx = self._game.active_player_idx
                    self.agent_selection = f"agent_{self._instant_priority_idx}"
                else:
                    # Both passed with empty stack — close the window.
                    self._exit_instant_phase()
            else:
                # Hand priority to the other player.
                self._instant_priority_idx = 1 - self._instant_priority_idx
                self.agent_selection = f"agent_{self._instant_priority_idx}"
            return

        if action.action_type == ActionType.PLAY_CARD:
            card = action.card
            if card is None or card not in active.hand:
                # Invalid selection — treat as a pass.
                self._log(f"  ⚠  Invalid instant selection; passing priority.")
                self._instant_passes += 1
                self._instant_priority_idx = 1 - self._instant_priority_idx
                self.agent_selection = f"agent_{self._instant_priority_idx}"
                return
            if card.card_type != CardType.INSTANT:
                self._log(f"  ⚠  {card.name} is not an instant; passing priority.")
                self._instant_passes += 1
                self._instant_priority_idx = 1 - self._instant_priority_idx
                self.agent_selection = f"agent_{self._instant_priority_idx}"
                return

            active.hand.remove(card)
            needed = max(0, card.cost - active.resource_points)
            if needed > 0:
                # Transition to PITCH to cover the instant's cost.
                self._pending_play_card = card
                self._pitched_this_play = []
                self._pending_instant_play = True
                self._pending_instant_player_idx = self._instant_priority_idx
                self._phase = Phase.PITCH
                self._log(f"\n    ▶  {active.name} plays {card} "
                          f"(needs {needed} more resource{'s' if needed != 1 else ''})")
                return

            # Free / already paid — push directly to the stack.
            active.resource_points -= card.cost
            self._log(f"\n    ▶  {active.name} plays {card}")
            self._push_instant_to_stack(card, self._instant_priority_idx)
            return

        # Unknown action in instant phase — treat as a pass to keep the game moving.
        self._instant_passes += 1
        self._instant_priority_idx = 1 - self._instant_priority_idx
        self.agent_selection = f"agent_{self._instant_priority_idx}"

    # ──────────────────────────────────────────────────────────
    # Internal: reaction-phase handling
    # ──────────────────────────────────────────────────────────

    def _enter_reaction_phase(self, attacker_idx: int, defender_idx: int) -> None:
        """Open the reaction window after the defender commits blocks.

        Attacker gets priority first and may play ATTACK_REACTION or INSTANT cards.
        When the attacker passes, priority shifts to the defender who may play
        DEFENSE_REACTION or INSTANT cards. When both players pass consecutively
        with an empty stack, combat resolves. Cards on the stack resolve LIFO,
        just like the instant window.
        """
        self._reaction_attacker_idx = attacker_idx
        self._reaction_priority_idx = attacker_idx
        self._reaction_passes = 0
        self._reaction_defense_bonus = 0
        self._instant_stack = []  # fresh stack for this reaction window
        self._phase = Phase.REACTION
        self.agent_selection = f"agent_{attacker_idx}"
        attacker_name = self._game.players[attacker_idx].name
        self._log(f"    ⚔  Reaction phase opens (priority: {attacker_name}).")

    def _exit_reaction_phase(self) -> None:
        """Close the reaction window and resolve combat."""
        self._log(f"    ▶  Reaction phase closes.")
        attacker_idx = self._reaction_attacker_idx
        defender_idx = 1 - attacker_idx
        attacker = self._game.players[attacker_idx]
        defender = self._game.players[defender_idx]
        committed = self._committed_defend_action
        self._committed_defend_action = None
        self._resolve_defend(committed, defender, attacker,
                             reaction_defense_bonus=self._reaction_defense_bonus)

    def _push_reaction_to_stack(self, card: "Card", owner_idx: int) -> None:
        """Place *card* on the reaction stack and pass priority to the opponent."""
        self._instant_stack.append((owner_idx, card))
        owner_name = self._game.players[owner_idx].name
        self._log(f"    📌  {owner_name} puts {card.name} on the stack "
                  f"(stack size: {len(self._instant_stack)}).")
        self._reaction_passes = 0
        self._reaction_priority_idx = 1 - owner_idx
        self.agent_selection = f"agent_{self._reaction_priority_idx}"

    def _resolve_top_of_reaction_stack(self) -> None:
        """Pop and resolve the topmost card on the reaction stack (LIFO)."""
        owner_idx, card = self._instant_stack.pop()
        owner = self._game.players[owner_idx]
        opp = self._game.players[1 - owner_idx]
        self._resolve_reaction_from_stack(card, owner, opp)

    def _resolve_reaction_from_stack(self, card: "Card", owner: "Player",
                                     opponent: "Player") -> None:
        """Resolve a card that was on the reaction stack."""
        if card.card_type == CardType.INSTANT:
            self._resolve_instant_from_stack(card, owner, opponent)
            return

        n = card.name
        if card.card_type == CardType.ATTACK_REACTION:
            for effect in card.effects:
                if effect.matches(EffectTrigger.ON_ATTACK_REACTION, {}):
                    if effect.action == EffectAction.ATTACK_POWER_BOOST:
                        self._pending_attack_power += effect.magnitude
                        self._log(f"    ⚔  {n} resolves — target attack gains "
                                  f"+{effect.magnitude} power "
                                  f"({self._pending_attack_power} total).")
                    elif effect.action == EffectAction.SWORD_ATTACK_GO_AGAIN:
                        if self._pending_attack is not None:
                            self._pending_attack.go_again = True
                        self._log(f"    ⚔  {n} resolves — target sword attack gains go again.")
            if n == "In the Swing":
                attacker = self._game.players[self._reaction_attacker_idx]
                if attacker.weapon_attack_count >= 2:
                    self._pending_attack_power += 3
                    self._log(f"    ⚔  In the Swing resolves — condition met, +3 power "
                              f"({self._pending_attack_power} total).")
                else:
                    self._log(f"    ⚔  In the Swing resolves — condition not met "
                              f"(fewer than 2 weapon attacks this turn).")
            elif n == "Ironsong Response":
                committed = self._committed_defend_action
                if committed and committed.defend_hand_indices:
                    self._pending_attack_power += 3
                    self._log(f"    ⚔  Ironsong Response resolves — Reprise! +3 power "
                              f"({self._pending_attack_power} total).")
                else:
                    self._log(f"    ⚔  Ironsong Response resolves — Reprise condition not met.")
            elif n == "Out for Blood":
                self._pending_attack_power += 2
                self._log(f"    ⚔  Out for Blood resolves — target weapon attack gains +2 power "
                          f"({self._pending_attack_power} total).")
                committed = self._committed_defend_action
                if committed and committed.defend_hand_indices:
                    owner.next_attack_go_again = True
                    self._log(f"    ⚔  Out for Blood Reprise — next attack this turn gains +1 power "
                              f"(tracked via go_again flag).")
            elif n == "Run Through":
                if self._pending_attack is not None:
                    self._pending_attack.go_again = True
                self._log(f"    ⚔  Run Through resolves — target sword attack gains go again.")
                owner.next_weapon_power_bonus += 2
                self._log(f"    ⚔  Run Through — next sword attack this turn gains +2 power.")
            else:
                self._log(f"    ⚔  {n} resolves.")

        elif card.card_type == CardType.DEFENSE_REACTION:
            bonus = card.defense
            self._reaction_defense_bonus += bonus
            self._log(f"    🛡  {n} resolves — +{bonus} defense "
                      f"({self._reaction_defense_bonus} total reaction defense).")

        owner.graveyard.append(card)

    def _handle_reaction_action(self, action: Action, active: Player,
                                opponent: Player) -> None:
        """Priority-holder action inside the reaction window."""
        active_idx = int(self.agent_selection[-1])
        is_attacker = active_idx == self._reaction_attacker_idx

        if action.action_type == ActionType.PASS_PRIORITY:
            self._reaction_passes += 1
            if self._reaction_passes >= 2:
                if self._instant_stack:
                    # Both passed with cards on the stack — resolve top.
                    self._resolve_top_of_reaction_stack()
                    self._reaction_passes = 0
                    # After resolution, attacker gets priority back.
                    self._reaction_priority_idx = self._reaction_attacker_idx
                    self.agent_selection = f"agent_{self._reaction_priority_idx}"
                else:
                    # Both passed with empty stack — resolve combat.
                    self._exit_reaction_phase()
            else:
                self._reaction_priority_idx = 1 - self._reaction_priority_idx
                self.agent_selection = f"agent_{self._reaction_priority_idx}"
            return

        if action.action_type == ActionType.PLAY_CARD:
            card = action.card
            in_hand = card is not None and card in active.hand
            in_arsenal = card is not None and card is active.arsenal
            if not in_hand and not in_arsenal:
                self._log(f"  ⚠  Invalid reaction card; passing priority.")
                self._reaction_passes += 1
                self._reaction_priority_idx = 1 - self._reaction_priority_idx
                self.agent_selection = f"agent_{self._reaction_priority_idx}"
                return

            valid = False
            if card.card_type == CardType.INSTANT:
                valid = True
            elif card.card_type == CardType.ATTACK_REACTION and is_attacker:
                valid = True
            elif card.card_type == CardType.DEFENSE_REACTION and not is_attacker:
                valid = True

            if not valid:
                self._log(f"  ⚠  {card.name} cannot be played here; passing priority.")
                self._reaction_passes += 1
                self._reaction_priority_idx = 1 - self._reaction_priority_idx
                self.agent_selection = f"agent_{self._reaction_priority_idx}"
                return

            if in_arsenal:
                active.arsenal = None
            else:
                active.hand.remove(card)
            needed = max(0, card.cost - active.resource_points)
            if needed > 0:
                self._pending_play_card = card
                self._pitched_this_play = []
                self._pending_reaction_play = True
                self._pending_reaction_player_idx = self._reaction_priority_idx
                self._phase = Phase.PITCH
                self._log(f"\n    ▶  {active.name} plays {card.name} "
                          f"(needs {needed} more resource{'s' if needed != 1 else ''})")
                return

            active.resource_points -= card.cost
            self._log(f"\n    ▶  {active.name} plays {card.name}")
            self._push_reaction_to_stack(card, active_idx)
            return

        # Unknown action — treat as pass.
        self._reaction_passes += 1
        self._reaction_priority_idx = 1 - self._reaction_priority_idx
        self.agent_selection = f"agent_{self._reaction_priority_idx}"

    # ──────────────────────────────────────────────────────────
    # Internal: turn management
    # ──────────────────────────────────────────────────────────

    def _begin_turn(self):
        active = self._game.active
        active.reset_turn_resources()

        self._log(f"\n{'═'*60}")
        self._log(f"  TURN {self._game.turn_number} — {active.name} ({active.hero_name})")
        self._log(f"{'═'*60}")
        self._log(f"  ♥  Life: {self._game.players[0].name}={self._game.players[0].life} "
                  f"| {self._game.players[1].name}={self._game.players[1].life}")
        self._log(f"  🃏  Hand: {', '.join(str(c) for c in active.hand)}")

        self._phase = Phase.ATTACK
        self.agent_selection = f"agent_{self._game.active_player_idx}"

    def _end_attack_phase(self, active: Player, opponent: Player):
        """Active player has passed — open an end-of-turn instant window, then arsenal."""
        self._enter_instant_phase(
            return_phase=Phase.ARSENAL,
            return_agent_idx=self._game.active_player_idx,
        )
        # agent_selection is set by _enter_instant_phase to the priority holder.

    def _trigger_defend_phase(self, attacker: Player, defender: Player):
        """Open an attack reaction instant window, then (after it closes) enter
        the DEFEND phase.

        The attack's ``ON_ATTACK`` triggered ability does NOT fire here — it
        fires when the reaction window closes (see ``_exit_instant_phase``).
        This lets the defender respond with instants that resolve BEFORE the
        attack's own effects (e.g., playing Sigil of Solace to gain life
        before Wild Ride's draw-discard triggers).
        """
        if self._pending_is_weapon:
            power = attacker.get_effective_weapon_power()
        else:
            power = self._pending_attack.power + attacker.next_brute_attack_bonus

        # Baseline power; ON_ATTACK effects fired at window close may further
        # modify _pending_attack_power (e.g., DRAW_DISCARD_POWER_BONUS adds +2).
        self._pending_attack_power = power
        self._pending_defend_indices = []
        self._pending_defend_equip_slots = []

        self._log(f"\n    ⚔  {attacker.name} attacks with "
                  f"{self._pending_attack.name} — {power} power")

        # Open an attack reaction INSTANT window. Defender reacts first since
        # the attack was declared against them, and either player may play
        # instants onto the stack. When both pass with an empty stack the
        # window closes, ON_ATTACK triggers fire, and the defender gets the
        # DEFEND step to choose blocks.
        defender_idx = 1 - self._game.active_player_idx
        self._enter_instant_phase(
            return_phase=Phase.DEFEND,
            return_agent_idx=defender_idx,
            priority_idx=defender_idx,
        )

    def _fire_effects(self, trigger: EffectTrigger, context: Dict[str, Any],
                      player: Player, opponent: Player) -> None:
        """Fire all active effects on *player* that match *trigger* and *context*.

        The environment calls this at every relevant game event (e.g. a discard).
        Effect resolution is fully generic — no card names are checked here.
        """
        for effect in player.active_effects:
            if not effect.matches(trigger, context):
                continue
            if effect.action == EffectAction.INTIMIDATE:
                if opponent.hand:
                    banished = self._rng.choice(opponent.hand)
                    opponent.hand.remove(banished)
                    opponent.banished.append(banished)
                    self._log(
                        f"    👁  {player.name} hero ability fires ({trigger.name}) — "
                        f"intimidate! {opponent.name} banishes {banished.name}."
                    )

    def _apply_card_effects(self, card: Card, trigger: EffectTrigger, context: Dict[str, Any],
                             active: Player, opponent: Player) -> None:
        """Fire all effects on *card* that match *trigger* and *context*."""
        for effect in card.effects:
            if not effect.matches(trigger, context):
                continue
            if effect.action == EffectAction.INTIMIDATE:
                if opponent.hand:
                    banished = self._rng.choice(opponent.hand)
                    opponent.hand.remove(banished)
                    opponent.banished.append(banished)
                    self._log(f"    👁  Intimidate! {opponent.name} banishes {banished.name}.")
            elif effect.action == EffectAction.WEAPON_ATTACK_POWER_BONUS:
                active.next_weapon_power_bonus += effect.magnitude
                self._log(f"    ⚡ {card.name} — weapon has already swung, next attack gains +{effect.magnitude} power.")
            elif effect.action in (EffectAction.DRAW_DISCARD_GO_AGAIN,
                                   EffectAction.DRAW_DISCARD_POWER_BONUS,
                                   EffectAction.DRAW_DISCARD_INTIMIDATE):
                active.draw(1)
                drawn = active.hand[-1] if active.hand else None
                if drawn:
                    self._log(f"    🎴 {card.name} — drew {drawn.name}.")
                if active.hand:
                    discarded = self._rng.choice(active.hand)
                    active.hand.remove(discarded)
                    active.graveyard.append(discarded)
                    self._log(f"    🎲 {card.name} — randomly discarded {discarded.name} "
                              f"(power: {discarded.power}).")
                    self._fire_effects(EffectTrigger.ON_DISCARD, {"card": discarded}, active, opponent)
                    if discarded.power >= 6:
                        if effect.action == EffectAction.DRAW_DISCARD_GO_AGAIN:
                            card.go_again = True
                            self._log(f"    ↩  {card.name} — discarded 6+ power card, gains go again!")
                        elif effect.action == EffectAction.DRAW_DISCARD_POWER_BONUS:
                            self._pending_attack_power += 2
                            self._log(f"    ⚡ {card.name} — discarded 6+ power card, +2 power! "
                                      f"({self._pending_attack_power} total)")
                        elif effect.action == EffectAction.DRAW_DISCARD_INTIMIDATE:
                            if opponent.hand:
                                banished = self._rng.choice(opponent.hand)
                                opponent.hand.remove(banished)
                                opponent.banished.append(banished)
                                self._log(f"    👁  {card.name} — discarded 6+ power card, intimidate! "
                                          f"{opponent.name} banishes {banished.name}.")

    def _resolve_equipment_activation(self, slot: str, active: Player):
        """Resolve an equipment activate ability (no action point cost, then destroy)."""
        eq = active.equipment.get(slot)
        if not eq or not eq.active or eq.destroyed:
            return
        name = eq.card.name
        if name == "Blossom of Spring":
            active.resource_points += 1
            eq.destroyed = True
            self._log(f"\n  ▶  {active.name} activates {name}.")
            self._log(f"    🌸 Blossom of Spring — gain 1 resource. Blossom of Spring is destroyed.")

    def _resolve_weapon_attack(self, attacker: Player, opponent: Player,
                               pre_pitched: Optional[List[Card]] = None):
        """Initiate a weapon attack → triggers defend phase.

        pre_pitched: cards already pitched during the PITCH phase; when provided
        the auto-pitch logic is skipped since resources are already covered.
        """
        weapon = attacker.weapon
        if not weapon:
            return

        weapon_cost = weapon.cost
        if pre_pitched is not None:
            # Pitching was handled interactively in the PITCH phase
            pitched = pre_pitched
        else:
            # No PITCH phase was used — auto-pitch to cover the cost (free or already covered)
            needed = max(0, weapon_cost - attacker.resource_points)
            pitched = []
            if needed > 0:
                pitchable = [c for c in attacker.hand if c.pitch > 0]
                total = 0
                for c in sorted(pitchable, key=lambda x: x.pitch, reverse=True):
                    if total >= needed:
                        break
                    pitched.append(c)
                    attacker.pitch(c)
                    total += c.pitch
        attacker.resource_points -= weapon_cost

        pitch_str = f" (pitched: {', '.join(c.name for c in pitched)})" if pitched else ""
        self._log(f"\n  ▶  {attacker.name} attacks with {weapon.name}{pitch_str}")

        attacker.action_points -= 1
        attacker.weapon_used_this_turn = True
        if attacker.weapon_additional_attack:
            attacker.weapon_additional_attack = False  # consume the one extra attack

        self._pending_attack = weapon
        self._pending_is_weapon = True
        self._trigger_defend_phase(attacker, opponent)

    # ──────────────────────────────────────────────────────────
    # Internal: card resolution (mirrors engine.py)
    # ──────────────────────────────────────────────────────────

    def _resolve_action(self, card: Card, active: Player, opponent: Player):
        n = card.name

        if n == "Barraging Beatdown":
            active.next_brute_attack_bonus = 3
            self._log(f"    ⚡ Next Brute attack gains conditional +3 power.")

        elif n == "Beast Mode":
            active.next_brute_attack_bonus = max(active.next_brute_attack_bonus, 3)
            self._log(f"    ⚡ Beast Mode — next Brute attack gains +3 power.")

        elif n == "Come to Fight":
            active.next_attack_go_again = True
            self._log(f"    ⚡ Come to Fight — next attack gains go again.")

        elif n == "En Garde":
            active.next_weapon_power_bonus += 3
            self._log(f"    ⚡ En Garde — weapon gets +3 power.")

        elif n == "Warrior's Valor":
            active.next_weapon_power_bonus += 3
            active.next_weapon_go_again = True
            self._log(f"    ⚡ Warrior's Valor — weapon gets +3 power and 'if hits, go again'.")

        elif n in ("On a Knife Edge", "Blade Flash", "Hit and Run"):
            active.next_weapon_go_again = True
            self._log(f"    ⚡ {n} — next weapon attack gains go again.")

        elif n == "Glistening Steelblade":
            active.next_weapon_go_again = True
            self._log(f"    ✨ Glistening Steelblade — next Dawnblade has go again + counter on hit.")

        elif n == "Slice and Dice":
            active.next_weapon_power_bonus += 1
            self._log(f"    ⚡ Slice and Dice — weapon attacks get +1/+2 power this turn.")

        elif n == "Visit the Blacksmith":
            active.next_weapon_power_bonus += 1
            self._log(f"    ⚡ Visit the Blacksmith — next sword attack gains +1 power.")

        elif n == "Sigil of Solace":
            active.gain_life(3)
            self._log(f"    💚 Sigil of Solace — {active.name} gains 3 life ({active.life}).")

        elif n == "Titanium Bauble":
            active.resource_points += 1
            self._log(f"    💰 Titanium Bauble — gain 1 resource ({active.resource_points} total).")

        # Fire ON_PLAY card effects (e.g. intimidate keyword on non-attack actions)
        self._apply_card_effects(card, EffectTrigger.ON_PLAY,
                                 {"weapon_attack_count": active.weapon_attack_count},
                                 active, opponent)

        if card.go_again:
            active.action_points += 1

        active.graveyard.append(card)

    def _resolve_instant(self, card: Card, active: Player, opponent: Player):
        n = card.name
        if n == "Sigil of Solace":
            active.gain_life(1)
            self._log(f"    💚 Sigil of Solace — {active.name} gains 1 life ({active.life}).")
        elif n == "Titanium Bauble":
            active.resource_points += 1
            self._log(f"    💰 Titanium Bauble — gain 1 resource.")
        elif n == "Flock of the Feather Walkers":
            self._log(f"    🦅 Flock of the Feather Walkers — +2 defense to a defending attack card.")
        elif n == "Sharpen Steel":
            active.next_weapon_power_bonus += 1
            self._log(f"    ⚡ Sharpen Steel — next weapon attack gains +1 power.")
        active.graveyard.append(card)
        active.action_points += 1  # instants don't consume action points

    def _on_hit(self, card: Card, attacker: Player, defender: Player, is_weapon: bool):
        if is_weapon and "Dawnblade" in card.name:
            if attacker.mentor_face_up:
                attacker.action_points += 1
                self._log(f"    🎓 Hala Goldenhelm! Sword hit — go again + lesson counter.")
                self._mentor_lesson(attacker)

        if card.name == "Raging Onslaught":
            attacker.draw(1)
            self._log(f"    🎴 Raging Onslaught hit — {attacker.name} draws a card.")

        if card.name == "Driving Blade":
            attacker.next_weapon_go_again = True
            self._log(f"    ⚡ Driving Blade hit — next weapon attack gains go again.")

    def _mentor_lesson(self, player: Player):
        player.mentor_lesson_counters += 1
        self._log(f"    🎓 Lesson counter on mentor ({player.mentor_lesson_counters}).")
        if player.mentor_lesson_counters >= 2:
            player.mentor_lesson_counters = 0
            player.mentor_face_up = False
            # Banish the mentor card
            if player.arsenal and player.arsenal.card_type == CardType.MENTOR:
                player.banished.append(player.arsenal)
                self._log(f"    🎓 {player.arsenal.name} banished.")
                player.arsenal = None
            if "Rhinar" in player.hero_name:
                target = next((c for c in player.deck if c.name == "Alpha Rampage"), None)
            else:
                target = next((c for c in player.deck if c.name == "Glistening Steelblade"), None)
            if target:
                player.deck.remove(target)
                player.arsenal = target
                self._rng.shuffle(player.deck)
                self._log(f"    🎓 Mentor fires! {target.name} placed face-up in arsenal.")

    # ──────────────────────────────────────────────────────────
    # Internal: utilities
    # ──────────────────────────────────────────────────────────

    def _snapshot_by_indices(self, hand: List[Card], indices: List[int]) -> List[Card]:
        """Return cards at given indices (safely, ignoring out-of-range)."""
        cards = []
        for i in sorted(set(indices)):
            if 0 <= i < len(hand):
                cards.append(hand[i])
        return cards

    def _get_obs(self) -> Dict[str, dict]:
        if self._game is None:
            return {a: {} for a in self.agents}
        p0, p1 = self._game.players
        # Expose the pending card only to the agent that is currently in the PITCH phase
        pending = self._pending_play_card if self._phase == Phase.PITCH else None
        active_idx = int(self.agent_selection[-1]) if self.agent_selection else 0
        return {
            "agent_0": build_observation(p0, p1, self._game,
                                         pending_card=pending if active_idx == 0 else None),
            "agent_1": build_observation(p1, p0, self._game,
                                         pending_card=pending if active_idx == 1 else None),
        }

    def _finalize(self):
        self.done = True
        winner = self._game.winner()
        self._log(f"\n{'★'*60}")
        if winner:
            loser = [p for p in self._game.players if p != winner][0]
            self._log(f"  🏆 GAME OVER! {winner.name} WINS! ({winner.life} life remaining)")
            self._log(f"  {loser.name} reduced to {loser.life} life.")
            winner_idx = self._game.players.index(winner)
            self._rewards[f"agent_{winner_idx}"] = 1.0
            self._rewards[f"agent_{1-winner_idx}"] = -1.0
        else:
            self._log(f"  ⏱  Draw after {self.MAX_TURNS} turns.")
            self._rewards = {"agent_0": 0.0, "agent_1": 0.0}
        self._log(f"{'★'*60}")

        self._terminations = {
            "agent_0": self._game.players[0].is_dead() or not self._game.is_over(),
            "agent_1": self._game.players[1].is_dead() or not self._game.is_over(),
        }
        # Truncation if turn limit hit
        if self._game.turn_number > self.MAX_TURNS:
            self._truncations = {"agent_0": True, "agent_1": True}
            self._terminations = {"agent_0": False, "agent_1": False}

    def _log(self, msg: str):
        if self.verbose:
            print(msg)
        if self._log_file:
            with open(self._log_file, "a", encoding="utf-8") as f:
                f.write(msg + "\n")
        if self._log_callback:
            self._log_callback(msg)

    # ──────────────────────────────────────────────────────────
    # Convenience: render
    # ──────────────────────────────────────────────────────────

    def render(self):
        if self._game is None:
            print("Game not started.")
            return
        p0, p1 = self._game.players
        print(f"\n── Turn {self._game.turn_number} | Phase: {self._phase.name} "
              f"| Acting: {self.agent_selection} ──")
        print(f"  {p0.name}: {p0.life} life | hand={len(p0.hand)} | "
              f"deck={len(p0.deck)} | weapon={p0.weapon.name if p0.weapon else 'none'}")
        print(f"  {p1.name}: {p1.life} life | hand={len(p1.hand)} | "
              f"deck={len(p1.deck)} | weapon={p1.weapon.name if p1.weapon else 'none'}")
        if self._pending_play_card:
            needed = max(0, self._pending_play_card.cost
                         - self._game.players[int(self.agent_selection[-1])].resource_points)
            print(f"  Pending play: {self._pending_play_card.name} (needs {needed} more resource(s))")
        if self._pending_attack:
            print(f"  Pending attack: {self._pending_attack.name} "
                  f"({self._pending_attack_power} power)")

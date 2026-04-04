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

from cards import Card, CardType, Color
from game_state import Player, GameState, Equipment
from actions import (
    Action, ActionType,
    legal_attack_actions, legal_defend_actions, legal_arsenal_actions,
)
from observations import build_observation, PLAYER_OBS_SIZE
from spaces import Discrete, Box, Dict as DictSpace


# ──────────────────────────────────────────────────────────────
# Phase enum
# ──────────────────────────────────────────────────────────────

class Phase(Enum):
    START   = auto()
    ATTACK  = auto()
    DEFEND  = auto()
    ARSENAL = auto()
    END     = auto()


# ──────────────────────────────────────────────────────────────
# FaBEnv
# ──────────────────────────────────────────────────────────────

class FaBEnv:
    """
    Two-agent AEC-style environment for FaB Classic Battles.

    Agents: ["agent_0", "agent_1"]
    agent_0 = player index 0, agent_1 = player index 1.
    """

    metadata = {"name": "fab_classic_battles_v0"}
    MAX_TURNS = 80

    def __init__(self, verbose: bool = False):
        self.verbose = verbose
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

        # Observation / action spaces (agent-specific but symmetric structure)
        obs_size = PLAYER_OBS_SIZE
        self.observation_spaces = {
            a: DictSpace({
                "agent":    Box(0.0, 1.0, shape=(obs_size,)),
                "opponent": Box(0.0, 1.0, shape=(obs_size,)),
                "global":   Box(0.0, 1.0, shape=(2,)),
            })
            for a in self.agents
        }
        # Action space upper bound; actual legal count varies. Agents MUST use legal_actions().
        self.action_spaces = {a: Discrete(256) for a in self.agents}

    # ──────────────────────────────────────────────────────────
    # reset
    # ──────────────────────────────────────────────────────────

    def reset(self, seed: Optional[int] = None) -> Tuple[Dict, Dict]:
        """
        Reset the environment and return initial observations.

        Returns:
            obs   — {agent_id: observation_dict}
            infos — {agent_id: {}}
        """
        if seed is not None:
            random.seed(seed)

        from main import make_rhinar, make_dorinthea
        p0 = make_rhinar()
        p1 = make_dorinthea()
        self._game = GameState(p0, p1)

        # Opening hands
        for p in self._game.players:
            p.draw_to_intellect()

        self._rewards = {"agent_0": 0.0, "agent_1": 0.0}
        self._terminations = {"agent_0": False, "agent_1": False}
        self._truncations = {"agent_0": False, "agent_1": False}
        self.done = False

        # Start with player 0's action phase
        self._begin_turn()

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
        if self._phase == Phase.ATTACK:
            self._handle_attack_action(action, active, opponent)
        elif self._phase == Phase.DEFEND:
            self._handle_defend_action(action, active, opponent)
        elif self._phase == Phase.ARSENAL:
            self._handle_arsenal_action(action, active, opponent)

        # ── Check game over ──
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

        if self._phase == Phase.ATTACK:
            return legal_attack_actions(active)
        elif self._phase == Phase.DEFEND:
            return legal_defend_actions(active, self._pending_attack_power)
        elif self._phase == Phase.ARSENAL:
            return legal_arsenal_actions(active)
        return []

    # ──────────────────────────────────────────────────────────
    # Internal: action handlers
    # ──────────────────────────────────────────────────────────

    def _handle_attack_action(self, action: Action, active: Player, opponent: Player):
        if action.action_type == ActionType.PASS:
            self._log(f"  ▶  {active.name} passes.")
            self._end_attack_phase(active, opponent)
            return

        # Pay pitches first
        for pi in action.pitch_indices:
            if 0 <= pi < len(active.hand):
                active.pitch(active.hand[pi])  # NOTE: pitching mutates hand, indices shift!
                # Rebuild pitch by name to avoid index drift — handled below

        # Re-resolve pitch by re-checking (pitch already consumed resource_points above)
        # Actually we need to be careful: pitch() removes from hand. Since we iterate indices
        # in ascending order and remove from hand each time, we must pitch in reverse order.
        # Reset and redo properly:
        # (The above loop has a bug with shifting indices — fixed implementation below)

        # Undo what we did and redo correctly
        # Actually the Action stores indices at decision time — let's snapshot the cards
        # by name before any mutation. We'll use a cleaner approach: snapshot the cards.
        pass  # see _pay_pitches below

    def _handle_attack_action(self, action: Action, active: Player, opponent: Player):
        """Correct implementation of attack action handler."""
        if action.action_type == ActionType.PASS:
            self._log(f"  ▶  {active.name} passes.")
            self._end_attack_phase(active, opponent)
            return

        if action.action_type == ActionType.WEAPON:
            self._resolve_weapon_attack(active, opponent)
            return

        if action.action_type == ActionType.PLAY_CARD:
            # Snapshot pitch cards by index (descending to avoid shift)
            pitch_cards = self._snapshot_by_indices(active.hand, action.pitch_indices)

            # Get the card being played
            if action.from_arsenal and active.arsenal and active.arsenal.card_type != CardType.MENTOR:
                card = active.arsenal
                active.arsenal = None
            elif 0 <= action.card_index < len(active.hand):
                card = active.hand[action.card_index]
                active.hand.remove(card)
            else:
                self._log(f"  ⚠  Invalid card index {action.card_index}, passing.")
                self._end_attack_phase(active, opponent)
                return

            # Pay pitches
            for pc in pitch_cards:
                if pc in active.hand:
                    active.pitch(pc)

            active.resource_points -= card.cost
            active.action_points -= 1
            self._log(f"\n  ▶  {active.name} plays {card}"
                      + (f" (pitched: {', '.join(c.name for c in pitch_cards)})"
                         if pitch_cards else ""))

            if card.card_type == CardType.INSTANT:
                self._resolve_instant(card, active, opponent)
            elif card.card_type == CardType.ACTION:
                self._resolve_action(card, active, opponent)
            elif card.card_type == CardType.ACTION_ATTACK:
                # Trigger defend phase before resolving
                self._pending_attack = card
                self._pending_is_weapon = False
                self._trigger_defend_phase(active, opponent)
                return  # defend phase takes over; resolve_attack called after defend

        # After instant/action: if we still have action points, stay in ATTACK phase;
        # otherwise end turn.
        if active.action_points <= 0:
            self._end_attack_phase(active, opponent)

    def _handle_defend_action(self, action: Action, defender: Player, attacker: Player):
        """Defender has chosen how to block the pending attack."""
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

        # Compute power — use the value already set by _trigger_defend_phase
        # (which includes any "when this attacks" bonuses, e.g. Bare Fangs +2)
        if is_weapon:
            power = attacker.get_effective_weapon_power()
            attacker.next_weapon_power_bonus = 0
        else:
            power = self._pending_attack_power
            attacker.next_brute_attack_bonus = 0

        total_def = sum(c.defense for c in def_cards) + sum(e.defense for e in def_equip)

        if def_cards or def_equip:
            names = [c.name for c in def_cards] + [e.card.name for e in def_equip]
            self._log(f"    🛡  {defender.name} defends: {', '.join(names)} (def: {total_def})")
        else:
            self._log(f"    🛡  {defender.name} does not defend.")

        # Damage
        damage = max(0, power - total_def)
        hit = damage > 0
        if hit:
            defender.take_damage(damage, card.name)
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

        # Mentor check for Rhinar
        if not is_weapon and card.power >= 6 and attacker.mentor_face_up:
            self._mentor_lesson(attacker)

        self._pending_attack = None
        self._pending_is_weapon = False

        # Return to attacker's action phase
        self._phase = Phase.ATTACK
        self.agent_selection = f"agent_{self._game.active_player_idx}"

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
            opponent.hand.extend(opponent.banished)
            opponent.banished.clear()

        # Pitch zone to deck bottom
        random.shuffle(active.pitch_zone)
        active.deck.extend(active.pitch_zone)
        active.pitch_zone.clear()
        active.resource_points = 0
        active.action_points = 0

        # Draw up
        active.draw_to_intellect()
        self._log(f"\n  🔄  {active.name} draws to {active.intellect}. "
                  f"Hand: {len(active.hand)}, Deck: {len(active.deck)}")

        # First turn: defender draws too
        if self._game.is_first_turn:
            opponent.draw_to_intellect()
            self._log(f"  🔄  (First turn) {opponent.name} also draws to {opponent.intellect}.")

        # Switch turns
        self._game.is_first_turn = False
        self._game.switch_turn()
        self._begin_turn()

    # ──────────────────────────────────────────────────────────
    # Internal: turn management
    # ──────────────────────────────────────────────────────────

    def _begin_turn(self):
        active = self._game.active
        active.reset_turn_resources()

        # Dorinthea hero ability
        if "Dorinthea" in active.hero_name and active.weapon:
            active.next_weapon_go_again = True
            self._log(f"  ✨ Dorinthea's ability — Dawnblade gains go again this turn.")

        self._log(f"\n{'═'*60}")
        self._log(f"  TURN {self._game.turn_number} — {active.name} ({active.hero_name})")
        self._log(f"{'═'*60}")
        self._log(f"  ♥  Life: {self._game.players[0].name}={self._game.players[0].life} "
                  f"| {self._game.players[1].name}={self._game.players[1].life}")
        self._log(f"  🃏  Hand: {', '.join(str(c) for c in active.hand)}")

        self._phase = Phase.ATTACK
        self.agent_selection = f"agent_{self._game.active_player_idx}"

    def _end_attack_phase(self, active: Player, opponent: Player):
        """Active player has passed — move to arsenal phase."""
        self._phase = Phase.ARSENAL
        # agent_selection stays as the active player for the arsenal decision

    def _trigger_defend_phase(self, attacker: Player, defender: Player):
        """Set up the defend phase so the defender can choose blocks."""
        if self._pending_is_weapon:
            power = attacker.get_effective_weapon_power()
        else:
            power = self._pending_attack.power + attacker.next_brute_attack_bonus

        # "When this attacks" effects fire here, before the defend step
        if not self._pending_is_weapon:
            power = self._on_attack(self._pending_attack, attacker, defender, power)

        # Intimidate fires before defend
        if self._pending_attack.intimidate and len(defender.hand) > 0:
            banished = random.choice(defender.hand)
            defender.hand.remove(banished)
            defender.banished.append(banished)
            self._log(f"    👁  Intimidate! {defender.name} banishes {banished.name}.")

        self._pending_attack_power = power
        defender_idx = 1 - self._game.active_player_idx
        self._phase = Phase.DEFEND
        self.agent_selection = f"agent_{defender_idx}"
        self._log(f"\n    ⚔  {attacker.name} attacks with {self._pending_attack.name} — {power} power")

    def _on_attack(self, card, attacker: Player, defender: Player, power: int) -> int:
        """
        Resolve 'when this attacks' effects. Returns (possibly modified) power.
        These fire at the attack step, before the defender chooses blocks.
        """
        n = card.name

        # Wild Ride, Bare Fangs, Wrecking Ball — draw a card then discard a random card.
        if n in ("Wild Ride", "Bare Fangs", "Wrecking Ball"):
            attacker.draw(1)
            drawn = attacker.hand[-1] if attacker.hand else None
            if drawn:
                self._log(f"    🎴 {n} — drew {drawn.name}.")

            if attacker.hand:
                discarded = random.choice(attacker.hand)
                attacker.hand.remove(discarded)
                attacker.graveyard.append(discarded)
                self._log(f"    🎲 {n} — randomly discarded {discarded.name} "
                          f"(power: {discarded.power}).")

                strong = discarded.power >= 6
                if n == "Wild Ride" and strong:
                    card.go_again = True
                    self._log(f"    ↩  Wild Ride — discarded card has 6+ power, gains go again!")
                elif n == "Bare Fangs" and strong:
                    power += 2
                    self._pending_attack_power = power
                    self._log(f"    ⚡ Bare Fangs — discarded 6+ power card, +2 power! ({power} total)")
                elif n == "Wrecking Ball" and strong:
                    card.intimidate = True
                    self._log(f"    👁  Wrecking Ball — discarded 6+ power card, gains intimidate!")

        return power

    def _resolve_weapon_attack(self, attacker: Player, opponent: Player):
        """Initiate a weapon attack → triggers defend phase."""
        weapon = attacker.weapon
        if not weapon:
            return

        weapon_cost = 1
        pitched = []
        if weapon_cost > 0:
            pitchable = [c for c in attacker.hand if c.pitch > 0]
            total = 0
            for c in sorted(pitchable, key=lambda x: x.pitch, reverse=True):
                if total >= weapon_cost:
                    break
                pitched.append(c)
                attacker.pitch(c)
                total += c.pitch

        pitch_str = f" (pitched: {', '.join(c.name for c in pitched)})" if pitched else ""
        self._log(f"\n  ▶  {attacker.name} attacks with {weapon.name}{pitch_str}")

        attacker.action_points -= 1
        attacker.weapon_used_this_turn = True

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
            if len(opponent.hand) > 0:
                banished = random.choice(opponent.hand)
                opponent.hand.remove(banished)
                opponent.banished.append(banished)
                self._log(f"    👁  Barraging Beatdown — Intimidate! {opponent.name} banishes {banished.name}.")
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
            active.next_weapon_power_bonus += 2
            active.next_weapon_go_again = True
            self._log(f"    ⚡ Warrior's Valor — weapon gets +2 power and 'if hits, go again'.")

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

        if card.go_again:
            active.action_points += 1

        active.graveyard.append(card)

    def _resolve_instant(self, card: Card, active: Player, opponent: Player):
        n = card.name
        if n == "Sigil of Solace":
            active.gain_life(3)
            self._log(f"    💚 Sigil of Solace — {active.name} gains 3 life ({active.life}).")
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
        if is_weapon and "Bone Basher" in card.name:
            if len(defender.hand) > 0:
                banished = random.choice(defender.hand)
                defender.hand.remove(banished)
                defender.banished.append(banished)
                self._log(f"    👁  Bone Basher hit — Intimidate! {defender.name} banishes {banished.name}.")

        if is_weapon and "Dawnblade" in card.name:
            attacker.dawnblade_counters += 1
            self._log(f"    ✨ Dawnblade hits! +1 power counter ({attacker.dawnblade_counters} total).")
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
                random.shuffle(player.deck)
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
        return {
            "agent_0": build_observation(p0, p1, self._game),
            "agent_1": build_observation(p1, p0, self._game),
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
        if self._pending_attack:
            print(f"  Pending attack: {self._pending_attack.name} "
                  f"({self._pending_attack_power} power)")

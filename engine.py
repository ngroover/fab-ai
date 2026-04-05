"""
Game engine for Classic Battles: Rhinar vs Dorinthea.
Both heroes have 20 life (young/Blitz format).
"""

import random
from typing import List, Optional, Tuple
from cards import Card, CardType, Color
from game_state import Player, GameState, Equipment
from ai import (rhinar_choose_action, rhinar_choose_defense, rhinar_choose_arsenal,
                dorinthea_choose_action, dorinthea_choose_defense, dorinthea_choose_arsenal,
                pitch_for_cost)


class FaBEngine:
    def __init__(self, game: GameState, verbose=True):
        self.game = game
        self.verbose = verbose

    def log(self, msg: str):
        if self.verbose:
            print(msg)

    def get_ai(self, player: Player):
        if "Rhinar" in player.hero_name:
            return rhinar_choose_action, rhinar_choose_defense, rhinar_choose_arsenal
        else:
            return dorinthea_choose_action, dorinthea_choose_defense, dorinthea_choose_arsenal

    # ─── COMBAT ───

    def resolve_attack(self, attacker: Player, defender: Player, card: Card,
                       is_weapon: bool = False) -> bool:
        """Resolve a single combat chain link. Returns True if attack hit."""
        # Compute effective power
        if is_weapon:
            power = attacker.get_effective_weapon_power()
        else:
            power = card.power + attacker.next_brute_attack_bonus
            attacker.next_brute_attack_bonus = 0

        self.log(f"\n    ⚔  Chain link {attacker.attacks_this_turn + 1}: "
                 f"{'[WEAPON] ' if is_weapon else ''}{card.name} — {power} power")

        # Intimidate on attack
        if card.intimidate and len(defender.hand) > 0:
            banished = random.choice(defender.hand)
            defender.hand.remove(banished)
            defender.banished.append(banished)
            self.log(f"    👁  Intimidate! {defender.name} banishes {banished.name}.")

        # Rhinar hero ability: Bone Basher hits trigger intimidate
        if is_weapon and "Bone Basher" in card.name:
            pass  # handled in on-hit below

        # Defender chooses blocks
        _, choose_def, _ = self.get_ai(defender)
        def_cards, def_equip = choose_def(defender, power)

        for c in def_cards:
            if c in defender.hand:
                defender.hand.remove(c)

        total_def = sum(c.defense for c in def_cards) + sum(e.defense for e in def_equip)

        if def_cards or def_equip:
            names = [c.name for c in def_cards] + [e.card.name for e in def_equip]
            self.log(f"    🛡  {defender.name} defends: {', '.join(names)} (def: {total_def})")
        else:
            self.log(f"    🛡  {defender.name} does not defend.")

        # Damage
        damage = max(0, power - total_def)
        hit = damage > 0

        if hit:
            defender.take_damage(damage, card.name)
        else:
            self.log(f"    ✅  Fully blocked!")

        # Battleworn equipment
        for eq in def_equip:
            if "Battleworn" in eq.card.text and not eq.destroyed:
                eq.destroyed = True
                self.log(f"    🔨 {eq.card.name} destroyed (Battleworn).")

        # Defending cards go to graveyard
        for c in def_cards:
            defender.graveyard.append(c)

        # On-hit effects
        if hit:
            self._on_hit(card, attacker, defender, is_weapon)

        # Attack card to graveyard (if not weapon)
        if not is_weapon:
            attacker.graveyard.append(card)

        attacker.attacks_this_turn += 1
        if is_weapon:
            attacker.weapon_attack_count += 1

        # Go again from weapon with buffed flag
        go = card.go_again
        if is_weapon and attacker.next_weapon_go_again:
            go = True
            attacker.next_weapon_go_again = False
        if go:
            attacker.action_points += 1
            self.log(f"    ↩  Go again! {attacker.name} gains 1 action point.")

        # Check mentor lesson counter for Rhinar (Chief Ruk'utan)
        if not is_weapon and card.power >= 6 and attacker.mentor_face_up:
            self._mentor_lesson(attacker)

        return hit

    def _on_hit(self, card: Card, attacker: Player, defender: Player, is_weapon: bool):
        if "Bone Basher" in (card.name if is_weapon else ""):
            # Bone Basher on-hit: intimidate
            if len(defender.hand) > 0:
                banished = random.choice(defender.hand)
                defender.hand.remove(banished)
                defender.banished.append(banished)
                self.log(f"    👁  Bone Basher hit — Intimidate! {defender.name} banishes {banished.name}.")

        if "Dawnblade" in (card.name if is_weapon else ""):
            # Dawnblade on-hit: +1 power counter
            attacker.dawnblade_counters += 1
            self.log(f"    ✨ Dawnblade hits! +1 power counter ({attacker.dawnblade_counters} total).")
            # Hala Goldenhelm: on sword hit gain go again + lesson counter
            if attacker.mentor_face_up:
                attacker.action_points += 1
                self.log(f"    🎓 Hala Goldenhelm! Sword hit — go again + lesson counter.")
                self._mentor_lesson(attacker)

        if card.name == "Raging Onslaught":
            attacker.draw(1)
            self.log(f"    🎴 Raging Onslaught hit — {attacker.name} draws a card.")

        if card.name == "Sigil of Solace":
            attacker.gain_life(3)
            self.log(f"    💚 Sigil of Solace — {attacker.name} gains 3 life ({attacker.life}).")

        if card.name in ("Driving Blade",):
            attacker.next_weapon_go_again = True
            self.log(f"    ⚡ Driving Blade hit — next weapon attack gains go again.")

        # Reprise effects on attack actions
        if card.name in ("Second Swing", "Out for Blood", "Run Through", "Ironsong Response"):
            # These trigger if defender defended with a card from hand (simplify: if they defended)
            # Already handled card removal so we check based on def_cards being nonempty via hit
            pass  # Reprise handled inline in the future; simplified for now

    def _mentor_lesson(self, player: Player):
        player.mentor_lesson_counters += 1
        self.log(f"    🎓 Lesson counter on mentor ({player.mentor_lesson_counters}).")
        if player.mentor_lesson_counters >= 2:
            player.mentor_face_up = False
            player.mentor_lesson_counters = 0
            # Search deck for the payoff card
            if "Rhinar" in player.hero_name:
                # Find Alpha Rampage in deck
                target = next((c for c in player.deck if c.name == "Alpha Rampage"), None)
                if target:
                    player.deck.remove(target)
                    player.arsenal = target
                    random.shuffle(player.deck)
                    self.log(f"    🎓 Chief Ruk'utan fires! Alpha Rampage placed face-up in arsenal.")
            else:
                # Find Glistening Steelblade in deck
                target = next((c for c in player.deck if c.name == "Glistening Steelblade"), None)
                if target:
                    player.deck.remove(target)
                    player.arsenal = target
                    random.shuffle(player.deck)
                    self.log(f"    🎓 Hala Goldenhelm fires! Glistening Steelblade placed face-up in arsenal.")

    def try_weapon_attack(self, attacker: Player, defender: Player) -> bool:
        if not attacker.weapon:
            return False
        if attacker.action_points < 1:
            return False

        # Bone Basher: once per turn (no go again mechanic to re-trigger)
        # Dawnblade: can swing multiple times if given go again (Dorinthea's core mechanic)
        is_dawnblade = "Dawnblade" in attacker.weapon.name
        if not is_dawnblade and attacker.weapon_used_this_turn:
            return False
        # For Dawnblade: can only re-swing if we have go again queued up
        if is_dawnblade and attacker.weapon_used_this_turn and not attacker.next_weapon_go_again:
            return False

        # Both Bone Basher and Dawnblade Resplendent cost 1 to attack
        weapon_cost = 1

        if not _can_afford_resource(attacker, weapon_cost):
            return False

        to_pitch = pitch_for_cost(attacker, weapon_cost)
        for c in to_pitch:
            attacker.pitch(c)
        attacker.resource_points -= weapon_cost

        attacker.action_points -= 1
        attacker.weapon_used_this_turn = True

        self.log(f"\n    🗡  {attacker.name} attacks with {attacker.weapon.name} "
                 f"({attacker.get_effective_weapon_power()} power)!")

        # Reset weapon bonus after reading it
        attacker.next_weapon_power_bonus = 0

        hit = self.resolve_attack(attacker, defender, attacker.weapon, is_weapon=True)
        return True

    # ─── NON-ATTACK ACTIONS ───

    def resolve_action(self, card: Card, attacker: Player, defender: Player):
        """Resolve a non-attack action card's effect."""
        n = card.name

        if n == "Barraging Beatdown":
            # Intimidate + next Brute attack gets conditional +3
            attacker.next_brute_attack_bonus = 3
            if len(defender.hand) > 0:
                banished = random.choice(defender.hand)
                defender.hand.remove(banished)
                defender.banished.append(banished)
                self.log(f"    👁  Barraging Beatdown — Intimidate! {defender.name} banishes {banished.name}.")
            self.log(f"    ⚡ Next Brute attack gains conditional +3 power.")

        elif n == "Beast Mode":
            attacker.next_brute_attack_bonus = max(attacker.next_brute_attack_bonus, 3)
            self.log(f"    ⚡ Beast Mode — next Brute attack gains +3 power.")

        elif n == "Come to Fight":
            attacker.next_attack_go_again = True
            self.log(f"    ⚡ Come to Fight — next attack gains go again.")

        elif n in ("En Garde", "Warrior's Valor"):
            attacker.next_weapon_power_bonus += 3 if n == "En Garde" else 2
            attacker.next_weapon_go_again = True if n == "Warrior's Valor" else attacker.next_weapon_go_again
            self.log(f"    ⚡ {n} — weapon gets +{3 if n == 'En Garde' else 2} power"
                     + (" and 'if hits, go again'" if n == "Warrior's Valor" else "") + ".")

        elif n in ("On a Knife Edge", "Blade Flash", "Hit and Run"):
            attacker.next_weapon_go_again = True
            self.log(f"    ⚡ {n} — next weapon attack gains go again.")

        elif n == "Glistening Steelblade":
            attacker.next_weapon_go_again = True
            self.log(f"    ✨ Glistening Steelblade — next Dawnblade attack has go again + counter on hit.")

        elif n == "Slice and Dice":
            # +1 on first weapon, +2 on second weapon this turn
            attacker.next_weapon_power_bonus += 1  # simplified: always give at least +1
            self.log(f"    ⚡ Slice and Dice — weapon attacks get +1/+2 power this turn.")

        elif n == "Visit the Blacksmith":
            attacker.next_weapon_power_bonus += 1
            self.log(f"    ⚡ Visit the Blacksmith — next sword attack gains +1 power.")

        elif n == "Sigil of Solace":
            attacker.gain_life(3)
            self.log(f"    💚 Sigil of Solace — {attacker.name} gains 3 life ({attacker.life}).")

        elif n == "Titanium Bauble":
            attacker.resource_points += 1
            self.log(f"    💰 Titanium Bauble — gain 1 resource ({attacker.resource_points} total).")

        # Go again
        if card.go_again:
            attacker.action_points += 1

        attacker.graveyard.append(card)

    def resolve_instant(self, card: Card, attacker: Player, defender: Player):
        n = card.name
        if n == "Sigil of Solace":
            attacker.gain_life(3)
            self.log(f"    💚 Sigil of Solace — {attacker.name} gains 3 life ({attacker.life}).")
        elif n == "Titanium Bauble":
            attacker.resource_points += 1
            self.log(f"    💰 Titanium Bauble — gain 1 resource.")
        elif n == "Flock of the Feather Walkers":
            self.log(f"    🦅 Flock of the Feather Walkers — +2 defense to a defending attack card.")
        elif n == "Sharpen Steel":
            attacker.next_weapon_power_bonus += 1
            self.log(f"    ⚡ Sharpen Steel — next weapon attack gains +1 power.")
        attacker.graveyard.append(card)
        # Instants don't cost action points — refund the one we spent
        attacker.action_points += 1

    # ─── MENTOR ───

    def handle_mentor(self, player: Player):
        """At start of turn: try to turn mentor face up if in arsenal."""
        if player.arsenal and player.arsenal.card_type == CardType.MENTOR:
            if not player.mentor_face_up:
                player.mentor_face_up = True
                self.log(f"    🎓 {player.name} turns {player.arsenal.name} face-up in arsenal.")

    # ─── TURN ───

    def play_turn(self, attacker: Player, defender: Player):
        self.log(f"\n{'═'*60}")
        self.log(f"  TURN {self.game.turn_number} — {attacker.name} ({attacker.hero_name})")
        self.log(f"{'═'*60}")

        # START PHASE
        attacker.reset_turn_resources()

        # Handle mentor face-up
        if attacker.arsenal and attacker.arsenal.card_type == CardType.MENTOR:
            self.handle_mentor(attacker)

        # Blossom of Spring: activate at start of turn (once per game — gain 1 resource, then destroy)
        blossom = attacker.equipment.get("head")
        if blossom and not blossom.destroyed and "Blossom of Spring" in blossom.card.name:
            attacker.resource_points += 1
            blossom.destroyed = True
            self.log(f"  🌸 Blossom of Spring — gain 1 resource. Blossom of Spring is destroyed.")

        # Dorinthea hero ability: Dawnblade gains go again at start of action phase
        if "Dorinthea" in attacker.hero_name and attacker.weapon:
            attacker.next_weapon_go_again = True
            self.log(f"  ✨ Dorinthea's ability — Dawnblade gains go again this turn.")

        self.log(f"  ♥  Life: {attacker.name}={attacker.life} | {defender.name}={defender.life}")
        self.log(f"  🃏  Hand ({len(attacker.hand)}): {', '.join(str(c) for c in attacker.hand)}")
        if attacker.arsenal and attacker.arsenal.card_type != CardType.MENTOR:
            self.log(f"  📦  Arsenal: {attacker.arsenal.name}")

        choose_action, _, _ = self.get_ai(attacker)

        # ACTION PHASE
        while attacker.action_points > 0 and not self.game.is_over():
            action = choose_action(attacker, defender)

            if action:
                card, to_pitch = action
                from_arsenal = (card == attacker.arsenal)

                # Pay resource costs
                for pc in to_pitch:
                    if pc in attacker.hand:
                        attacker.pitch(pc)
                attacker.resource_points -= card.cost

                # Spend action point
                attacker.action_points -= 1

                if from_arsenal:
                    attacker.arsenal = None
                elif card in attacker.hand:
                    attacker.hand.remove(card)

                self.log(f"\n  ▶  {attacker.name} plays {card}"
                         + (f" (pitched: {', '.join(c.name for c in to_pitch)})" if to_pitch else ""))

                if card.card_type == CardType.INSTANT:
                    self.resolve_instant(card, attacker, defender)

                elif card.card_type == CardType.ACTION:
                    self.resolve_action(card, attacker, defender)

                elif card.card_type == CardType.ACTION_ATTACK:
                    self.resolve_attack(attacker, defender, card)
                    if self.game.is_over():
                        break

                    # After go again: check weapon
                    if attacker.action_points > 0 and not attacker.weapon_used_this_turn:
                        if _should_use_weapon(attacker):
                            self.try_weapon_attack(attacker, defender)
                            if self.game.is_over():
                                break

            else:
                # No more hand actions — try weapon
                if not attacker.weapon_used_this_turn:
                    used = self.try_weapon_attack(attacker, defender)
                    if self.game.is_over():
                        break
                    if not used:
                        break
                elif attacker.next_weapon_go_again and not attacker.weapon_used_this_turn:
                    # Shouldn't reach here but safety check
                    break
                else:
                    break

        # Try weapon if not used yet and we still have action points
        if not attacker.weapon_used_this_turn and not self.game.is_over():
            self.try_weapon_attack(attacker, defender)

        if self.game.is_over():
            return

        # END PHASE
        # Arsenal a card if empty (and no mentor already there)
        _, _, choose_arsenal = self.get_ai(attacker)
        if not attacker.arsenal and attacker.hand:
            to_store = choose_arsenal(attacker)
            if to_store and to_store in attacker.hand:
                attacker.hand.remove(to_store)
                attacker.arsenal = to_store
                self.log(f"\n  📦  {attacker.name} stores {to_store.name} in arsenal.")

        # Return banished cards to hand (intimidate cleanup)
        if defender.banished:
            defender.hand.extend(defender.banished)
            defender.banished.clear()

        # Pitch zone to bottom of deck
        random.shuffle(attacker.pitch_zone)
        attacker.deck.extend(attacker.pitch_zone)
        attacker.pitch_zone.clear()
        attacker.resource_points = 0
        attacker.action_points = 0

        # Draw up
        attacker.draw_to_intellect()
        self.log(f"\n  🔄  {attacker.name} draws to {attacker.intellect}. "
                 f"Hand: {len(attacker.hand)}, Deck: {len(attacker.deck)}")

        # First turn: defender also draws
        if self.game.is_first_turn:
            defender.draw_to_intellect()
            self.log(f"  🔄  (First turn) {defender.name} also draws to {defender.intellect}.")

    # ─── MAIN LOOP ───

    def run_game(self) -> Optional[Player]:
        self.log(f"\n{'★'*60}")
        self.log(f"  CLASSIC BATTLES: RHINAR VS DORINTHEA")
        self.log(f"  {self.game.players[0].name} vs {self.game.players[1].name}")
        self.log(f"  (Blitz format — 20 life each)")
        self.log(f"{'★'*60}")

        # Place mentors in arsenal at game start
        for player in self.game.players:
            mentor = next((c for c in player.deck if c.card_type == CardType.MENTOR), None)
            if mentor:
                player.deck.remove(mentor)
                player.arsenal = mentor
                self.log(f"  {player.name} starts with {mentor.name} in arsenal.")

        # Both players draw opening hands
        for p in self.game.players:
            p.draw_to_intellect()
            self.log(f"  {p.name} draws opening hand of {len(p.hand)} cards.")

        max_turns = 80

        while not self.game.is_over() and self.game.turn_number <= max_turns:
            active = self.game.active
            defending = self.game.defending
            self.play_turn(active, defending)
            if self.game.is_over():
                break
            self.game.is_first_turn = False
            self.game.switch_turn()

        self.log(f"\n{'★'*60}")
        if self.game.is_over():
            winner = self.game.winner()
            loser = [p for p in self.game.players if p != winner][0]
            self.log(f"  🏆 GAME OVER! {winner.name} ({winner.hero_name}) WINS!")
            self.log(f"  {winner.name}: {winner.life} life remaining.")
            self.log(f"  {loser.name}: reduced to {loser.life} life.")
        else:
            p0, p1 = self.game.players
            self.log(f"  ⏱  Draw after {max_turns} turns. "
                     f"{p0.name}: {p0.life} | {p1.name}: {p1.life}")
        self.log(f"{'★'*60}")

        return self.game.winner()


def _can_afford_resource(player: Player, cost: int) -> bool:
    available = player.resource_points + sum(c.pitch for c in player.hand)
    return available >= cost


def _should_use_weapon(player: Player) -> bool:
    if not player.weapon:
        return False
    is_dawnblade = "Dawnblade" in player.weapon.name
    cost = 1
    # Dawnblade can swing again if go again is queued, even if already used
    if is_dawnblade and player.weapon_used_this_turn and not player.next_weapon_go_again:
        return False
    if not is_dawnblade and player.weapon_used_this_turn:
        return False
    return _can_afford_resource(player, cost)

"""
Unit tests for the terminal win/draw phases.

Covers:
  - GameState.winner_index() — who, if anyone, is the sole survivor.
  - The turn counter living in GameState and advancing with each turn.
  - FaBEnv._finalize() resolving to exactly one of three terminal phases:
        Phase.PLAYER1_WIN — player 2 (agent_1) at ≤ 0 life, player 1 survives.
        Phase.PLAYER2_WIN — player 1 (agent_0) at ≤ 0 life, player 2 survives.
        Phase.DRAW        — both dead at once, or the turn limit was reached.
  - The DRAW_TURN_LIMIT constant ending undecided games in a draw.
  - End-to-end: a lethal attack drives the env into Phase.PLAYER1_WIN, and a
    step past the turn limit drives it into Phase.DRAW.

Player numbering: "player 1" is index 0 (agent_0), "player 2" is index 1
(agent_1), matching GameState(player1, player2).
"""

import unittest

from fab_env import FaBEnv, Phase
from actions import Action, ActionType
from cards import build_rhinar_deck, build_dorinthea_deck
from game_state import Player, GameState


def _fresh_env(seed=0):
    """Reset an env and return it (still in CHOOSE_FIRST)."""
    env = FaBEnv(verbose=False)
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=seed)
    return env


def _attack_env(seed=14):
    """Reset at *seed* and resolve CHOOSE_FIRST so Rhinar (player 1) goes first.

    Seed 14 gives Rhinar the coin flip and an opening hand containing Wounded
    Bull (a 6-power Brute attack). Returns (env, rhinar, dorinthea).
    """
    env = FaBEnv(verbose=False)
    env.reset(build_rhinar_deck(), build_dorinthea_deck(), seed=seed)
    env.step(next(a for a in env.legal_actions()
                  if a.action_type == ActionType.GO_FIRST))
    return env, env._game.players[0], env._game.players[1]


# ──────────────────────────────────────────────────────────────
# GameState helpers
# ──────────────────────────────────────────────────────────────

class TestWinnerIndex(unittest.TestCase):
    """GameState.winner_index() reports the sole survivor, or None."""

    def setUp(self):
        self.game = _fresh_env(seed=1)._game

    def test_none_when_both_alive(self):
        self.game.players[0].life = 5
        self.game.players[1].life = 3
        self.assertIsNone(self.game.winner_index())
        self.assertFalse(self.game.is_over())

    def test_player1_wins_when_player2_dead(self):
        self.game.players[0].life = 7
        self.game.players[1].life = 0
        self.assertEqual(self.game.winner_index(), 0)
        self.assertTrue(self.game.is_over())

    def test_player2_wins_when_player1_dead(self):
        self.game.players[0].life = -2
        self.game.players[1].life = 4
        self.assertEqual(self.game.winner_index(), 1)
        self.assertTrue(self.game.is_over())

    def test_none_when_both_dead(self):
        self.game.players[0].life = 0
        self.game.players[1].life = -3
        self.assertIsNone(self.game.winner_index())
        self.assertTrue(self.game.is_over())

    def test_winner_object_matches_index(self):
        self.game.players[1].life = 0
        self.assertIs(self.game.winner(), self.game.players[0])


class TestTurnCounter(unittest.TestCase):
    """The turn counter lives in GameState and advances on each turn switch."""

    def test_starts_at_one(self):
        game = _fresh_env(seed=2)._game
        self.assertEqual(game.turn_number, 1)

    def test_increments_on_switch_turn(self):
        game = _fresh_env(seed=2)._game
        start = game.turn_number
        game.switch_turn()
        self.assertEqual(game.turn_number, start + 1)
        game.switch_turn()
        self.assertEqual(game.turn_number, start + 2)


# ──────────────────────────────────────────────────────────────
# _finalize terminal phases
# ──────────────────────────────────────────────────────────────

class TestFinalizePhases(unittest.TestCase):
    """_finalize() picks the right terminal phase, rewards, and flags."""

    def setUp(self):
        self.env = _fresh_env(seed=3)
        self.p0, self.p1 = self.env._game.players

    def test_player1_win(self):
        self.p0.life = 9
        self.p1.life = 0
        self.env._finalize()
        self.assertTrue(self.env.done)
        self.assertEqual(self.env._phase, Phase.PLAYER1_WIN)
        self.assertEqual(self.env._rewards, {"agent_0": 1.0, "agent_1": -1.0})
        self.assertEqual(self.env._terminations, {"agent_0": True, "agent_1": True})
        self.assertEqual(self.env._truncations, {"agent_0": False, "agent_1": False})

    def test_player2_win(self):
        self.p0.life = -1
        self.p1.life = 12
        self.env._finalize()
        self.assertEqual(self.env._phase, Phase.PLAYER2_WIN)
        self.assertEqual(self.env._rewards, {"agent_0": -1.0, "agent_1": 1.0})
        self.assertEqual(self.env._terminations, {"agent_0": True, "agent_1": True})

    def test_simultaneous_death_is_draw(self):
        self.p0.life = 0
        self.p1.life = 0
        self.env._finalize()
        self.assertEqual(self.env._phase, Phase.DRAW)
        self.assertEqual(self.env._rewards, {"agent_0": 0.0, "agent_1": 0.0})
        # Both-dead is a genuine terminal draw, not a truncation.
        self.assertEqual(self.env._terminations, {"agent_0": True, "agent_1": True})
        self.assertEqual(self.env._truncations, {"agent_0": False, "agent_1": False})

    def test_turn_limit_is_draw(self):
        self.p0.life = 15
        self.p1.life = 15
        self.env._game.turn_number = self.env.DRAW_TURN_LIMIT + 1
        self.env._finalize()
        self.assertEqual(self.env._phase, Phase.DRAW)
        self.assertEqual(self.env._rewards, {"agent_0": 0.0, "agent_1": 0.0})
        # Running out the clock is a truncation, not a termination.
        self.assertEqual(self.env._terminations, {"agent_0": False, "agent_1": False})
        self.assertEqual(self.env._truncations, {"agent_0": True, "agent_1": True})


class TestDrawTurnLimitConstant(unittest.TestCase):
    """The configurable draw turn limit is exposed and defaults to 40."""

    def test_default_is_40(self):
        self.assertEqual(FaBEnv.DRAW_TURN_LIMIT, 40)

    def test_max_turns_alias(self):
        self.assertEqual(FaBEnv.MAX_TURNS, FaBEnv.DRAW_TURN_LIMIT)


# ──────────────────────────────────────────────────────────────
# End-to-end through step()
# ──────────────────────────────────────────────────────────────

class TestStepEndsGame(unittest.TestCase):
    """step() finalizes into the right terminal phase during real play."""

    def test_lethal_attack_yields_player1_win(self):
        env, rhinar, dorinthea = _attack_env(seed=14)

        # Isolate Wounded Bull (6 power) and pre-pay its cost so playing it
        # launches the attack immediately.
        card = next(c for c in rhinar.hand if c.name == "Wounded Bull")
        rhinar.hand = [card]
        rhinar.resource_points = 3

        # Strip the defender so she cannot block or react — the hit lands in full.
        dorinthea.life = 5
        dorinthea.hand = []
        dorinthea.equipment = {}

        play = next(a for a in env.legal_actions()
                    if a.action_type == ActionType.PLAY_CARD
                    and a.card.name == "Wounded Bull")
        env.step(play)

        # 6 damage to 5 life → dead. Player 1 (agent_0) wins.
        self.assertTrue(env.done)
        self.assertEqual(env._phase, Phase.PLAYER1_WIN)
        self.assertTrue(dorinthea.is_dead())
        self.assertEqual(env._rewards["agent_0"], 1.0)
        self.assertEqual(env._rewards["agent_1"], -1.0)
        # Terminal phase exposes no further actions.
        self.assertEqual(env.legal_actions(), [])

    def test_exact_zero_life_is_lethal(self):
        env, rhinar, dorinthea = _attack_env(seed=14)
        card = next(c for c in rhinar.hand if c.name == "Wounded Bull")
        rhinar.hand = [card]
        rhinar.resource_points = 3
        dorinthea.life = 6  # exactly the attack's power → reduced to 0
        dorinthea.hand = []
        dorinthea.equipment = {}

        play = next(a for a in env.legal_actions()
                    if a.action_type == ActionType.PLAY_CARD
                    and a.card.name == "Wounded Bull")
        env.step(play)

        self.assertEqual(dorinthea.life, 0)
        self.assertTrue(env.done)
        self.assertEqual(env._phase, Phase.PLAYER1_WIN)

    def test_step_past_turn_limit_yields_draw(self):
        env, rhinar, dorinthea = _attack_env(seed=14)
        # Both players healthy, but the clock has run out.
        rhinar.life = 15
        dorinthea.life = 15
        env._game.turn_number = env.DRAW_TURN_LIMIT + 1

        # Any further action triggers the over-check → draw by turn limit.
        env.step(Action(ActionType.PASS))

        self.assertTrue(env.done)
        self.assertEqual(env._phase, Phase.DRAW)
        self.assertEqual(env._rewards, {"agent_0": 0.0, "agent_1": 0.0})
        self.assertTrue(env._truncations["agent_0"])
        self.assertTrue(env._truncations["agent_1"])
        self.assertEqual(env.legal_actions(), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)

# fab-ai — CLAUDE.md

Flesh and Blood (FaB) TCG AI and simulation environment.
Blitz format, **Rhinar vs Dorinthea**, 20 life / intellect 4 each.

---

## File map

| File | Responsibility |
|------|---------------|
| `cards.py` | `Card` dataclass; enums `CardType`, `Color`, `EquipSlot`, `CardClass`; `build_rhinar_deck()`, `build_dorinthea_deck()`, equipment builders, `_build_card_catalog()` |
| `game_state.py` | `Equipment`, `Player`, `GameState` — pure data, no rules logic |
| `actions.py` | `ActionType` enum, `Action` dataclass, legal-action generators: `legal_attack_actions`, `legal_pitch_actions`, `legal_defend_actions`, `legal_arsenal_actions` |
| `observations.py` | `CARD_VOCAB`, `CARD_IDX`, `PLAYER_OBS_SIZE`; `encode_player()`, `encode_opponent_public()`, `build_observation()` |
| `spaces.py` | Lightweight gymnasium-compatible `Discrete`, `Box`, `Dict` spaces (no gymnasium dependency) |
| `fab_env.py` | `FaBEnv` — main gym-style environment; `Phase` enum; `_make_rhinar()` / `_make_dorinthea()` factories |
| `agents.py` | Rule-based agents: `RhinarAgent`, `DorintheiAgent`, `HumanAgent`; each has `select_action`, `select_defend`, `select_arsenal`, `select_pitch` |
| `seed_decks.py` | Deterministic deck seeds for reproducible tests |
| `deck_db.py` | Card lookup / deck database helpers |
| `run_env.py` | CLI entry point to run games or RL training loops |
| `test_seed42.py` | Regression tests (seeded at 42) |
| `web_viewer.py` | Browser-based game viewer (~2 k lines) |

---

## Architecture

```
FaBEnv.step(action)
  ├─ Phase.ATTACK  → _handle_attack_action → _resolve_played_card / _resolve_weapon_attack
  ├─ Phase.PITCH   → _handle_pitch_action
  ├─ Phase.DEFEND  → _handle_defend_action → _resolve_defend
  └─ Phase.ARSENAL → _handle_arsenal_action
```

`FaBEnv.legal_actions()` returns the valid `Action` list for the current phase.
`FaBEnv._get_obs()` returns `{"p0": obs_dict, "p1": obs_dict}` via `build_observation()`.

---

## Key data shapes

### Action (2-step for card play)
1. `PLAY_CARD` — pick card (`action.card`, `action.from_arsenal`)
2. `PITCH` — pick pitch combo (`action.pitch_indices` into `player.hand`)

Other types: `WEAPON`, `PASS`, `DEFEND`, `ARSENAL`, `ACTIVATE_EQUIPMENT`.

### Observation dict (per player)
```python
{
  "agent":        List[float],  # length PLAYER_OBS_SIZE — full self info
  "opponent":     List[float],  # length PLAYER_OBS_SIZE — public opponent info
  "global":       List[float],  # [turn_number/80, is_first_turn]
  "pending_card": List[float],  # CARD_FEATURES — non-zero only during PITCH phase
}
```

`PLAYER_OBS_SIZE = MAX_HAND * CARD_FEATURES + CARD_FEATURES + 4 + 1 + 1 + 8`
where `CARD_FEATURES = VOCAB_SIZE + 6` and `VOCAB_SIZE = len(CARD_VOCAB)` (currently 47).

---

## Player state fields (game_state.py:Player)

Core: `life`, `intellect`, `deck`, `hand`, `graveyard`, `banished`, `pitch_zone`, `arsenal`, `equipment` (dict by slot), `weapon`.

Turn resources: `action_points`, `resource_points`, `weapon_used_this_turn`, `weapon_attack_count`.

Per-turn bonuses: `next_weapon_go_again`, `next_weapon_power_bonus`, `next_attack_go_again`, `next_brute_attack_bonus`, `attacks_this_turn`, `weapon_additional_attack`, `dawnblade_counters`.

Mentor: `mentor_face_up`, `mentor_lesson_counters`.

---

## Conventions

- No external dependencies (no gymnasium, numpy, torch) in core files; `spaces.py` is the shim.
- `cards.py` is the single source of truth for card stats — edit there, not in agents.
- Legal-action generators live in `actions.py`, card-effect resolution in `fab_env.py`.
- Tests use `seed_decks.py` + `random.seed(42)` for determinism.
- `web_viewer.py` is self-contained; avoid coupling it to core logic changes.

---

## Phases (fab_env.py:Phase enum)

`ATTACK` → `PITCH` → `DEFEND` → `ARSENAL` → (back to `ATTACK` for next player)

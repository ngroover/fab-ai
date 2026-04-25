"""
web_viewer.py — Mobile-friendly web UI for viewing FaB game logs.

Usage:
  python web_viewer.py                                      # http on port 5000
  python web_viewer.py --port 8080                          # custom port
  python web_viewer.py --ssl-cert cert.pem --ssl-key key.pem  # HTTPS

Generate a self-signed cert (dev only):
  openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
    -subj "/CN=localhost"

Open https://<your-machine-ip>:5000 on your phone to browse logs.
Generate logs with:  python run_env.py --log
"""

from __future__ import annotations

import argparse
import os
from datetime import datetime
from pathlib import Path

import threading

from flask import Flask, abort, jsonify, redirect, render_template_string, request

import deck_db
from cards import (
    build_rhinar_deck, build_rhinar_equipment,
    build_dorinthea_deck, build_dorinthea_equipment,
    Keyword,
)
from card_effects import EffectAction

deck_db.init_db()

LOGS_DIR = Path(__file__).parent / "logs"

app = Flask(__name__)

# ──────────────────────────────────────────────────────────────
# HTML templates (inline — no template files needed)
# ──────────────────────────────────────────────────────────────

BASE_CSS = """
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0f1117;
    color: #e2e8f0;
    min-height: 100vh;
}
a { color: #63b3ed; text-decoration: none; }
a:hover { text-decoration: underline; }
header {
    background: #1a202c;
    border-bottom: 1px solid #2d3748;
    padding: 12px 16px;
    display: flex;
    align-items: center;
    gap: 12px;
    position: sticky;
    top: 0;
    z-index: 10;
}
header h1 { font-size: 1.1rem; font-weight: 700; color: #f6e05e; }
header .subtitle { font-size: 0.75rem; color: #718096; }
.back-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: #63b3ed;
    font-size: 0.85rem;
    padding: 4px 10px;
    border: 1px solid #2d3748;
    border-radius: 6px;
}
.back-link:hover { background: #2d3748; text-decoration: none; }
"""

INDEX_TEMPLATE = """
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>FaB Game Logs</title>
  <style>
    {{ css }}
    .container { padding: 16px; max-width: 700px; margin: 0 auto; }
    .empty {
        text-align: center;
        color: #718096;
        padding: 60px 20px;
    }
    .empty .hint {
        margin-top: 12px;
        font-size: 0.85rem;
        background: #1a202c;
        border: 1px solid #2d3748;
        border-radius: 8px;
        padding: 12px;
        text-align: left;
        font-family: monospace;
        color: #a0aec0;
    }
    .log-list { list-style: none; display: flex; flex-direction: column; gap: 10px; }
    .log-item {
        background: #1a202c;
        border: 1px solid #2d3748;
        border-radius: 10px;
        overflow: hidden;
        transition: border-color 0.15s;
    }
    .log-item:hover { border-color: #4a5568; }
    .log-item a {
        display: flex;
        flex-direction: column;
        gap: 4px;
        padding: 14px 16px;
        color: inherit;
        text-decoration: none;
    }
    .log-name { font-weight: 600; font-size: 0.95rem; color: #e2e8f0; }
    .log-meta { display: flex; gap: 12px; font-size: 0.78rem; color: #718096; flex-wrap: wrap; }
    .badge {
        display: inline-flex;
        align-items: center;
        padding: 2px 8px;
        border-radius: 12px;
        font-size: 0.72rem;
        font-weight: 600;
    }
    .badge-rhinar  { background: #744210; color: #fbd38d; }
    .badge-dorinthea { background: #1a365d; color: #90cdf4; }
    .badge-draw    { background: #2d3748; color: #a0aec0; }
    .section-title {
        font-size: 0.75rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: #718096;
        margin: 20px 0 8px;
    }
    .refresh-btn {
        float: right;
        font-size: 0.8rem;
        color: #63b3ed;
        padding: 4px 10px;
        border: 1px solid #2d3748;
        border-radius: 6px;
        cursor: pointer;
        background: transparent;
        text-decoration: none;
    }
    .refresh-btn:hover { background: #2d3748; }
    .run-section {
        background: #1a202c;
        border: 1px solid #2d3748;
        border-radius: 10px;
        padding: 14px 16px;
        margin-top: 16px;
        display: flex;
        align-items: center;
        gap: 10px;
        flex-wrap: wrap;
    }
    .run-section label { font-size: 0.85rem; color: #a0aec0; white-space: nowrap; }
    .seed-input {
        background: #0f1117;
        border: 1px solid #4a5568;
        border-radius: 6px;
        color: #e2e8f0;
        font-size: 0.85rem;
        padding: 5px 10px;
        width: 150px;
        outline: none;
    }
    .seed-input:focus { border-color: #63b3ed; }
    .seed-input::placeholder { color: #4a5568; }
    .run-btn {
        background: #2b6cb0;
        border: none;
        border-radius: 6px;
        color: #fff;
        cursor: pointer;
        font-size: 0.85rem;
        font-weight: 600;
        padding: 6px 18px;
    }
    .run-btn:hover { background: #3182ce; }
    .run-btn:disabled { background: #4a5568; cursor: not-allowed; }
  </style>
</head>
<body>
  <header>
    <div>
      <h1>⚔️ FaB Game Logs</h1>
      <div class="subtitle">Flesh and Blood — Classic Battles</div>
    </div>
    <a class="refresh-btn" href="/decks" style="margin-right:6px;background:#276749;border-color:#2f855a;color:#9ae6b4;">🃏 Decks</a>
    <a class="refresh-btn" href="/play" style="margin-right:6px;background:#1a365d;border-color:#2b4c7e;color:#90cdf4;">▶ Play</a>
    <a class="refresh-btn" href="/">↻ Refresh</a>
  </header>
  <div class="container">
    <div class="run-section">
      <form method="POST" action="/run" style="display:flex;align-items:center;gap:10px;flex-wrap:wrap;width:100%">
        <label>▶ Run new game:</label>
        <input type="number" name="seed" placeholder="seed (optional)" class="seed-input" min="0">
        <button type="submit" class="run-btn" onclick="this.disabled=true;this.textContent='Running…';this.form.submit()">Run Game</button>
      </form>
    </div>
    {% if not logs %}
      <div class="empty">
        <div style="font-size:3rem;">📋</div>
        <p style="margin-top:12px;">No game logs yet.</p>
        <div class="hint">
          Generate logs with:<br><br>
          python run_env.py --log<br>
          python run_env.py --log --quiet<br>
          python run_env.py --log --seed 42
        </div>
      </div>
    {% else %}
      <div class="section-title">{{ logs|length }} game{{ 's' if logs|length != 1 }} · most recent first</div>
      <ul class="log-list">
        {% for log in logs %}
        <li class="log-item">
          <a href="/log/{{ log.filename }}">
            <span class="log-name">{{ log.display_name }}</span>
            <span class="log-meta">
              <span>📅 {{ log.date }}</span>
              <span>🕐 {{ log.time }}</span>
              <span>📦 {{ log.size }}</span>
              {% if log.winner == 'Rhinar' %}
                <span class="badge badge-rhinar">🏆 Rhinar wins</span>
              {% elif log.winner == 'Dorinthea' %}
                <span class="badge badge-dorinthea">🏆 Dorinthea wins</span>
              {% elif log.winner == 'Draw' %}
                <span class="badge badge-draw">⏱ Draw</span>
              {% endif %}
            </span>
          </a>
        </li>
        {% endfor %}
      </ul>
    {% endif %}
  </div>
</body>
</html>
"""

LOG_TEMPLATE = """
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{{ filename }} — FaB Log</title>
  <style>
    {{ css }}
    .toolbar {
        display: flex;
        align-items: center;
        gap: 10px;
        flex: 1;
    }
    .log-title { font-size: 0.85rem; color: #e2e8f0; font-weight: 600; }
    .log-subtitle { font-size: 0.72rem; color: #718096; }
    pre {
        font-family: 'Menlo', 'Monaco', 'Courier New', monospace;
        font-size: 0.78rem;
        line-height: 1.55;
        padding: 16px;
        overflow-x: auto;
        white-space: pre-wrap;
        word-break: break-word;
        color: #e2e8f0;
    }
    .turn-header   { color: #f6e05e; font-weight: bold; }
    .life-line     { color: #fc8181; }
    .attack-line   { color: #f6ad55; }
    .defend-line   { color: #68d391; }
    .damage-line   { color: #fc8181; font-weight: bold; }
    .go-again-line { color: #76e4f7; }
    .game-over     { color: #f6e05e; font-weight: bold; }
    .wins-line     { color: #68d391; font-weight: bold; }
    .draw-line     { color: #a0aec0; }
    .pitch-line    { color: #b794f4; }
    .hand-line     { color: #90cdf4; }
    .store-line    { color: #e9d8fd; }
    .controls {
        display: flex;
        gap: 8px;
        padding: 8px 16px;
        background: #111827;
        border-bottom: 1px solid #2d3748;
        flex-wrap: wrap;
        align-items: center;
    }
    .size-btn {
        font-size: 0.75rem;
        padding: 3px 10px;
        border: 1px solid #4a5568;
        border-radius: 5px;
        background: transparent;
        color: #a0aec0;
        cursor: pointer;
    }
    .size-btn.active, .size-btn:hover { background: #2d3748; color: #e2e8f0; }
    .label { font-size: 0.72rem; color: #718096; }
  </style>
</head>
<body>
  <header>
    <a class="back-link" href="/">← Logs</a>
    <div class="toolbar">
      <div>
        <div class="log-title">{{ filename }}</div>
        <div class="log-subtitle">{{ line_count }} lines · {{ size }}</div>
      </div>
    </div>
  </header>
  <div class="controls">
    <span class="label">Font size:</span>
    <button class="size-btn" onclick="setSize('0.68rem')">XS</button>
    <button class="size-btn active" onclick="setSize('0.78rem')">S</button>
    <button class="size-btn" onclick="setSize('0.9rem')">M</button>
    <button class="size-btn" onclick="setSize('1rem')">L</button>
  </div>
  <pre id="log-content">{{ rendered_content | safe }}</pre>
  <script>
    function setSize(s) {
      document.getElementById('log-content').style.fontSize = s;
      document.querySelectorAll('.size-btn').forEach(b => b.classList.remove('active'));
      event.target.classList.add('active');
    }
  </script>
</body>
</html>
"""

# ──────────────────────────────────────────────────────────────
# Log metadata helpers
# ──────────────────────────────────────────────────────────────

def _parse_winner(text: str) -> str:
    """Scan log text for the winner line."""
    for line in text.splitlines():
        if "WINS!" in line:
            if "Rhinar" in line:
                return "Rhinar"
            if "Dorinthea" in line:
                return "Dorinthea"
        if "GAME OVER" in line and ("DRAW" in line.upper() or "TIMEOUT" in line.upper()):
            return "Draw"
    return ""


def _human_size(n: int) -> str:
    if n < 1024:
        return f"{n} B"
    if n < 1024 * 1024:
        return f"{n/1024:.1f} KB"
    return f"{n/1024/1024:.1f} MB"


def _list_logs():
    if not LOGS_DIR.exists():
        return []

    entries = []
    for path in sorted(LOGS_DIR.glob("*.log"), key=lambda p: p.stat().st_mtime, reverse=True):
        stat = path.stat()
        mtime = datetime.fromtimestamp(stat.st_mtime)
        text = path.read_text(encoding="utf-8", errors="replace")
        winner = _parse_winner(text)

        # Pretty display name: strip extension, replace underscores
        display = path.stem.replace("_", " ").replace("game ", "Game ")

        entries.append({
            "filename": path.name,
            "display_name": display,
            "date": mtime.strftime("%b %d, %Y"),
            "time": mtime.strftime("%H:%M:%S"),
            "size": _human_size(stat.st_size),
            "winner": winner,
        })
    return entries


# ──────────────────────────────────────────────────────────────
# ANSI-to-HTML colour mapping for the game's emoji/unicode output
# ──────────────────────────────────────────────────────────────

import html as _html

def _render_log(text: str) -> str:
    """Convert plain log text to HTML with colour highlights."""
    lines = []
    for raw in text.splitlines():
        escaped = _html.escape(raw)

        # Classify line for colouring (order matters — most specific first)
        if "══" in raw or "★★" in raw:
            cls = "game-over"
        elif "WINS!" in raw:
            cls = "wins-line"
        elif "TURN" in raw and "══" not in raw:
            cls = "turn-header"
        elif "takes" in raw and "damage" in raw:
            cls = "damage-line"
        elif "♥" in raw or "Life:" in raw:
            cls = "life-line"
        elif "⚔" in raw or "attacks" in raw:
            cls = "attack-line"
        elif "🛡" in raw or "defends" in raw or "blocks" in raw:
            cls = "defend-line"
        elif "↩" in raw or "Go again" in raw or "go again" in raw:
            cls = "go-again-line"
        elif "pitched" in raw or "pitches" in raw or "▶" in raw:
            cls = "pitch-line"
        elif "🃏" in raw or "Hand:" in raw or "draws" in raw or "🔄" in raw:
            cls = "hand-line"
        elif "📦" in raw or "stores" in raw or "arsenal" in raw.lower():
            cls = "store-line"
        elif "Draw" in raw or "DRAW" in raw or "timeout" in raw.lower():
            cls = "draw-line"
        else:
            cls = None

        if cls:
            lines.append(f'<span class="{cls}">{escaped}</span>')
        else:
            lines.append(escaped)

    return "\n".join(lines)


# ──────────────────────────────────────────────────────────────
# Card catalog helpers
# ──────────────────────────────────────────────────────────────

def _build_card_catalog() -> list:
    """Return a deduplicated list of all card templates as dicts, keyed by card_id."""
    seen: dict = {}
    sources = [
        ("Rhinar", build_rhinar_deck()),
        ("Rhinar", build_rhinar_equipment()),
        ("Dorinthea", build_dorinthea_deck()),
        ("Dorinthea", build_dorinthea_equipment()),
    ]
    for hero, card_list in sources:
        for c in card_list:
            if c.card_id not in seen:
                seen[c.card_id] = {
                    "card_id": c.card_id,
                    "name": c.name,
                    "card_type": c.card_type.value,
                    "cost": c.cost,
                    "pitch": c.pitch,
                    "power": c.power,
                    "defense": c.defense,
                    "color": c.color.name.capitalize() if c.color else None,
                    "go_again": Keyword.GO_AGAIN in c.keywords,
                    "text": c.text,
                    "intimidate": any(e.action == EffectAction.INTIMIDATE for e in c.effects),
                    "no_block": c.no_block,
                    "equip_slot": c.equip_slot.value if c.equip_slot else None,
                    "hero": hero,
                    "card_class": c.card_class.value,
                }

    # Merge cards imported via import_cards.py (stored in the card_catalog table).
    # Cards already defined in the Python-side catalog take precedence.
    try:
        for row in deck_db.get_catalog_cards():
            if row["card_id"] not in seen:
                seen[row["card_id"]] = {
                    "card_id":   row["card_id"],
                    "name":      row["name"],
                    "card_type": row["card_type"],
                    "cost":      row["cost"],
                    "pitch":     row["pitch"],
                    "power":     row["power"],
                    "defense":   row["defense"],
                    "color":     row["color"],
                    "go_again":  bool(row["go_again"]),
                    "text":      row["text"] or "",
                    "intimidate": bool(row["intimidate"]),  # DB-sourced; no Card field
                    "no_block":  bool(row["no_block"]),
                    "equip_slot": row["equip_slot"],
                    "hero":      None,
                    "card_class": row["card_class"],
                }
    except Exception:
        # card_catalog table may not exist yet (before first import_cards run)
        pass

    return list(seen.values())


def _catalog_row_to_card(row: dict):
    """Convert a card_catalog DB row into a Card dataclass instance."""
    from cards import Card, CardType, Color, EquipSlot, CardClass, Keyword

    type_map   = {v.value: v for v in CardType}
    color_map  = {"Red": Color.RED, "Yellow": Color.YELLOW, "Blue": Color.BLUE}
    equip_map  = {
        "head": EquipSlot.HEAD, "chest": EquipSlot.CHEST,
        "arms": EquipSlot.ARMS, "legs":  EquipSlot.LEGS,
        "weapon": EquipSlot.WEAPON,
    }
    class_map  = {v.value: v for v in CardClass}

    kws = []
    if bool(row["go_again"]):
        kws.append(Keyword.GO_AGAIN)

    return Card(
        name      = row["name"],
        card_type = type_map.get(row["card_type"], CardType.ACTION),
        cost      = row["cost"],
        pitch     = row["pitch"],
        power     = row["power"],
        defense   = row["defense"],
        color     = color_map.get(row["color"]),
        text      = row["text"] or "",
        no_block  = bool(row["no_block"]),
        keywords  = kws,
        equip_slot= equip_map.get(row["equip_slot"]),
        # Unknown classes (e.g. Ranger) fall back to Generic for game purposes
        card_class= class_map.get(row["card_class"], CardClass.GENERIC),
    )


def _build_card_lookup() -> dict:
    """Return {card_id: Card} for every card known to the game."""
    lookup: dict = {}
    for card_list in [
        build_rhinar_deck(), build_rhinar_equipment(),
        build_dorinthea_deck(), build_dorinthea_equipment(),
    ]:
        for c in card_list:
            lookup[c.card_id] = c

    # Also include cards imported via import_cards.py
    try:
        for row in deck_db.get_catalog_cards():
            if row["card_id"] not in lookup:
                lookup[row["card_id"]] = _catalog_row_to_card(row)
    except Exception:
        pass

    return lookup


_HERO_CLASS = {"Rhinar": "Brute", "Dorinthea": "Warrior"}


def _validate_deck_cards(hero: str, cards: dict) -> list[str]:
    """
    Return a list of card_id strings that are incompatible with `hero`.
    A card is incompatible if it belongs to a different class (e.g. a
    Warrior card in a Brute deck).  Generic cards and unknown heroes
    are always allowed.
    """
    required_class = _HERO_CLASS.get(hero)
    if not required_class:
        return []
    lookup = _build_card_lookup()
    bad = []
    for card_id in cards:
        card = lookup.get(card_id)
        if card and card.card_class.value not in ("Generic", required_class):
            bad.append(card_id)
    return bad


def _player_from_deck(deck_record: dict):
    """
    Build a Player from a saved deck record.
    Hero stats and equipment are chosen based on deck_record["hero"].
    """
    from game_state import Player

    lookup = _build_card_lookup()
    deck_cards = []
    for card_id, qty in deck_record["cards"].items():
        card = lookup.get(card_id)
        if card:
            deck_cards.extend([card] * qty)

    if deck_record.get("hero") == "Dorinthea":
        equip = build_dorinthea_equipment()
        return Player(
            name=deck_record["name"],
            hero_name="Dorinthea, Quicksilver Prodigy",
            life=20, intellect=4,
            deck=deck_cards,
            equipment_list=equip[1:],
            weapon=equip[0],
        )
    else:  # Rhinar or Custom
        equip = build_rhinar_equipment()
        return Player(
            name=deck_record["name"],
            hero_name="Rhinar (Young Brute)",
            life=20, intellect=4,
            deck=deck_cards,
            equipment_list=equip[1:],
            weapon=equip[0],
        )


# ──────────────────────────────────────────────────────────────
# Deck builder HTML template
# ──────────────────────────────────────────────────────────────

DECKS_TEMPLATE = """
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Deck Lists — FaB</title>
  <style>
    {{ css }}
    .container { padding: 16px; max-width: 700px; margin: 0 auto; }
    .deck-list { list-style: none; display: flex; flex-direction: column; gap: 10px; margin-top: 16px; }
    .deck-item {
      background: #1a202c;
      border: 1px solid #2d3748;
      border-radius: 10px;
      padding: 14px 16px;
      display: flex;
      align-items: center;
      gap: 12px;
    }
    .deck-item:hover { border-color: #4a5568; }
    .deck-name { font-weight: 600; font-size: 0.95rem; flex: 1; }
    .deck-meta { font-size: 0.78rem; color: #718096; }
    .btn {
      font-size: 0.8rem;
      padding: 5px 14px;
      border-radius: 6px;
      border: none;
      cursor: pointer;
      font-weight: 600;
    }
    .btn-primary { background: #2b6cb0; color: #fff; }
    .btn-primary:hover { background: #3182ce; }
    .btn-danger { background: #742a2a; color: #feb2b2; }
    .btn-danger:hover { background: #9b2c2c; }
    .btn-sm { padding: 3px 10px; font-size: 0.75rem; }
    .new-btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      background: #276749;
      color: #9ae6b4;
      border: none;
      border-radius: 6px;
      padding: 6px 16px;
      font-size: 0.85rem;
      font-weight: 600;
      cursor: pointer;
      text-decoration: none;
    }
    .new-btn:hover { background: #2f855a; text-decoration: none; }
    .empty { text-align: center; color: #718096; padding: 40px 0; }
  </style>
</head>
<body>
  <header>
    <div>
      <h1>🃏 Deck Lists</h1>
      <div class="subtitle">Flesh and Blood — Saved Decks</div>
    </div>
    <a class="back-link" href="/">← Home</a>
    <a class="new-btn" href="/decks/builder">+ New Deck</a>
  </header>
  <div class="container">
    {% if not decks %}
      <div class="empty">
        <div style="font-size:3rem;">🃏</div>
        <p style="margin-top:12px;">No decks saved yet.</p>
        <a class="new-btn" href="/decks/builder" style="margin-top:16px;display:inline-flex;">+ Build your first deck</a>
      </div>
    {% else %}
      <ul class="deck-list">
        {% for d in decks %}
        <li class="deck-item">
          <div style="flex:1">
            <div class="deck-name">{{ d.name }}</div>
            <div class="deck-meta">{{ d.hero }} · {{ d.updated_at[:10] }}</div>
          </div>
          <a class="btn btn-primary btn-sm" href="/decks/builder/{{ d.id }}">Edit</a>
          <button class="btn btn-danger btn-sm" onclick="deleteDeck({{ d.id }}, this)">Delete</button>
        </li>
        {% endfor %}
      </ul>
    {% endif %}
  </div>
  <script>
    async function deleteDeck(id, btn) {
      if (!confirm('Delete this deck?')) return;
      const res = await fetch('/api/decks/' + id, {method: 'DELETE'});
      if (res.ok) { btn.closest('li').remove(); }
      else { alert('Delete failed'); }
    }
  </script>
</body>
</html>
"""

DECK_BUILDER_TEMPLATE = """
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Deck Builder — FaB</title>
  <style>
    {{ css }}
    /* ── Layout ── */
    .builder-layout {
      display: grid;
      grid-template-columns: 1fr 340px;
      grid-template-rows: auto 1fr;
      height: calc(100vh - 52px);
    }
    @media (max-width: 700px) {
      .builder-layout {
        grid-template-columns: 1fr;
        grid-template-rows: auto auto 1fr;
        height: auto;
      }
      .deck-panel { order: -1; max-height: 280px; }
    }

    /* ── Toolbar ── */
    .toolbar {
      grid-column: 1 / -1;
      background: #111827;
      border-bottom: 1px solid #2d3748;
      padding: 8px 14px;
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;
    }
    .deck-name-input {
      background: #1a202c;
      border: 1px solid #4a5568;
      border-radius: 6px;
      color: #e2e8f0;
      font-size: 0.9rem;
      font-weight: 600;
      padding: 5px 10px;
      width: 200px;
    }
    .deck-name-input:focus { outline: none; border-color: #63b3ed; }
    select.hero-select {
      background: #1a202c;
      border: 1px solid #4a5568;
      border-radius: 6px;
      color: #e2e8f0;
      font-size: 0.85rem;
      padding: 5px 10px;
    }
    .save-btn {
      background: #276749;
      border: none;
      border-radius: 6px;
      color: #9ae6b4;
      cursor: pointer;
      font-size: 0.85rem;
      font-weight: 600;
      padding: 6px 18px;
    }
    .save-btn:hover { background: #2f855a; }
    .save-btn:disabled { background: #2d3748; color: #718096; cursor: not-allowed; }
    .status-msg { font-size: 0.8rem; color: #68d391; }
    .status-msg.err { color: #fc8181; }

    /* ── Catalog panel ── */
    .catalog-panel {
      overflow-y: auto;
      padding: 10px;
      background: #0f1117;
      border-right: 1px solid #2d3748;
    }
    .filter-bar {
      display: flex;
      gap: 6px;
      flex-wrap: wrap;
      margin-bottom: 8px;
    }
    .filter-bar input, .filter-bar select {
      background: #1a202c;
      border: 1px solid #4a5568;
      border-radius: 5px;
      color: #e2e8f0;
      font-size: 0.78rem;
      padding: 4px 8px;
      flex: 1;
      min-width: 80px;
    }
    .filter-bar input:focus, .filter-bar select:focus { outline: none; border-color: #63b3ed; }

    .card-row {
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 6px 8px;
      border-radius: 6px;
      margin-bottom: 4px;
      background: #1a202c;
      border: 1px solid #2d3748;
      cursor: default;
    }
    .card-row:hover { border-color: #4a5568; }
    .card-row.in-deck { border-color: #276749; background: #1a2e22; }
    .card-info { flex: 1; min-width: 0; }
    .card-name { font-size: 0.85rem; font-weight: 600; color: #e2e8f0; }
    .card-stats {
      font-size: 0.7rem;
      color: #718096;
      margin-top: 2px;
      display: flex;
      gap: 6px;
      flex-wrap: wrap;
    }
    .pill {
      display: inline-block;
      padding: 1px 6px;
      border-radius: 8px;
      font-size: 0.68rem;
      font-weight: 600;
    }
    .pill-red    { background: #744210; color: #fbd38d; }
    .pill-yellow { background: #744210; color: #fefcbf; }
    .pill-blue   { background: #1a365d; color: #90cdf4; }
    .pill-none   { background: #2d3748; color: #a0aec0; }
    .pill-brute   { background: #6b2737; color: #fed7d7; }
    .pill-warrior { background: #1e3a5f; color: #bee3f8; }
    .pill-generic { background: #2d3748; color: #a0aec0; }
    .card-row.incompatible { opacity: 0.4; }
    .card-row.incompatible .add-btn { background: #2d3748; color: #4a5568; cursor: not-allowed; }
    .add-btn {
      width: 26px; height: 26px;
      border-radius: 5px;
      background: #276749;
      color: #9ae6b4;
      border: none;
      font-size: 1rem;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
    }
    .add-btn:hover { background: #2f855a; }
    .qty-badge {
      background: #2b6cb0;
      color: #90cdf4;
      font-size: 0.72rem;
      font-weight: 700;
      padding: 1px 7px;
      border-radius: 8px;
      min-width: 22px;
      text-align: center;
    }

    /* ── Deck panel ── */
    .deck-panel {
      display: flex;
      flex-direction: column;
      background: #111827;
      overflow: hidden;
    }
    .deck-header {
      padding: 8px 12px;
      background: #1a202c;
      border-bottom: 1px solid #2d3748;
      font-size: 0.78rem;
      color: #a0aec0;
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .deck-count {
      font-weight: 700;
      color: #f6e05e;
    }
    .deck-cards-list {
      flex: 1;
      overflow-y: auto;
      padding: 8px;
    }
    .deck-card-row {
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 5px 8px;
      border-radius: 5px;
      margin-bottom: 3px;
      background: #1a202c;
      border: 1px solid #2d3748;
    }
    .deck-card-name { flex: 1; font-size: 0.82rem; color: #e2e8f0; }
    .qty-ctrl {
      display: flex;
      align-items: center;
      gap: 3px;
    }
    .qty-btn {
      width: 20px; height: 20px;
      border-radius: 4px;
      border: 1px solid #4a5568;
      background: #2d3748;
      color: #e2e8f0;
      font-size: 0.85rem;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .qty-btn:hover { background: #4a5568; }
    .qty-num {
      width: 22px;
      text-align: center;
      font-size: 0.82rem;
      font-weight: 700;
      color: #f6e05e;
    }
    .empty-deck {
      text-align: center;
      color: #4a5568;
      padding: 40px 10px;
      font-size: 0.85rem;
    }
  </style>
</head>
<body>
  <header>
    <div>
      <h1>🔨 Deck Builder</h1>
      <div class="subtitle">Flesh and Blood</div>
    </div>
    <a class="back-link" href="/decks">← Decks</a>
  </header>

  <div class="builder-layout">
    <!-- ── Toolbar ── -->
    <div class="toolbar">
      <input id="deckName" class="deck-name-input" placeholder="Deck name…" value="{{ deck_name }}">
      <select id="heroSelect" class="hero-select">
        <option value="Rhinar" {% if hero == 'Rhinar' %}selected{% endif %}>Rhinar</option>
        <option value="Dorinthea" {% if hero == 'Dorinthea' %}selected{% endif %}>Dorinthea</option>
        <option value="Custom" {% if hero == 'Custom' %}selected{% endif %}>Custom</option>
      </select>
      <button class="save-btn" id="saveBtn" onclick="saveDeck()">💾 Save</button>
      <span id="statusMsg" class="status-msg"></span>
    </div>

    <!-- ── Catalog panel ── -->
    <div class="catalog-panel">
      <div class="filter-bar">
        <input id="searchInput" placeholder="Search cards…" oninput="renderCatalog()">
        <select id="filterType" onchange="renderCatalog()">
          <option value="">All types</option>
          <option>Action - Attack</option>
          <option>Action</option>
          <option>Instant</option>
          <option>Attack Reaction</option>
          <option>Defense Reaction</option>
          <option>Equipment</option>
          <option>Weapon</option>
          <option>Mentor</option>
          <option>Resource</option>
        </select>
        <select id="filterColor" onchange="renderCatalog()">
          <option value="">All colors</option>
          <option>Red</option>
          <option>Yellow</option>
          <option>Blue</option>
        </select>
        <select id="filterClass" onchange="renderCatalog()">
          <option value="">All classes</option>
          <option>Brute</option>
          <option>Warrior</option>
          <option>Generic</option>
        </select>
      </div>
      <div id="catalogList"></div>
    </div>

    <!-- ── Deck panel ── -->
    <div class="deck-panel">
      <div class="deck-header">
        <span>Deck</span>
        <span class="deck-count" id="deckTotal">0</span>
        <span>cards</span>
      </div>
      <div class="deck-cards-list" id="deckList">
        <div class="empty-deck">Add cards from the catalog →</div>
      </div>
    </div>
  </div>

  <script>
    // ── State ──────────────────────────────────────────────────
    const ALL_CARDS = {{ cards_json | safe }};
    const DECK_ID   = {{ deck_id }};        // null for new deck
    let deck = {{ deck_cards_json | safe }};       // {card_id: quantity}

    // ── Class compatibility ────────────────────────────────────
    // Maps hero name -> the class of cards it can use (plus Generic)
    const HERO_CLASS = { "Rhinar": "Brute", "Dorinthea": "Warrior" };

    function currentHeroClass() {
      return HERO_CLASS[document.getElementById('heroSelect').value] || null;
    }

    function isCompatible(card) {
      const hc = currentHeroClass();
      if (!hc) return true;                    // Custom hero: no restriction
      return card.card_class === "Generic" || card.card_class === hc;
    }

    // ── Catalog rendering ──────────────────────────────────────
    function renderCatalog() {
      const q      = document.getElementById('searchInput').value.toLowerCase();
      const fType  = document.getElementById('filterType').value;
      const fColor = document.getElementById('filterColor').value;
      const fClass = document.getElementById('filterClass').value;

      const filtered = ALL_CARDS.filter(c => {
        if (q && !c.name.toLowerCase().includes(q)) return false;
        if (fType  && c.card_type   !== fType)      return false;
        if (fColor && c.color       !== fColor)     return false;
        if (fClass && c.card_class  !== fClass)     return false;
        return true;
      });

      const html = filtered.map(c => {
        const qty = deck[c.card_id] || 0;
        const compat = isCompatible(c);
        const inDeck = qty > 0 ? ' in-deck' : '';
        const incompatCls = !compat ? ' incompatible' : '';
        const colorPill = c.color
          ? `<span class="pill pill-${c.color.toLowerCase()}">${c.color}</span>`
          : `<span class="pill pill-none">—</span>`;
        const classPill = `<span class="pill pill-${c.card_class.toLowerCase()}">${c.card_class}</span>`;
        const stats = [];
        if (c.cost)    stats.push(`Cost ${c.cost}`);
        if (c.pitch)   stats.push(`Pitch ${c.pitch}`);
        if (c.power)   stats.push(`Pwr ${c.power}`);
        if (c.defense) stats.push(`Def ${c.defense}`);
        const qtyBadge = qty > 0 ? `<span class="qty-badge">${qty}</span>` : '';
        const addBtn = compat
          ? `<button class="add-btn" onclick="addCard('${c.card_id}')" title="Add to deck">+</button>`
          : `<button class="add-btn" disabled title="${c.card_class} cards cannot be added to this deck">✕</button>`;
        return `
          <div class="card-row${inDeck}${incompatCls}" id="cr-${c.card_id}">
            <div class="card-info">
              <div class="card-name">${escHtml(c.name)}</div>
              <div class="card-stats">
                ${colorPill}
                ${classPill}
                <span style="color:#a0aec0">${escHtml(c.card_type)}</span>
                ${stats.map(s=>`<span>${s}</span>`).join('')}
              </div>
            </div>
            ${qtyBadge}
            ${addBtn}
          </div>`;
      }).join('');

      document.getElementById('catalogList').innerHTML = html || '<div style="color:#4a5568;padding:20px;text-align:center">No cards match</div>';
    }

    // ── Deck rendering ─────────────────────────────────────────
    function renderDeck() {
      const entries = Object.entries(deck).filter(([,q]) => q > 0);
      const total = entries.reduce((s,[,q]) => s+q, 0);
      document.getElementById('deckTotal').textContent = total;

      if (entries.length === 0) {
        document.getElementById('deckList').innerHTML = '<div class="empty-deck">Add cards from the catalog →</div>';
        return;
      }

      // Sort by card name for readability
      entries.sort(([a],[b]) => {
        const na = ALL_CARDS.find(c=>c.card_id===a)?.name || a;
        const nb = ALL_CARDS.find(c=>c.card_id===b)?.name || b;
        return na.localeCompare(nb);
      });

      const html = entries.map(([cid, qty]) => {
        const card = ALL_CARDS.find(c=>c.card_id===cid);
        const name = card ? card.name : cid;
        const colorPill = card && card.color
          ? `<span class="pill pill-${card.color.toLowerCase()}" style="margin-right:4px">${card.color[0]}</span>`
          : '';
        return `
          <div class="deck-card-row" id="dr-${cid}">
            ${colorPill}
            <span class="deck-card-name">${escHtml(name)}</span>
            <div class="qty-ctrl">
              <button class="qty-btn" onclick="changeQty('${cid}',-1)">−</button>
              <span class="qty-num">${qty}</span>
              <button class="qty-btn" onclick="changeQty('${cid}',+1)">+</button>
            </div>
          </div>`;
      }).join('');

      document.getElementById('deckList').innerHTML = html;
    }

    // ── Actions ────────────────────────────────────────────────
    function addCard(cardId) {
      const card = ALL_CARDS.find(c => c.card_id === cardId);
      if (card && !isCompatible(card)) return;   // guard against direct calls
      deck[cardId] = (deck[cardId] || 0) + 1;
      renderDeck();
      // Update catalog row in place
      const row = document.getElementById('cr-' + cardId);
      if (row) {
        row.classList.add('in-deck');
        let badge = row.querySelector('.qty-badge');
        if (!badge) {
          badge = document.createElement('span');
          badge.className = 'qty-badge';
          row.insertBefore(badge, row.querySelector('.add-btn'));
        }
        badge.textContent = deck[cardId];
      }
    }

    function changeQty(cardId, delta) {
      const cur = deck[cardId] || 0;
      const next = Math.max(0, cur + delta);
      if (next === 0) {
        delete deck[cardId];
      } else {
        deck[cardId] = next;
      }
      renderDeck();
      // Refresh catalog row
      const row = document.getElementById('cr-' + cardId);
      if (row) {
        const badge = row.querySelector('.qty-badge');
        const qty = deck[cardId] || 0;
        if (qty === 0) {
          row.classList.remove('in-deck');
          if (badge) badge.remove();
        } else {
          row.classList.add('in-deck');
          if (badge) badge.textContent = qty;
        }
      }
    }

    // ── Save ───────────────────────────────────────────────────
    async function saveDeck() {
      const name = document.getElementById('deckName').value.trim();
      const hero = document.getElementById('heroSelect').value;
      const msg  = document.getElementById('statusMsg');
      const btn  = document.getElementById('saveBtn');

      if (!name) { showStatus('Enter a deck name first', true); return; }

      btn.disabled = true;
      btn.textContent = 'Saving…';

      const body = JSON.stringify({name, hero, cards: deck});
      let res;
      try {
        if (DECK_ID) {
          res = await fetch('/api/decks/' + DECK_ID, {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body,
          });
        } else {
          res = await fetch('/api/decks', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body,
          });
          if (res.ok) {
            const data = await res.json();
            // Redirect to the edit URL so subsequent saves go to PUT
            history.replaceState(null, '', '/decks/builder/' + data.id);
          }
        }
        if (res.ok) {
          showStatus('Saved!', false);
        } else {
          showStatus('Save failed', true);
        }
      } catch(e) {
        showStatus('Error: ' + e, true);
      }
      btn.disabled = false;
      btn.textContent = '💾 Save';
    }

    function showStatus(text, isErr) {
      const el = document.getElementById('statusMsg');
      el.textContent = text;
      el.className = 'status-msg' + (isErr ? ' err' : '');
      setTimeout(() => { el.textContent = ''; }, 3000);
    }

    function escHtml(s) {
      return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
    }

    // ── Init ───────────────────────────────────────────────────
    document.getElementById('heroSelect').addEventListener('change', renderCatalog);
    renderCatalog();
    renderDeck();
  </script>
</body>
</html>
"""


# ──────────────────────────────────────────────────────────────
# Routes
# ──────────────────────────────────────────────────────────────

@app.route("/")
def index():
    logs = _list_logs()
    return render_template_string(
        INDEX_TEMPLATE,
        css=BASE_CSS,
        logs=logs,
    )


@app.route("/log/<path:filename>")
def view_log(filename: str):
    # Safety: only serve .log files from the logs directory
    if not filename.endswith(".log") or "/" in filename or "\\" in filename:
        abort(400)

    path = LOGS_DIR / filename
    if not path.exists() or not path.is_file():
        abort(404)

    text = path.read_text(encoding="utf-8", errors="replace")
    stat = path.stat()
    rendered = _render_log(text)

    return render_template_string(
        LOG_TEMPLATE,
        css=BASE_CSS,
        filename=filename,
        rendered_content=rendered,
        line_count=len(text.splitlines()),
        size=_human_size(stat.st_size),
    )


@app.route("/run", methods=["POST"])
def run_game_route():
    """Run a complete AI vs AI game, save the log, then open it."""
    from run_env import run_game

    seed_str = request.form.get("seed", "").strip()
    seed = None
    if seed_str:
        try:
            seed = int(seed_str)
        except ValueError:
            pass

    # Snapshot existing logs so we can find the new one afterwards
    existing = set(LOGS_DIR.glob("*.log")) if LOGS_DIR.exists() else set()

    run_game(verbose=False, seed=seed, save_log=True)

    new_logs = set(LOGS_DIR.glob("*.log")) - existing
    if new_logs:
        newest = max(new_logs, key=lambda p: p.stat().st_mtime)
        return redirect(f"/log/{newest.name}")
    return redirect("/")


# ──────────────────────────────────────────────────────────────
# Interactive play — session management
# ──────────────────────────────────────────────────────────────

class _GameCancelled(Exception):
    pass


def _card_to_dict(card):
    if card is None:
        return None
    return {
        "name": card.name,
        "type": card.card_type.value,
        "cost": card.cost,
        "pitch": card.pitch,
        "power": card.power,
        "defense": card.defense,
        "color": card.color.name.lower() if card.color else None,
        "go_again": Keyword.GO_AGAIN in card.keywords,
        "no_block": card.no_block,
        "text": card.text,
    }


def _build_gamestate_snapshot(env) -> dict:
    """Build a per-player gamestate snapshot for the viewer UI.

    Each view exposes the observable zones from that player's perspective:
    own hand/arsenal/pitch/graveyard/equipment/weapon, opponent's public
    pitch/graveyard/equipment/weapon plus counts for hidden zones, and the
    shared combat chain (pending attack + accumulated defenders). Decks are
    never exposed.
    """
    if env is None or env._game is None:
        return {}

    players = env._game.players
    active_idx = env._game.active_player_idx
    defender_idx = 1 - active_idx
    defender = players[defender_idx]

    pending_defend_cards = []
    for idx in env._pending_defend_indices:
        if 0 <= idx < len(defender.hand):
            pending_defend_cards.append(_card_to_dict(defender.hand[idx]))
    pending_defend_equip = []
    for slot in env._pending_defend_equip_slots:
        eq = defender.equipment.get(slot)
        if eq is not None:
            pending_defend_equip.append(_card_to_dict(eq.card))

    def _self_view(p):
        return {
            "name": p.name,
            "hero": p.hero_name,
            "life": p.life,
            "intellect": p.intellect,
            "action_points": p.action_points,
            "resource_points": p.resource_points,
            "hand": [_card_to_dict(c) for c in p.hand],
            "arsenal": _card_to_dict(p.arsenal),
            "pitch_zone": [_card_to_dict(c) for c in p.pitch_zone],
            "graveyard": [_card_to_dict(c) for c in p.graveyard],
            "banished": [_card_to_dict(c) for c in p.banished],
            "weapon": _card_to_dict(p.weapon),
            "equipment": [
                {"slot": slot, "card": _card_to_dict(eq.card),
                 "destroyed": eq.destroyed}
                for slot, eq in p.equipment.items()
            ],
            "deck_count": len(p.deck),
        }

    def _opponent_view(p):
        return {
            "name": p.name,
            "hero": p.hero_name,
            "life": p.life,
            "intellect": p.intellect,
            "action_points": p.action_points,
            "resource_points": p.resource_points,
            "hand_count": len(p.hand),
            "arsenal_present": p.arsenal is not None,
            "pitch_zone": [_card_to_dict(c) for c in p.pitch_zone],
            "graveyard": [_card_to_dict(c) for c in p.graveyard],
            "banished_count": len(p.banished),
            "weapon": _card_to_dict(p.weapon),
            "equipment": [
                {"slot": slot, "card": _card_to_dict(eq.card),
                 "destroyed": eq.destroyed}
                for slot, eq in p.equipment.items()
            ],
            "deck_count": len(p.deck),
        }

    combat_chain = {
        "attacker_idx": active_idx,
        "attacker_name": players[active_idx].name,
        "defender_name": defender.name,
        "attack_card": _card_to_dict(env._pending_attack),
        "attack_power": env._pending_attack_power,
        "defend_cards": pending_defend_cards,
        "defend_equipment": pending_defend_equip,
        # Already-resolved links still on the chain (cleared when chain closes)
        "chained_attacks": [_card_to_dict(c) for c in players[active_idx].combat_chain],
        "chained_defenders": [_card_to_dict(c) for c in defender.combat_chain],
    }

    return {
        "p0_view": {
            "self": _self_view(players[0]),
            "opponent": _opponent_view(players[1]),
            "combat_chain": combat_chain,
        },
        "p1_view": {
            "self": _self_view(players[1]),
            "opponent": _opponent_view(players[0]),
            "combat_chain": combat_chain,
        },
    }


class _WebHumanAgent:
    """Drives a human player through the web UI instead of stdin."""

    def __init__(self, session: '_GameSession', agent_id: str):
        self.session = session
        self.agent_id = agent_id

    def _fmt_card(self, card) -> str:
        details = []
        if card.cost:      details.append(f"cost:{card.cost}")
        if card.pitch:     details.append(f"pitch:{card.pitch}")
        if card.power:     details.append(f"pow:{card.power}")
        if card.defense:   details.append(f"def:{card.defense}")
        if Keyword.GO_AGAIN in card.keywords:  details.append("go-again")
        if any(e.action == EffectAction.INTIMIDATE for e in card.effects): details.append("intimidate")
        suffix = f" ({', '.join(details)})" if details else ""
        return card.name + suffix

    def _fmt_action(self, action, player) -> str:
        from actions import ActionType
        if action.action_type == ActionType.PASS:
            return "PASS — end action phase"
        if action.action_type == ActionType.PASS_PRIORITY:
            return "PASS PRIORITY — let the stack resolve"
        if action.action_type == ActionType.WEAPON:
            wp = player.get_effective_weapon_power()
            return f"WEAPON — {player.weapon.name} for {wp} power"
        if action.action_type == ActionType.ACTIVATE_EQUIPMENT:
            eq = player.equipment.get(action.equip_slot)
            name = eq.card.name if eq else action.equip_slot
            return f"ACTIVATE — {name} ({action.equip_slot})"
        if action.action_type == ActionType.PLAY_CARD:
            if action.from_arsenal:
                card, src = player.arsenal, "arsenal"
            else:
                card, src = action.card, f"hand"
            label = f"PLAY {self._fmt_card(card)} from {src}"
            if action.pitch_indices:
                pitched = [player.hand[i].name for i in action.pitch_indices]
                label += f" | pitch: {', '.join(pitched)}"
            return label
        if action.action_type == ActionType.PITCH:
            if not action.pitch_indices:
                return "PITCH — no cards needed (cost already covered)"
            pitched = [player.hand[i] for i in action.pitch_indices if i < len(player.hand)]
            names = [self._fmt_card(c) for c in pitched]
            total = sum(c.pitch for c in pitched)
            return f"PITCH — {', '.join(names)} (total: {total} resource{'s' if total != 1 else ''})"
        if action.action_type == ActionType.DEFEND:
            if not action.defend_hand_indices and not action.defend_equip_slots:
                return "NO BLOCK — take full damage"
            parts, total = [], 0
            for i in action.defend_hand_indices:
                if 0 <= i < len(player.hand):
                    c = player.hand[i]
                    parts.append(f"{c.name} (def:{c.defense})")
                    total += c.defense
            for slot in action.defend_equip_slots:
                if slot in player.equipment:
                    eq = player.equipment[slot]
                    parts.append(f"{eq.card.name}/{slot} (def:{eq.defense})")
                    total += eq.defense
            return f"BLOCK — {', '.join(parts)} [total:{total}]"
        if action.action_type == ActionType.ARSENAL:
            if action.arsenal_hand_index == -1:
                return "DON'T STORE — skip arsenal"
            card = player.hand[action.arsenal_hand_index]
            return f"STORE — {self._fmt_card(card)}"
        if action.action_type == ActionType.GO_FIRST:
            return "GO FIRST — you take the first turn"
        if action.action_type == ActionType.GO_SECOND:
            return "GO SECOND — opponent takes the first turn"
        if action.action_type == ActionType.PITCH_ORDER:
            if 0 <= action.pitch_order_index < len(player.pitch_zone):
                card = player.pitch_zone[action.pitch_order_index]
                remaining = len(player.pitch_zone)
                return f"PUT NEXT TO BOTTOM — {self._fmt_card(card)} ({remaining} card{'s' if remaining != 1 else ''} remaining)"
        return str(action)

    def _pend(self, legal, player, phase, attack_power=0):
        labels = [self._fmt_action(a, player) for a in legal]
        self.session.set_pending(legal, labels, phase, self.agent_id, attack_power)
        self.session.wait_for_choice()
        return self.session.take_choice()

    def select_action(self, obs, legal, player, opponent):
        return self._pend(legal, player, "ATTACK")

    def select_defend(self, obs, legal, player, attack_power):
        return self._pend(legal, player, "DEFEND", attack_power)

    def select_arsenal(self, obs, legal, player):
        return self._pend(legal, player, "ARSENAL")

    def select_pitch(self, obs, legal, player, pending_card=None):
        return self._pend(legal, player, "PITCH")

    def select_instant(self, obs, legal, player, attack_power=0):
        return self._pend(legal, player, "INSTANT", attack_power)

    def select_reaction(self, obs, legal, player, attack_power=0):
        return self._pend(legal, player, "REACTION", attack_power)

    def select_pitch_order(self, obs, legal, player):
        return self._pend(legal, player, "PITCH_ORDER")

    def select_choose_first(self, legal, player):
        return self._pend(legal, player, "CHOOSE_FIRST")


class _GameSession:
    """
    Holds the state of one running interactive game.

    The game runs in a background daemon thread. When a human decision is
    needed, the game thread calls set_pending() then blocks on wait_for_choice().
    The Flask thread calls submit_choice() which unblocks the game thread.
    """

    def __init__(self):
        self._lock = threading.Lock()
        self._choice_event = threading.Event()
        self._reset_state()

    def _reset_state(self):
        self.status = "idle"        # idle | running | waiting_human | game_over
        self.current_agent = None
        self.phase = None
        self.legal_labels: list = []
        self._legal_actions: list = []
        self.attack_power = 0
        self.winner = None
        self.player_stats: dict = {}
        self.gamestate: dict = {}
        self.log_lines: list = []
        self.log_total = 0
        self.log_lines_p0: list = []
        self.log_total_p0 = 0
        self.log_lines_p1: list = []
        self.log_total_p1 = 0
        self.p0_agent = "ai"
        self.p1_agent = "ai"
        self._cancelled = False
        self._choice_idx = None

    # ── called from Flask thread ──────────────────────────────

    def reset(self):
        with self._lock:
            self._cancelled = True
            self._reset_state()
        self._choice_event.set()   # unblock any waiting game thread

    def start(self, p0_agent: str, p1_agent: str, seed,
              deck0_id: int = None, deck1_id: int = None):
        self.reset()
        with self._lock:
            self._cancelled = False
            self.status = "running"
            self.p0_agent = p0_agent
            self.p1_agent = p1_agent
        self._choice_event.clear()
        t = threading.Thread(
            target=self._run,
            args=(p0_agent, p1_agent, seed, deck0_id, deck1_id),
            daemon=True,
        )
        t.start()

    def submit_choice(self, idx: int) -> bool:
        with self._lock:
            if self.status != "waiting_human":
                return False
            if not (0 <= idx < len(self._legal_actions)):
                return False
            self._choice_idx = idx
            self.status = "running"
        self._choice_event.set()
        return True

    def get_state_json(self) -> dict:
        with self._lock:
            return {
                "status": self.status,
                "current_agent": self.current_agent,
                "phase": self.phase,
                "legal_actions": [
                    {"index": i, "label": lbl}
                    for i, lbl in enumerate(self.legal_labels)
                ],
                "attack_power": self.attack_power,
                "log": list(self.log_lines[-150:]),
                "log_total": self.log_total,
                "log_p0": list(self.log_lines_p0[-150:]),
                "log_total_p0": self.log_total_p0,
                "log_p1": list(self.log_lines_p1[-150:]),
                "log_total_p1": self.log_total_p1,
                "winner": self.winner,
                "player_stats": dict(self.player_stats),
                "gamestate": dict(self.gamestate),
                "p0_agent": self.p0_agent,
                "p1_agent": self.p1_agent,
            }

    # ── called from game thread ───────────────────────────────

    def append_log(self, msg: str):
        with self._lock:
            for line in msg.split("\n"):
                if line.strip():
                    self.log_lines.append(line)
                    self.log_total += 1
            if len(self.log_lines) > 500:
                self.log_lines = self.log_lines[-500:]

    def append_log_p0(self, msg: str):
        with self._lock:
            for line in msg.split("\n"):
                if line.strip():
                    self.log_lines_p0.append(line)
                    self.log_total_p0 += 1
            if len(self.log_lines_p0) > 500:
                self.log_lines_p0 = self.log_lines_p0[-500:]

    def append_log_p1(self, msg: str):
        with self._lock:
            for line in msg.split("\n"):
                if line.strip():
                    self.log_lines_p1.append(line)
                    self.log_total_p1 += 1
            if len(self.log_lines_p1) > 500:
                self.log_lines_p1 = self.log_lines_p1[-500:]

    def set_pending(self, legal, labels, phase, agent_id, attack_power=0):
        with self._lock:
            self._legal_actions = legal
            self.legal_labels = labels
            self.phase = phase
            self.current_agent = agent_id
            self.attack_power = attack_power
            self.status = "waiting_human"
        self._choice_event.clear()

    def wait_for_choice(self):
        self._choice_event.wait()

    def take_choice(self):
        with self._lock:
            if self._cancelled:
                raise _GameCancelled()
            return self._legal_actions[self._choice_idx]

    def _update_stats(self, env):
        stats = {}
        for i, p in enumerate(env._game.players):
            stats[f"agent_{i}"] = {
                "name": p.name,
                "hero": p.hero_name,
                "life": p.life,
                "hand_size": len(p.hand),
                "action_points": p.action_points,
                "resource_points": p.resource_points,
            }
        gamestate = _build_gamestate_snapshot(env)
        with self._lock:
            self.player_stats = stats
            self.gamestate = gamestate

    def _run(self, p0_agent: str, p1_agent: str, seed,
             deck0_id=None, deck1_id=None):
        try:
            self._run_inner(p0_agent, p1_agent, seed, deck0_id, deck1_id)
        except _GameCancelled:
            pass
        except Exception as exc:
            self.append_log(f"  ⚠  Game error: {exc}")
            with self._lock:
                self.status = "game_over"
                self.winner = None

    def _run_inner(self, p0_agent: str, p1_agent: str, seed,
                   deck0_id=None, deck1_id=None):
        from fab_env import FaBEnv, Phase
        from agents import RhinarAgent, DorintheiAgent
        from mcts_agent import MCTSAgent

        env = FaBEnv(verbose=False, log_callback=self.append_log,
                     log_callback_p0=self.append_log_p0,
                     log_callback_p1=self.append_log_p1)

        def _make_agent(agent_type: str, player_idx: int):
            if agent_type == "human":
                return _WebHumanAgent(self, f"agent_{player_idx}")
            if agent_type == "mcts":
                return MCTSAgent(player_idx=player_idx)
            return RhinarAgent() if player_idx == 0 else DorintheiAgent()

        agent_0 = _make_agent(p0_agent, 0)
        agent_1 = _make_agent(p1_agent, 1)

        # Build players from selected decks
        p0 = None
        p1 = None
        if deck0_id is not None:
            rec = deck_db.get_deck(deck0_id)
            if rec:
                p0 = _player_from_deck(rec)
        if deck1_id is not None:
            rec = deck_db.get_deck(deck1_id)
            if rec:
                p1 = _player_from_deck(rec)

        obs, _ = env.reset(seed=seed, player0=p0, player1=p1)

        for _agent in (agent_0, agent_1):
            if hasattr(_agent, "set_env"):
                _agent.set_env(env)

        while not env.done:
            agent_id = env.agent_selection
            agent = agent_0 if agent_id == "agent_0" else agent_1
            player_idx = int(agent_id[-1])
            player = env._game.players[player_idx]
            opponent = env._game.players[1 - player_idx]

            legal = env.legal_actions()
            if not legal:
                break

            self._update_stats(env)
            with self._lock:
                self.current_agent = agent_id
                self.phase = env._phase.name

            if env._phase == Phase.CHOOSE_FIRST:
                action = agent.select_choose_first(legal, player)
            elif env._phase == Phase.ATTACK:
                action = agent.select_action(obs[agent_id], legal, player, opponent)
            elif env._phase == Phase.DEFEND:
                action = agent.select_defend(obs[agent_id], legal, player,
                                             env._pending_attack_power)
            elif env._phase == Phase.INSTANT:
                # Only expose attack_power when the window is an attack
                # reaction (pending attack set); end-of-combat / end-of-turn
                # windows should show no incoming-power badge.
                ap = (env._pending_attack_power
                      if env._pending_attack is not None else 0)
                action = agent.select_instant(obs[agent_id], legal, player, ap)
            elif env._phase == Phase.REACTION:
                action = agent.select_reaction(obs[agent_id], legal, player,
                                               env._pending_attack_power)
            elif env._phase == Phase.ARSENAL:
                action = agent.select_arsenal(obs[agent_id], legal, player)
            elif env._phase == Phase.PITCH:
                action = agent.select_pitch(obs[agent_id], legal, player,
                                            env._pending_play_card)
            elif env._phase == Phase.PITCH_ORDER:
                action = agent.select_pitch_order(obs[agent_id], legal, player)
            else:
                action = legal[0]

            obs, _, _, _, _ = env.step(action)
            self._update_stats(env)

        winner_name = None
        for aid in env.agents:
            if env._rewards.get(aid, 0) > 0:
                idx = int(aid[-1])
                winner_name = env._game.players[idx].name
                break

        with self._lock:
            self.winner = winner_name
            self.status = "game_over"


_session = _GameSession()


# ──────────────────────────────────────────────────────────────
# Interactive play — HTML template
# ──────────────────────────────────────────────────────────────

PLAY_TEMPLATE = """
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>FaB — Interactive Game</title>
  <style>
    {{ css }}

    /* ── Setup form ──────────────────────────────────── */
    #setup-area { padding: 16px; max-width: 480px; margin: 0 auto; }
    .setup-card {
      background: #1a202c; border: 1px solid #2d3748;
      border-radius: 10px; padding: 20px;
    }
    .setup-card h2 { font-size: 1rem; font-weight: 700; margin-bottom: 16px; color: #f6e05e; }
    .hero-row { display: flex; align-items: center; gap: 16px; margin-bottom: 16px; }
    .hero-col { flex: 1; }
    .hero-col h3 { font-size: 0.88rem; font-weight: 700; margin-bottom: 8px; color: #e2e8f0; }
    .agent-select {
      width: 100%;
      background: #0f1117;
      border: 1px solid #4a5568;
      border-radius: 6px;
      color: #e2e8f0;
      font-size: 0.83rem;
      padding: 5px 8px;
    }
    .agent-select:focus { outline: none; border-color: #63b3ed; }
    .vs-col { color: #4a5568; font-weight: 800; font-size: 1.1rem; }
    .deck-select {
      width: 100%;
      background: #0f1117;
      border: 1px solid #4a5568;
      border-radius: 6px;
      color: #e2e8f0;
      font-size: 0.8rem;
      padding: 5px 8px;
      margin-bottom: 10px;
    }
    .deck-select:focus { outline: none; border-color: #63b3ed; }
    .slot-label { font-size: 0.72rem; color: #718096; margin-bottom: 4px; }
    .seed-row { display: flex; align-items: center; gap: 10px; margin-bottom: 14px; }
    .seed-row label { font-size: 0.83rem; color: #a0aec0; white-space: nowrap; }
    .start-btn {
      width: 100%; padding: 10px; background: #2b6cb0;
      border: none; border-radius: 8px; color: #fff;
      font-size: 0.95rem; font-weight: 700; cursor: pointer; font-family: inherit;
    }
    .start-btn:hover { background: #3182ce; }
    .start-btn:disabled { background: #4a5568; cursor: not-allowed; }

    /* ── Stats bar ───────────────────────────────────── */
    #stats-bar {
      display: flex; align-items: stretch;
      background: #1a202c; border-bottom: 1px solid #2d3748;
      position: sticky; top: 48px; z-index: 5;
    }
    .player-box {
      flex: 1; padding: 8px 12px;
      border-right: 1px solid #2d3748;
      transition: background 0.2s;
    }
    .player-box:last-child { border-right: none; }
    .player-box.active { background: #1e3a5f; }
    .player-name { font-weight: 700; font-size: 0.82rem; color: #e2e8f0; }
    .player-life-row { display: flex; align-items: baseline; gap: 8px; }
    .player-life { font-size: 1.25rem; font-weight: 800; color: #fc8181; line-height: 1.2; }
    .player-resources { font-size: 0.72rem; color: #90cdf4; white-space: nowrap; }
    .player-meta { font-size: 0.68rem; color: #718096; margin-top: 2px; }
    .turn-box {
      display: flex; flex-direction: column;
      align-items: center; justify-content: center;
      padding: 6px 12px; border-right: 1px solid #2d3748;
      min-width: 70px;
    }
    .turn-label { font-size: 0.65rem; color: #718096; text-transform: uppercase; }
    .phase-label { font-size: 0.78rem; font-weight: 700; color: #f6e05e; }

    /* ── View tabs ───────────────────────────────────── */
    #view-tabs {
      display: flex; gap: 6px; padding: 6px 12px 0;
      background: #0a0e18; border-bottom: 1px solid #2d3748;
      flex-wrap: wrap;
    }
    .view-tab {
      background: #1a202c; border: 1px solid #2d3748;
      border-bottom: none; border-radius: 6px 6px 0 0;
      color: #a0aec0; font-size: 0.78rem;
      font-family: inherit; padding: 5px 10px;
      cursor: pointer;
    }
    .view-tab:hover { background: #2d3748; color: #e2e8f0; }
    .view-tab.active {
      background: #1e3a5f; color: #f6e05e;
      border-color: #2b6cb0;
    }

    /* ── Log area ────────────────────────────────────── */
    #log-area {
      font-family: 'Menlo', 'Monaco', 'Courier New', monospace;
      font-size: 0.75rem; line-height: 1.5;
      padding: 10px 14px; overflow-y: auto; height: 38vh;
      background: #0a0e18; border-bottom: 1px solid #2d3748;
      white-space: pre-wrap; word-break: break-word;
    }
    #log-area.compact { height: 20vh; }

    /* ── Gamestate area ──────────────────────────────── */
    #gamestate-area {
      padding: 10px 12px; overflow-y: auto; height: 20vh;
      background: #0a0e18; border-bottom: 1px solid #2d3748;
      font-size: 0.78rem;
    }
    .gs-side {
      background: #111826; border: 1px solid #2d3748;
      border-radius: 8px; padding: 8px 10px; margin-bottom: 10px;
    }
    .gs-side.self { border-color: #3a6a9a; }
    .gs-side.opp  { border-color: #6a3a3a; }
    .gs-side h3 {
      font-size: 0.82rem; font-weight: 700; margin-bottom: 6px;
      display: flex; align-items: center; gap: 8px;
      color: #e2e8f0;
    }
    .gs-side h3 .tag {
      font-size: 0.65rem; padding: 1px 6px; border-radius: 4px;
      background: #2d3748; color: #a0aec0; font-weight: 600;
    }
    .gs-side h3 .life { color: #fc8181; font-weight: 800; }
    .gs-side h3 .gs-ap { color: #f6e05e; font-size: 0.72rem; font-weight: 600; }
    .gs-side h3 .gs-rp { color: #90cdf4; font-size: 0.72rem; font-weight: 600; }
    .gs-zone {
      margin-top: 6px;
    }
    .gs-zone .label {
      font-size: 0.68rem; text-transform: uppercase;
      color: #718096; margin-bottom: 3px; letter-spacing: 0.04em;
    }
    .gs-cards {
      display: flex; flex-wrap: wrap; gap: 4px;
    }
    .gs-card {
      background: #1a202c; border: 1px solid #2d3748;
      border-radius: 4px; padding: 3px 6px;
      font-size: 0.72rem; color: #e2e8f0;
      display: inline-flex; align-items: center; gap: 4px;
      max-width: 100%;
    }
    .gs-card.hidden {
      background: #2d3748; color: #718096; font-style: italic;
      border-style: dashed;
    }
    .gs-card .pip {
      display: inline-block; width: 8px; height: 8px; border-radius: 50%;
      border: 1px solid #1a202c;
    }
    .gs-card .pip.red    { background: #e53e3e; }
    .gs-card .pip.yellow { background: #f6e05e; }
    .gs-card .pip.blue   { background: #63b3ed; }
    .gs-card .stats {
      color: #a0aec0; font-size: 0.65rem;
    }
    .gs-empty { color: #4a5568; font-style: italic; font-size: 0.7rem; }
    .gs-chain {
      background: #1a1a2e; border: 1px solid #4a4a7a;
      border-radius: 8px; padding: 8px 10px; margin-bottom: 10px;
    }
    .gs-chain h3 {
      font-size: 0.82rem; font-weight: 700; color: #f6e05e;
      margin-bottom: 6px;
    }
    .gs-chain .subline {
      font-size: 0.72rem; color: #a0aec0; margin-bottom: 4px;
    }
    .gs-chain .incoming { color: #fc8181; font-weight: 700; }

    /* ── Action panel ────────────────────────────────── */
    #action-panel { padding: 12px 14px; overflow-y: auto; max-height: calc(100vh - 38vh - 120px); }
    .phase-info {
      font-size: 0.8rem; font-weight: 700; color: #a0aec0;
      margin-bottom: 10px; display: flex; align-items: center;
      gap: 10px; flex-wrap: wrap;
    }
    .atk-badge {
      background: #7b341e; color: #feb2b2;
      padding: 2px 10px; border-radius: 12px; font-size: 0.75rem;
    }
    .action-btn {
      display: block; width: 100%; text-align: left;
      background: #1a365d; border: 1px solid #2b4c7e;
      border-radius: 8px; color: #e2e8f0;
      font-size: 0.82rem; padding: 9px 12px; margin-bottom: 7px;
      cursor: pointer; font-family: inherit; transition: background 0.12s;
    }
    .action-btn:hover:not(:disabled) { background: #2a4c84; border-color: #4a80cc; }
    .action-btn:disabled { opacity: 0.45; cursor: not-allowed; }
    .action-btn.muted {
      background: #2d3748; border-color: #4a5568; color: #a0aec0;
    }
    .action-btn.muted:hover:not(:disabled) { background: #3d4a5e; }
    .ai-thinking {
      color: #718096; font-size: 0.88rem; text-align: center;
      padding: 14px 0; animation: pulse 1.4s ease-in-out infinite;
    }
    @keyframes pulse { 0%,100%{opacity:1} 50%{opacity:0.35} }
    .winner-msg {
      font-size: 1.35rem; font-weight: 800; color: #f6e05e;
      text-align: center; padding: 18px 0 10px;
    }
    .winner-msg.draw { color: #a0aec0; }
    .new-game-btn {
      display: block; width: 100%; padding: 10px;
      background: #2d3748; border: 1px solid #4a5568;
      border-radius: 8px; color: #e2e8f0;
      font-size: 0.9rem; font-weight: 600;
      cursor: pointer; font-family: inherit; margin-top: 6px;
    }
    .new-game-btn:hover { background: #3d4a5e; }
    #quit-bar {
      display: flex; justify-content: flex-end;
      padding: 4px 12px 0;
    }
    .quit-btn {
      background: none; border: 1px solid #4a5568;
      border-radius: 6px; color: #718096;
      font-size: 0.78rem; font-family: inherit;
      padding: 3px 10px; cursor: pointer;
    }
    .quit-btn:hover { background: #2d3748; color: #e2e8f0; border-color: #718096; }

    /* Log colouring (applied by JS) */
    .turn-header   { color: #f6e05e; font-weight: bold; }
    .life-line     { color: #fc8181; }
    .attack-line   { color: #f6ad55; }
    .defend-line   { color: #68d391; }
    .damage-line   { color: #fc8181; font-weight: bold; }
    .go-again-line { color: #76e4f7; }
    .game-over     { color: #f6e05e; font-weight: bold; }
    .wins-line     { color: #68d391; font-weight: bold; }
    .draw-line     { color: #a0aec0; }
    .pitch-line    { color: #b794f4; }
    .hand-line     { color: #90cdf4; }
    .store-line    { color: #e9d8fd; }
  </style>
</head>
<body>
  <header>
    <a class="back-link" href="/">← Logs</a>
    <div style="flex:1">
      <h1>⚔️ Interactive Game</h1>
    </div>
  </header>

  <!-- ── Setup form ── -->
  <div id="setup-area">
    <div class="setup-card">
      <h2>New Game</h2>
      <form id="start-form">
        <div class="hero-row">
          <div class="hero-col">
            <h3>🐾 Player 1</h3>
            <div class="slot-label">Deck</div>
            <select name="deck0" id="deck0" class="deck-select" required>
              <option value="" disabled selected>— select a deck —</option>
              {% for d in saved_decks %}
                <option value="{{ d.id }}">{{ d.name }} ({{ d.hero }})</option>
              {% endfor %}
            </select>
            <div class="slot-label">Agent</div>
            <select name="player0" class="agent-select">
              <option value="ai">🤖 AI</option>
              <option value="mcts">🌲 MCTS</option>
              <option value="human">👤 Human</option>
            </select>
          </div>
          <div class="vs-col">VS</div>
          <div class="hero-col">
            <h3>⚔️ Player 2</h3>
            <div class="slot-label">Deck</div>
            <select name="deck1" id="deck1" class="deck-select" required>
              <option value="" disabled selected>— select a deck —</option>
              {% for d in saved_decks %}
                <option value="{{ d.id }}">{{ d.name }} ({{ d.hero }})</option>
              {% endfor %}
            </select>
            <div class="slot-label">Agent</div>
            <select name="player1" class="agent-select">
              <option value="ai">🤖 AI</option>
              <option value="mcts">🌲 MCTS</option>
              <option value="human">👤 Human</option>
            </select>
          </div>
        </div>
        <div class="seed-row">
          <label>Seed:</label>
          <input type="number" name="seed" placeholder="random" min="0" class="seed-input">
        </div>
        <button type="submit" class="start-btn">▶ Start Game</button>
      </form>
    </div>
  </div>

  <!-- ── Game area ── -->
  <div id="game-area" style="display:none">
    <div id="stats-bar">
      <div class="player-box" id="box-a0">
        <div class="player-name" id="name-a0">Rhinar</div>
        <div class="player-life-row">
          <div class="player-life" id="life-a0">❤️ 20</div>
          <div class="player-resources" id="res-a0"></div>
        </div>
        <div class="player-meta" id="meta-a0"></div>
      </div>
      <div class="turn-box">
        <div class="turn-label">Phase</div>
        <div class="phase-label" id="phase-disp">—</div>
      </div>
      <div class="player-box" id="box-a1">
        <div class="player-name" id="name-a1">Dorinthea</div>
        <div class="player-life-row">
          <div class="player-life" id="life-a1">❤️ 20</div>
          <div class="player-resources" id="res-a1"></div>
        </div>
        <div class="player-meta" id="meta-a1"></div>
      </div>
    </div>

    <div id="quit-bar">
      <button class="quit-btn" onclick="quitGame()">✕ Quit &amp; New Game</button>
    </div>

    <div id="view-tabs"></div>

    <div id="log-label" style="padding:2px 14px;font-size:0.65rem;color:#718096;background:#0a0e18;border-bottom:1px solid #1a2030;display:none"></div>
    <div id="log-area"></div>
    <div id="gamestate-area" style="display:none"></div>

    <div id="action-panel">
      <div id="status-msg"></div>
      <div id="action-btns"></div>
    </div>
  </div>

  <script>
    let lastLogKey = null;
    let lastActionKey = null;
    let polling = false;
    let activeView = 'log';     // 'log' | 'p0' | 'p1'
    let lastViewKey = null;
    let lastTabKey = null;

    // ── Polling ────────────────────────────────────────────────
    async function poll() {
      if (polling) return;
      polling = true;
      try {
        const r = await fetch('/play/state');
        if (!r.ok) return;
        const s = await r.json();
        updateUI(s);
      } catch(e) { /* ignore network hiccups */ }
      finally { polling = false; }
    }

    function updateUI(s) {
      const idle = (s.status === 'idle');
      document.getElementById('setup-area').style.display = idle ? '' : 'none';
      document.getElementById('game-area').style.display  = idle ? 'none' : '';
      if (idle) return;

      updateStats(s);
      updateViewTabs(s);
      updateLog(s);
      updateGamestate(s);
      updatePanel(s);
    }

    // ── View tabs ──────────────────────────────────────────────
    function updateViewTabs(s) {
      const tabs = [{key: 'log', label: '📜 Logs'}];
      if (s.p0_agent === 'human') {
        const name = (s.player_stats.agent_0 && s.player_stats.agent_0.name) || 'Player 1';
        tabs.push({key: 'p0', label: '👁 ' + name + ' view'});
      }
      if (s.p1_agent === 'human') {
        const name = (s.player_stats.agent_1 && s.player_stats.agent_1.name) || 'Player 2';
        tabs.push({key: 'p1', label: '👁 ' + name + ' view'});
      }

      const validKeys = tabs.map(t => t.key);
      if (!validKeys.includes(activeView)) activeView = 'log';

      const tabKey = tabs.map(t => t.key + ':' + t.label).join('|') + '#' + activeView;
      if (tabKey !== lastTabKey) {
        lastTabKey = tabKey;
        const container = document.getElementById('view-tabs');
        container.innerHTML = tabs.map(t => {
          const cls = 'view-tab' + (t.key === activeView ? ' active' : '');
          return `<button class="${cls}" onclick="setView('${t.key}')">${escHtml(t.label)}</button>`;
        }).join('');
      }

      const inPlayerView = (activeView === 'p0' || activeView === 'p1');
      document.getElementById('log-area').style.display = '';
      document.getElementById('log-area').classList.toggle('compact', inPlayerView);
      document.getElementById('gamestate-area').style.display = inPlayerView ? '' : 'none';
      const lbl = document.getElementById('log-label');
      if (activeView === 'p0') {
        const n0 = (s.player_stats.agent_0 && s.player_stats.agent_0.name) || 'Player 1';
        lbl.textContent = '📜 ' + n0 + "'s private log";
        lbl.style.display = '';
      } else if (activeView === 'p1') {
        const n1 = (s.player_stats.agent_1 && s.player_stats.agent_1.name) || 'Player 2';
        lbl.textContent = '📜 ' + n1 + "'s private log";
        lbl.style.display = '';
      } else {
        lbl.textContent = '';
        lbl.style.display = 'none';
      }
    }

    function setView(k) {
      activeView = k;
      lastLogKey = null;    // force log re-render when view changes
      lastTabKey = null;     // force tab highlight re-render
      lastViewKey = null;    // force gamestate re-render
      poll();
    }

    // ── Stats bar ──────────────────────────────────────────────
    function updateStats(s) {
      const agents = ['agent_0', 'agent_1'];
      const suffixes = ['a0', 'a1'];
      const agentTypes = [s.p0_agent, s.p1_agent];
      const agentLabels = {'human': '👤 You', 'mcts': '🌲 MCTS', 'ai': '🤖 AI'};

      agents.forEach((aid, i) => {
        const info = s.player_stats[aid];
        const sfx = suffixes[i];
        if (info) {
          document.getElementById('name-' + sfx).textContent = info.name;
          document.getElementById('life-' + sfx).textContent = '❤️ ' + info.life;
          const resParts = [];
          if (info.action_points > 0) resParts.push('⚡' + info.action_points + ' AP');
          if (info.resource_points > 0) resParts.push('💰' + info.resource_points);
          document.getElementById('res-' + sfx).textContent = resParts.join('  ');
          const lbl = agentLabels[agentTypes[i]] || '🤖 AI';
          document.getElementById('meta-' + sfx).textContent =
            lbl + '  ·  🃏 ' + info.hand_size;
        }
        document.getElementById('box-' + sfx).classList
          .toggle('active', aid === s.current_agent && s.status === 'waiting_human');
      });

      document.getElementById('phase-disp').textContent = s.phase || '—';
    }

    // ── Log area ───────────────────────────────────────────────
    function updateLog(s) {
      let logData, logTotal;
      if (activeView === 'p0') {
        logData = s.log_p0 || [];
        logTotal = s.log_total_p0 || 0;
      } else if (activeView === 'p1') {
        logData = s.log_p1 || [];
        logTotal = s.log_total_p1 || 0;
      } else {
        logData = s.log || [];
        logTotal = s.log_total || 0;
      }

      const key = activeView + ':' + logTotal;
      if (key === lastLogKey) return;
      lastLogKey = key;

      const area = document.getElementById('log-area');
      const atBottom = area.scrollHeight - area.clientHeight <= area.scrollTop + 40;

      area.innerHTML = logData.map(line => {
        const cls = classifyLine(line);
        const esc = line.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
        return cls ? `<span class="${cls}">${esc}</span>` : esc;
      }).join('\\n');

      if (atBottom) area.scrollTop = area.scrollHeight;
    }

    // ── Gamestate area ─────────────────────────────────────────
    function updateGamestate(s) {
      if (activeView === 'log') return;
      const viewKey = activeView + ':' + JSON.stringify(s.gamestate || {});
      if (viewKey === lastViewKey) return;
      lastViewKey = viewKey;

      const area = document.getElementById('gamestate-area');
      const gs = s.gamestate || {};
      const view = (activeView === 'p0') ? gs.p0_view : gs.p1_view;
      if (!view) {
        area.innerHTML = '<div class="gs-empty">Gamestate not available yet…</div>';
        return;
      }
      const viewerIdx = (activeView === 'p0') ? 0 : 1;
      area.innerHTML =
        renderChain(view.combat_chain, viewerIdx) +
        renderSelfSide(view.self) +
        renderOppSide(view.opponent);
    }

    function renderCard(c) {
      if (!c) return '';
      const pip = c.color ? `<span class="pip ${c.color}"></span>` : '';
      const stats = [];
      if (c.pitch)   stats.push('pitch ' + c.pitch);
      if (c.cost)    stats.push('cost ' + c.cost);
      if (c.power)   stats.push(c.power + 'p');
      if (c.defense) stats.push(c.defense + 'd');
      const sub = stats.length ? `<span class="stats">${stats.join(' · ')}</span>` : '';
      return `<span class="gs-card">${pip}<span>${escHtml(c.name)}</span>${sub}</span>`;
    }

    function renderCards(arr) {
      if (!arr || arr.length === 0) return '<span class="gs-empty">— empty —</span>';
      return `<div class="gs-cards">${arr.map(renderCard).join('')}</div>`;
    }

    function renderHiddenCards(n) {
      if (!n) return '<span class="gs-empty">— empty —</span>';
      const hidden = [];
      for (let i = 0; i < n; i++) {
        hidden.push('<span class="gs-card hidden">🂠 hidden</span>');
      }
      return `<div class="gs-cards">${hidden.join('')}</div>`;
    }

    function renderZone(label, inner) {
      return `<div class="gs-zone"><div class="label">${label}</div>${inner}</div>`;
    }

    function renderEquipment(weapon, equipment) {
      const items = [];
      if (weapon) {
        items.push(`<span class="gs-card">🗡 ${escHtml(weapon.name)}<span class="stats">${weapon.power || 0}p</span></span>`);
      }
      (equipment || []).forEach(eq => {
        if (!eq || !eq.card) return;
        const destroyedTag = eq.destroyed ? ' <span class="stats">(destroyed)</span>' : '';
        const cls = 'gs-card' + (eq.destroyed ? ' hidden' : '');
        const slot = eq.slot ? `<span class="stats">${escHtml(eq.slot)}</span>` : '';
        const def = eq.card.defense ? `<span class="stats">${eq.card.defense}d</span>` : '';
        items.push(`<span class="${cls}">🛡 ${escHtml(eq.card.name)}${slot}${def}${destroyedTag}</span>`);
      });
      if (items.length === 0) return '<span class="gs-empty">— none —</span>';
      return `<div class="gs-cards">${items.join('')}</div>`;
    }

    function renderResources(ap, rp) {
      const parts = [];
      if (ap > 0) parts.push(`<span class="gs-ap">⚡${ap} AP</span>`);
      if (rp > 0) parts.push(`<span class="gs-rp">💰${rp}</span>`);
      return parts.join(' ');
    }

    function renderSelfSide(me) {
      const arsenal = me.arsenal
        ? renderCard(me.arsenal)
        : '<span class="gs-empty">— empty —</span>';
      const resHtml = renderResources(me.action_points, me.resource_points);
      const banishHtml = (me.banished && me.banished.length)
        ? renderCards(me.banished)
        : '<span class="gs-empty">— empty —</span>';
      return `
        <div class="gs-side self">
          <h3>
            <span>${escHtml(me.name)}</span>
            <span class="tag">YOU</span>
            <span class="life">❤️ ${me.life}</span>
            ${resHtml}
          </h3>
          ${renderZone('Equipment', renderEquipment(me.weapon, me.equipment))}
          ${renderZone('Hand (' + me.hand.length + ')', renderCards(me.hand))}
          ${renderZone('Arsenal', '<div class="gs-cards">' + arsenal + '</div>')}
          ${renderZone('Pitch zone (' + me.pitch_zone.length + ')', renderCards(me.pitch_zone))}
          ${renderZone('Graveyard (' + me.graveyard.length + ')', renderCards(me.graveyard))}
          ${renderZone('Banished (' + (me.banished ? me.banished.length : 0) + ')', banishHtml)}
        </div>`;
    }

    function renderOppSide(op) {
      const arsenalHtml = op.arsenal_present
        ? '<div class="gs-cards"><span class="gs-card hidden">🂠 face-down</span></div>'
        : '<span class="gs-empty">— empty —</span>';
      const resHtml = renderResources(op.action_points, op.resource_points);
      const oppBanishHtml = (op.banished_count > 0)
        ? renderHiddenCards(op.banished_count)
        : '<span class="gs-empty">— empty —</span>';
      return `
        <div class="gs-side opp">
          <h3>
            <span>${escHtml(op.name)}</span>
            <span class="tag">OPPONENT</span>
            <span class="life">❤️ ${op.life}</span>
            ${resHtml}
          </h3>
          ${renderZone('Equipment', renderEquipment(op.weapon, op.equipment))}
          ${renderZone('Hand (' + op.hand_count + ')', renderHiddenCards(op.hand_count))}
          ${renderZone('Arsenal', arsenalHtml)}
          ${renderZone('Pitch zone (' + op.pitch_zone.length + ')', renderCards(op.pitch_zone))}
          ${renderZone('Graveyard (' + op.graveyard.length + ')', renderCards(op.graveyard))}
          ${renderZone('Banished (' + (op.banished_count || 0) + ')', oppBanishHtml)}
        </div>`;
    }

    function renderChain(ch, viewerIdx) {
      const hasResolved = (ch.chained_attacks && ch.chained_attacks.length > 0)
                       || (ch.chained_defenders && ch.chained_defenders.length > 0);
      const hasPending  = ch && ch.attack_card;

      if (!hasResolved && !hasPending) {
        return `
          <div class="gs-chain">
            <h3>⚔ Combat chain</h3>
            <div class="subline">No active combat chain.</div>
          </div>`;
      }

      const incoming = (ch.attacker_idx !== viewerIdx);
      const attackerLabel = incoming
        ? `<span class="incoming">${escHtml(ch.attacker_name)} attacks you</span>`
        : `You attack ${escHtml(ch.defender_name)}`;

      let resolvedHtml = '';
      if (hasResolved) {
        const atkCards = (ch.chained_attacks && ch.chained_attacks.length)
          ? renderCards(ch.chained_attacks)
          : '<span class="gs-empty">— none —</span>';
        const defCards = (ch.chained_defenders && ch.chained_defenders.length)
          ? renderCards(ch.chained_defenders)
          : '<span class="gs-empty">— none —</span>';
        resolvedHtml = `
          <div class="subline">Resolved links on chain</div>
          ${renderZone('Attacks', atkCards)}
          ${renderZone('Defenders', defCards)}`;
      }

      let pendingHtml = '';
      if (hasPending) {
        const defenders = (ch.defend_cards && ch.defend_cards.length)
          ? renderCards(ch.defend_cards)
          : '<span class="gs-empty">— no blockers committed —</span>';
        const equip = (ch.defend_equipment && ch.defend_equipment.length)
          ? renderZone('Defending equipment', renderCards(ch.defend_equipment)) : '';
        pendingHtml = `
          <div class="subline">Current link — ${attackerLabel}</div>
          ${renderZone('Attack', '<div class="gs-cards">' + renderCard(ch.attack_card) + '</div>')}
          <div class="subline">Power: <b>${ch.attack_power}</b></div>
          ${renderZone('Defenders', defenders)}
          ${equip}`;
      } else {
        pendingHtml = `<div class="subline">${attackerLabel} — awaiting next action</div>`;
      }

      return `
        <div class="gs-chain">
          <h3>⚔ Combat chain</h3>
          ${resolvedHtml}
          ${pendingHtml}
        </div>`;
    }

    function classifyLine(line) {
      if (line.includes('══') || line.includes('★★')) return 'game-over';
      if (line.includes('WINS!'))                       return 'wins-line';
      if (line.includes('TURN ') && !line.includes('══')) return 'turn-header';
      if (line.includes('takes') && line.includes('damage')) return 'damage-line';
      if (line.includes('Life:'))                       return 'life-line';
      if (line.includes('attacks with') || line.includes('⚔')) return 'attack-line';
      if (line.includes('defends') || line.includes('blocks') || line.includes('🛡')) return 'defend-line';
      if (line.includes('Go again') || line.includes('go again') || line.includes('↩')) return 'go-again-line';
      if (line.includes('pitched') || line.includes('▶'))  return 'pitch-line';
      if (line.includes('Hand:') || line.includes('draws') || line.includes('🃏')) return 'hand-line';
      if (line.includes('stores') || line.includes('arsenal') || line.includes('📦')) return 'store-line';
      if (line.includes('Draw') || line.includes('DRAW'))  return 'draw-line';
      return null;
    }

    // ── Action panel ───────────────────────────────────────────
    function updatePanel(s) {
      const msg  = document.getElementById('status-msg');
      const btns = document.getElementById('action-btns');

      if (s.status === 'game_over') {
        lastActionKey = null;
        const w = s.winner;
        msg.innerHTML = w
          ? `<div class="winner-msg">🏆 ${w} wins!</div>`
          : `<div class="winner-msg draw">⏱ Draw / Timeout</div>`;
        btns.innerHTML = '<button class="new-game-btn" onclick="newGame()">← New Game</button>';
        return;
      }

      if (s.status === 'waiting_human') {
        const actorName = s.current_agent === 'agent_0' ? 'Rhinar' : 'Dorinthea';
        let infoHtml = `<div class="phase-info">${actorName} — ${s.phase}`;
        if (s.phase === 'DEFEND') {
          infoHtml += ` <span class="atk-badge">⚔ Incoming: ${s.attack_power} power</span>`;
        } else if (s.phase === 'INSTANT' && s.attack_power > 0) {
          infoHtml += ` <span class="atk-badge">⏸ Reaction window (incoming: ${s.attack_power} power)</span>`;
        }
        infoHtml += '</div>';
        msg.innerHTML = infoHtml;

        const actionKey = s.legal_actions.map(a => a.index + ':' + a.label).join('|');
        if (actionKey !== lastActionKey) {
          lastActionKey = actionKey;
          btns.innerHTML = s.legal_actions.map(a => {
            const isMuted = a.label.startsWith('PASS') || a.label.startsWith('NO BLOCK')
                         || a.label.startsWith("DON'T STORE");
            const cls = 'action-btn' + (isMuted ? ' muted' : '');
            return `<button class="${cls}" onclick="submitAction(${a.index})">${escHtml(a.label)}</button>`;
          }).join('');
        }
        return;
      }

      // running / ai deciding
      lastActionKey = null;
      msg.innerHTML = '<div class="ai-thinking">⟳ AI is deciding…</div>';
      btns.innerHTML = '';
    }

    // ── User actions ───────────────────────────────────────────
    async function submitAction(idx) {
      document.querySelectorAll('.action-btn').forEach(b => b.disabled = true);
      try {
        await fetch('/play/action', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({index: idx}),
        });
      } finally {
        lastActionKey = null;  // force button re-render even if labels are unchanged
        await poll();
      }
    }

    function newGame() {
      fetch('/play/reset', {method: 'POST'}).then(() => {
        lastLogTotal = -1;
        poll();
      });
    }

    function quitGame() {
      if (!confirm('Quit the current game and start a new one?')) return;
      newGame();
    }

    document.getElementById('start-form').addEventListener('submit', async e => {
      e.preventDefault();
      if (!document.getElementById('deck0').value) {
        alert('Please select a deck for Player 1.'); return;
      }
      if (!document.getElementById('deck1').value) {
        alert('Please select a deck for Player 2.'); return;
      }
      const btn = e.target.querySelector('button[type=submit]');
      btn.disabled = true;
      btn.textContent = 'Starting…';
      await fetch('/play/start', {method: 'POST', body: new FormData(e.target)});
      lastLogTotal = -1;
      btn.disabled = false;
      btn.textContent = '▶ Start Game';
      poll();
    });

    function escHtml(s) {
      return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
    }

    setInterval(poll, 500);
    poll();
  </script>
</body>
</html>
"""


# ──────────────────────────────────────────────────────────────
# Interactive play — routes
# ──────────────────────────────────────────────────────────────

@app.route("/play")
def play_page():
    saved_decks = deck_db.list_decks()
    return render_template_string(PLAY_TEMPLATE, css=BASE_CSS, saved_decks=saved_decks)


@app.route("/play/start", methods=["POST"])
def play_start():
    valid_agents = {"human", "ai", "mcts"}
    p0_agent = request.form.get("player0", "ai")
    p1_agent = request.form.get("player1", "ai")
    if p0_agent not in valid_agents:
        p0_agent = "ai"
    if p1_agent not in valid_agents:
        p1_agent = "ai"
    seed_str = request.form.get("seed", "").strip()
    seed = int(seed_str) if seed_str.isdigit() else None
    deck0_str = request.form.get("deck0", "").strip()
    deck1_str = request.form.get("deck1", "").strip()
    deck0_id = int(deck0_str) if deck0_str.isdigit() else None
    deck1_id = int(deck1_str) if deck1_str.isdigit() else None
    _session.start(p0_agent, p1_agent, seed, deck0_id, deck1_id)
    return jsonify({"ok": True})


@app.route("/play/reset", methods=["POST"])
def play_reset():
    _session.reset()
    return jsonify({"ok": True})


@app.route("/play/state")
def play_state():
    return jsonify(_session.get_state_json())


@app.route("/play/action", methods=["POST"])
def play_action():
    data = request.get_json(force=True) or {}
    idx = data.get("index", -1)
    ok = _session.submit_choice(int(idx))
    return jsonify({"ok": ok})


# ──────────────────────────────────────────────────────────────
# Deck builder routes
# ──────────────────────────────────────────────────────────────

@app.route("/decks")
def decks_list():
    decks = deck_db.list_decks()
    return render_template_string(DECKS_TEMPLATE, css=BASE_CSS, decks=decks)


@app.route("/decks/builder")
@app.route("/decks/builder/<int:deck_id>")
def deck_builder(deck_id: int = None):
    import json

    cards = _build_card_catalog()

    if deck_id is not None:
        saved = deck_db.get_deck(deck_id)
        if saved is None:
            abort(404)
        deck_name = saved["name"]
        hero = saved["hero"]
        deck_cards = saved["cards"]
    else:
        deck_name = ""
        hero = "Rhinar"
        deck_cards = {}

    return render_template_string(
        DECK_BUILDER_TEMPLATE,
        css=BASE_CSS,
        cards_json=json.dumps(cards),
        deck_id=json.dumps(deck_id),
        deck_name=deck_name,
        hero=hero,
        deck_cards_json=json.dumps(deck_cards),
    )


# ── Deck API ───────────────────────────────────────────────────

@app.route("/api/cards")
def api_cards():
    return jsonify(_build_card_catalog())


@app.route("/api/decks", methods=["GET"])
def api_list_decks():
    return jsonify(deck_db.list_decks())


@app.route("/api/decks", methods=["POST"])
def api_create_deck():
    data = request.get_json(force=True) or {}
    name = (data.get("name") or "").strip()
    hero = (data.get("hero") or "").strip()
    cards = data.get("cards") or {}
    if not name:
        return jsonify({"error": "name required"}), 400
    bad = _validate_deck_cards(hero, cards)
    if bad:
        return jsonify({"error": f"Cards not allowed in a {_HERO_CLASS.get(hero, hero)} deck: {bad}"}), 400
    deck_id = deck_db.create_deck(name, hero, cards)
    return jsonify({"id": deck_id}), 201


@app.route("/api/decks/<int:deck_id>", methods=["GET"])
def api_get_deck(deck_id: int):
    d = deck_db.get_deck(deck_id)
    if d is None:
        abort(404)
    return jsonify(d)


@app.route("/api/decks/<int:deck_id>", methods=["PUT"])
def api_update_deck(deck_id: int):
    data = request.get_json(force=True) or {}
    name = (data.get("name") or "").strip()
    hero = (data.get("hero") or "").strip()
    cards = data.get("cards") or {}
    if not name:
        return jsonify({"error": "name required"}), 400
    bad = _validate_deck_cards(hero, cards)
    if bad:
        return jsonify({"error": f"Cards not allowed in a {_HERO_CLASS.get(hero, hero)} deck: {bad}"}), 400
    ok = deck_db.update_deck(deck_id, name, hero, cards)
    if not ok:
        abort(404)
    return jsonify({"ok": True})


@app.route("/api/decks/<int:deck_id>", methods=["DELETE"])
def api_delete_deck(deck_id: int):
    ok = deck_db.delete_deck(deck_id)
    if not ok:
        abort(404)
    return jsonify({"ok": True})


# ──────────────────────────────────────────────────────────────
# Entry point
# ──────────────────────────────────────────────────────────────

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="FaB Game Log Web Viewer")
    parser.add_argument("--port", type=int, default=5000, help="Port to listen on (default: 5000)")
    parser.add_argument("--host", default="0.0.0.0", help="Host to bind to (default: 0.0.0.0)")
    parser.add_argument("--ssl-cert", metavar="CERT", help="Path to SSL certificate file (PEM) to enable HTTPS")
    parser.add_argument("--ssl-key", metavar="KEY", help="Path to SSL private key file (PEM) to enable HTTPS")
    args = parser.parse_args()

    ssl_context = None
    if args.ssl_cert or args.ssl_key:
        if not args.ssl_cert or not args.ssl_key:
            parser.error("--ssl-cert and --ssl-key must both be provided to enable HTTPS")
        ssl_context = (args.ssl_cert, args.ssl_key)

    scheme = "https" if ssl_context else "http"
    print(f"\n  FaB Game Log Viewer")
    print(f"  ───────────────────")
    print(f"  Serving logs from: {LOGS_DIR}")
    print(f"  Open on your phone: {scheme}://<your-ip>:{args.port}")
    print(f"  Local:              {scheme}://localhost:{args.port}")
    if ssl_context:
        print(f"  SSL:                enabled")
    print()

    app.run(host=args.host, port=args.port, debug=False, ssl_context=ssl_context)

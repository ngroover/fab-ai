"""
web_viewer.py — Mobile-friendly web UI for viewing FaB game logs.

Usage:
  python web_viewer.py             # serves on http://0.0.0.0:5000
  python web_viewer.py --port 8080 # custom port

Open http://<your-machine-ip>:5000 on your phone to browse logs.
Generate logs with:  python run_env.py --log
"""

from __future__ import annotations

import argparse
import os
from datetime import datetime
from pathlib import Path

from flask import Flask, abort, render_template_string, request

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
  </style>
</head>
<body>
  <header>
    <div>
      <h1>⚔️ FaB Game Logs</h1>
      <div class="subtitle">Flesh and Blood — Classic Battles</div>
    </div>
    <a class="refresh-btn" href="/">↻ Refresh</a>
  </header>
  <div class="container">
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
        elif "♥" in raw or "Life:" in raw:
            cls = "life-line"
        elif "takes" in raw and "damage" in raw:
            cls = "damage-line"
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


# ──────────────────────────────────────────────────────────────
# Entry point
# ──────────────────────────────────────────────────────────────

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="FaB Game Log Web Viewer")
    parser.add_argument("--port", type=int, default=5000, help="Port to listen on (default: 5000)")
    parser.add_argument("--host", default="0.0.0.0", help="Host to bind to (default: 0.0.0.0)")
    args = parser.parse_args()

    print(f"\n  FaB Game Log Viewer")
    print(f"  ───────────────────")
    print(f"  Serving logs from: {LOGS_DIR}")
    print(f"  Open on your phone: http://<your-ip>:{args.port}")
    print(f"  Local:              http://localhost:{args.port}\n")

    app.run(host=args.host, port=args.port, debug=False)

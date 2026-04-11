"""
deck_db.py — SQLite persistence layer for FaB deck lists.

Schema
------
decklists        id, name, hero, created_at, updated_at
decklist_cards   decklist_id, card_id, quantity
"""

from __future__ import annotations

import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional

DB_PATH = Path(__file__).parent / "decks.db"


def _utcnow() -> str:
    return datetime.now(timezone.utc).isoformat()


@contextmanager
def _conn():
    con = sqlite3.connect(DB_PATH)
    con.row_factory = sqlite3.Row
    con.execute("PRAGMA foreign_keys = ON")
    try:
        yield con
        con.commit()
    except Exception:
        con.rollback()
        raise
    finally:
        con.close()


def init_db() -> None:
    """Create tables if they do not already exist."""
    with _conn() as con:
        con.executescript("""
            CREATE TABLE IF NOT EXISTS decklists (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT    NOT NULL,
                hero       TEXT    NOT NULL,
                created_at TEXT    NOT NULL,
                updated_at TEXT    NOT NULL
            );

            CREATE TABLE IF NOT EXISTS decklist_cards (
                decklist_id INTEGER NOT NULL
                    REFERENCES decklists(id) ON DELETE CASCADE,
                card_id     TEXT    NOT NULL,
                quantity    INTEGER NOT NULL DEFAULT 1,
                PRIMARY KEY (decklist_id, card_id)
            );

            CREATE TABLE IF NOT EXISTS card_catalog (
                card_id    TEXT    PRIMARY KEY,
                name       TEXT    NOT NULL,
                card_type  TEXT    NOT NULL,
                cost       INTEGER NOT NULL DEFAULT 0,
                pitch      INTEGER NOT NULL DEFAULT 0,
                power      INTEGER NOT NULL DEFAULT 0,
                defense    INTEGER NOT NULL DEFAULT 0,
                color      TEXT,
                go_again   INTEGER NOT NULL DEFAULT 0,
                intimidate INTEGER NOT NULL DEFAULT 0,
                no_block   INTEGER NOT NULL DEFAULT 0,
                equip_slot TEXT,
                card_class TEXT    NOT NULL DEFAULT 'Generic',
                text       TEXT    NOT NULL DEFAULT '',
                blitz_legal INTEGER NOT NULL DEFAULT 0,
                cc_legal    INTEGER NOT NULL DEFAULT 0
            );
        """)


# ── CRUD ─────────────────────────────────────────────────────────────────────

def create_deck(name: str, hero: str, cards: Dict[str, int]) -> int:
    """
    Insert a new decklist.

    Parameters
    ----------
    name  : display name for the deck
    hero  : hero name (e.g. "Rhinar" or "Dorinthea")
    cards : mapping of card_id -> quantity

    Returns the new deck id.
    """
    now = _utcnow()
    with _conn() as con:
        cur = con.execute(
            "INSERT INTO decklists (name, hero, created_at, updated_at) VALUES (?, ?, ?, ?)",
            (name, hero, now, now),
        )
        deck_id = cur.lastrowid
        con.executemany(
            "INSERT INTO decklist_cards (decklist_id, card_id, quantity) VALUES (?, ?, ?)",
            [(deck_id, cid, qty) for cid, qty in cards.items() if qty > 0],
        )
    return deck_id


def list_decks() -> List[dict]:
    """Return all decklists (without card contents)."""
    with _conn() as con:
        rows = con.execute(
            "SELECT id, name, hero, created_at, updated_at FROM decklists ORDER BY updated_at DESC"
        ).fetchall()
    return [dict(r) for r in rows]


def get_deck(deck_id: int) -> Optional[dict]:
    """Return a single decklist with its card contents, or None if not found."""
    with _conn() as con:
        row = con.execute(
            "SELECT id, name, hero, created_at, updated_at FROM decklists WHERE id = ?",
            (deck_id,),
        ).fetchone()
        if row is None:
            return None
        deck = dict(row)
        card_rows = con.execute(
            "SELECT card_id, quantity FROM decklist_cards WHERE decklist_id = ? ORDER BY card_id",
            (deck_id,),
        ).fetchall()
    deck["cards"] = {r["card_id"]: r["quantity"] for r in card_rows}
    return deck


def update_deck(deck_id: int, name: str, hero: str, cards: Dict[str, int]) -> bool:
    """
    Replace a decklist's metadata and card contents.

    Returns True if the deck existed and was updated, False otherwise.
    """
    with _conn() as con:
        cur = con.execute(
            "UPDATE decklists SET name = ?, hero = ?, updated_at = ? WHERE id = ?",
            (name, hero, _utcnow(), deck_id),
        )
        if cur.rowcount == 0:
            return False
        con.execute("DELETE FROM decklist_cards WHERE decklist_id = ?", (deck_id,))
        con.executemany(
            "INSERT INTO decklist_cards (decklist_id, card_id, quantity) VALUES (?, ?, ?)",
            [(deck_id, cid, qty) for cid, qty in cards.items() if qty > 0],
        )
    return True


def delete_deck(deck_id: int) -> bool:
    """Delete a decklist.  Returns True if it existed."""
    with _conn() as con:
        cur = con.execute("DELETE FROM decklists WHERE id = ?", (deck_id,))
    return cur.rowcount > 0


# ── Card catalog (imported from external sources) ─────────────────────────────

def upsert_catalog_cards(cards: List[dict]) -> int:
    """
    Insert or replace cards in the card_catalog table.

    Parameters
    ----------
    cards : list of dicts with keys matching the card_catalog columns.

    Returns the number of rows inserted/replaced.
    """
    with _conn() as con:
        con.executemany(
            """
            INSERT OR REPLACE INTO card_catalog
                (card_id, name, card_type, cost, pitch, power, defense,
                 color, go_again, intimidate, no_block, equip_slot,
                 card_class, text, blitz_legal, cc_legal)
            VALUES
                (:card_id, :name, :card_type, :cost, :pitch, :power, :defense,
                 :color, :go_again, :intimidate, :no_block, :equip_slot,
                 :card_class, :text, :blitz_legal, :cc_legal)
            """,
            cards,
        )
    return len(cards)


def get_catalog_cards(card_classes: Optional[List[str]] = None) -> List[dict]:
    """
    Return all cards from the card_catalog table as dicts.

    Parameters
    ----------
    card_classes : optional list of class names to filter by
                   (e.g. ["Generic", "Brute", "Warrior"]).
                   If None, all classes are returned.
    """
    with _conn() as con:
        if card_classes:
            placeholders = ",".join("?" * len(card_classes))
            rows = con.execute(
                f"SELECT * FROM card_catalog WHERE card_class IN ({placeholders})"
                " ORDER BY name, pitch",
                card_classes,
            ).fetchall()
        else:
            rows = con.execute(
                "SELECT * FROM card_catalog ORDER BY name, pitch"
            ).fetchall()
    return [dict(r) for r in rows]


def clear_catalog_cards() -> int:
    """Delete all rows from card_catalog.  Returns the number of rows deleted."""
    with _conn() as con:
        cur = con.execute("DELETE FROM card_catalog")
    return cur.rowcount

#!/usr/bin/env python3
"""
import_cards.py — Import Flesh and Blood cards from the official open-source
card database into decks.db, making them available for deck building.

Source: git@github.com:the-fab-cube/flesh-and-blood-cards.git

The script clones (or updates) the upstream repository, reads
  csvs/english/card.csv
and upserts the matching cards into the card_catalog table in decks.db.
Cards already stored in the Python-side CARD_CATALOG (the two classic-battle
decks) are skipped so the canonical game data is never overwritten.

Usage:
    python import_cards.py [options]

Options:
    --repo-dir PATH           Local path for the card repo clone.
                              (default: ./flesh-and-blood-cards)
    --filter-class CLASSES    Comma-separated list of card classes to import.
                              Use "all" to import every class.
                              (default: Generic,Brute,Warrior)
    --format FORMAT           Format-legality filter: blitz | cc | commoner
                              (default: blitz)
    --no-update               Skip git pull even if the repo already exists.
    --clear                   Delete all previously imported catalog cards
                              before importing.
    --https                   Clone via HTTPS instead of SSH.
                              Use this if you don't have an SSH key configured.
"""

from __future__ import annotations

import argparse
import csv
import re
import subprocess
import sys
from pathlib import Path

# Ensure the project root is on sys.path so deck_db can be imported when the
# script is run from another directory.
sys.path.insert(0, str(Path(__file__).parent))
import deck_db
from cards import CARD_CATALOG  # existing hardcoded cards — we won't overwrite these

REPO_SSH   = "git@github.com:the-fab-cube/flesh-and-blood-cards.git"
REPO_HTTPS = "https://github.com/the-fab-cube/flesh-and-blood-cards.git"
DEFAULT_REPO_DIR = Path(__file__).parent / "flesh-and-blood-cards"
CARD_CSV_PATH    = "csvs/english/card.csv"

# ── Type mapping ──────────────────────────────────────────────────────────────
# Each entry is (substring_to_match_in_lowercase_types_field, CardType_value).
# Order matters: more-specific patterns must come before broader ones.
_TYPE_RULES: list[tuple[str, str]] = [
    ("attack reaction",   "Attack Reaction"),
    ("defense reaction",  "Defense Reaction"),
    ("action — attack",   "Action - Attack"),   # em-dash variant
    ("action - attack",   "Action - Attack"),   # hyphen variant
    ("action attack",     "Action - Attack"),   # no separator
    ("instant",           "Instant"),
    ("mentor",            "Mentor"),
    ("hero",              "Hero"),
    ("resource",          "Resource"),
    ("token",             "Token"),
    ("weapon",            "Weapon"),
    ("equipment",         "Equipment"),
    ("action",            "Action"),
]

# Equipment slot keywords found inside the Types string
_EQUIP_SLOT_RULES: list[tuple[str, str]] = [
    ("head",      "head"),
    ("chest",     "chest"),
    ("arms",      "arms"),
    ("legs",      "legs"),
    ("off-hand",  "arms"),   # closest EquipSlot equivalent
]

# Card-class traits we recognise (lowercase → display form)
_KNOWN_CLASSES: dict[str, str] = {
    "generic":        "Generic",
    "brute":          "Brute",
    "warrior":        "Warrior",
    "ranger":         "Ranger",
    "ninja":          "Ninja",
    "wizard":         "Wizard",
    "mechanologist":  "Mechanologist",
    "guardian":       "Guardian",
    "runeblade":      "Runeblade",
    "illusionist":    "Illusionist",
    "shapeshifter":   "Shapeshifter",
    "assassin":       "Assassin",
    "bard":           "Bard",
    "merchant":       "Merchant",
    "draconic":       "Draconic",
    "elemental":      "Elemental",
}

# Format-legality column names in the CSV
_FORMAT_COLUMNS: dict[str, str] = {
    "blitz":    "Blitz Legal",
    "cc":       "CC Legal",
    "commoner": "Commoner Legal",
}


# ── Field parsers ─────────────────────────────────────────────────────────────

def _parse_type(types_str: str) -> str:
    t = types_str.lower()
    for pattern, value in _TYPE_RULES:
        if pattern in t:
            return value
    return "Action"  # safe fallback


def _parse_equip_slot(types_str: str, card_type: str) -> str | None:
    if card_type == "Weapon":
        return "weapon"
    if card_type != "Equipment":
        return None
    t = types_str.lower()
    for key, slot in _EQUIP_SLOT_RULES:
        if key in t:
            return slot
    return None


def _parse_int(value: str, default: int = 0) -> int:
    """Convert a stat field that may be empty or '*' to int."""
    v = value.strip()
    if not v or v in ("*", "X", "XX", "—", "-", "--"):
        return default
    try:
        return int(v)
    except ValueError:
        return default


def _parse_color(value: str) -> str | None:
    v = value.strip().capitalize()
    return v if v in ("Red", "Yellow", "Blue") else None


def _parse_class(traits_str: str) -> str:
    """Return the display-form card class from the Traits field."""
    t = traits_str.lower()
    for key, display in _KNOWN_CLASSES.items():
        if key in t:
            return display
    return "Generic"


def _is_legal(row: dict, format_col: str) -> bool:
    return row.get(format_col, "").strip().lower() == "true"


def _make_card_id(name: str, color: str | None) -> str:
    """Generate a card_id matching the project's existing slug convention."""
    slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
    return f"{slug}-{color.lower()}" if color else slug


# ── Row parser ────────────────────────────────────────────────────────────────

def _parse_row(row: dict, format_col: str) -> dict | None:
    """
    Convert one CSV row to a card dict for deck_db.upsert_catalog_cards().
    Returns None if the card should be skipped.
    """
    if not _is_legal(row, format_col):
        return None

    name = row.get("Name", "").strip()
    if not name:
        return None

    types_str  = row.get("Types", "").strip()
    card_type  = _parse_type(types_str)

    # Never import hero or token cards — they aren't deckable cards
    if card_type in ("Hero", "Token"):
        return None

    color      = _parse_color(row.get("Color", ""))
    pitch      = _parse_int(row.get("Pitch", ""))
    cost       = _parse_int(row.get("Cost", ""))
    power      = _parse_int(row.get("Power", ""))

    # In FaB, an attack card with no defense stat cannot be used to block
    raw_def    = row.get("Defense", "").strip()
    defense    = _parse_int(raw_def)
    no_block   = (raw_def in ("", "—", "-", "--") and card_type == "Action - Attack")

    keywords   = row.get("Card Keywords", "").lower()
    go_again   = "go again"   in keywords
    intimidate = "intimidate" in keywords

    equip_slot = _parse_equip_slot(types_str, card_type)
    card_class = _parse_class(row.get("Traits", ""))

    # Prefer plain-text functional text; fall back to formatted text
    text = (row.get("Functional Text", "") or "").strip()

    card_id = _make_card_id(name, color)

    return {
        "card_id":    card_id,
        "name":       name,
        "card_type":  card_type,
        "cost":       cost,
        "pitch":      pitch,
        "power":      power,
        "defense":    defense,
        "color":      color,
        "go_again":   int(go_again),
        "intimidate": int(intimidate),
        "no_block":   int(no_block),
        "equip_slot": equip_slot,
        "card_class": card_class,
        "text":       text,
        "blitz_legal": int(row.get("Blitz Legal", "").strip().lower() == "true"),
        "cc_legal":    int(row.get("CC Legal",    "").strip().lower() == "true"),
    }


# ── Repo management ───────────────────────────────────────────────────────────

def _clone_or_update(repo_dir: Path, use_https: bool, no_update: bool) -> None:
    url = REPO_HTTPS if use_https else REPO_SSH

    if repo_dir.exists():
        if no_update:
            print(f"  Using existing repo at {repo_dir} (--no-update set).")
            return
        print(f"  Updating existing repo at {repo_dir} ...")
        result = subprocess.run(
            ["git", "pull", "--ff-only"],
            cwd=repo_dir,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"  Warning: git pull failed — {result.stderr.strip()}")
            print("  Continuing with the existing local copy.")
        else:
            msg = result.stdout.strip()
            print(f"  {msg}" if msg else "  Already up to date.")
    else:
        print(f"  Cloning {url}")
        print(f"  -> {repo_dir}")
        result = subprocess.run(
            ["git", "clone", "--depth", "1", url, str(repo_dir)],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"\nError: git clone failed:\n{result.stderr}", file=sys.stderr)
            print(
                "\nTip: if you don't have an SSH key set up, re-run with --https",
                file=sys.stderr,
            )
            sys.exit(1)
        print("  Clone complete.")


# ── CSV loader ────────────────────────────────────────────────────────────────

def _load_cards(
    csv_path: Path,
    filter_classes: set[str] | None,
    format_col: str,
    existing_ids: set[str],
) -> tuple[list[dict], dict[str, int]]:
    """
    Read the CSV and return (cards_to_import, stats_dict).

    Deduplicates by card_id (first occurrence wins, matching the project
    convention where name+color is the unique key).
    """
    seen: dict[str, dict] = {}
    stats = {
        "rows_read":       0,
        "skipped_illegal": 0,
        "skipped_class":   0,
        "skipped_type":    0,
        "skipped_exists":  0,
        "duplicates":      0,
    }

    with open(csv_path, newline="", encoding="utf-8") as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            stats["rows_read"] += 1
            card = _parse_row(row, format_col)

            if card is None:
                # Could be format-illegal, hero, or token — check which
                name = row.get("Name", "").strip()
                types_str = row.get("Types", "").strip()
                ct = _parse_type(types_str)
                if ct in ("Hero", "Token"):
                    stats["skipped_type"] += 1
                else:
                    stats["skipped_illegal"] += 1
                continue

            # Class filter
            if filter_classes and card["card_class"] not in filter_classes:
                stats["skipped_class"] += 1
                continue

            cid = card["card_id"]

            # Don't overwrite cards already defined in the Python catalog
            if cid in existing_ids:
                stats["skipped_exists"] += 1
                continue

            if cid in seen:
                stats["duplicates"] += 1
                continue

            seen[cid] = card

    return list(seen.values()), stats


# ── Entry point ───────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--repo-dir",
        type=Path,
        default=DEFAULT_REPO_DIR,
        metavar="PATH",
        help="Local path for the card repo clone (default: ./flesh-and-blood-cards)",
    )
    parser.add_argument(
        "--filter-class",
        default="Generic,Brute,Warrior",
        metavar="CLASSES",
        help=(
            "Comma-separated card classes to import, or 'all' for everything. "
            "(default: Generic,Brute,Warrior)"
        ),
    )
    parser.add_argument(
        "--format",
        dest="fmt",
        default="blitz",
        choices=["blitz", "cc", "commoner"],
        help="Format-legality filter (default: blitz)",
    )
    parser.add_argument(
        "--no-update",
        action="store_true",
        help="Skip git pull if the repo already exists",
    )
    parser.add_argument(
        "--clear",
        action="store_true",
        help="Delete all previously imported catalog cards before importing",
    )
    parser.add_argument(
        "--https",
        action="store_true",
        help="Clone via HTTPS instead of SSH",
    )
    args = parser.parse_args()

    if args.filter_class.strip().lower() == "all":
        filter_classes: set[str] | None = None
    else:
        filter_classes = {c.strip() for c in args.filter_class.split(",") if c.strip()}

    format_col = _FORMAT_COLUMNS[args.fmt]

    print("=" * 60)
    print("FaB Card Importer")
    print("=" * 60)

    # Initialise DB (creates tables if missing)
    deck_db.init_db()

    if args.clear:
        n = deck_db.clear_catalog_cards()
        print(f"Cleared {n} existing catalog card(s).")

    # ── Step 1: clone / update repo ───────────────────────────────────────────
    print(f"\n[1/3] Card repository")
    _clone_or_update(args.repo_dir, args.https, args.no_update)

    csv_path = args.repo_dir / CARD_CSV_PATH
    if not csv_path.exists():
        print(f"\nError: expected CSV not found:\n  {csv_path}", file=sys.stderr)
        print(
            "The repository structure may have changed.  "
            f"Check that '{CARD_CSV_PATH}' exists inside {args.repo_dir}.",
            file=sys.stderr,
        )
        sys.exit(1)

    # ── Step 2: read and parse CSV ────────────────────────────────────────────
    print(f"\n[2/3] Reading cards")
    existing_ids = set(CARD_CATALOG.keys())
    classes_label = (
        "all classes"
        if filter_classes is None
        else ", ".join(sorted(filter_classes))
    )
    print(f"  Format   : {args.fmt}")
    print(f"  Classes  : {classes_label}")
    print(f"  CSV      : {csv_path.name}")

    cards, stats = _load_cards(csv_path, filter_classes, format_col, existing_ids)

    print(f"\n  Rows read          : {stats['rows_read']:,}")
    print(f"  Skipped (not {args.fmt} legal): {stats['skipped_illegal']:,}")
    print(f"  Skipped (hero/token)     : {stats['skipped_type']:,}")
    print(f"  Skipped (class filter)   : {stats['skipped_class']:,}")
    print(f"  Skipped (already in game): {stats['skipped_exists']:,}")
    print(f"  Duplicates collapsed     : {stats['duplicates']:,}")
    print(f"  Cards to import          : {len(cards):,}")

    if not cards:
        print("\nNothing to import — database unchanged.")
        return

    # ── Step 3: upsert into DB ────────────────────────────────────────────────
    print(f"\n[3/3] Importing into decks.db ...")
    inserted = deck_db.upsert_catalog_cards(cards)
    print(f"  Done! {inserted:,} card(s) imported.")

    # Summary by class
    by_class: dict[str, int] = {}
    for c in cards:
        by_class[c["card_class"]] = by_class.get(c["card_class"], 0) + 1
    print("\n  Breakdown by class:")
    for cls, count in sorted(by_class.items()):
        print(f"    {cls:<20} {count:>5}")

    print(
        "\nCards are now available in the deck builder at /decks/builder\n"
        "Run  python web_viewer.py  to start the web UI."
    )


if __name__ == "__main__":
    main()

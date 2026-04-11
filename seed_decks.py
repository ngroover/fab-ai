#!/usr/bin/env python3
"""
seed_decks.py — Insert the Rhinar and Dorinthea classic battle decks into
the deck database.

Run from the repo root:
    python seed_decks.py

The script is idempotent: it skips a deck if one with the same name already
exists in the database.
"""

from collections import Counter

import deck_db
import cards as card_module


def _build_card_counts(card_list):
    """Return a dict of card_id -> quantity from a list of Card instances."""
    return dict(Counter(card.card_id for card in card_list))


def main():
    deck_db.init_db()

    existing_names = {d["name"] for d in deck_db.list_decks()}

    # ── Rhinar ────────────────────────────────────────────────
    rhinar_name = "Rhinar Classic Battle"
    if rhinar_name in existing_names:
        print(f"Skipped '{rhinar_name}' — already exists")
    else:
        card_counts = _build_card_counts(card_module.build_rhinar_deck())
        deck_id = deck_db.create_deck(rhinar_name, "Rhinar", card_counts)
        total = sum(card_counts.values())
        print(f"Created '{rhinar_name}' (id={deck_id}, {total} cards)")

    # ── Dorinthea ─────────────────────────────────────────────
    dorinthea_name = "Dorinthea Classic Battle"
    if dorinthea_name in existing_names:
        print(f"Skipped '{dorinthea_name}' — already exists")
    else:
        card_counts = _build_card_counts(card_module.build_dorinthea_deck())
        deck_id = deck_db.create_deck(dorinthea_name, "Dorinthea", card_counts)
        total = sum(card_counts.values())
        print(f"Created '{dorinthea_name}' (id={deck_id}, {total} cards)")

    # ── Summary ───────────────────────────────────────────────
    print("\nDecks in database:")
    for deck in deck_db.list_decks():
        print(f"  [{deck['id']}] {deck['name']}  (hero: {deck['hero']})")


if __name__ == "__main__":
    main()

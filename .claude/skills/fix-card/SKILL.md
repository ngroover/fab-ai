---
name: fix-card
description: Use this to fix the behavior of a specific flesh and blood card
---

- Look up the card in cards.py to find the Card object for the specified card
- Use FabEnv and call reset with different seeds until you find a game in which one of the players holds the card in the opening hand
- Look for a unit test for the specified card in test directory.  If one doesn't exist create one.
- Use the seed determined earlier to create a unit test that can play the specified card
- Use the legal action to play the specified card and observe the behavior
- Use the text from the Card object to determine the expected behavior
- Put asserts in the unit test to verify the expected behavior

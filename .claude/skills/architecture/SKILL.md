---
name: architecture
description: Use this file to architect the system into separate directories and files.
---

- The codebase shall consist of the following components:
- card data
- actions (legal actions, agent actions)
- game state
- environment
- web interface
- actions (card actions, card abilities)
- effects (card effects, game effects)
- agents
- decklist data
- unit tests

- Each of the components may be placed in a directory of it's own a span across multiple files as needed
- The agents shall interact with the game state using a markov decision process.  The agent will choose from a list of legal actions and apply the action to the gamestate using a step function
- The environment should have the flexiblity to be passed both an agent and decklist for each player for the game.
- The environment will be responsible for managing the game state and providing an interface for the agents to interact with the game state.
- the environment will use the card data and interpret the effects and abilities on the card.
- The environment should be abstract from the specific cards so new cards can be created easily by reusing effects and abilities.
- Each card should have a unit test. The unit test should ensure that the card behaves as expected in a variety of scenarios.
- the card data will include the numerical and text data as well as a list of effects and/or abilities.
- the card data should be contained in a python class but uniquely identified so it can be stored in a database.
- Decklists should be able to be stored and retrieved in a sqlite database
- The web interface should have ability to play games and choose agents and decklists.
- The web interface should have the ability to choose legal actions for the human agents.
- The web interface should ahve the ability to modify decklists and create new decklists.
- A decklist should be composed of a list of card identifiers and a name.


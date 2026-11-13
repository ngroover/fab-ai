# Training and playing the AlphaZero agent

How to generate self-play games, train a network on them, measure it, and play
against it yourself. Everything here uses the Rust environment in
`environment/`; the Python side only trains.

- [Setup](#setup)
- [The quick way: run the whole loop](#the-quick-way-run-the-whole-loop)
- [Play against the network](#play-against-the-network)
- [The steps one at a time](#the-steps-one-at-a-time)
- [Agents](#agents)
- [Files](#files)
- [Troubleshooting](#troubleshooting)

## Setup

You need Rust (for the game, the search and self-play) and Python with PyTorch
and NumPy (for training only).

```sh
# Build the environment. Always use --release: a debug build is many times slower.
cd environment
cargo build --release
cd ..

# Training dependencies. On a machine with no GPU the CPU-only build is far smaller:
pip install torch numpy --index-url https://download.pytorch.org/whl/cpu
# or simply:  pip install torch numpy
```

All commands below are run from the repository root. The built program is
`environment/target/release/environment`; the examples call it `ENV`:

```sh
ENV=environment/target/release/environment
```

## The quick way: run the whole loop

One command plays self-play games, trains, evaluates, and repeats:

```sh
python3 train_loop.py --dir runs/first --rounds 5
```

Each round:

1. **Self-play.** 800 games of the search against itself, using the newest
   network. The first round has no network yet and uses random playouts.
2. **Train.** Fits the network to the last 3 rounds of games, starting from the
   previous round's weights.
3. **Evaluate.** Plays the new network against ISMCTS, against the previous
   round's network, and against the first round's. Each side plays both heroes.

It prints a line per step and a table at the end:

```
round 0: self-play, 800 games with random playouts
round 0: trained on 136919 records — held out: policy 1.0959  value 0.4978  top-1 48.8%
round 0: vs ismcts    83.0% of 200 games  (as Rhinar 90/100, as Dorinthea 76/100)
round 1: self-play, 800 games with the round 0 network
round 1: trained on 272174 records — held out: policy 1.0861  value 0.7526  top-1 56.1%
round 1: vs ismcts    85.0% of 200 games  (as Rhinar 88/100, as Dorinthea 82/100)
round 1: vs previous  71.0% of 200 games  (as Rhinar 76/100, as Dorinthea 66/100)
```

On a 4-core machine with no GPU a round takes about 5 to 7 minutes.

**Stopping and resuming.** Stop it with Ctrl-C whenever you like. Running the
same command again carries on after the last finished round. To train further,
run it again with a larger `--rounds`.

**The result.** The networks are `runs/first/net_000.bin`, `net_001.bin`, and so
on; the highest number is the newest. `runs/first/results.jsonl` has every
round's scores.

Useful options (`python3 train_loop.py --help` lists them all):

| Option | Default | Meaning |
|---|---|---|
| `--rounds` | 5 | Rounds the run should have in total |
| `--games` | 800 | Self-play games per round |
| `--iterations` | 400 | Search iterations per self-play move |
| `--window` | 3 | Train on this many of the latest rounds |
| `--eval-games` | 200 | Games per evaluation match |
| `--threads` | all cores | Threads for self-play and matches |
| `--seed` | 0 | Makes the whole run reproducible |

## Play against the network

```sh
$ENV play --network runs/first/net_004.bin --iterations 400
```

Menus then ask, for each player, which hero and which agent. To play Dorinthea
against the network as Rhinar:

```
P1: choose a hero            ->  1) Rhinar
P1: choose an agent          ->  4) Network (tree search with the trained network)
P2: choose a hero            ->  2) Dorinthea
P2: choose an agent          ->  1) Human (terminal input)
```

At each of your decisions you can print the game state, print the game log, or
pick an action. Decisions with only one legal action are played for you.

Options:

| Option | Meaning |
|---|---|
| `--network FILE` | Adds the network in `FILE` to the agent menu |
| `--iterations N` | How hard the network (or ISMCTS) searches each move. 400 answers instantly; 2000 is stronger and still quick |
| `--seed N` | Replays the same deal |
| `--reveal` | Also shows you the hidden information: the opponent's hand and the full log. Useful for checking what the network was looking at |

Without `--network` the menu offers only human, random and ISMCTS. To watch the
network play itself or ISMCTS, pick non-human agents for both players; the whole
game log is printed at the end.

## The steps one at a time

The loop is these three commands in a cycle. Run them yourself to experiment
with one step.

### 1. Self-play: generate training data

```sh
# First round: no network, the search uses random playouts.
$ENV selfplay --out data/round0.bin --games 800 --iterations 400

# Later rounds: the search uses a network.
$ENV selfplay --out data/round1.bin --games 800 --iterations 400 --network net0.bin
```

Plays Rhinar against Dorinthea with the search on both sides and writes one
record per decision. It prints the number of games, who won, and how many
records were written. 800 games is roughly 140,000 records and 80 MB.

Options: `--threads N`, `--seed N` (the same seed gives the same file whatever
the thread count), `--sampled-moves N` (how many of each game's opening
decisions are picked at random in proportion to the search's visits; default 20).

### 2. Train: fit a network to the data

```sh
# From scratch:
python3 train_network.py data/round0.bin --out net0.bin

# Continuing from earlier weights, on several rounds of data:
python3 train_network.py data/round0.bin data/round1.bin --init net0.bin --out net1.bin
```

Every tenth game is held out. Each epoch prints the losses on the training
games and on the held-out games:

```
untrained baseline: policy 1.1287  value 0.9856  top-1 35.1%
lowest possible policy loss: 0.9422
epoch   1  train: policy 1.0989  value 0.3318  top-1 49.3%
           valid: policy 1.1023  value 0.5623  top-1 48.4%   (5.3s)
```

How to read it:

- **value** is the error predicting who won. Lower is better; the baseline line
  is what an untrained network scores.
- **policy** is how far the network's move preferences are from the search's.
  It cannot go below the "lowest possible" line.
- **top-1** is how often the network's favourite move is the search's.
- Watch the **valid** line. If it gets worse while **train** keeps improving,
  the network is memorizing; the weights saved are from the best **valid**
  epoch, so extra epochs do no harm.

Options: `--epochs` (default 10), `--hidden 256 256` (layer widths, ignored with
`--init`), `--lr`, `--batch`, `--value-mix`. `python3 train_network.py --help`
lists them all.

### 3. Evaluate: is the new network better?

```sh
$ENV match --a net:net1.bin --b ismcts --games 400 --iterations 400
$ENV match --a net:net1.bin --b net:net0.bin --games 400 --iterations 400
```

Agent A plays Rhinar in half the games and Dorinthea in the other half, which
matters because the heroes are not evenly matched. Output:

```
Games played:  400  (400 iterations, seed 1, 4 threads, 229.9s)
A (net:net1.bin) wins:  340  as Rhinar 170/200, as Dorinthea 170/200
B (ismcts) wins:  59  as Rhinar 30/200, as Dorinthea 29/200
Draws:         1
A's score:     85.1%
```

How many games to play: a score from 100 games is only good to about ±10
points, 400 games to about ±5, 1,000 games to about ±3. A common rule is to call
a network better only if it scores above 55% over 400 games.

## Agents

The names `match` accepts for `--a` and `--b`:

| Name | What it is |
|---|---|
| `random` | Picks a legal action at random |
| `ismcts` | Tree search that judges positions by random playouts. The fixed yardstick |
| `puct` | The AlphaZero-style search, but still with random playouts and no network. About as strong as `ismcts` |
| `net:FILE` | The AlphaZero-style search using the trained network in `FILE` |

`--iterations` sets how many search iterations every searching agent runs per
move. Compare agents at the same number.

## Files

| File | Written by | Read by | Contains |
|---|---|---|---|
| `records_NNN.bin` / `--out` of `selfplay` | `environment selfplay` | `train_network.py` | One record per decision: the mover's observation, the visits the search gave each legal action, the game's result |
| `net_NNN.bin` / `--out` of the trainer | `train_network.py` | `environment` (`--network`, `net:FILE`) | The network's layer sizes and weights |
| `results.jsonl` | `train_loop.py` | you | One line per round: training metrics, match scores, timings |

`runs/` is ignored by git. The byte layouts are documented at the top of
`environment/src/self_play.rs` (records) and `train_network.py` (weights).

## Troubleshooting

- **`environment/target/release/environment not found`** — build it:
  `cd environment && cargo build --release`.
- **`cannot use the network: the network takes ... observations`** — the
  network was trained before the observation or action encoding changed (a card
  was added, say). Networks and records from before the change cannot be
  reused; start a new run.
- **`... is not a self-play file` / `is cut short`** — the records file is from
  an interrupted `selfplay`. Generate it again. (`train_loop.py` never leaves
  one behind: it writes to `.partial` first.)
- **`ModuleNotFoundError: No module named 'torch'`** — install PyTorch (see
  Setup). Only training needs it; self-play, matches and `play` do not.
- **Everything is slow** — check you built with `--release`, and that
  `--threads` is not higher than your core count.

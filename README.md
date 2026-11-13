# fab-ai
Flesh and blood TCG AI and simulation environment

# Generate Certificate
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=<your_ip>" -addext "subjectAltName=IP:<your_ip>,IP:127.0.0.1,DNS:localhost"

# Run with certificate
FAB_DIST_TOKEN=<password_here> FAB_PASSWORD=<password_here> python3 web_viewer.py --ssl-cert cert.pem --ssl-key key.pem --port=8080

# Generate card embeddings
python3 card_embeddings.py --embed-dim 32

# Look at similiar cards embeddings
python3 card_embeddings.py --similar "alpha-rampage-red"

# Seed decks
python3 seed_decks.py

# install dependencies
sudo apt install python3-torch

# connect a self play machine
FAB_DIST_TOKEN=<password_here> python3 dist_worker.py --coord tcp://<trainer_node>

# run training machine
FAB_DIST_TOKEN=<password_here> python3 run_distributed.py --steps 10000 --batch 64

# Play in the terminal (Rust environment)
cd environment && cargo play

Numbered menus pick each player's hero (Rhinar or Dorinthea) and agent (human or
random). A human player can print the game state, print the game log, or choose
an action at every decision. A decision with only one legal action is played
automatically (it still shows up in the log), so players and agents are only
asked when there is a real choice. Options: `cargo play --seed 7` for a reproducible
game, `cargo play --reveal` to also show hidden information (the opponent's hand,
the full log).

The game log is compiled in by default. To compile it out when measuring raw
simulation speed: `cargo run --release --no-default-features -- 100000`

# Generate self-play training data (Rust environment)
cd environment && cargo run --release -- selfplay --out selfplay.bin --games 1000 --iterations 400

Plays Rhinar against Dorinthea with the PUCT search on both sides and writes one
record per decision: the mover's observation, the visits the search gave each
legal action, and the game's result. Until there is a network the search judges
positions with random playouts. `--seed N` makes a run reproducible, whatever
`--threads N` is. The file format is described at the top of
`environment/src/self_play.rs`.

# Train the network on self-play data
python3 train_network.py environment/selfplay.bin --out network.bin

Fits a policy/value network (an MLP, `--hidden 256 256` by default) to the
self-play records: the policy to the search's visit shares, the value to the
games' results (mixed half and half with the search's own estimate; `--value-mix`).
Every tenth game is held out for validation: each epoch prints both sets' losses
beside what an untrained network would score, and the weights kept are those of
the epoch that did best on the held-out games. Give
several record files to train on all of them, and `--init network.bin` to carry
on from earlier weights. The weights file format is described at the top of
`train_network.py`. Needs PyTorch and NumPy.

# Play with a trained network (Rust environment)
cd environment && cargo run --release -- 100 --p1 net --p2 ismcts --network ../network.bin --iterations 400

The `net` agent is the PUCT search judging positions with the network instead of
random playouts (`puct`). Swap `--p1` and `--p2` to give it the other hero.
`selfplay` takes `--network` too, to generate the next round of training data
with the network.

# Run the whole loop: self-play, train, evaluate, repeat
cd environment && cargo build --release && cd ..
python3 train_loop.py --dir runs/first --rounds 5

Each round plays self-play games with the newest network (random playouts in the
first round), trains on the last few rounds' records, and measures the new
network in matches where each side plays both heroes: against ISMCTS, against
the previous round's network and against the first round's. Everything goes in
`--dir`, with one line per round in `results.jsonl`; running the command again
carries on after the last finished round. A single match can be played with
`environment match --a net:runs/first/net_004.bin --b ismcts --games 400`.

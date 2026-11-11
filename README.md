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
an action at every decision. Options: `cargo play --seed 7` for a reproducible
game, `cargo play --reveal` to also show hidden information (the opponent's hand,
the full log).

The game log is compiled in by default. To compile it out when measuring raw
simulation speed: `cargo run --release --no-default-features -- 100000`

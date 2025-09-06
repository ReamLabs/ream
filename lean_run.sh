#!/usr/bin/env bash

read -p "Enter number of nodes: " n

if ! [[ "$n" =~ ^[0-9]+$ ]] || [ "$n" -lt 1 ]; then
  echo "Please enter a positive integer"
  exit 1
fi

# Open first node in a new terminal window
osascript <<EOF
tell application "Terminal"
    activate
    do script "cd $(pwd) && cargo run --release -- --data-dir db lean_node \
      --network ephemery \
      --validator-registry-path ./bin/ream/assets/lean/validator_registry.yml | tee db_first_node.log; exec \$SHELL"
end tell
EOF

echo "Waiting for first node to generate peer id..."
sleep 5

# Extract local_peer_id from logs and strip PeerId wrapper
peer_id=$(grep -m1 --color=never -oE 'local_peer_id=[^ ]+' db_first_node.log | cut -d= -f2)
peer_id=${peer_id//\"/}       # remove quotes
peer_id=${peer_id//PeerId(/}  # remove PeerId(
peer_id=${peer_id//)/}        # remove trailing )

echo "First node peer_id: $peer_id"

# Launch remaining nodes in new tabs
base_socket_port=9000
base_http_port=5052

for ((i=2; i<=n; i++)); do
  data_dir="db$i"
  socket_port=$((base_socket_port + i - 2))
  http_port=$((base_http_port + i - 2))
  log_file="db${i}_node.log"

  osascript <<EOF
tell application "Terminal"
    activate
    tell application "System Events" to keystroke "t" using command down
    delay 0.2
    do script "cd $(pwd) && cargo run --release -- --data-dir $data_dir lean_node \
      --network ephemery \
      --validator-registry-path ./bin/ream/assets/lean/validator_registry.yml \
      --socket-port $socket_port \
      --http-port $http_port \
      --bootnodes /ip4/127.0.0.1/udp/9000/quic-v1/p2p/$peer_id | tee $log_file; exec \$SHELL" in front window
end tell
EOF
done

echo "✅ All $n nodes launched (first in window, rest in tabs)."

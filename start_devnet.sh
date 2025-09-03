#!/bin/bash

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}   Ream 4-Node Lean Chain Devnet${NC}"
echo -e "${BLUE}========================================${NC}"

# Function to generate network specification
generate_spec() {
    local genesis_delay=${1:-60}  # Default 60 seconds delay
    local genesis_time=$(($(date +%s) + genesis_delay))

    cat > devnet/devnet-spec.yaml <<EOF
GENESIS_TIME: $genesis_time
SECONDS_PER_SLOT: 4
JUSTIFICATION_LOOKBACK_SLOTS: 3
NUM_VALIDATORS: 4
EOF

    echo -e "${GREEN}✓ Generated network spec with genesis time: $genesis_time${NC}"
    echo -e "  Genesis will occur at: $(date -d @$genesis_time 2>/dev/null || date -r $genesis_time)"
    echo -e "  Time until genesis: ${genesis_delay} seconds"
}

# Function to check if a port is in use
check_port() {
    local port=$1
    if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1; then
        return 0  # Port is in use
    else
        return 1  # Port is free
    fi
}

# Function to wait for a node to start and get its peer ID
get_peer_id() {
    local log_file=$1
    local node_name=$2
    local max_attempts=30
    local attempt=0

    echo -e "${YELLOW}  Waiting for $node_name to start...${NC}" >&2

    while [ $attempt -lt $max_attempts ]; do
        if [ -f "$log_file" ]; then
            # Look for peer ID in logs - matches the actual log format: local_peer_id=<peer_id>
            local peer_id=$(grep -oE "local_peer_id=[a-zA-Z0-9]+" "$log_file" 2>/dev/null | tail -1 | cut -d'=' -f2)
            if [ ! -z "$peer_id" ]; then
                echo -e "${GREEN}  ✓ $node_name started with peer ID: $peer_id${NC}" >&2
                echo "$peer_id"  # Only output the peer ID to stdout
                return 0
            fi
        fi
        sleep 1
        ((attempt++))
    done

    echo -e "${RED}  ✗ Failed to get peer ID for $node_name${NC}" >&2
    return 1
}

# Check if ream binary exists
if [ ! -f "target/release/ream" ]; then
    echo -e "${RED}Error: Ream binary not found at target/release/ream${NC}"
    echo -e "${YELLOW}Please run: cargo build --release --bin ream${NC}"
    exit 1
fi

# Check for required ports
echo -e "\n${BLUE}Checking port availability...${NC}"
PORTS=(9000 9001 9002 9003 5052 5053 5054 5055 8080 8081 8082 8083)
PORT_NAMES=("P2P-0" "P2P-1" "P2P-2" "P2P-3" "HTTP-0" "HTTP-1" "HTTP-2" "HTTP-3" "Metrics-0" "Metrics-1" "Metrics-2" "Metrics-3")

for i in "${!PORTS[@]}"; do
    if check_port ${PORTS[$i]}; then
        echo -e "${RED}  ✗ Port ${PORTS[$i]} (${PORT_NAMES[$i]}) is already in use${NC}"
        echo -e "${YELLOW}    Stop the process using this port or change the configuration${NC}"
        exit 1
    else
        echo -e "${GREEN}  ✓ Port ${PORTS[$i]} (${PORT_NAMES[$i]}) is available${NC}"
    fi
done

# Clean up any existing logs
mkdir -p devnet/logs

echo -e "\n${BLUE}Cleaning up old logs...${NC}"
rm -f devnet/logs/*.log
echo -e "${GREEN}✓ Logs cleaned${NC}"

# Generate network specification
echo -e "\n${BLUE}Generating network specification...${NC}"
generate_spec 15  # 15 seconds delay for genesis

# Start Node 0 (Bootstrap node)
echo -e "\n${BLUE}Starting Node 0 (Bootstrap)...${NC}"
RUST_BACKTRACE=1 NO_COLOR=1 RUST_LOG_STYLE=never ./target/release/ream lean_node \
    --network devnet/devnet-spec.yaml \
    --validator-registry-path devnet/node0/registry.yaml \
    --data-dir devnet/db/node0 \
    --socket-address 127.0.0.1 \
    --socket-port 9000 \
    --http-address 127.0.0.1 \
    --http-port 5052 \
    --metrics \
    --metrics-address 127.0.0.1 \
    --metrics-port 8080 \
    --bootnodes none \
    > devnet/logs/node0.log 2>&1 &

NODE0_PID=$!
echo -e "${GREEN}✓ Node 0 started (PID: $NODE0_PID)${NC}"

# Wait for Node 0 to start and get its peer ID
sleep 3
NODE0_PEER_ID=$(get_peer_id "devnet/logs/node0.log" "Node 0")

if [ -z "$NODE0_PEER_ID" ]; then
    echo -e "${YELLOW}Warning: Could not detect Node 0 peer ID, continuing with multiaddr only${NC}"
    NODE0_MULTIADDR="/ip4/127.0.0.1/udp/9000/quic-v1"
else
    NODE0_MULTIADDR="/ip4/127.0.0.1/udp/9000/quic-v1/p2p/$NODE0_PEER_ID"
fi

# Start Node 1
echo -e "\n${BLUE}Starting Node 1...${NC}"
RUST_BACKTRACE=1 NO_COLOR=1 RUST_LOG_STYLE=never ./target/release/ream lean_node \
    --network devnet/devnet-spec.yaml \
    --validator-registry-path devnet/node1/registry.yaml \
    --data-dir devnet/db/node1 \
    --socket-address 127.0.0.1 \
    --socket-port 9001 \
    --http-address 127.0.0.1 \
    --http-port 5053 \
    --metrics \
    --metrics-address 127.0.0.1 \
    --metrics-port 8081 \
    --bootnodes "$NODE0_MULTIADDR" \
    > devnet/logs/node1.log 2>&1 &

NODE1_PID=$!
echo -e "${GREEN}✓ Node 1 started (PID: $NODE1_PID)${NC}"

# Wait for Node 1 to start
sleep 2
NODE1_PEER_ID=$(get_peer_id "devnet/logs/node1.log" "Node 1")

if [ -z "$NODE1_PEER_ID" ]; then
    NODE1_MULTIADDR="/ip4/127.0.0.1/udp/9001/quic-v1"
else
    NODE1_MULTIADDR="/ip4/127.0.0.1/udp/9001/quic-v1/p2p/$NODE1_PEER_ID"
fi

# Start Node 2
echo -e "\n${BLUE}Starting Node 2...${NC}"
RUST_BACKTRACE=1 NO_COLOR=1 RUST_LOG_STYLE=never ./target/release/ream lean_node \
    --network devnet/devnet-spec.yaml \
    --validator-registry-path devnet/node2/registry.yaml \
    --data-dir devnet/db/node2 \
    --socket-address 127.0.0.1 \
    --socket-port 9002 \
    --http-address 127.0.0.1 \
    --http-port 5054 \
    --metrics \
    --metrics-address 127.0.0.1 \
    --metrics-port 8082 \
    --bootnodes "$NODE0_MULTIADDR,$NODE1_MULTIADDR" \
    > devnet/logs/node2.log 2>&1 &

NODE2_PID=$!
echo -e "${GREEN}✓ Node 2 started (PID: $NODE2_PID)${NC}"

# Wait for Node 2 to start
sleep 2
NODE2_PEER_ID=$(get_peer_id "devnet/logs/node2.log" "Node 2")

if [ -z "$NODE2_PEER_ID" ]; then
    NODE2_MULTIADDR="/ip4/127.0.0.1/udp/9002/quic-v1"
else
    NODE2_MULTIADDR="/ip4/127.0.0.1/udp/9002/quic-v1/p2p/$NODE2_PEER_ID"
fi

# Start Node 3
echo -e "\n${BLUE}Starting Node 3...${NC}"
RUST_BACKTRACE=1 NO_COLOR=1 RUST_LOG_STYLE=never ./target/release/ream lean_node \
    --network devnet/devnet-spec.yaml \
    --validator-registry-path devnet/node3/registry.yaml \
    --data-dir devnet/db/node3 \
    --socket-address 127.0.0.1 \
    --socket-port 9003 \
    --http-address 127.0.0.1 \
    --http-port 5055 \
    --metrics \
    --metrics-address 127.0.0.1 \
    --metrics-port 8083 \
    --bootnodes "$NODE0_MULTIADDR,$NODE1_MULTIADDR,$NODE2_MULTIADDR" \
    > devnet/logs/node3.log 2>&1 &

NODE3_PID=$!
echo -e "${GREEN}✓ Node 3 started (PID: $NODE3_PID)${NC}"

# Save PIDs to file for stop script
echo "$NODE0_PID" > devnet/.node_pids
echo "$NODE1_PID" >> devnet/.node_pids
echo "$NODE2_PID" >> devnet/.node_pids
echo "$NODE3_PID" >> devnet/.node_pids

# Display summary
echo -e "\n${GREEN}========================================${NC}"
echo -e "${GREEN}   Devnet Started Successfully!${NC}"
echo -e "${GREEN}========================================${NC}"
echo -e "\n${BLUE}Node Details:${NC}"
echo -e "  Node 0: P2P=9000, HTTP=5052, Metrics=8080 (PID: $NODE0_PID)"
echo -e "  Node 1: P2P=9001, HTTP=5053, Metrics=8081 (PID: $NODE1_PID)"
echo -e "  Node 2: P2P=9002, HTTP=5054, Metrics=8082 (PID: $NODE2_PID)"
echo -e "  Node 3: P2P=9003, HTTP=5055, Metrics=8083 (PID: $NODE3_PID)"

echo -e "\n${BLUE}Useful Commands:${NC}"
echo -e "  Monitor all logs:     ${YELLOW}tail -f devnet/logs/*.log${NC}"
echo -e "  Monitor connections:  ${YELLOW}tail -f devnet/logs/*.log | grep -E 'Dialing peer|Connected'${NC}"
echo -e "  Monitor blocks:       ${YELLOW}tail -f devnet/logs/*.log | grep 'block for slot'${NC}"
echo -e "  Check node status:    ${YELLOW}curl http://127.0.0.1:5052/api/v1/status${NC}"
echo -e "  View metrics:         ${YELLOW}curl http://127.0.0.1:8080/metrics${NC}"
echo -e "  Stop devnet:          ${YELLOW}./devnet/stop_devnet.sh${NC}"

echo -e "\n${YELLOW}Waiting for genesis...${NC}"
echo -e "${YELLOW}Check logs for activity: tail -f devnet/logs/*.log${NC}"

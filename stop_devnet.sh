#!/bin/bash

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}   Stopping Ream Devnet${NC}"
echo -e "${BLUE}========================================${NC}"

# Function to kill process safely
kill_process() {
    local pid=$1
    local name=$2

    if kill -0 $pid 2>/dev/null; then
        echo -e "${YELLOW}  Stopping $name (PID: $pid)...${NC}"
        kill -TERM $pid

        # Wait for process to terminate (max 5 seconds)
        local count=0
        while kill -0 $pid 2>/dev/null && [ $count -lt 5 ]; do
            sleep 1
            ((count++))
        done

        # Force kill if still running
        if kill -0 $pid 2>/dev/null; then
            echo -e "${YELLOW}  Force killing $name...${NC}"
            kill -KILL $pid
        fi

        echo -e "${GREEN}  ✓ $name stopped${NC}"
    else
        echo -e "${YELLOW}  $name was not running${NC}"
    fi
}

# Try to stop using saved PIDs first
if [ -f "devnet/.node_pids" ]; then
    echo -e "\n${BLUE}Stopping nodes using saved PIDs...${NC}"

    NODE_NUM=0
    while IFS= read -r pid; do
        if [ ! -z "$pid" ]; then
            kill_process $pid "Node $NODE_NUM"
        fi
        ((NODE_NUM++))
    done < "devnet/.node_pids"

    rm -f devnet/.node_pids
else
    echo -e "${YELLOW}No PID file found, looking for ream processes...${NC}"
fi

# Also kill any remaining ream lean_node processes
echo -e "\n${BLUE}Checking for any remaining ream processes...${NC}"
PIDS=$(pgrep -f "ream lean_node" 2>/dev/null)

if [ ! -z "$PIDS" ]; then
    echo -e "${YELLOW}Found additional ream processes, stopping them...${NC}"
    for pid in $PIDS; do
        kill_process $pid "Ream process"
    done
else
    echo -e "${GREEN}✓ No additional ream processes found${NC}"
fi

# Optional: Clean up logs
echo -e "\n${BLUE}Clean up logs? (y/N)${NC}"
read -r -n 1 -t 5 REPLY
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Cleaning up logs...${NC}"
    rm -f devnet/logs/*.log
    echo -e "${GREEN}✓ Logs cleaned${NC}"
else
    echo -e "${BLUE}Logs preserved in devnet/logs/${NC}"
fi

echo -e "\n${GREEN}========================================${NC}"
echo -e "${GREEN}   Devnet Stopped Successfully!${NC}"
echo -e "${GREEN}========================================${NC}"

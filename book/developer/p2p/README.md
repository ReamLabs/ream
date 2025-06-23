# P2P Networking

This document describes the architecture of Ream's networking layer.

## Architecture Overview

The networking stack is organized into 5 main layers:

```ignore
┌──────────────────────────────────────────────────────────────────────────────┐
│                       ream-p2p::config::NetworkConfig                        │
│                    (Container for network configurations)                    │
│       ┌────────────────────────────┐  ┌────────────────────────────┐         │
│       │      DiscoveryConfig       │  │      GossipsubConfig       │         │
│       └────────────────────────────┘  └────────────────────────────┘         │
└─────────────────────────────────────┬────────────────────────────────────────┘
                                      │
                                      ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│              ream-network-manager::service::NetworkManagerService            │
│ (Orchestrates between network components and the rest of the beacon client)  │
└─────────────────────────────────────┬────────────────────────────────────────┘
                                      │
                                      ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                         ream-p2p::network::Network                           │
│             (The network instance with initialized configurations)           │
└─────────────────────────────────────┬────────────────────────────────────────┘
                                      │
                                      ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                       ream-p2p::network::ReamBehaviour                       │
│                        (Handles ream's p2p behaviours)                       │
│  ┌───────────────────────┐┌───────────────────────┐┌──────────────────────┐  │
│  │   DiscoveryBehaviour  ││  GossipsubBehaviour   ││   ReqRespBehaviour   │  │
│  └───────────────────────┘└───────────────────────┘└──────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                            libp2p::swarm::Swarm                              │
│                      (Manages the p2p transport layer)                       │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Components

### `ream-p2p::config::NetworkConfig`
The configuration container that holds all network-related settings:
- **DiscoveryConfig**: Configuration for discv5 peer discovery including bootnodes, subnet subscriptions, and discovery parameters
- **GossipsubConfig**: Configuration for gossipsub behavior including topics, message sizes, and propagation settings

### `ream-network-manager::service::NetworkManagerService`
The `NetworkManagerService` acts as the entrypoint and central coordinator for all networking activities. It is responsible for:
- Initializing network configurations
- Creating and managing the Network component
- Processing incoming network events (gossip messages, request/response)
- Routing messages between the network layer and the beacon chain logic

### `ream-p2p::network::Network`
The `Network` struct is the core P2P networking component that:
- Creates and manages the libp2p swarm and all network behaviors (Discovery, Gossipsub, ReqResp)
- Handles peer connections and disconnections
- Coordinates between different protocol behaviors (GossipSub, Req/Resp, Discovery)
- Provides the main event loop for processing network events
- Maintains network state and peer information

### `ream-p2p::network::ReamBehaviour`
The `ReamBehaviour` component creates and manages several behaviors:
- **Discovery Behaviour**: Integrates with discv5 for peer discovery and subnet management
- **GossipSub Behaviour**: Handles topic-based message propagation for consensus objects
- **Req/Resp Behaviour**: Manages request-response protocols for block/blob synchronization

### `libp2p::swarm::Swarm`
The libp2p swarm provides:
- Unified interface for all network behaviors
- Connection management and multiplexing
- Transport layer abstraction

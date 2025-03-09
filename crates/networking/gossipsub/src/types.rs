use std::collections::HashSet;

use libp2p::swarm::ConnectionId;

use crate::topics::TopicName;

pub struct ConnectionInfo {
    pub connections: Vec<ConnectionId>,
    pub topics: HashSet<TopicName>,
}

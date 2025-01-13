use discv5::Enr;

pub struct Bootnodes {
    pub bootnodes: Vec<Enr>,
}

impl Bootnodes {
    pub fn new() -> Self {
        let bootnodes: Vec<Enr> =
            serde_yaml::from_str(include_str!("../resources/bootstrap_nodes.yaml"))
                .expect("should deserialize bootnodes");
        Self { bootnodes }
    }
}

impl Default for Bootnodes {
    fn default() -> Self {
        Self::new()
    }
}

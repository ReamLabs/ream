use discv5::Enr;

pub struct NetworkConfig {

    #[serde(skip)]
    pub discv5_config: discv5::Config,



    pub boot_nodes_enr: Vec<Enr>,

    pub disable_discovery:bool
}

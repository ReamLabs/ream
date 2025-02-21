use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start the node
    #[command(name = "node")]
    Node(NodeCommand),


}

#[derive(Debug, Parser)]
pub struct NodeCommand {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_node_command() {
        let cli = Cli::parse_from(["program", "node", "--verbosity", "2"]);

        match cli.command {
            Commands::Node(cmd) => {
                assert_eq!(cmd.verbosity, 2);
            }
        }
    }
}

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rumour")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Generate a new node keypair
    Keygen {
        /// Where to save the private key
        #[arg(long)]
        output: String,
    },
    /// Run a gossip node
    Node {
        /// Bind address (e.g., 127.0.0.1:9001)
        #[arg(long)]
        bind: String,

        /// Peer addresses to connect to (e.g., 127.0.0.1:9002 127.0.0.1:9003)
        #[arg(long, num_args = 0..)]
        peers: Vec<String>,

        /// Number of peers to relay to per message (default 3)
        #[arg(long, default_value = "3")]
        fanout: usize,

        /// Path to private key file
        #[arg(long)]
        key_file: String,
    },
}

use anyhow::Result;
use clap::Parser;
use rumour::{Node, config::{Cli, Command}};
use rumour::crypto;
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Keygen { output } => {
            let (private_bytes, public_bytes) = crypto::generate_keypair()?;
            crypto::save_private_key(&output, &private_bytes)?;
            let id = crypto::derive_node_id(&public_bytes);
            println!("Node ID:    {}", id);
            println!("Public key: {}", hex::encode(&public_bytes));
            println!("Saved key:  {}", output);
        }
        Command::Node { bind, peers, fanout, key_file } => {
            // Load private key (required for node operation)
            let private_key = crypto::load_private_key(&key_file)?;

            // Initialize the Node (ID and public key are derived from the keypair)
            let node = Node::new(
                &bind,
                peers,
                fanout,
                private_key,
            )
            .await?;
            
            println!("Node {} listening on {}", node.id, bind);
            node.start_background_tasks();

            // Wrap stdin in a BufReader to handle lines
            let stdin = tokio::io::stdin();
            let mut reader = BufReader::new(stdin);
            let mut line = String::new();
            let mut seq: u64 = 0;

            loop {
                line.clear();
                // properly waits for a newline delimiter
                let n = reader.read_line(&mut line).await?; 
                if n == 0 {
                    break; // EOF
                }

                let payload = line.trim().as_bytes().to_vec();
                
                // Don't broadcast empty lines (e.g. accidental double newlines)
                if payload.is_empty() {
                    continue;
                }

                seq += 1;
                node.broadcast(payload, seq).await?;
            }
        }
    }

    Ok(())
}
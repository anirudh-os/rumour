mod config;
mod node;
mod proto;
mod ratelimit;

use anyhow::Result;
use clap::Parser;
use config::Args;
use node::Node;
use tokio::io::{AsyncBufReadExt, BufReader}; 
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize the Node
    let node = Node::new(args.id, &args.bind, args.peers, args.fanout).await?;
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

    Ok(())
}
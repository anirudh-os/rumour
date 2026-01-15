mod config;
mod node;
mod proto;
mod ratelimit;

use anyhow::Result;
use bytes::BytesMut;
use clap::Parser;
use config::Args;
use node::Node;
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize the Node
    let node = Node::new(args.id, &args.bind, args.peers, args.fanout).await?;

    // Start background listeners: receive, GC
    node.start_background_tasks();

    // Main loop: Read from Stdin and Broadcast
    let mut stdin = tokio::io::stdin();
    let mut input = BytesMut::with_capacity(1024);
    let mut seq: u64 = 0;

    loop {
        input.clear();
        let n = stdin.read_buf(&mut input).await?;
        if n == 0 {
            break;
        }

        seq += 1;
        node.broadcast(input.to_vec(), seq).await?;
    }

    Ok(())
}
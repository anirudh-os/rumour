pub mod gossip {
    tonic::include_proto!("gossip");
}
pub use gossip::GossipEnvelope;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/gossip.proto");

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(false)
        .compile_protos(&["gossip.proto"], &["proto"])?;

    Ok(())
}

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    #[arg(long)]
    pub id: u64,

    #[arg(long)]
    pub bind: String,

    #[arg(long)]
    pub peers: Vec<String>,

    #[arg(long, default_value = "3")]
    pub fanout: usize,
}
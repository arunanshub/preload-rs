mod cli;

use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();
    println!("{:#?}", cli);
}

mod cli;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    println!("{:#?}", cli);

    Ok(())
}

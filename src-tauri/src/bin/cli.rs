use clap::Parser;
use zenvoy_lib::cli::Cli;

fn main() {
    let _cli = Cli::parse();
    // Command dispatch will be implemented in subsequent tasks
    println!("zen CLI ready");
}

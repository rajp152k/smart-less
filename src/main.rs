use clap::Parser;

fn main() {
    std::process::exit(smart_less::run(smart_less::Cli::parse()));
}

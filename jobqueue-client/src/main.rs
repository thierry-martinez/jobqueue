use libjobqueue;

#[derive(clap::Parser)]
#[command(author, version, about = "")]
pub struct Cli {
    pub command_name: String,
    pub command_args: Vec<String>,
}

fn execute(args::Cli) -> Result<(), error::Error> {
    let command_line = libjobqueue::CommandLine::new(args.command_name, args.command_args);
    
}

fn main() {
    let args = Cli::parse();
    match execute(args) {
	Ok(()) => (),
	Err(error) => {
            println!("Error: {error}");
            std::process::exit(1);
	}
    }
}

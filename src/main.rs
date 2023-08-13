mod convert;
mod output;

use clap::{Parser, ValueEnum};
use languagetool_rust::{
	check::{CheckRequest, Data},
	error::Error,
	server::ServerClient,
};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use output::Position;
use std::{fs, path::PathBuf, time::Duration};

#[derive(ValueEnum, Clone, Debug)]
enum Task {
	Check,
	Watch,
}

#[derive(Parser, Debug)]
struct Args {
	task: Task,

	/// File to check, may be a folder with `watch`
	path: PathBuf,

	/// Document Language. Defaults to auto-detect, but explicit codes ("de-DE", "en-US", ...) enable more checks
	#[clap(short, long, default_value = None)]
	language: Option<String>,

	/// Delay in seconds
	#[clap(short, long, default_value_t = 0.1)]
	delay: f64,

	/// Print results without annotations for easy regex evaluation
	#[clap(short, long, default_value_t = false)]
	plain: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args = Args::parse();
	match args.task {
		Task::Check => check(args).await?,
		Task::Watch => watch(args).await?,
	}
	Ok(())
}

async fn check(args: Args) -> Result<(), Box<dyn std::error::Error>> {
	let client = ServerClient::new("http://127.0.0.1", "8081");
	handle_file(&client, &args, &args.path).await?;
	Ok(())
}

async fn watch(args: Args) -> Result<(), Box<dyn std::error::Error>> {
	let (tx, rx) = std::sync::mpsc::channel();
	let client = ServerClient::new("http://127.0.0.1", "8081");
	let mut watcher = new_debouncer(Duration::from_secs_f64(args.delay), None, tx)?;
	watcher
		.watcher()
		.watch(&args.path, RecursiveMode::Recursive)?;

	for events in rx {
		for event in events.unwrap() {
			match event.path.extension() {
				Some(ext) if ext == "typ" => {},
				_ => continue,
			}
			handle_file(&client, &args, &event.path)
				.await
				.unwrap_or_else(|err| println!("{}", err));
		}
	}

	Ok(())
}

async fn handle_file(client: &ServerClient, args: &Args, file: &PathBuf) -> Result<(), Error> {
	let text = fs::read_to_string(&file)?;

	let root = typst_syntax::parse(&text);
	let data = convert::convert(&root);

	if args.plain {
		println!("START");
	}
	let mut position = Position::new(&text);
	for items in data {
		let req = CheckRequest::default()
			.with_language(match &args.language {
				Some(value) => value.clone(),
				None => "auto".into(),
			})
			.with_data(Data::from_iter(items.0));

		let response = &client.check(&req).await?;
		if args.plain {
			output::output_plain(&file, &mut position, response, items.1);
		} else {
			output::output_pretty(&file, &mut position, response, items.1);
		}
	}
	if args.plain {
		println!("End");
	}
	Ok(())
}

mod output;

use clap::{Parser, ValueEnum};
use languagetool_rust::server::ServerClient;
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use output::Position;
use std::{
	collections::HashSet,
	fs::File,
	io::BufReader,
	path::{Path, PathBuf},
	time::Duration,
};

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

	/// Server Address
	#[clap(long, default_value = "http://127.0.0.1")]
	host: String,

	/// Server Port
	#[clap(long, default_value = "8081")]
	port: String,

	/// Split long documents into smaller chunks
	#[clap(long, default_value_t = 10_000)]
	max_request_length: usize,

	/// Overwrite `host`, `port` and `max-request-length` to the official API at `https://api.languagetoolplus.com`
	#[clap(long, default_value_t = false)]
	use_official_api: bool,

	/// Path to rules file
	#[clap(short, long, default_value = None)]
	rules: Option<String>,

	/// Path to dictionary file
	#[clap(short = 'w', long, default_value = None)]
	dictionary: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut args = Args::parse();

	if args.use_official_api {
		args.host = String::from("https://api.languagetoolplus.com");
		args.port = String::new();
		args.max_request_length = 1_000;
	}

	let dict = match args.dictionary {
		Some(ref dict_path) => {
			let dict_file = std::fs::read_to_string(dict_path)?;
			dict_file
				.lines()
				.map(str::trim)
				.map(str::to_owned)
				.collect::<HashSet<String>>()
		},
		_ => Default::default(),
	};

	match args.task {
		Task::Check => check(args, &dict).await?,
		Task::Watch => watch(args, &dict).await?,
	}
	Ok(())
}

async fn check(args: Args, dict: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
	let client = ServerClient::new(&args.host, &args.port);
	handle_file(&args.path, &client, &dict, &args).await?;
	Ok(())
}

async fn watch(args: Args, dict: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
	let (tx, rx) = std::sync::mpsc::channel();
	let client = ServerClient::new(&args.host, &args.port);
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
			handle_file(&event.path, &client, dict, &args).await?;
		}
	}
	Ok(())
}

async fn handle_file(
	path: &Path,
	client: &ServerClient,
	dict: &HashSet<String>,
	args: &Args,
) -> Result<(), Box<dyn std::error::Error>> {
	let mut text = std::fs::read_to_string(path)?;
	if !args.plain {
		// annotate snippet uses 1 step for tab, while the terminal uses more
		text = text.replace("\t", "    ");
	}

	let rules = if let Some(path) = &args.rules {
		let file = File::open(path)?;
		let reader = BufReader::new(file);
		serde_json::from_reader(reader)?
	} else {
		typst_languagetool::Rules::new()
	};

	if args.plain {
		println!("START");
	}
	let mut position = Position::new(&text);

	typst_languagetool::check(
		&client,
		&text,
		args.language.as_ref().map(|s| s.as_str()),
		&rules,
		args.max_request_length,
		dict,
		|response, _total| {
			if args.plain {
				output::output_plain(path, &mut position, response);
			} else {
				output::output_pretty(path, &mut position, response, 30);
			}
		},
	)
	.await?;

	if args.plain {
		println!("END");
	}

	Ok(())
}

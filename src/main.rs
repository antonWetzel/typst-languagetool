mod convert;
mod output;
mod rules;

use clap::{Parser, ValueEnum};
use languagetool_rust::{
	check::{CheckRequest, Data},
	server::ServerClient,
	CheckResponse,
};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use output::Position;
use rules::Rules;
use std::{
	collections::HashSet,
	error::Error,
	fs,
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
	#[clap(short = 'H', long, default_value = "http://127.0.0.1")]
	host: String,

	/// Server Port
	#[clap(short = 'P', long, default_value = "8081")]
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
	handle_file(&client, &args, &args.path, &dict).await?;
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
			handle_file(&client, &args, &event.path, &dict)
				.await
				.unwrap_or_else(|err| println!("{}", err));
		}
	}

	Ok(())
}

async fn handle_file(
	client: &ServerClient,
	args: &Args,
	file: &Path,
	dict: &HashSet<String>,
) -> Result<(), Box<dyn Error>> {
	let text = fs::read_to_string(file)?;
	let rules = match &args.rules {
		None => Rules::new(),
		Some(path) => Rules::load(path)?,
	};

	let root = typst_syntax::parse(&text);
	let data = convert::convert(&root, &rules, args.max_request_length);

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

		let response = &mut client.check(&req).await?;

		filter_response(response, dict);

		if args.plain {
			output::output_plain(file, &mut position, response, items.1);
		} else {
			output::output_pretty(file, &mut position, response, items.1);
		}
	}
	if args.plain {
		println!("END");
	}
	Ok(())
}

fn filter_response(response: &mut CheckResponse, dict: &HashSet<String>) {
	for m in std::mem::take(&mut response.matches).into_iter() {
		// Only handle misspellings
		if m.rule.issue_type.as_str() != "misspelling" {
			response.matches.push(m);
			continue;
		}
		// Check if the word is contained in the dictionary
		let ctx = &m.context;
		let mut chars = ctx.text.char_indices();
		let start = chars.nth(ctx.offset).map_or(0, |(idx, _)| idx);
		let end = chars
			.nth(ctx.length.wrapping_sub(1))
			.map_or(ctx.text.len(), |(idx, _)| idx);
        print!("({start}, {end})");
		let word = &ctx.text[start..end];
		if dict.contains(word) {
			continue;
		}
		response.matches.push(m);
	}
}

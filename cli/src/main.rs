mod output;

use clap::{Parser, ValueEnum};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use typst_languagetool::{LanguageTool, TextWithPosition};

use std::{
	fs::File,
	io::BufReader,
	path::{Path, PathBuf},
	time::Duration,
};

use crate::output::{output_plain, output_pretty};

#[derive(ValueEnum, Clone, Debug)]
enum Task {
	Check,
	Watch,
}

#[derive(Parser, Debug)]
struct Args {
	task: Task,

	/// File to check, may be a folder with `watch`.
	path: PathBuf,

	/// Document Language ("de-DE", "en-US", ...).
	#[clap(short, long, default_value = "en-US")]
	language: String,

	/// Delay for file changes.
	#[clap(long, default_value_t = 0.1, id = "SECONDS")]
	delay: f64,

	/// Print results without annotations for easy regex evaluation.
	#[clap(short, long, default_value_t = false)]
	plain: bool,

	/// Path to rules file.
	#[clap(short, long, default_value = None)]
	rules: Option<PathBuf>,

	/// Path to dictionary file.
	#[clap(short, long, default_value = None)]
	dictionary: Option<PathBuf>,

	#[clap(long = "disabled-check", id = "ID")]
	/// Languagetool Rule ID to ignore.
	disabled_checks: Vec<String>,

	/// Use bundled languagetool jar.
	#[clap(long, default_value_t = false)]
	bundled: bool,

	/// Custom location for the languagetool jar.
	#[clap(long, default_value = None)]
	jar_location: Option<String>,

	/// Host for remote languagetool server.
	#[clap(long, default_value = None)]
	host: Option<String>,

	/// Port for remote languagetool server.
	#[clap(long, default_value = None)]
	port: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let args = Args::parse();

	let mut lt = LanguageTool::new(
		args.bundled,
		args.jar_location.as_ref(),
		args.host.as_ref(),
		args.port.as_ref(),
		&args.language,
	)?;

	if let Some(path) = &args.dictionary {
		let content = std::fs::read_to_string(path)?;
		let words = content
			.lines()
			.map(|line| String::from(line))
			.collect::<Vec<_>>();
		lt.allow_words(&words).await?;
	}
	lt.disable_checks(&args.disabled_checks).await?;

	match args.task {
		Task::Check => check(args, &mut lt).await?,
		Task::Watch => watch(args, &mut lt).await?,
	}

	Ok(())
}

async fn check(args: Args, lt: &mut LanguageTool) -> anyhow::Result<()> {
	handle_file(&args.path, lt, &args).await?;
	Ok(())
}

async fn watch(args: Args, lt: &mut LanguageTool) -> anyhow::Result<()> {
	let (tx, rx) = std::sync::mpsc::channel();
	let mut watcher = new_debouncer(Duration::from_secs_f64(args.delay), tx)?;
	watcher
		.watcher()
		.watch(&args.path, RecursiveMode::Recursive)?;

	for events in rx {
		for event in events.unwrap() {
			match event.path.extension() {
				Some(ext) if ext == "typ" => {},
				_ => continue,
			}
			handle_file(&event.path, lt, &args).await?;
		}
	}
	Ok(())
}

async fn handle_file(path: &Path, lt: &LanguageTool, args: &Args) -> anyhow::Result<()> {
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
	let mut position = TextWithPosition::new(&text);
	let suggestions = lt.check_source(&text, &rules).await?;
	for suggestion in suggestions {
		if args.plain {
			output_plain(path, &mut position, suggestion);
		} else {
			output_pretty(path, &mut position, suggestion, 50);
		}
	}
	if args.plain {
		println!("END");
	}

	Ok(())
}

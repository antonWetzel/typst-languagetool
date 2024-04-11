mod output;

use clap::{Parser, ValueEnum};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use typst_languagetool::{LanguageTool, Position};

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

	/// File to check, may be a folder with `watch`
	path: PathBuf,

	/// Document Language. ("de-DE", "en-US", ...)
	#[clap(short, long, default_value = "en-US")]
	language: String,

	/// Delay in seconds
	#[clap(short, long, default_value_t = 0.1)]
	delay: f64,

	/// Print results without annotations for easy regex evaluation
	#[clap(short, long, default_value_t = false)]
	plain: bool,

	/// Path to rules file
	#[clap(short, long, default_value = None)]
	rules: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args = Args::parse();

	match args.task {
		Task::Check => check(args)?,
		Task::Watch => watch(args)?,
	}

	Ok(())
}

fn check(args: Args) -> Result<(), Box<dyn std::error::Error>> {
	let lt = LanguageTool::new(&args.language)?;
	handle_file(&args.path, &lt, &args)?;
	Ok(())
}

fn watch(args: Args) -> Result<(), Box<dyn std::error::Error>> {
	let (tx, rx) = std::sync::mpsc::channel();
	let lt = LanguageTool::new(&args.language)?;
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
			handle_file(&event.path, &lt, &args)?;
		}
	}
	Ok(())
}

fn handle_file(
	path: &Path,
	lt: &LanguageTool,
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
	let suggestions = typst_languagetool::check(&lt, &text, &rules)?;
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

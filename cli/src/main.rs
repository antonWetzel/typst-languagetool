mod output;

use clap::{Parser, ValueEnum};

use colored::Colorize;
use lt_world::LtWorld;
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use typst_languagetool::{LanguageTool, LanguageToolBackend, Suggestion};

use std::{
	collections::HashMap,
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

	/// File to check, may be a folder with `watch`.
	#[clap(short, long, default_value = None)]
	path: Option<PathBuf>,

	/// Main file for the document. Defaults to `path`.
	#[clap(short, long, default_value = None)]
	root: Option<PathBuf>,

	/// Main file for the document.
	/// Defaults to `path`.
	#[clap(short, long, default_value = None)]
	main: Option<PathBuf>,

	/// Document Language ("de-DE", "en-US", ...).
	#[clap(short, long, default_value = "en-US")]
	language: String,

	/// Delay for file changes.
	#[clap(long, default_value_t = 0.1, id = "SECONDS")]
	delay: f64,

	/// Length in chars to seperate chunks
	#[clap(long, default_value_t = 1000)]
	chunk_size: usize,

	/// Print results without annotations for easy regex evaluation.
	#[clap(long, default_value_t = false)]
	plain: bool,

	/// Path to dictionary file.
	#[clap(short, long, default_value = None)]
	dictionary: Option<PathBuf>,

	/// Languagetool Rule ID to ignore.
	#[clap(long = "disabled-check", id = "ID")]
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

	let world = match (args.path.clone(), args.main.clone()) {
		(_, Some(main)) => lt_world::LtWorld::new(main, args.root.clone()),
		(Some(main), None) => lt_world::LtWorld::new(main, args.root.clone()),

		_ => return Err(anyhow::anyhow!("Invalid typst settings.")),
	};

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
		Task::Check => check(args, lt, world).await?,
		Task::Watch => watch(args, lt, world).await?,
	}

	Ok(())
}

async fn check(args: Args, mut lt: LanguageTool, mut world: LtWorld) -> anyhow::Result<()> {
	handle_file(
		args.path.as_ref().unwrap(),
		&mut lt,
		&args,
		&mut world,
		args.chunk_size,
		&mut Cache::new(),
	)
	.await?;
	Ok(())
}

async fn watch(args: Args, mut lt: LanguageTool, mut world: LtWorld) -> anyhow::Result<()> {
	let (tx, rx) = std::sync::mpsc::channel();
	let mut watcher = new_debouncer(Duration::from_secs_f64(args.delay), tx)?;
	let mut cache = Cache::new();
	watcher
		.watcher()
		.watch(world.root(), RecursiveMode::Recursive)?;

	for events in rx {
		for event in events.unwrap() {
			match event.path.extension() {
				Some(ext) if ext == "typ" => {},
				_ => continue,
			}

			handle_file(
				&event.path,
				&mut lt,
				&args,
				&mut world,
				args.chunk_size,
				&mut cache,
			)
			.await?;
		}
	}
	Ok(())
}

async fn handle_file(
	path: &Path,
	lt: &LanguageTool,
	args: &Args,
	world: &LtWorld,
	chunk_size: usize,
	cache: &mut Cache,
) -> anyhow::Result<()> {
	let Some(doc) = world.compile() else {
		if args.plain {
			println!("Failed to compile document!");
		} else {
			println!("{}", "Failed to compile document!\n".red().bold());
		}
		return Ok(());
	};

	let file_id = world.file_id(path);
	let paragraphs = typst_languagetool::convert::document(&doc, chunk_size, file_id);
	let mut collector = typst_languagetool::FileCollector::new(file_id, world);
	let mut next_cache = Cache::new();
	for (text, mapping) in paragraphs {
		let suggestions = if let Some(suggestions) = cache.get(&text) {
			suggestions
		} else {
			lt.check_text(&text).await?
		};

		collector.add(&suggestions, mapping);
		next_cache.insert(text, suggestions);
	}
	*cache = next_cache;

	let (source, diagnostics) = collector.finish();

	if args.plain {
		println!("START");
		for diagnostic in diagnostics {
			output::plain(&path, &source, diagnostic);
		}
		println!("END");
	} else {
		println!("{}", "\n\nChecking Document\n".green().bold());
		for diagnostic in diagnostics {
			output::pretty(&path, &source, diagnostic);
		}
	}

	Ok(())
}

#[derive(Debug)]
struct Cache {
	cache: HashMap<String, Vec<Suggestion>>,
}

impl Cache {
	pub fn new() -> Self {
		Self { cache: HashMap::new() }
	}

	pub fn get(&mut self, text: &str) -> Option<Vec<Suggestion>> {
		self.cache.remove(text)
	}

	pub fn insert(&mut self, text: String, suggestions: Vec<Suggestion>) {
		self.cache.insert(text, suggestions);
	}
}

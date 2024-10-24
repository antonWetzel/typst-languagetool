mod output;

use anyhow::Context;
use clap::{Parser, ValueEnum};

use colored::Colorize;
use lt_world::LtWorld;
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use typst::World;
use typst_languagetool::{LanguageTool, LanguageToolBackend, Suggestion};

use std::{
	collections::HashMap,
	ops::Not,
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

	/// Delay for file changes.
	#[clap(long, default_value_t = 0.1, id = "SECONDS")]
	delay: f64,

	/// Length in chars to seperate chunks
	#[clap(long, default_value_t = 1000)]
	chunk_size: usize,

	/// Print results without annotations for easy regex evaluation.
	#[clap(long, default_value_t = false)]
	plain: bool,

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

	let lt = LanguageTool::new(
		args.bundled,
		args.jar_location.as_ref(),
		args.host.as_ref(),
		args.port.as_ref(),
	)?;

	let world = lt_world::LtWorld::new(args.root.clone().unwrap_or(".".into()));

	match args.task {
		Task::Check => check(args, lt, world).await?,
		Task::Watch => watch(args, lt, world).await?,
	}

	Ok(())
}

async fn check(args: Args, mut lt: LanguageTool, mut world: LtWorld) -> anyhow::Result<()> {
	handle_file(
		args.path
			.as_ref()
			.or_else(|| args.main.as_ref())
			.context("No path or main specified")?,
		&mut lt,
		&args,
		&mut world,
		args.chunk_size,
		&mut Cache::new(),
		args.path.is_none(),
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
				false,
			)
			.await?;
		}
	}
	Ok(())
}

async fn handle_file(
	path: &Path,
	lt: &mut LanguageTool,
	args: &Args,
	world: &LtWorld,
	chunk_size: usize,
	cache: &mut Cache,
	include_all: bool,
) -> anyhow::Result<()> {
	let world = world.with_main(args.main.clone().unwrap_or(path.to_owned()));
	let doc = match world.compile() {
		Ok(doc) => doc,
		Err(err) => {
			if args.plain {
				println!("Failed to compile document!");
			} else {
				println!("{}", "Failed to compile document!\n".red().bold());
			}
			for dia in err {
				println!("\t{:?}", dia);
			}
			return Ok(());
		},
	};

	let file_id = world.file_id(path).unwrap();
	let file_id_opt = include_all.not().then_some(file_id);

	let paragraphs = typst_languagetool::convert::document(&doc, chunk_size, file_id_opt);
	let mut collector = typst_languagetool::FileCollector::new(file_id_opt, &world);
	let mut next_cache = Cache::new();
	for (text, mapping) in paragraphs {
		let lang = mapping.long_language();
		let suggestions = if let Some(suggestions) = cache.get(&text, &lang) {
			suggestions
		} else {
			lt.check_text(lang.clone(), &text).await?
		};

		collector.add(&world, &suggestions, &mapping);
		next_cache.insert(text, lang, suggestions);
	}
	*cache = next_cache;

	let diagnostics = collector.finish();

	if include_all {
		if args.plain {
			plain_start();
			for diagnostic in diagnostics {
				let id = diagnostic.locations[0].0;
				let source = world.source(id).unwrap();
				let path = id.vpath().as_rootless_path();
				output::plain(&path, &source, diagnostic);
			}
			plain_end();
		} else {
			pretty_start();
			for diagnostic in diagnostics {
				let id = diagnostic.locations[0].0;
				let source = world.source(id).unwrap();
				let path = id.vpath().as_rootless_path();
				output::pretty(&path, &source, diagnostic);
			}
		}
	} else {
		let source = world.source(file_id).unwrap();
		if args.plain {
			plain_start();
			for diagnostic in diagnostics {
				output::plain(&path, &source, diagnostic);
			}
			plain_end();
		} else {
			pretty_start();
			println!("{}", "\n\nChecking Document\n".green().bold());
			for diagnostic in diagnostics {
				output::pretty(&path, &source, diagnostic);
			}
		}
	}
	Ok(())
}

fn plain_start() {
	println!("START");
}

fn plain_end() {
	println!("END");
}

fn pretty_start() {
	println!("{}", "\n\nChecking Document\n".green().bold());
}

#[derive(Debug)]
struct Cache {
	cache: HashMap<String, (String, Vec<Suggestion>)>,
}

impl Cache {
	pub fn new() -> Self {
		Self { cache: HashMap::new() }
	}

	pub fn get(&mut self, text: &str, lang: &str) -> Option<Vec<Suggestion>> {
		let entry = self.cache.remove(text)?;
		(lang == entry.0).then_some(entry.1)
	}

	pub fn insert(&mut self, text: String, lang: String, suggestions: Vec<Suggestion>) {
		self.cache.insert(text, (lang, suggestions));
	}
}

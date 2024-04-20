use std::collections::HashMap;
use std::fmt::Display;
use std::ops::Not;
use std::path::{Path, PathBuf};

use lt_world::LtWorld;
use typst::syntax::Source;
use typst_languagetool::{LanguageTool, LanguageToolBackend, Suggestion};

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{self, Result};
use tower_lsp::lsp_types::notification::*;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let stdin = tokio::io::stdin();
	let stdout = tokio::io::stdout();

	let (service, socket) = LspService::new(|client| Backend {
		client,
		state: RwLock::new(State::Started),
	});

	Server::new(stdin, stdout, socket).serve(service).await;
	Ok(())
}

#[derive(Debug)]
struct Backend {
	client: Client,
	state: RwLock<State>,
}

#[derive(Debug)]
enum State {
	Started,
	Running {
		lt: LanguageTool,
		options: Options,
		world: LtWorld,
		cache: Cache,
	},
}

fn map_err(err: impl Into<anyhow::Error>) -> jsonrpc::Error {
	jsonrpc::Error {
		code: jsonrpc::ErrorCode::InternalError,
		message: format!("{}", err.into()).into(),
		data: None,
	}
}

impl Backend {
	async fn error(&self, err: impl Display) {
		self.client.log_message(MessageType::ERROR, err).await;
	}
}

fn make_absolute(cwd: &Path, path: &mut Option<PathBuf>) {
	if let Some(path) = path {
		if path.is_absolute() {
			return;
		}
		*path = cwd.join(&path)
	}
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
	async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
		let mut options = (|| {
			let options = params.initialization_options?;
			let options = serde_ignored::deserialize(options, |path| {
				eprintln!("unknown option: {}", path);
			})
			.ok()?;
			Some(options)
		})()
		.unwrap_or(Options::default());

		let capabilities = ServerCapabilities {
			text_document_sync: Some(TextDocumentSyncCapability::Options(
				TextDocumentSyncOptions {
					open_close: Some(true),
					save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
						include_text: Some(false),
					})),
					change: Some(TextDocumentSyncKind::INCREMENTAL),
					..Default::default()
				},
			)),

			code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
			..Default::default()
		};

		let cwd = std::env::current_dir().unwrap();
		make_absolute(&cwd, &mut options.path);
		make_absolute(&cwd, &mut options.main);
		make_absolute(&cwd, &mut options.root);

		let lt = options.create_lt().await.map_err(map_err)?;

		let world = match (options.path.clone(), options.main.clone()) {
			(_, Some(main)) => lt_world::LtWorld::new(main, options.root.clone()),
			(Some(main), None) => lt_world::LtWorld::new(main, options.root.clone()),

			_ => return Err(map_err(anyhow::anyhow!("Invalid typst settings."))),
		};
		self.client
			.log_message(MessageType::INFO, "First compilation")
			.await;

		let mut state = self.state.write().await;
		*state = State::Running { lt, options, world, cache: Cache::new() };
		Ok(InitializeResult { server_info: None, capabilities })
	}

	async fn initialized(&self, _: InitializedParams) {
		self.client
			.log_message(MessageType::INFO, "server initialized!")
			.await;

		let mut state = self.state.write().await;
		let State::Running { world, cache, lt, options, .. } = &mut *state else {
			return self.error("invalid state in 'did_open'").await;
		};

		let Some(doc) = world.compile() else {
			self.client
				.log_message(MessageType::INFO, "Failed to compile document.")
				.await;
			return;
		};
		let paragraphs = typst_languagetool::convert::document(&doc, options.chunk_size);
		let l = paragraphs.len();
		for (idx, (text, _)) in paragraphs.into_iter().enumerate() {
			let Ok(suggestions) = lt.check_text(&text).await else {
				continue;
			};
			cache.insert(text, suggestions);
			eprintln!("Initial check: {}/{}", idx + 1, l);
		}
	}

	async fn did_open(&self, params: DidOpenTextDocumentParams) {
		let mut state = self.state.write().await;
		let State::Running { world, .. } = &mut *state else {
			return self.error("invalid state in 'did_open'").await;
		};
		world.use_shadow_file(
			&params.text_document.uri.to_file_path().unwrap(),
			params.text_document.text,
		);
	}

	async fn did_close(&self, params: DidCloseTextDocumentParams) {
		let mut state = self.state.write().await;
		let State::Running { world, .. } = &mut *state else {
			return self.error("invalid state in 'did_close'").await;
		};
		world.use_original_file(&params.text_document.uri.to_file_path().unwrap());
	}

	async fn did_save(&self, params: DidSaveTextDocumentParams) {
		self.client
			.log_message(
				MessageType::INFO,
				format!("Checking: {:#?}", params.text_document.uri.path()),
			)
			.await;

		let diagnostics = {
			let mut state = self.state.write().await;

			let State::Running { lt, world, cache, options, .. } = &mut *state else {
				return self.error("invalid state in 'did_save'").await;
			};

			let path = params.text_document.uri.to_file_path().unwrap();

			match get_diagnostics(&path, lt, world, cache, options.chunk_size).await {
				Ok(d) => d,
				Err(err) => return self.error(err).await,
			}
		};

		let params = PublishDiagnosticsParams {
			uri: params.text_document.uri,
			version: None,
			diagnostics,
		};
		self.client
			.send_notification::<PublishDiagnostics>(params)
			.await;
	}

	async fn did_change(&self, params: DidChangeTextDocumentParams) {
		let mut state = self.state.write().await;
		let State::Running { world, options, lt, cache } = &mut *state else {
			return self.error("invalid state in 'did_change'").await;
		};
		let source = world
			.shadow_file(&params.text_document.uri.to_file_path().unwrap())
			.unwrap();

		for change in &params.content_changes {
			if let Some(range) = change.range {
				let start = source
					.line_column_to_byte(range.start.line as usize, range.start.character as usize)
					.unwrap();
				let end = source
					.line_column_to_byte(range.end.line as usize, range.end.character as usize)
					.unwrap();
				source.edit(start..end, &change.text);
			} else {
				source.replace(&change.text);
			}
		}

		if options.on_change.not() {
			return;
		}
		let update = params.content_changes.iter().any(|change| {
			change.text.is_empty() || change.text.chars().any(|c| c.is_alphanumeric().not())
		});

		if update.not() {
			return;
		}

		let path = params.text_document.uri.to_file_path().unwrap();

		let diagnostics = match get_diagnostics(&path, lt, world, cache, options.chunk_size).await {
			Ok(d) => d,
			Err(err) => return self.error(err).await,
		};

		let params = PublishDiagnosticsParams {
			uri: params.text_document.uri,
			version: None,
			diagnostics,
		};
		self.client
			.send_notification::<PublishDiagnostics>(params)
			.await;
	}

	async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
		let mut unknown_options = String::new();
		let options = match serde_ignored::deserialize::<_, _, Options>(params.settings, |path| {
			unknown_options += format!("{} ", path).as_str();
		}) {
			Ok(o) => o,
			Err(err) => return self.error(err).await,
		};
		if unknown_options.is_empty().not() {
			self.client
				.log_message(
					MessageType::WARNING,
					format!("Unknown Options: {}", unknown_options),
				)
				.await
		}

		let lt = match options.create_lt().await {
			Ok(lt) => lt,
			Err(err) => return self.error(err).await,
		};
		self.client
			.log_message(
				MessageType::INFO,
				format!("Updating Options: {:#?}", options),
			)
			.await;
		let mut state = self.state.write().await;
		match &mut *state {
			State::Running { lt: old_lt, options: old_options, .. } => {
				*old_lt = lt;
				*old_options = options;
			},
			_ => unreachable!(),
		}
	}

	async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
		let mut action = CodeActionResponse::new();

		let Some(diagnostic) = params.context.diagnostics.last() else {
			return Ok(None);
		};
		let Some(data) = &diagnostic.data else {
			return Ok(None);
		};

		let replacements = match serde_json::from_value::<Vec<String>>(data.clone()) {
			Ok(r) => r,
			Err(err) => {
				self.error(err).await;
				return Ok(None);
			},
		};

		for (i, value) in replacements.into_iter().enumerate() {
			let title = format!("Replace with \"{}\"", value);
			let replace = TextEdit { range: diagnostic.range, new_text: value };
			let edit = [(params.text_document.uri.clone(), vec![replace])]
				.into_iter()
				.collect();

			action.push(
				CodeAction {
					title,
					is_preferred: Some(i == 0),
					kind: Some(CodeActionKind::QUICKFIX),
					diagnostics: Some(params.context.diagnostics.clone()),
					edit: Some(WorkspaceEdit {
						changes: Some(edit),
						..Default::default()
					}),
					command: None,
					disabled: None,
					data: None,
				}
				.into(),
			);
		}
		Ok(Some(action))
	}

	async fn shutdown(&self) -> Result<()> {
		Ok(())
	}
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(default)]
struct Options {
	language: String,
	dictionary: Vec<String>,
	disabled_checks: Vec<String>,

	bundled: bool,
	jar_location: Option<String>,
	host: Option<String>,
	port: Option<String>,

	chunk_size: usize,
	on_change: bool,

	path: Option<PathBuf>,
	root: Option<PathBuf>,
	main: Option<PathBuf>,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			language: "en-US".into(),
			dictionary: Vec::new(),
			disabled_checks: Vec::new(),

			bundled: false,
			jar_location: None,
			host: None,
			port: None,

			chunk_size: 1000,
			on_change: false,

			path: None,
			root: None,
			main: None,
		}
	}
}

impl Options {
	async fn create_lt(&self) -> anyhow::Result<LanguageTool> {
		let mut lt = LanguageTool::new(
			self.bundled,
			self.jar_location.as_ref(),
			self.host.as_ref(),
			self.port.as_ref(),
			&self.language,
		)?;
		lt.allow_words(&self.dictionary).await?;
		lt.disable_checks(&self.disabled_checks).await?;
		Ok(lt)
	}
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

async fn get_diagnostics(
	path: &Path,
	lt: &LanguageTool,
	world: &LtWorld,
	cache: &mut Cache,
	chunk_size: usize,
) -> anyhow::Result<Vec<Diagnostic>> {
	let Some(doc) = world.compile() else {
		eprintln!("TODO: Warning could not compile");
		return Ok(Vec::new());
	};

	let paragraphs = typst_languagetool::convert::document(&doc, chunk_size);
	let file_id = world.file_id(path);
	let mut collector = typst_languagetool::FileCollector::new(file_id, world);
	let mut next_cache = Cache::new();
	let l = paragraphs.len();
	for (idx, (text, mapping)) in paragraphs.into_iter().enumerate() {
		let suggestions = if let Some(suggestions) = cache.get(&text) {
			suggestions
		} else {
			eprintln!("Checking {}/{}", idx, l);
			lt.check_text(&text).await?
		};
		collector.add(&suggestions, mapping);
		next_cache.insert(text, suggestions);
	}
	*cache = next_cache;

	let (source, diagnostics) = collector.finish();

	let diagnostics = diagnostics
		.into_iter()
		.map(|diagnostic| {
			let (start_line, start_column) =
				byte_to_position(&source, diagnostic.locations[0].start);
			let (end_line, end_column) = byte_to_position(&source, diagnostic.locations[0].end);

			Diagnostic {
				range: Range {
					start: tower_lsp::lsp_types::Position {
						line: start_line as u32,
						character: start_column as u32,
					},
					end: tower_lsp::lsp_types::Position {
						line: end_line as u32,
						character: end_column as u32,
					},
				},
				severity: Some(DiagnosticSeverity::INFORMATION),
				code: Some(NumberOrString::String(diagnostic.rule_id)),
				code_description: None,
				source: None,
				message: diagnostic.message,
				related_information: None,
				tags: None,
				data: serde_json::to_value(diagnostic.replacements).ok(),
			}
		})
		.collect();

	Ok(diagnostics)
}
fn byte_to_position(source: &Source, index: usize) -> (usize, usize) {
	let line = source.byte_to_line(index).unwrap();
	let start = source.line_to_byte(line).unwrap();
	let head = source.get(start..index).unwrap();
	let column = head.chars().count();
	(line, column)
}

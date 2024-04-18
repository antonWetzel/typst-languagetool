use std::collections::HashMap;
use std::fmt::Display;
use std::ops::Not;

use typst_languagetool::{LanguageTool, Rules, TextWithPosition};

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
		sources: HashMap<Url, Source>,
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

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
	async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
		let options = (|| {
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

		let lt = options.create_lt().await.map_err(map_err)?;
		let mut state = self.state.write().await;
		*state = State::Running { lt, options, sources: HashMap::new() };

		Ok(InitializeResult { server_info: None, capabilities })
	}

	async fn initialized(&self, _: InitializedParams) {
		self.client
			.log_message(MessageType::INFO, "server initialized!")
			.await;
	}

	async fn did_open(&self, params: DidOpenTextDocumentParams) {
		let source = Source::new(&params.text_document.text);
		let mut state = self.state.write().await;
		let State::Running { sources, .. } = &mut *state else {
			return self.error("invalid state in 'did_open'").await;
		};
		sources.insert(params.text_document.uri, source);
	}

	async fn did_close(&self, params: DidCloseTextDocumentParams) {
		let mut state = self.state.write().await;
		let State::Running { sources, .. } = &mut *state else {
			return self.error("invalid state in 'did_close'").await;
		};
		sources.remove(&params.text_document.uri);
	}

	async fn did_save(&self, params: DidSaveTextDocumentParams) {
		self.client
			.log_message(
				MessageType::INFO,
				format!("Checking: {:#?}", params.text_document.uri.path()),
			)
			.await;

		let diagnostics = {
			let state = self.state.read().await;

			let State::Running { lt, options, sources, .. } = &*state else {
				return self.error("invalid state in 'did_save'").await;
			};

			let content = sources.get(&params.text_document.uri).unwrap().text();

			match get_diagnostics(&content, &lt, &options, 0).await {
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
		let on_change = {
			let mut state = self.state.write().await;
			let State::Running { sources, options, .. } = &mut *state else {
				return self.error("invalid state in 'did_change'").await;
			};
			let source = sources.get_mut(&params.text_document.uri).unwrap();
			for change in &params.content_changes {
				if let Some(range) = change.range {
					source.edit(range, &change.text);
				}
			}
			options.on_change
		};

		if on_change.not() {
			return;
		}
		let update = params.content_changes.iter().any(|change| {
			change.text.is_empty() || change.text.chars().any(|c| c.is_alphanumeric().not())
		});

		if update.not() {
			return;
		}

		let (start, end, diagnostics) = {
			let state = self.state.read().await;
			let State::Running { lt, options, sources, .. } = &*state else {
				return self.error("invalid state in 'did_change'").await;
			};

			let source = sources.get(&params.text_document.uri).unwrap();
			let line = params
				.content_changes
				.iter()
				.find_map(|change| change.range)
				.unwrap()
				.end
				.line as usize;

			let mut start = line;
			while start > 0 && source.line(start).is_empty().not() {
				start -= 1;
			}
			let mut end = line;
			while source.line(end).is_empty().not() {
				end += 1;
			}
			let mut content = String::new();
			for i in start..end {
				content += source.line(i);
				content += "\n";
			}

			let diagnostics = match get_diagnostics(&content, lt, options, start).await {
				Ok(d) => d,
				Err(err) => return self.error(err).await,
			};

			(start, end, diagnostics)
		};

		self.client
			.log_message(
				MessageType::INFO,
				format!(
					"Checking: {:#?} ({}-{})",
					params.text_document.uri.path(),
					start + 1,
					end + 1,
				),
			)
			.await;

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
	rules: Rules,
	dictionary: Vec<String>,
	disabled_checks: Vec<String>,
	bundled: bool,
	jar_location: Option<String>,
	host: Option<String>,
	port: Option<String>,
	on_change: bool,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			language: "en-US".into(),
			rules: Rules::new(),
			dictionary: Vec::new(),
			disabled_checks: Vec::new(),
			bundled: false,
			jar_location: None,
			host: None,
			port: None,
			on_change: false,
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

async fn get_diagnostics(
	text: &str,
	lt: &LanguageTool,
	options: &Options,
	line: usize,
) -> anyhow::Result<Vec<Diagnostic>> {
	let mut position = TextWithPosition::new_with_line(&text, line);

	let diagnostics = lt
		.check_source(text, &options.rules)
		.await?
		.into_iter()
		.map(|suggestion| {
			let start = position.get_position(suggestion.start, false);
			let end = position.get_position(suggestion.end, false);

			Diagnostic {
				range: Range {
					start: tower_lsp::lsp_types::Position {
						line: start.line as u32,
						character: start.column as u32,
					},
					end: tower_lsp::lsp_types::Position {
						line: end.line as u32,
						character: end.column as u32,
					},
				},
				severity: Some(DiagnosticSeverity::INFORMATION),
				code: Some(NumberOrString::String(suggestion.rule_id)),
				code_description: None,
				source: None,
				message: suggestion.message,
				related_information: None,
				tags: None,
				data: serde_json::to_value(suggestion.replacements).ok(),
			}
		})
		.collect::<Vec<_>>();

	Ok(diagnostics)
}

#[derive(Debug)]
struct Source {
	lines: Vec<String>,
}

impl Source {
	pub fn new(text: &str) -> Self {
		Self { lines: Self::lines(text).collect() }
	}

	fn lines(text: &str) -> impl Iterator<Item = String> + '_ {
		text.split("\n").map(|line| String::from(line))
	}

	pub fn edit(&mut self, range: Range, text: &str) {
		let start = self
			.lines
			.get(range.start.line as usize)
			.map(|line| line.chars().take(range.start.character as usize))
			.unwrap_or("".chars().take(0));
		let end = self
			.lines
			.get(range.end.line as usize)
			.map(|line| line.chars().skip(range.end.character as usize))
			.unwrap_or("".chars().skip(0));
		let max = (range.end.line as usize + 1).min(self.lines.len());
		let mut res = start.collect::<String>();
		res += text;
		res.extend(end);

		self.lines.splice(
			range.start.line as usize..max,
			Self::lines(&res).map(|line| String::from(line)),
		);
	}

	pub fn line(&self, index: usize) -> &str {
		self.lines
			.get(index)
			.map(|line| line.as_str())
			.unwrap_or("")
	}

	pub fn text(&self) -> String {
		let mut res = String::from(&self.lines[0]);
		for line in &self.lines[1..] {
			res += "\n";
			res += line;
		}
		res
	}
}

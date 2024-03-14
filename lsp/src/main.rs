use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;
use std::str::Chars;

use languagetool_rust::ServerClient;
use lsp_types::notification::{
	DidChangeConfiguration, DidOpenTextDocument, DidSaveTextDocument, PublishDiagnostics,
};
use lsp_types::request::CodeActionRequest;
use lsp_types::{
	CodeAction, CodeActionKind, CodeActionProviderCapability, CodeActionResponse, Diagnostic,
	DiagnosticSeverity, NumberOrString, PublishDiagnosticsParams, Range, SaveOptions,
	TextDocumentSyncOptions, TextDocumentSyncSaveOptions, TextEdit, WorkspaceEdit,
};
use lsp_types::{InitializeParams, ServerCapabilities};

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use typst_languagetool::Rules;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	eprintln!("starting LSP server");

	let (connection, io_threads) = Connection::stdio();

	let server_capabilities = serde_json::to_value(&ServerCapabilities {
		text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Options(
			TextDocumentSyncOptions {
				open_close: Some(true),
				save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
					include_text: Some(true),
				})),
				..Default::default()
			},
		)),
		code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
		..Default::default()
	})
	.unwrap();
	let initialization_params = match connection.initialize(server_capabilities) {
		Ok(it) => it,
		Err(e) => {
			if e.channel_is_disconnected() {
				io_threads.join()?;
			}
			return Err(e.into());
		},
	};
	main_loop(connection, initialization_params).await?;
	io_threads.join()?;

	eprintln!("shutting down server");
	Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(default)]
struct Options {
	language: Option<String>,
	host: String,
	port: String,
	request_length: usize,
	rules: Rules,
	dictionary: HashSet<String>,
	local_languagetool_directory: Option<PathBuf>,
}

const DEFAULT_HOST: &str = "http://127.0.0.1";

impl Default for Options {
	fn default() -> Self {
		Self {
			language: None,
			host: DEFAULT_HOST.into(),
			port: "8081".into(),
			request_length: 1000,
			rules: Rules::new(),
			dictionary: HashSet::new(),
			local_languagetool_directory: None,
		}
	}
}

struct ServerProcess(std::process::Child);

impl Drop for ServerProcess {
	fn drop(&mut self) {
		self.0.kill().unwrap();
		eprintln!("Language tool process should close, but it likes to stay open");
	}
}

async fn main_loop(
	connection: Connection,
	params: serde_json::Value,
) -> Result<(), Box<dyn Error>> {
	let mut options = (|| {
		let params = serde_json::from_value::<InitializeParams>(params).ok()?;
		let options = params.initialization_options?;
		let options = serde_json::from_value(options).ok()?;

		Some(options)
	})()
	.unwrap_or(Options::default());

	eprintln!("{:#?}", options);

	let _server = if let Some(path) = &options.local_languagetool_directory {
		let jar = path.join("languagetool-server.jar");

		let mut config = path.clone();
		config.push("server.properties");

		if options.host.as_str() != DEFAULT_HOST {
			eprint!(
				"Overwritting host {:?} with {:?} because a local server is used.",
				options.host, DEFAULT_HOST
			);
			options.host = DEFAULT_HOST.into();
		}

		let mut command = std::process::Command::new("java");
		command
			.arg("-cp")
			.arg(jar)
			.arg("org.languagetool.server.HTTPServer")
			.arg("--port")
			.arg(&options.port)
			.arg("--allow-origin")
			.stdout(std::io::stderr());

		if config.exists() {
			command.arg("--config").arg(config);
		}

		match command.spawn() {
			Ok(server) => Some(ServerProcess(server)),
			Err(err) => {
				eprintln!("Failed to start server because {:?}", err);
				None
			},
		}
	} else {
		None
	};

	eprintln!("starting client at {}:{}", options.host, options.port);
	let client = ServerClient::new(&options.host, &options.port);
	check_languagetool_server(&client).await?;

	for msg in &connection.receiver {
		match msg {
			Message::Request(req) => {
				if connection.handle_shutdown(&req)? {
					return Ok(());
				}
				let req = match cast_request::<CodeActionRequest>(req) {
					Ok((id, mut params)) => {
						let mut action = CodeActionResponse::new();

						let (replacements, diagnostics) = (|| {
							let diagnostic = params.context.diagnostics.pop()?;
							let replacements =
								serde_json::from_value::<Vec<String>>(diagnostic.data.clone()?)
									.ok()?;
							Some((replacements, Some(vec![diagnostic])))
						})()
						.unwrap_or((Vec::new(), None));

						for value in replacements {
							let title = format!("Replace with \"{}\"", value);
							let replace = TextEdit { range: params.range, new_text: value };
							let edit = [(params.text_document.uri.clone(), vec![replace])]
								.into_iter()
								.collect();

							action.push(
								CodeAction {
									title,
									is_preferred: Some(true),
									kind: Some(CodeActionKind::QUICKFIX),
									diagnostics: diagnostics.clone(),
									edit: Some(WorkspaceEdit {
										changes: Some(edit),
										..Default::default()
									}),

									..Default::default()
								}
								.into(),
							);
						}
						send_response::<CodeActionRequest>(&connection, id, Some(action))?;
						continue;
					},
					Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
					Err(ExtractError::MethodMismatch(req)) => req,
				};
				eprintln!("unkown request: {:?}", req);
			},
			Message::Response(resp) => {
				eprintln!("unkown response: {:?}", resp);
			},
			Message::Notification(not) => {
				let not = match cast_notification::<DidSaveTextDocument>(not) {
					Ok(params) => {
						let content = params.text.unwrap();

						let diagnostics = get_diagnostics(&content, &client, &options).await?;

						let params = PublishDiagnosticsParams {
							uri: params.text_document.uri,
							version: None,
							diagnostics,
						};
						send_notification::<PublishDiagnostics>(&connection, params)?;
						continue;
					},
					Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
					Err(ExtractError::MethodMismatch(not)) => not,
				};
				let not = match cast_notification::<DidOpenTextDocument>(not) {
					Ok(params) => {
						let content = params.text_document.text;

						let diagnostics = get_diagnostics(&content, &client, &options).await?;

						let params = PublishDiagnosticsParams {
							uri: params.text_document.uri,
							version: None,
							diagnostics,
						};
						send_notification::<PublishDiagnostics>(&connection, params)?;
						continue;
					},
					Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
					Err(ExtractError::MethodMismatch(not)) => not,
				};
				let not = match cast_notification::<DidChangeConfiguration>(not) {
					Ok(params) => {
						let new_options = serde_json::from_value::<Options>(params.settings)?;
						// todo: handle changes
						assert_eq!(new_options.host, options.host);
						assert_eq!(new_options.port, options.port);
						assert_eq!(
							new_options.local_languagetool_directory,
							options.local_languagetool_directory,
						);
						options = new_options;
						eprintln!("{:#?}", options);
						continue;
					},
					Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
					Err(ExtractError::MethodMismatch(not)) => not,
				};
				eprintln!("unknown notification: {:?}", not);
			},
		}
	}
	Ok(())
}

async fn check_languagetool_server(client: &ServerClient) -> Result<(), Box<dyn Error>> {
	eprintln!("Waiting for the server to respond.");
	for _ in 0..(4 * 60) {
		if client.ping().await.is_ok() {
			return Ok(());
		}
		std::thread::sleep(std::time::Duration::from_millis(250));
	}
	client.ping().await?;
	Ok(())
}

fn cast_request<R>(req: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
	R: lsp_types::request::Request,
	R::Params: serde::de::DeserializeOwned,
{
	req.extract(R::METHOD)
}

fn cast_notification<N>(not: Notification) -> Result<N::Params, ExtractError<Notification>>
where
	N: lsp_types::notification::Notification,
	N::Params: serde::de::DeserializeOwned,
{
	not.extract(N::METHOD)
}

#[allow(dead_code)]
fn send_request<R>(
	connection: &Connection,
	id: i32,
	params: R::Params,
) -> Result<(), Box<dyn Error>>
where
	R: lsp_types::request::Request,
{
	let message = Message::Request(Request::new(id.into(), R::METHOD.into(), params));
	connection.sender.send(message)?;

	Ok(())
}

fn send_response<R>(
	connection: &Connection,
	id: RequestId,
	result: R::Result,
) -> Result<(), Box<dyn Error>>
where
	R: lsp_types::request::Request,
{
	let message = Message::Response(Response::new_ok(id, result));
	connection.sender.send(message)?;
	Ok(())
}

fn send_notification<N>(connection: &Connection, params: N::Params) -> Result<(), Box<dyn Error>>
where
	N: lsp_types::notification::Notification,
{
	let message = Message::Notification(Notification::new(N::METHOD.into(), params));
	connection.sender.send(message)?;
	Ok(())
}

async fn get_diagnostics(
	text: &str,
	client: &ServerClient,
	options: &Options,
) -> Result<Vec<Diagnostic>, Box<dyn Error>> {
	let mut diagnostics = Vec::new();
	let mut position = Position::new(&text);

	typst_languagetool::check(
		client,
		text,
		options.language.as_ref().map(|l| l.as_str()),
		&options.rules,
		options.request_length,
		&options.dictionary,
		|response, total| {
			let mut last = 0;
			for info in response.matches {
				position.advance(info.offset - last);
				let mut end = position.clone();
				end.advance(info.length);

				let replacements = info
					.replacements
					.into_iter()
					.map(|l| l.value)
					.collect::<Vec<_>>();

				let diagnostic = Diagnostic {
					range: Range {
						start: lsp_types::Position {
							line: position.line,
							character: position.column,
						},
						end: lsp_types::Position { line: end.line, character: end.column },
					},
					severity: Some(DiagnosticSeverity::INFORMATION),
					code: Some(NumberOrString::String(info.rule.id)),
					code_description: None,
					source: None,
					message: info.message,
					related_information: None,
					tags: None,
					data: serde_json::to_value(replacements).ok(),
				};
				diagnostics.push(diagnostic);
				last = info.offset;
			}

			position.advance(total - last);
		},
	)
	.await?;
	Ok(diagnostics)
}

#[derive(Clone)]
pub struct Position<'a> {
	line: u32,
	column: u32,
	content: Chars<'a>,
}

impl<'a> Position<'a> {
	pub fn new(content: &'a str) -> Self {
		Self {
			line: 0,
			column: 0,
			content: content.chars(),
		}
	}

	fn advance(&mut self, amount: usize) {
		for _ in 0..amount {
			match self.content.next().unwrap() {
				'\n' => {
					self.line += 1;
					self.column = 0;
				},
				_ => {
					self.column += 1;
				},
			}
		}
	}
}

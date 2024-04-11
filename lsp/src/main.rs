use std::error::Error;

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
use typst_languagetool::{LanguageTool, Position, Rules, JVM};

fn main() -> Result<(), Box<dyn Error>> {
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
	main_loop(connection, initialization_params)?;
	io_threads.join()?;

	eprintln!("shutting down server");
	Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(default)]
struct Options {
	language: String,
	rules: Rules,
	dictionary: Vec<String>,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			language: "en-US".into(),
			rules: Rules::new(),
			dictionary: Vec::new(),
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

fn main_loop(connection: Connection, params: serde_json::Value) -> Result<(), Box<dyn Error>> {
	let mut options = (|| {
		let params = serde_json::from_value::<InitializeParams>(params).ok()?;
		let options = params.initialization_options?;
		let options = serde_json::from_value(options).ok()?;

		Some(options)
	})()
	.unwrap_or(Options::default());

	eprintln!("{:#?}", options);

	let jvm = JVM::new()?;
	let mut lt = LanguageTool::new(&jvm, &options.language)?;

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

						let diagnostics = get_diagnostics(&content, &mut lt, &options)?;

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

						let diagnostics = get_diagnostics(&content, &mut lt, &options)?;

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
						if new_options.language != options.language {
							lt = LanguageTool::new(&jvm, &options.language)?;
						}
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

fn get_diagnostics(
	text: &str,
	lt: &mut LanguageTool,
	options: &Options,
) -> Result<Vec<Diagnostic>, Box<dyn Error>> {
	let mut position = Position::new(&text);

	let diagnostics = typst_languagetool::check(lt, text, &options.rules)?
		.into_iter()
		.map(|suggestion| {
			let start = position.seek(suggestion.start, false);
			let end = position.seek(suggestion.end, false);

			Diagnostic {
				range: Range {
					start: lsp_types::Position {
						line: start.line as u32,
						character: start.column as u32,
					},
					end: lsp_types::Position {
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

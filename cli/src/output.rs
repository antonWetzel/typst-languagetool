use std::{io::stdout, io::Write, ops::Not, path::Path};

use annotate_snippets::{Level, Renderer, Snippet};
use typst_languagetool::{Position, Suggestion};

const MAX_SUGGESTIONS: usize = 20;

pub fn output_plain(file: &Path, position: &mut Position, suggestion: Suggestion) {
	let mut out = stdout().lock();
	let start = position.seek(suggestion.start, false);
	let end = position.seek(suggestion.end, false);
	write!(
		out,
		"{} {}:{}-{}:{} info {}",
		file.display(),
		start.line + 1,
		start.column + 1,
		end.line + 1,
		end.column + 1,
		suggestion.message,
	)
	.unwrap();

	let mut suggestions = suggestion
		.replacements
		.into_iter()
		.filter(|suggestion| suggestion.trim().is_empty().not())
		.take(MAX_SUGGESTIONS);
	if let Some(first) = suggestions.next() {
		write!(out, " ({}", first).unwrap();
		for suggestion in suggestions {
			write!(out, ", {}", suggestion).unwrap();
		}
		writeln!(out, ")").unwrap();
	} else {
		writeln!(out).unwrap();
	}
}

pub fn output_pretty(
	file: &Path,
	position: &mut Position,
	suggestion: Suggestion,
	context_range: usize,
) {
	let file_name = format!("{}", file.display());

	let start = position.seek(suggestion.start, false);
	let pretty_start = position.seek(suggestion.start.saturating_sub(context_range), true);
	let end = position.seek(suggestion.end, false);
	let pretty_end = position.seek(suggestion.end + context_range, true);

	let mut snippet = Snippet::source(&position.substring(pretty_start.utf_8, pretty_end.utf_8))
		.line_start(start.line + 1)
		.origin(&file_name)
		.fold(true);

	let start = start.utf_8 - pretty_start.utf_8;
	let end = end.utf_8 - pretty_start.utf_8;

	snippet = snippet.annotation(Level::Info.span(start..end).label(&suggestion.message));

	for replacement in suggestion
		.replacements
		.iter()
		.filter(|replacement| replacement.trim().is_empty().not())
		.take(MAX_SUGGESTIONS)
	{
		snippet = snippet.annotation(Level::Help.span(end..end).label(&replacement));
	}
	let message = Level::Info
		.title(&suggestion.rule_description)
		.id(&suggestion.rule_id)
		.snippet(snippet);

	let renderer = Renderer::styled();
	println!("{}", renderer.render(message));
}

use std::{io::Write, io::stdout, ops::Not, path::Path};

use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use typst::syntax::Source;
use typst_languagetool::Diagnostic;

const MAX_SUGGESTIONS: usize = 20;

pub fn plain(file: &Path, source: &Source, diagnostic: Diagnostic) {
	let mut out = stdout().lock();

	let (start_line, start_column) = byte_to_position(source, diagnostic.locations[0].1.start);
	let (end_line, end_column) = byte_to_position(source, diagnostic.locations[0].1.end);
	write!(
		out,
		"{} {}:{}-{}:{} info {}",
		file.display(),
		start_line + 1,
		start_column + 1,
		end_line + 1,
		end_column + 1,
		diagnostic.message,
	)
	.unwrap();

	let mut suggestions = diagnostic
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

pub fn pretty(file: &Path, source: &Source, diagnostic: Diagnostic) {
	let file_name = format!("{}", file.display());

	let (start_line, _) = byte_to_position(source, diagnostic.locations[0].1.start);
	let (end_line, _) = byte_to_position(source, diagnostic.locations[0].1.end);
	let text = source.text();
	let context = if start_line == end_line {
		source.line_to_range(start_line).unwrap()
	} else {
		let start = source.line_to_byte(start_line).unwrap();
		let end = source.line_to_byte(end_line + 1).unwrap_or(text.len());
		start..end
	};

	let mut snippet = Snippet::source(&text[context.clone()])
		.line_start(start_line + 1)
		.path(&file_name)
		.fold(true);

	let start = diagnostic.locations[0].1.start - context.start;
	let end = diagnostic.locations[0].1.end - context.start;

	snippet = snippet.annotation(
		AnnotationKind::Primary
			.span(start..end)
			.label(&diagnostic.message),
	);

	for replacement in diagnostic
		.replacements
		.iter()
		.filter(|replacement| replacement.trim().is_empty().not())
		.take(MAX_SUGGESTIONS)
	{
		snippet = snippet.annotation(AnnotationKind::Context.span(start..end).label(replacement));
	}
	let message = Level::INFO
		.primary_title(&diagnostic.rule_description)
		.id(&diagnostic.rule_id)
		.element(snippet);

	let renderer = Renderer::styled();
	println!("{}", renderer.render(&[message]));
}

fn byte_to_position(source: &Source, index: usize) -> (usize, usize) {
	let line = source.byte_to_line(index).unwrap();
	let start = source.line_to_byte(line).unwrap();
	let head = source.get(start..index).unwrap();
	let column = head.chars().count();
	(line, column)
}

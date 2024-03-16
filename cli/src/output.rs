use std::{io::stdout, io::Write, ops::Not, path::Path};

use annotate_snippets::{Level, Renderer, Snippet};

use languagetool_rust::{check::Match, CheckResponse};

const MAX_SUGGESTIONS: usize = 20;

pub fn output_plain(file: &Path, position: &mut Position, response: CheckResponse) {
	let mut out = stdout().lock();
	for info in response.matches {
		let (_, start_line, start_cloumn) = position.seek(info.offset, false);
		let (_, end_line, end_cloumn) = position.seek(info.offset + info.length, false);
		write!(
			out,
			"{} {}:{}-{}:{} info {}",
			file.display(),
			start_line,
			start_cloumn,
			end_line,
			end_cloumn,
			info.message,
		)
		.unwrap();

		let mut suggestions = info
			.replacements
			.into_iter()
			.filter(|suggestion| suggestion.value.trim().is_empty().not())
			.take(MAX_SUGGESTIONS);
		if let Some(first) = suggestions.next() {
			write!(out, " ({}", first.value).unwrap();
			for suggestion in suggestions {
				write!(out, ", {}", suggestion.value).unwrap();
			}
			writeln!(out, ")").unwrap();
		} else {
			writeln!(out).unwrap();
		}
	}
}

pub fn output_pretty(
	file: &Path,
	position: &mut Position,
	response: CheckResponse,
	context_range: usize,
) {
	let file_name = format!("{}", file.display());
	for info in response.matches {
		print_pretty(&file_name, position, info, context_range);
	}
}

fn print_pretty(file_name: &str, position: &mut Position, info: Match, context_range: usize) {
	let start = position.seek(info.offset, false);
	let pretty_start = position.seek(info.offset.saturating_sub(context_range), true);
	let end = position.seek(info.offset + info.length, false);
	let pretty_end = position.seek(info.offset + info.length + context_range, true);

	let mut snippet = Snippet::source(&position.content.text[pretty_start.0..pretty_end.0])
		.line_start(start.1)
		.origin(file_name)
		.fold(true);

	let start = start.0 - pretty_start.0;
	let end = end.0 - pretty_start.0;

	snippet = snippet.annotation(Level::Info.span(start..end).label(&info.message));

	for replacement in info
		.replacements
		.iter()
		.filter(|replacement| replacement.value.trim().is_empty().not())
		.take(MAX_SUGGESTIONS)
	{
		snippet = snippet.annotation(Level::Help.span(end..end).label(&replacement.value));
	}

	if let Some(urls) = &info.rule.urls {
		for url in urls {
			snippet = snippet.annotation(Level::Note.span(end..end).label(&url.value));
		}
	}
	let message = Level::Info
		.title(&info.rule.description)
		.id(&info.rule.id)
		.snippet(snippet);

	let renderer = Renderer::styled();
	println!("{}", renderer.render(message));
}

pub struct Position<'a> {
	line: usize,
	column: usize,
	content: StringCursor<'a>,
}

impl<'a> Position<'a> {
	pub fn new(content: &'a str) -> Self {
		Self {
			line: 1,
			column: 1,
			content: StringCursor::new(content),
		}
	}

	fn seek(&mut self, char_index: usize, stop_at_newline: bool) -> (usize, usize, usize) {
		let start = self.content.utf_8_index;
		let end = self
			.content
			.utf_8_offset(char_index, stop_at_newline)
			.unwrap_or(self.content.text.len());
		if start < end {
			for c in self.content.text[start..end].chars() {
				match c {
					'\n' => {
						self.line += 1;
						self.column = 1;
					},
					_ => {
						self.column += 1;
					},
				}
			}
		} else if end > start {
			for c in self.content.text[end..start].chars() {
				match c {
					'\n' => {
						self.line -= 1;
						self.column = 1;
					},
					_ => {
						self.column -= 1;
					},
				}
			}
		}
		(end, self.line, self.column)
	}
}

#[derive(Debug)]
struct StringCursor<'a> {
	text: &'a str,
	utf_8_index: usize,
	char_index: usize,
}

impl<'a> StringCursor<'a> {
	pub fn new(text: &'a str) -> Self {
		Self { text, utf_8_index: 0, char_index: 0 }
	}

	pub fn utf_8_offset(&mut self, char_index: usize, stop_at_newline: bool) -> Option<usize> {
		if self.char_index < char_index {
			for c in self.text[self.utf_8_index..]
				.chars()
				.take(char_index - self.char_index)
			{
				if stop_at_newline && matches!(c, '\n' | '\r') {
					return Some(self.utf_8_index);
				}
				self.utf_8_index += c.len_utf8();
				self.char_index += 1;
			}
		} else if self.char_index > char_index {
			for c in self.text[..self.utf_8_index]
				.chars()
				.rev()
				.take(self.char_index - char_index)
			{
				if stop_at_newline && matches!(c, '\n' | '\r') {
					return Some(self.utf_8_index);
				}
				self.utf_8_index -= c.len_utf8();
				self.char_index -= 1;
			}
		}
		(self.char_index == char_index).then_some(self.utf_8_index)
	}
}

#[test]
fn test_variable_width() {
	let text = "ÖÖ";
	let mut cursor = StringCursor::new(text);
	assert_eq!(cursor.utf_8_offset(2, false), Some(4));
	assert_eq!(cursor.utf_8_offset(3, false), None);
	assert_eq!(cursor.utf_8_offset(0, false), Some(0));
	assert_eq!(cursor.utf_8_offset(1, false), Some(2));
	assert_eq!(cursor.utf_8_offset(2, false), Some(4));
	assert_eq!(cursor.utf_8_offset(3, false), None);
}

#[test]
fn test_newline_stop() {
	let text = "abc\ndef\nghi";
	let mut cursor = StringCursor::new(text);
	assert_eq!(cursor.utf_8_offset(4, false), Some(4));
	assert_eq!(cursor.utf_8_offset(1, true), Some(4));
	assert_eq!(cursor.utf_8_offset(20, true), Some(7));
	assert_eq!(cursor.utf_8_offset(0, false), Some(0));
	assert_eq!(cursor.utf_8_offset(20, true), Some(3));
}

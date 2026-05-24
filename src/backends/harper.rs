use harper_core::linting::Linter;

use crate::LanguageToolBackend;
use crate::Suggestion;

#[derive(Debug)]
pub struct HarperBackend {}

impl LanguageToolBackend for HarperBackend {
	async fn allow_words(&mut self, _lang: String, _words: &[String]) -> anyhow::Result<()> {
		// future todo: maybe use this to setup the dictionary
		return Ok(());
	}

	async fn disable_checks(&mut self, _lang: String, _checks: &[String]) -> anyhow::Result<()> {
		// future todo: maybe use this to setup the linter
		return Ok(());
	}

	async fn check_text(
		&mut self,
		_lang: String, // future todo: use this to set the dialect
		text: &str,
	) -> anyhow::Result<Vec<crate::Suggestion>> {
		let parser = harper_core::parsers::PlainEnglish;

		let document = harper_core::Document::new_curated(text, &parser);

		let dict = harper_core::spell::FstDictionary::curated();
		let mut linter =
			harper_core::linting::LintGroup::new_curated(dict, harper_core::Dialect::American);

		let lints = linter.lint(&document);

		let mut utf_16_cursor = Utf16Cursor::new(text);

		let suggestions = lints.into_iter().map(|lint| {
			let start = utf_16_cursor.get(lint.span.start);
			let end = utf_16_cursor.get(lint.span.end);
			let replacements = lint
				.suggestions
				.into_iter()
				.map(|suggestion| match suggestion {
					harper_core::linting::Suggestion::ReplaceWith(chars) => {
						chars.into_iter().collect()
					},
					harper_core::linting::Suggestion::InsertAfter(chars) => text
						[lint.span.start..lint.span.end]
						.chars()
						.chain(chars)
						.collect(),
					harper_core::linting::Suggestion::Remove => "".to_owned(),
				})
				.collect();

			Suggestion {
				start,
				end,
				message: lint.message,
				replacements,
				rule_description: lint.lint_kind.to_string(),
				rule_id: lint.priority.to_string(),
			}
		});

		return Ok(suggestions.collect());
	}
}

pub struct Utf16Cursor<'a> {
	text: &'a str,
	current_byte: usize,
	current_utf16: usize,
}

impl<'a> Utf16Cursor<'a> {
	pub fn new(text: &'a str) -> Self {
		Self { text, current_byte: 0, current_utf16: 0 }
	}

	pub fn get(&mut self, byte_index: usize) -> usize {
		if byte_index > self.current_byte {
			// Move forward if the target is ahead.
			let slice = &self.text[self.current_byte..];
			for ch in slice.chars() {
				self.current_byte += ch.len_utf8();
				self.current_utf16 += ch.len_utf16();
				if self.current_byte >= byte_index {
					break;
				}
			}
		} else {
			// Move forward if the target is ahead.
			let slice = &self.text[..self.current_byte];
			for ch in slice.chars().rev() {
				self.current_byte -= ch.len_utf8();
				self.current_utf16 -= ch.len_utf16();
				if self.current_byte <= byte_index {
					break;
				}
			}
		}

		if byte_index != self.current_byte {
			panic!("utf16 index not found");
		}
		self.current_utf16
	}
}

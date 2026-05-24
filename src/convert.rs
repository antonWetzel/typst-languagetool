use std::{collections::HashSet, ops::Range};

use typst::{
	World,
	foundations::{Content, SequenceElem, StyleChain, StyledElem, Value},
	math::EquationElem,
	model::{FigureElem, HeadingElem, ParbreakElem},
	syntax::{FileId, Source, Span, SyntaxKind},
	text::{Lang, SpaceElem, TextElem},
};

use crate::Suggestion;

fn is_call_to_ignored_function(
	node: &typst::syntax::LinkedNode,
	ignore_functions: &HashSet<String>,
) -> bool {
	match node.kind() {
		SyntaxKind::FuncCall => node
			.leftmost_leaf()
			.map(|leaf| ignore_functions.contains(leaf.text().as_str()))
			.unwrap_or(false),
		SyntaxKind::Ref => ignore_functions.contains("cite"),
		_ => false,
	}
}

fn should_ignore(node: &typst::syntax::LinkedNode, ignore_functions: &HashSet<String>) -> bool {
	let mut current = Some(node);
	while let Some(node) = current {
		if is_call_to_ignored_function(node, ignore_functions) {
			return true;
		}
		current = node.parent();
	}
	false
}

#[derive(Debug)]
pub struct Mapping {
	chars: Vec<(Span, Range<u16>)>,
	language: Lang,
}

impl Mapping {
	pub fn location(
		&self,
		suggestion: &Suggestion,
		world: &impl World,
		source: Option<&Source>,
		ignore_functions: &HashSet<String>,
	) -> Vec<(FileId, Range<usize>)> {
		let Some(chars) = &self.chars.get(suggestion.start..suggestion.end) else {
			return Vec::new();
		};
		let mut locations = Vec::<(FileId, Range<usize>)>::new();
		for (span, range) in chars.iter().cloned() {
			let Some(id) = span.id() else {
				continue;
			};
			let source = if let Some(source) = source {
				if source.id() != id {
					continue;
				}
				source.clone()
			} else {
				let Ok(source) = world.source(id) else {
					continue;
				};
				source
			};

			let Some(node) = source.find(span) else {
				continue;
			};

			if should_ignore(&node, ignore_functions) {
				continue;
			}

			match node.kind() {
				SyntaxKind::Text => {
					let start = node.range().start;
					let range = (start + range.start as usize)..(start + range.end as usize);
					match locations.last_mut() {
						Some((last_id, last_range))
							if *last_id == id
								&& (last_range.start..=last_range.end).contains(&range.start) =>
						{
							last_range.end = range.end
						},
						_ => locations.push((id, range)),
					}
				},
				_ => {
					let range = node.range();
					match locations.last_mut() {
						Some((last_id, last_range)) if *last_id == id && *last_range == range => {},
						_ => locations.push((id, range)),
					}
				},
			}
		}
		locations
	}

	pub fn short_language(&self) -> &str {
		self.language.as_str()
	}

	// https://languagetool.org/http-api/swagger-ui/#!/default/get_languages
	// defaults to european region codes (maybe).
	// todo: default to highest population.
	pub fn long_language(&self) -> String {
		match self.language {
			Lang::FRENCH => "fr-FR".into(),
			Lang::SWEDISH => "sv-SE".into(),
			Lang::ITALIAN => "it-IT".into(),
			Lang::SPANISH => "es-ES".into(),
			Lang::DUTCH => "nl-NL".into(),
			Lang::CHINESE => "zh-CN".into(),
			Lang::UKRAINIAN => "uk-UA".into(),
			Lang::SLOVENIAN => "sl-SI".into(),
			Lang::RUSSIAN => "ru-RU".into(),
			Lang::ROMANIAN => "ro-RO".into(),
			Lang::POLISH => "pl-PL".into(),
			Lang::JAPANESE => "ja-JP".into(),
			Lang::GREEK => "el-GR".into(),
			Lang::DANISH => "da-DK".into(),
			Lang::CATALAN => "ca-ES".into(),
			Lang::PORTUGUESE => "pt-PT".into(),
			Lang::ENGLISH => "en-GB".into(),
			Lang::GERMAN => "de-DE".into(),
			lang => lang.as_str().into(),
		}
	}
}

pub fn document(
	content: &Content,
	chunk_size: usize,
	file_id: Option<FileId>,
) -> Vec<(String, Mapping)> {
	let mut converter = Converter {
		text: String::new(),
		mapping: Mapping {
			chars: Vec::new(),
			language: Lang::ENGLISH,
		},
		chunk_size,
		contains_file: false,
		file_id,
		prev: Vec::new(),
	};
	converter.iter_content(content, StyleChain::default());
	converter.break_chunk();
	converter.prev
}

struct Converter {
	text: String,
	mapping: Mapping,
	chunk_size: usize,
	contains_file: bool,
	file_id: Option<FileId>,

	prev: Vec<(String, Mapping)>,
}

const SPACE: &str = "\n\n";
const BREAK: &str = "\n\n";

impl Converter {
	pub fn break_chunk(&mut self) {
		if self.text.is_empty() {
			return;
		}
		let text = std::mem::take(&mut self.text);
		let mapping = Mapping {
			chars: Vec::new(),
			language: self.mapping.language,
		};
		let mapping = std::mem::replace(&mut self.mapping, mapping);

		if self.file_id.is_some() && self.contains_file {
			return;
		}
		self.prev.push((text, mapping));
	}

	pub fn maybe_add_text(&mut self, text: &str, span: Span) {
		if self.text.ends_with(text) {
			return;
		}
		self.add_text(text, span);
	}

	pub fn add_text(&mut self, text: &str, span: Span) {
		self.text += text;
		let mut buf = [0; 2];
		for (idx, c) in text.char_indices() {
			let n = c.encode_utf16(&mut buf).len();
			let range = (idx as u16)..((idx + c.len_utf8()) as u16);
			for _ in &buf[..n] {
				self.mapping.chars.push((span, range.clone()));
			}
		}
		if self.text.len() > self.chunk_size {
			self.break_chunk();
		}
	}

	pub fn iter_content(&mut self, content: &Content, style: StyleChain) {
		if let Some(styled) = content.to_packed::<StyledElem>() {
			let style = style.chain(&styled.styles);
			self.iter_content(&styled.child, style);
		} else if let Some(text) = content.to_packed::<TextElem>() {
			let lang = style.get(TextElem::lang);
			let _region = style.get(TextElem::region); // todo: add for checking instead of calculation region
			self.add_text(&text.text, text.span());
			if self.mapping.language != lang {
				self.break_chunk();
			}
			self.mapping.language = lang;
		} else if let Some(heading) = content.to_packed::<HeadingElem>() {
			// todo: chunking based on this level
			let _level = heading.resolve_level(style);
			self.iter_content(&heading.body, style);
		} else if let Some(sequence) = content.to_packed::<SequenceElem>() {
			for child in sequence.children.iter() {
				self.iter_content(child, style);
			}
		} else if let Some(space) = content.to_packed::<SpaceElem>() {
			self.maybe_add_text(SPACE, space.span());
		} else if let Some(parbreak) = content.to_packed::<ParbreakElem>() {
			self.maybe_add_text(BREAK, parbreak.span());
		} else if let Some(figure) = content.to_packed::<FigureElem>() {
			if let Some(caption) = figure.caption.get_ref(style) {
				self.iter_content(&caption.body, style);
			}
			self.iter_content(&figure.body, style);
		} else if let Some(_equation) = content.to_packed::<EquationElem>() {
			// ?
		} else {
			for (_key, field) in content.fields() {
				self.iter_value(&field, style);
			}
			self.maybe_add_text(SPACE, content.span());
		}
	}

	pub fn iter_value(&mut self, value: &Value, style: StyleChain) {
		match value {
			Value::Content(content) => {
				self.iter_content(content, style);
			},
			Value::Array(array) => {
				for value in array.iter() {
					self.iter_value(value, style);
				}
			},
			// symbol?
			_ => {},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Suggestion;
	use std::path::Path;

	struct TestHarness<'a> {
		world: lt_world::LtWorldRunning<'a>,
		text: String,
		mapping: Mapping,
	}

	impl<'a> TestHarness<'a> {
		fn new(world: &'a lt_world::LtWorld, main_file: &Path) -> Self {
			let world = world.with_main(main_file.to_path_buf());
			let doc = world.compile().unwrap();
			let paragraphs = document(&doc, 1000, None);
			assert_eq!(paragraphs.len(), 1, "expected exactly one paragraph");
			let (text, mapping) = paragraphs.into_iter().next().unwrap();
			Self { world, text, mapping }
		}

		fn suggestion_for(&self, needle: &str) -> Suggestion {
			let start = self
				.text
				.find(needle)
				.unwrap_or_else(|| panic!("expected '{}' in text: {:?}", needle, self.text));
			Suggestion {
				start,
				end: start + needle.len(),
				message: "test".into(),
				replacements: vec![],
				rule_description: "test".into(),
				rule_id: "test".into(),
			}
		}

		fn locations_with_ignore(
			&self,
			suggestion: &Suggestion,
			ignore_functions: &[&str],
		) -> Vec<(typst::syntax::FileId, std::ops::Range<usize>)> {
			let ignore_set: HashSet<String> =
				ignore_functions.iter().map(|s| s.to_string()).collect();
			self.mapping
				.location(suggestion, &self.world, None, &ignore_set)
		}

		fn is_ignored(&self, needle: &str, ignore_functions: &[&str]) -> bool {
			let suggestion = self.suggestion_for(needle);
			self.locations_with_ignore(&suggestion, ignore_functions)
				.is_empty()
		}
	}

	#[test]
	fn test_ignore_functions_filters_ancestors() {
		let world = lt_world::LtWorld::new("example".into());
		let harness = TestHarness::new(&world, Path::new("example/ignore.typ"));

		assert!(
			harness.is_ignored("𝜆", &["ignorespelling"]),
			"lambda should be ignored when ignorespelling is in ignore_functions"
		);
		assert!(
			!harness.is_ignored("𝜆", &[]),
			"lambda should not be ignored when ignorespelling is not in ignore_functions"
		);
	}

	#[test]
	fn test_ignore_functions_content_block_syntax() {
		let world = lt_world::LtWorld::new("example".into());
		let harness = TestHarness::new(&world, Path::new("example/content_block.typ"));

		assert!(
			harness.is_ignored("mistaek", &["prog"]),
			"content in #prog[] should be ignored when prog is in ignore_functions"
		);
		assert!(
			!harness.is_ignored("mistaek", &[]),
			"content in #prog[] should not be ignored when prog is not in ignore_functions"
		);

		assert!(
			harness.is_ignored("anohter", &["prog"]),
			"content in #prog([]) should be ignored when prog is in ignore_functions"
		);
		assert!(
			!harness.is_ignored("anohter", &[]),
			"content in #prog([]) should not be ignored when prog is not in ignore_functions"
		);
	}
}

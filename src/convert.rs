use std::{collections::HashSet, ops::Range};

use typst::{
	World,
	layout::{Abs, Em, PagedDocument, Point},
	syntax::{FileId, Source, Span, SyntaxKind},
	text::{Lang, TextItem},
};

use crate::Suggestion;

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
		let chars = &self.chars[suggestion.start..suggestion.end];
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

			match node.kind() {
				SyntaxKind::Text => {
					let start = node.range().start;
					let range = (start + range.start as usize)..(start + range.end as usize);
					match locations.last_mut() {
						Some((last_id, last_range))
							if *last_id == id && last_range.end == range.start =>
						{
							last_range.end = range.end
						},
						_ => locations.push((id, range)),
					}
				},
				SyntaxKind::FuncCall
					if ignore_functions.contains(node.leftmost_leaf().unwrap().text().as_str()) => {},
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

const LINE_SPACING: Em = Em::new(0.65);

pub fn document(
	doc: &PagedDocument,
	chunk_size: usize,
	file_id: Option<FileId>,
) -> Vec<(String, Mapping)> {
	let mut res = Vec::new();

	for page in &doc.pages {
		let mut converter = Converter::new(chunk_size, Lang::ENGLISH);
		converter.frame(&page.frame, Point::zero(), &mut res, file_id);
		if converter.contains_file {
			res.push((converter.text, converter.mapping));
		}
	}
	res
}

struct Converter {
	text: String,
	mapping: Mapping,
	x: Abs,
	y: Abs,
	span: (Span, u16),
	chunk_size: usize,
	contains_file: bool,
}

impl Converter {
	fn new(chunk_size: usize, language: Lang) -> Self {
		Self {
			text: String::new(),
			mapping: Mapping { chars: Vec::new(), language },
			x: Abs::zero(),
			y: Abs::zero(),
			span: (Span::detached(), 0),
			contains_file: false,
			chunk_size,
		}
	}

	fn insert_space(&mut self) {
		self.text += " ";
		self.mapping.chars.push((Span::detached(), 0..0));
	}

	fn seperate(&mut self, res: &mut Vec<(String, Mapping)>) {
		let language = self.mapping.language;
		if self.contains_file {
			let text = std::mem::take(&mut self.text);
			let mapping = std::mem::replace(
				&mut self.mapping,
				Mapping {
					chars: Vec::new(),
					language: Lang::ENGLISH,
				},
			);
			res.push((text, mapping));
		}
		*self = Converter::new(self.chunk_size, language);
	}

	fn insert_parbreak(&mut self, res: &mut Vec<(String, Mapping)>) {
		if self.mapping.chars.len() > self.chunk_size {
			self.seperate(res);
			return;
		}
		self.text += "\n\n";
		self.mapping.chars.push((Span::detached(), 0..0));
		self.mapping.chars.push((Span::detached(), 0..0));
	}

	fn whitespace(&mut self, text: &TextItem, pos: Point, res: &mut Vec<(String, Mapping)>) {
		if self.x.approx_eq(pos.x) {
			return;
		}
		let line_spacing = (text.font.metrics().cap_height + LINE_SPACING).at(text.size);
		let next_line = (self.y + line_spacing).approx_eq(pos.y);
		if !next_line {
			self.insert_parbreak(res);
			return;
		}
		let span = text.glyphs[0].span;
		if span == self.span {
			return;
		}
		self.insert_space();
	}

	fn frame(
		&mut self,
		frame: &typst::layout::Frame,
		pos: Point,
		res: &mut Vec<(String, Mapping)>,
		file_id: Option<FileId>,
	) {
		for &(p, ref item) in frame.items() {
			self.item(p + pos, item, res, file_id);
		}
	}

	fn item(
		&mut self,
		pos: Point,
		item: &typst::layout::FrameItem,
		res: &mut Vec<(String, Mapping)>,
		file_id: Option<FileId>,
	) {
		use typst::layout::FrameItem as I;
		match item {
			I::Group(g) => self.frame(&g.frame, pos, res, file_id),
			I::Text(t) => {
				if self.mapping.language != t.lang {
					self.seperate(res);
				}
				self.mapping.language = t.lang;

				self.whitespace(t, pos, res);
				self.x = pos.x + t.width();
				self.y = pos.y;
				self.text += t.text.as_str();

				let mut iter = t.text.encode_utf16();
				for g in t.glyphs.iter().cloned() {
					let Some(text) = t.text.get(g.range()) else {
						continue;
					};
					for t in text.encode_utf16() {
						assert_eq!(t, iter.next().unwrap());

						let m = (g.span.0, g.span.1..(g.span.1 + g.range.len() as u16));
						if let Some(id) = m.0.id() {
							self.span = (m.0, m.1.end);
							self.contains_file |=
								file_id.map(|file_id| file_id == id).unwrap_or(true);
						}
						self.mapping.chars.push(m);
					}
				}
				assert_eq!(None, iter.next());
			},
			I::Link(..) | I::Tag(..) | I::Shape(..) | I::Image(..) => {},
		}
	}
}

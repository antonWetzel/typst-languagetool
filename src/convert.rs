use std::ops::{Not, Range};

use typst::{
	layout::{Abs, Point},
	model::Document,
	syntax::{Source, Span, SyntaxKind},
};

use crate::Suggestion;

pub struct Mapping {
	chars: Vec<(Span, Range<u16>)>,
}

impl Mapping {
	pub fn location(&self, suggestion: &Suggestion, source: &Source) -> Vec<Range<usize>> {
		let chars = &self.chars[suggestion.start..suggestion.end];
		let mut locations = Vec::<Range<usize>>::new();
		for (span, range) in chars.iter().cloned() {
			let Some(id) = span.id() else {
				continue;
			};
			if id != source.id() {
				continue;
			}
			let Some(node) = source.find(span) else {
				continue;
			};
			if node.kind() == SyntaxKind::Text {
				let start = node.range().start;
				let range = (start + range.start as usize)..(start + range.end as usize);
				match locations.last_mut() {
					Some(last_range) if last_range.end == range.start => last_range.end = range.end,
					_ => locations.push(range),
				}
			} else {
				let range = node.range();
				match locations.last_mut() {
					Some(last_range) if *last_range == range => {},
					_ => locations.push(range),
				}
			}
		}
		locations
	}
}

pub fn document(doc: &Document, chunk_size: usize) -> Vec<(String, Mapping)> {
	let mut converter = Converter::new(chunk_size);
	let mut res = Vec::new();

	for page in &doc.pages {
		converter.frame(&page.frame, Point::zero(), &mut res);
		converter.pagebreak = true;
	}
	res.push((converter.text, converter.mapping));
	res
}

struct Converter {
	text: String,
	mapping: Mapping,
	x: Abs,
	y: Abs,
	span: (Span, u16),
	pagebreak: bool,
	chunk_size: usize,
}

impl Converter {
	fn new(chunk_size: usize) -> Self {
		Self {
			text: String::new(),
			mapping: Mapping { chars: Vec::new() },
			x: Abs::zero(),
			y: Abs::zero(),
			span: (Span::detached(), 0),
			pagebreak: false,
			chunk_size,
		}
	}

	fn insert_space(&mut self) {
		self.text += " ";
		self.mapping.chars.push((Span::detached(), 0..0));
	}

	fn insert_parbreak(&mut self, res: &mut Vec<(String, Mapping)>) {
		if self.pagebreak || self.mapping.chars.len() > self.chunk_size {
			let text = std::mem::take(&mut self.text);
			let mapping = std::mem::replace(&mut self.mapping, Mapping { chars: Vec::new() });
			res.push((text, mapping));
			*self = Converter::new(self.chunk_size);
			return;
		}
		self.text += "\n\n";
		self.mapping.chars.push((Span::detached(), 0..0));
		self.mapping.chars.push((Span::detached(), 0..0));
	}

	fn frame(
		&mut self,
		frame: &typst::layout::Frame,
		pos: Point,
		res: &mut Vec<(String, Mapping)>,
	) {
		for &(p, ref item) in frame.items() {
			self.item(p + pos, item, res);
		}
	}

	fn item(
		&mut self,
		pos: Point,
		item: &typst::layout::FrameItem,
		res: &mut Vec<(String, Mapping)>,
	) {
		use typst::introspection::Meta as M;
		use typst::layout::FrameItem as I;
		match item {
			I::Group(g) => self.frame(&g.frame, pos, res),
			I::Text(t) => {
				let (same_span, missing_space) = {
					let start = t.glyphs[0].span;
					(start.0 == self.span.0, start.1 != self.span.1)
				};

				let same_x = self.x.approx_eq(pos.x);
				let same_y = self.y.approx_eq(pos.y);

				match (same_span, same_x, same_y) {
					(_, true, _) => {},
					(true, _, _) if missing_space.not() => {},
					(true, _, _) => self.insert_space(),
					(false, false, true) => self.insert_space(),
					(false, false, false) => self.insert_parbreak(res),
				}
				self.x = pos.x + t.width();
				self.y = pos.y;

				self.text += t.text.as_str();
				let mut iter = t.glyphs.iter();
				for _ in t.text.encode_utf16() {
					let g = iter.next();
					let m = g
						.map(|g| (g.span.0, g.span.1..(g.span.1 + g.range.len() as u16)))
						.unwrap_or((Span::detached(), 0..0));
					if m.0.is_detached().not() {
						self.span = (m.0, m.1.end);
					}
					self.mapping.chars.push(m);
				}
			},
			I::Meta(M::Link(..) | M::Elem(..) | M::Hide, _) | I::Shape(..) | I::Image(..) => {},
		}
	}
}

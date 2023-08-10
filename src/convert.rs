use languagetool_rust::check::DataAnnotation;
use typst_syntax::{SyntaxKind, SyntaxNode};

pub fn convert(node: &SyntaxNode) -> Vec<(Vec<DataAnnotation>, usize)> {
	let state = State { mode: Mode::Markdown };
	let mut data = vec![(Vec::new(), 0)];
	for child in node.children() {
		let last = data.last_mut().unwrap();
		state.convert(child, last);
		if last.1 >= 3000 && child.kind() == SyntaxKind::Parbreak {
			data.push((Vec::new(), 0));
		}
	}
	data
}

#[derive(PartialEq, Clone, Copy)]
enum Mode {
	Markdown,
	Code,
	Math,
}

#[derive(Clone, Copy)]
struct State {
	mode: Mode,
}

impl State {
	fn convert(mut self, node: &SyntaxNode, items: &mut (Vec<DataAnnotation>, usize)) {
		let item = match node.kind() {
			_ if self.mode == Mode::Math => DataAnnotation::new_markup(node.text().into()),
			SyntaxKind::Text if self.mode == Mode::Markdown => {
				DataAnnotation::new_text(node.text().into())
			},
			SyntaxKind::Equation => {
				self.mode = Mode::Math;
				DataAnnotation::new_interpreted_markup(node.text().into(), String::from("0"))
			},
			SyntaxKind::LeftBrace => {
				self.mode = Mode::Code;
				DataAnnotation::new_markup(node.text().into())
			},
			SyntaxKind::FuncCall => {
				self.mode = Mode::Code;
				DataAnnotation::new_markup(node.text().into())
			},
			SyntaxKind::HeadingMarker => {
				DataAnnotation::new_interpreted_markup(node.text().into(), String::from("\n\n"))
			},
			SyntaxKind::Ref => {
				DataAnnotation::new_interpreted_markup(node.text().into(), String::from("X"))
			},
			SyntaxKind::LeftBracket => {
				DataAnnotation::new_interpreted_markup(node.text().into(), String::from("\n\n"))
			},
			SyntaxKind::Markup => {
				self.mode = Mode::Markdown;
				DataAnnotation::new_text(node.text().into())
			},
			SyntaxKind::Emph => DataAnnotation::new_text(node.text().into()),
			SyntaxKind::Space if self.mode == Mode::Markdown => {
				DataAnnotation::new_text(node.text().into())
			},
			SyntaxKind::Parbreak => {
				DataAnnotation::new_interpreted_markup(node.text().into(), String::from("\n\n"))
			},
			_ => {
				if self.mode == Mode::Markdown {
					self.mode = Mode::Code;
				}
				DataAnnotation::new_markup(node.text().into())
			},
		};
		items.1 += node.text().chars().count();
		items.0.push(item);
		for child in node.children() {
			self.convert(child, items);
		}
		let item = match node.kind() {
			SyntaxKind::Heading => {
				DataAnnotation::new_interpreted_markup(String::new(), String::from("\n\n"))
			},
			_ => return,
		};
		items.0.push(item);
	}
}

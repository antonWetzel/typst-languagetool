use std::ops::Not;

use languagetool_rust::check::DataAnnotation;
use typst_syntax::{SyntaxKind, SyntaxNode};

use crate::rules::Rules;

pub fn convert(
	node: &SyntaxNode,
	rules: &Rules,
	max_length: usize,
) -> Vec<(Vec<DataAnnotation>, usize)> {
	let state = State { mode: Mode::Text, after_argument: "" };
	let mut output = Output::new();
	for child in node.children() {
		state.convert(child, &mut output, rules);
		if child.kind() == SyntaxKind::Parbreak {
			output.maybe_seperate(max_length);
		}
	}
	#[cfg(feature = "print-converted")]
	println!("=====\n{:}\n=====", output.converted_text);
	output.result()
}

enum OutputState {
	Text(String),
	Markup(String),
	Encoded(String, String),
}

struct Output {
	items: Vec<(Vec<DataAnnotation>, usize)>,
	state: OutputState,

	#[cfg(feature = "print-converted")]
	converted_text: String,
}

impl Output {
	pub fn new() -> Self {
		Self {
			items: vec![(Vec::new(), 0)],
			state: OutputState::Text(String::new()),
			#[cfg(feature = "print-converted")]
			converted_text: String::new(),
		}
	}

	fn add_item(&mut self, item: DataAnnotation) {
		if let Some(text) = &item.text {
			self.items.last_mut().unwrap().1 += text.chars().count();
		}
		if let Some(text) = &item.markup {
			self.items.last_mut().unwrap().1 += text.chars().count();
		}
		self.items.last_mut().unwrap().0.push(item);
	}

	// is possible without cloning, but not naive in safe rust
	pub fn add_text(&mut self, text: &str) {
		#[cfg(feature = "print-converted")]
		self.converted_text.push_str(text);
		self.state = match &self.state {
			OutputState::Text(t) => OutputState::Text(t.clone() + &text),
			OutputState::Markup(t) => {
				self.add_item(DataAnnotation::new_markup(t.clone()));
				OutputState::Text(text.into())
			},
			OutputState::Encoded(t, a) => {
				self.add_item(DataAnnotation::new_interpreted_markup(t.clone(), a.clone()));
				OutputState::Text(text.into())
			},
		}
	}

	pub fn add_markup(&mut self, text: &str) {
		self.state = match &self.state {
			OutputState::Text(t) => {
				self.add_item(DataAnnotation::new_text(t.clone()));
				OutputState::Markup(text.into())
			},
			OutputState::Markup(t) => OutputState::Markup(t.clone() + text),
			OutputState::Encoded(t, a) => {
				self.add_item(DataAnnotation::new_interpreted_markup(t.clone(), a.clone()));
				OutputState::Markup(text.into())
			},
		}
	}
	pub fn add_encoded(&mut self, text: &str, res: &str) {
		#[cfg(feature = "print-converted")]
		self.converted_text.push_str(res);
		self.state = match &self.state {
			OutputState::Text(t) => {
				self.add_item(DataAnnotation::new_text(t.clone()));
				OutputState::Encoded(text.into(), res.into())
			},
			OutputState::Markup(t) => {
				self.add_item(DataAnnotation::new_markup(t.clone()));
				OutputState::Encoded(text.into(), res.into())
			},
			OutputState::Encoded(t, a) => OutputState::Encoded(t.clone() + &text, a.clone() + res),
		}
	}

	fn flush(&mut self) {
		match &self.state {
			OutputState::Text(t) => self.add_item(DataAnnotation::new_text(t.clone())),
			OutputState::Markup(t) => self.add_item(DataAnnotation::new_markup(t.clone())),
			OutputState::Encoded(t, a) => {
				self.add_item(DataAnnotation::new_interpreted_markup(t.clone(), a.clone()));
			},
		}
	}

	pub fn maybe_seperate(&mut self, max: usize) {
		if self.items.last().unwrap().1 > max {
			self.flush();
			self.state = OutputState::Text(String::new());
			self.items.push((Vec::new(), 0));
		}
	}

	pub fn result(mut self) -> Vec<(Vec<DataAnnotation>, usize)> {
		self.flush();
		self.items
	}
}

#[derive(PartialEq, Clone, Copy)]
enum Mode {
	Text,
	Markup,
}

#[derive(Clone, Copy)]
struct State<'a> {
	mode: Mode,
	after_argument: &'a str,
}

impl<'a> State<'a> {
	fn convert(mut self, node: &SyntaxNode, output: &mut Output, rules: &'a Rules) {
		match node.kind() {
			SyntaxKind::Text if self.mode == Mode::Text => output.add_text(node.text()),
			SyntaxKind::Equation => {
				output.add_encoded(node.text(), "0");
				Self::skip(node, output);
			},
			SyntaxKind::FuncCall => {
				self.mode = Mode::Markup;
				let name = get_function_name(node).unwrap_or("");
				let rule = rules.functions.get(name);
				if let Some(f) = rule {
					output.add_encoded("", &f.before);
					self.after_argument = &f.after_argument;
				} else {
					self.after_argument = "";
				}
				for child in node.children() {
					self.convert(child, output, rules);
				}
				if let Some(f) = rule {
					output.add_encoded("", &f.after);
				}
			},
			SyntaxKind::ContentBlock => {
				for child in node.children() {
					self.convert(child, output, rules);
				}
				if self.after_argument.is_empty().not() {
					output.add_encoded("", self.after_argument);
				}
			},

			SyntaxKind::Code
			| SyntaxKind::ModuleImport
			| SyntaxKind::ModuleInclude
			| SyntaxKind::LetBinding
			| SyntaxKind::ShowRule
			| SyntaxKind::SetRule => {
				self.mode = Mode::Markup;
				for child in node.children() {
					self.convert(child, output, rules);
				}
			},
			SyntaxKind::Heading => {
				output.add_encoded("", "\n\n");
				self.mode = Mode::Markup;
				for child in node.children() {
					self.convert(child, output, rules);
				}
				output.add_encoded("", "\n\n");
			},
			SyntaxKind::Ref => {
				output.add_encoded("", "X");
				Self::skip(node, output);
			},
			SyntaxKind::Markup => {
				self.mode = Mode::Text;
				for child in node.children() {
					self.convert(child, output, rules);
				}
			},
			SyntaxKind::Shorthand if node.text() == "~" => {
				output.add_encoded(node.text(), " ");
			},
			SyntaxKind::Space if self.mode == Mode::Text => {
				// if there is whitespace after the linebreak ("...\n\t  "), only use ("...\n") as text
				let linebreak = node.text().rfind(typst_syntax::is_newline).map(|x| x + 1);
				match linebreak {
					Some(linebreak) if linebreak < node.text().len() => {
						output.add_encoded(node.text(), &node.text()[0..linebreak])
					},
					_ => output.add_text(node.text()),
				}
			},
			SyntaxKind::ListItem => {
				self.mode = Mode::Markup;
				for child in node.children() {
					self.convert(child, output, rules);
				}
			},
			SyntaxKind::ListMarker => output.add_encoded(node.text(), "- "),
			SyntaxKind::Parbreak => output.add_encoded(node.text(), "\n\n"),
			SyntaxKind::SmartQuote if self.mode == Mode::Text => output.add_text(node.text()),

			SyntaxKind::Named => {
				let name = node.children().next().unwrap().text();
				let rule = rules.arguments.get(name.as_str());
				if let Some(f) = rule {
					output.add_encoded("", &f.before);
				}
				for child in node.children() {
					self.convert(child, output, rules);
				}
				if let Some(f) = rule {
					output.add_encoded("", &f.after);
				}
			},
			_ => {
				output.add_markup(node.text());
				for child in node.children() {
					self.convert(child, output, rules);
				}
			},
		}
	}

	fn skip(node: &SyntaxNode, output: &mut Output) {
		output.add_markup(node.text());
		for child in node.children() {
			Self::skip(child, output);
		}
	}
}

fn get_function_name(node: &SyntaxNode) -> Option<&str> {
	match node.kind() {
		SyntaxKind::FuncCall => get_function_name(node.children().next()?),
		SyntaxKind::Ident => Some(node.text().as_str()),
		SyntaxKind::FieldAccess => get_function_name(node.children().last()?),
		_ => None,
	}
}

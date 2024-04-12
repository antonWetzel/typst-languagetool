use std::{error::Error, ops::Not};

use typst_syntax::{SyntaxKind, SyntaxNode};

use crate::{rules::Rules, LanguageTool, TextBuilder};

pub fn convert<'a, 'b>(
	node: &SyntaxNode,
	rules: &Rules,
	lt: &'b mut LanguageTool<'a>,
) -> Result<TextBuilder<'a, 'b>, Box<dyn Error>> {
	let state = State { mode: Mode::Text, after_argument: "" };
	let mut text = lt.text_builder()?;
	for child in node.children() {
		state.convert(child, &mut text, rules)?;
	}
	Ok(text)
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
	fn convert(
		mut self,
		node: &SyntaxNode,
		output: &mut TextBuilder,
		rules: &'a Rules,
	) -> Result<(), Box<dyn Error>> {
		match node.kind() {
			SyntaxKind::Text if self.mode == Mode::Text => output.add_text(node.text())?,
			SyntaxKind::Equation => {
				output.add_encoded(node.text(), "0")?;
				Self::skip(node, output)?;
			},
			SyntaxKind::FuncCall => {
				self.mode = Mode::Markup;
				let name = get_function_name(node).unwrap_or("");
				let rule = rules.functions.get(name);
				if let Some(f) = rule {
					output.add_encoded("", &f.before)?;
					self.after_argument = &f.after_argument;
				} else {
					self.after_argument = "";
				}
				for child in node.children() {
					self.convert(child, output, rules)?;
				}
				if let Some(f) = rule {
					output.add_encoded("", &f.after)?;
				}
			},
			SyntaxKind::ContentBlock => {
				for child in node.children() {
					self.convert(child, output, rules)?;
				}
				if self.after_argument.is_empty().not() {
					output.add_encoded("", self.after_argument)?;
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
					self.convert(child, output, rules)?;
				}
			},
			SyntaxKind::Heading => {
				output.add_encoded("", "\n\n")?;
				self.mode = Mode::Markup;
				for child in node.children() {
					self.convert(child, output, rules)?;
				}
				output.add_encoded("", "\n\n")?;
			},
			SyntaxKind::Ref => {
				output.add_encoded("", "X")?;
				Self::skip(node, output)?;
			},
			SyntaxKind::Markup => {
				self.mode = Mode::Text;
				for child in node.children() {
					self.convert(child, output, rules)?;
				}
			},
			SyntaxKind::Shorthand => match node.text().as_str() {
				"~" => output.add_encoded(node.text(), " ")?,
				"--" => output.add_encoded(node.text(), "-")?,
				"---" => output.add_encoded(node.text(), "-")?,
				"-?" => output.add_encoded(node.text(), "-")?,
				_ => output.add_text(node.text())?,
			},
			SyntaxKind::Space if self.mode == Mode::Text => {
				// if there is whitespace after the linebreak ("...\n\t  "), only use ("...\n") as text
				let linebreak = node.text().rfind(typst_syntax::is_newline).map(|x| x + 1);
				match linebreak {
					Some(linebreak) if linebreak < node.text().len() => {
						output.add_encoded(node.text(), &node.text()[0..linebreak])?
					},
					_ => output.add_text(node.text())?,
				}
			},
			SyntaxKind::ListItem => {
				self.mode = Mode::Markup;
				for child in node.children() {
					self.convert(child, output, rules)?;
				}
			},
			SyntaxKind::ListMarker => output.add_encoded(node.text(), "- ")?,
			SyntaxKind::Parbreak => output.add_encoded(node.text(), "\n\n")?,
			SyntaxKind::SmartQuote if self.mode == Mode::Text => output.add_text(node.text())?,

			SyntaxKind::Named => {
				let name = node.children().next().unwrap().text();
				let rule = rules.arguments.get(name.as_str());
				if let Some(f) = rule {
					output.add_encoded("", &f.before)?;
				}
				for child in node.children() {
					self.convert(child, output, rules)?;
				}
				if let Some(f) = rule {
					output.add_encoded("", &f.after)?;
				}
			},
			_ => {
				output.add_markup(node.text())?;
				for child in node.children() {
					self.convert(child, output, rules)?;
				}
			},
		}
		Ok(())
	}

	fn skip(node: &SyntaxNode, output: &mut TextBuilder) -> Result<(), Box<dyn Error>> {
		output.add_markup(node.text())?;
		for child in node.children() {
			Self::skip(child, output)?;
		}
		Ok(())
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

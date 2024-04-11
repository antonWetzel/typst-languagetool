use std::error::Error;

use j4rs::{Instance, InvocationArg, Jvm, JvmBuilder};

pub struct LanguageTool {
	jvm: Jvm,
	lang_tool: Instance,
}

impl LanguageTool {
	pub fn new(lang: &str) -> Result<Self, Box<dyn Error>> {
		// bad fix for the jassets folder
		let home = concat!(env!("CARGO_MANIFEST_DIR"), "/target/release");
		let jvm = JvmBuilder::new().with_base_path(home).build().unwrap();

		let lang_code = InvocationArg::try_from(lang).unwrap();
		let lang = jvm
			.invoke_static(
				"org.languagetool.Languages",
				"getLanguageForShortCode",
				&[lang_code],
			)
			.unwrap();

		let lang_tool = jvm
			.create_instance(
				"org.languagetool.JLanguageTool",
				&[InvocationArg::try_from(lang).unwrap()],
			)
			.unwrap();

		Ok(Self { jvm, lang_tool })
	}

	pub fn text_builder(&self) -> Result<TextBuilder, Box<dyn Error>> {
		let annotated_text_builder = self.jvm.create_instance(
			"org.languagetool.markup.AnnotatedTextBuilder",
			InvocationArg::empty(),
		)?;
		Ok(TextBuilder {
			text_builder: annotated_text_builder,
			jvm: &self.jvm,
		})
	}

	pub fn check(&self, text: TextBuilder) -> Result<Vec<Suggestion>, Box<dyn Error>> {
		let annotated_text =
			self.jvm
				.invoke(&text.text_builder, "build", InvocationArg::empty())?;

		let matches = self.jvm.invoke(
			&self.lang_tool,
			"check",
			&[InvocationArg::try_from(annotated_text)?],
		)?;

		let res = for_each(
			&self.jvm,
			&matches,
			"org.languagetool.rules.RuleMatch",
			|m| {
				let start = self
					.jvm
					.invoke(&m, "getFromPos", InvocationArg::empty())
					.unwrap();
				let start = self.jvm.to_rust::<i32>(start).unwrap();

				let end = self
					.jvm
					.invoke(&m, "getToPos", InvocationArg::empty())
					.unwrap();
				let end = self.jvm.to_rust::<i32>(end).unwrap();

				let message = self
					.jvm
					.invoke(&m, "getMessage", InvocationArg::empty())
					.unwrap();
				let message = self.jvm.to_rust::<String>(message).unwrap();

				let replacements = self
					.jvm
					.invoke(&m, "getSuggestedReplacements", InvocationArg::empty())
					.unwrap();

				let replacements = for_each(
					&self.jvm,
					&replacements,
					"java.lang.String",
					|replacement| {
						let s = self.jvm.to_rust::<String>(replacement)?;
						Ok(s)
					},
				)?;

				let rule = self.jvm.invoke(&m, "getRule", InvocationArg::empty())?;
				let rule_id = self.jvm.invoke(&rule, "getId", InvocationArg::empty())?;
				let rule_id = self.jvm.to_rust::<String>(rule_id)?;
				let rule_description =
					self.jvm
						.invoke(&rule, "getDescription", InvocationArg::empty())?;
				let rule_description = self.jvm.to_rust::<String>(rule_description)?;

				Ok(Suggestion {
					start: start as usize,
					end: end as usize,
					replacements,
					message,
					rule_id,
					rule_description,
				})
			},
		)?;

		Ok(res)
	}
}

fn for_each<V>(
	jvm: &Jvm,
	instance: &Instance,
	class: &str,
	mut action: impl FnMut(Instance) -> Result<V, Box<dyn Error>>,
) -> Result<Vec<V>, Box<dyn Error>> {
	let size = jvm
		.invoke(instance, "size", InvocationArg::empty())
		.unwrap();
	let size = jvm.to_rust::<i32>(size).unwrap();
	let mut res = Vec::with_capacity(size as usize);
	for i in 0..size {
		let m = jvm
			.invoke(
				instance,
				"get",
				&[InvocationArg::try_from(i)
					.unwrap()
					.into_primitive()
					.unwrap()],
			)
			.unwrap();
		let m = jvm.cast(&m, class).unwrap();
		let m = action(m)?;
		res.push(m)
	}
	Ok(res)
}

pub struct TextBuilder<'a> {
	text_builder: Instance,
	jvm: &'a Jvm,
}

impl<'a> TextBuilder<'a> {
	pub fn add_text(&self, text: &str) -> Result<(), Box<dyn Error>> {
		self.jvm.invoke(
			&self.text_builder,
			"addText",
			&[InvocationArg::try_from(text)?],
		)?;
		Ok(())
	}

	pub fn add_markup(&self, markup: &str) -> Result<(), Box<dyn Error>> {
		self.jvm.invoke(
			&self.text_builder,
			"addMarkup",
			&[InvocationArg::try_from(markup)?],
		)?;
		Ok(())
	}

	pub fn add_encoded(&self, markup: &str, text: &str) -> Result<(), Box<dyn Error>> {
		self.jvm.invoke(
			&self.text_builder,
			"addMarkup",
			&[
				InvocationArg::try_from(markup)?,
				InvocationArg::try_from(text)?,
			],
		)?;
		Ok(())
	}
}

#[derive(Debug, Clone)]
pub struct Suggestion {
	pub start: usize,
	pub end: usize,
	pub message: String,
	pub replacements: Vec<String>,
	pub rule_description: String,
	pub rule_id: String,
}

pub struct TextPosition {
	pub utf_8: usize,
	pub line: usize,
	pub column: usize,
}

pub struct Position<'a> {
	line: usize,
	column: usize,
	content: StringCursor<'a>,
}

impl<'a> Position<'a> {
	pub fn new(content: &'a str) -> Self {
		Self {
			line: 0,
			column: 0,
			content: StringCursor::new(content),
		}
	}

	pub fn seek(&mut self, char_index: usize, stop_at_newline: bool) -> TextPosition {
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
						self.column = 0;
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
		TextPosition {
			utf_8: end,
			line: self.line,
			column: self.column,
		}
	}

	pub fn substring(&self, start: usize, end: usize) -> &str {
		&self.content.text[start..end]
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

#[cfg(test)]
mod test {
	use super::*;

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
}

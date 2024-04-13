use std::{error::Error, ops::Not};

use jni::{
	objects::{JObject, JValue, JValueGen},
	AttachGuard, InitArgsBuilder, JNIEnv, JavaVM,
};

pub struct LanguageTool<'a> {
	lang_tool: JObject<'a>,
	guard: AttachGuard<'a>,
}

pub struct JVM {
	jvm: JavaVM,
}

impl JVM {
	pub fn new(class_path: &str) -> Result<Self, Box<dyn Error>> {
		let jvm_args = InitArgsBuilder::new()
			.version(jni::JNIVersion::V8)
			.option(format!("-Djava.class.path={}", class_path))
			.build()?;
		let jvm = JavaVM::new(jvm_args)?;
		Ok(Self { jvm })
	}

	#[cfg(feature = "bundle-jar")]
	pub fn new_bundled() -> Result<Self, Box<dyn Error>> {
		Self::new(include!(concat!(env!("OUT_DIR"), "./jar_path.rs")))
	}
}

impl<'a> LanguageTool<'a> {
	pub fn new(jvm: &'a JVM, lang: &str) -> Result<Self, Box<dyn Error>> {
		let mut guard = jvm.jvm.attach_current_thread()?;
		let lang_code = guard.new_string(lang)?;
		let lang = guard.call_static_method(
			"org/languagetool/Languages",
			"getLanguageForShortCode",
			"(Ljava/lang/String;)Lorg/languagetool/Language;",
			&[JValue::Object(&lang_code)],
		)?;

		let lang_tool = guard.new_object(
			"org/languagetool/JLanguageTool",
			"(Lorg/languagetool/Language;)V",
			&[lang.borrow()],
		)?;

		Ok(Self { lang_tool, guard })
	}

	pub fn text_builder<'b>(&'b mut self) -> Result<TextBuilder<'a, 'b>, Box<dyn Error>> {
		let annotated_text_builder =
			self.guard
				.new_object("org/languagetool/markup/AnnotatedTextBuilder", "()V", &[])?;
		Ok(TextBuilder {
			text_builder: annotated_text_builder,
			env: &mut self.guard,
		})
	}

	pub fn allow_words(&mut self, words: &[impl AsRef<str>]) -> Result<(), Box<dyn Error>> {
		let rules = self
			.guard
			.call_method(
				&self.lang_tool,
				"getAllActiveRules",
				"()Ljava/util/List;",
				&[],
			)?
			.l()?;
		let list = self.guard.get_list(&rules)?;
		let args = self.guard.new_object("java/util/ArrayList", "()V", &[])?;
		let args = self.guard.get_list(&args)?;
		for word in words {
			let word = self.guard.new_string(word)?;
			args.add(&mut self.guard, &word)?;
		}

		for i in 0..list.size(&mut self.guard)? {
			let Some(rule) = list.get(&mut self.guard, i)? else {
				continue;
			};
			if self
				.guard
				.is_instance_of(&rule, "org/languagetool/rules/spelling/SpellingCheckRule")?
				.not()
			{
				continue;
			}

			self.guard.call_method(
				&rule,
				"acceptPhrases",
				"(Ljava/util/List;)V",
				&[JValue::Object(args.as_ref())],
			)?;
		}
		Ok(())
	}

	pub fn disable_checks(&mut self, checks: &[impl AsRef<str>]) -> Result<(), Box<dyn Error>> {
		let args = self.guard.new_object("java/util/ArrayList", "()V", &[])?;
		let args = self.guard.get_list(&args)?;
		for check in checks {
			let check = self.guard.new_string(check)?;
			args.add(&mut self.guard, &check)?;
		}

		self.guard.call_method(
			&self.lang_tool,
			"disableRules",
			"(Ljava/util/List;)V",
			&[JValue::Object(args.as_ref())],
		)?;
		Ok(())
	}

	pub fn check<'b>(
		&mut self,
		text: JValueGen<JObject<'a>>,
	) -> Result<Vec<Suggestion>, Box<dyn Error>> {
		let matches = self
			.guard
			.call_method(
				&self.lang_tool,
				"check",
				"(Lorg/languagetool/markup/AnnotatedText;)Ljava/util/List;",
				&[text.borrow()],
			)?
			.l()?;

		let list = self.guard.get_list(&matches)?;
		let size = list.size(&mut self.guard)?;

		let mut suggestions = Vec::with_capacity(size as usize);

		for i in 0..size {
			let Some(m) = list.get(&mut self.guard, i)? else {
				continue;
			};
			let start = self.guard.call_method(&m, "getFromPos", "()I", &[])?.i()?;
			let end = self.guard.call_method(&m, "getToPos", "()I", &[])?.i()?;

			let message = self
				.guard
				.call_method(&m, "getMessage", "()Ljava/lang/String;", &[])?
				.l()?;
			let message = self.guard.get_string(&message.into())?.into();

			let replacements = self
				.guard
				.call_method(&m, "getSuggestedReplacements", "()Ljava/util/List;", &[])?
				.l()?;
			let list = self.guard.get_list(&replacements)?;
			let size = list.size(&mut self.guard)?;
			let mut replacements = Vec::with_capacity(size as usize);
			for i in 0..size {
				let Some(replacement) = list.get(&mut self.guard, i)? else {
					continue;
				};
				let replacement = self.guard.get_string(&replacement.into())?.into();
				replacements.push(replacement);
			}

			let rule = self
				.guard
				.call_method(&m, "getRule", "()Lorg/languagetool/rules/Rule;", &[])?
				.l()?;
			let rule_id = self
				.guard
				.call_method(&rule, "getId", "()Ljava/lang/String;", &[])?
				.l()?;
			let rule_id = self.guard.get_string(&rule_id.into())?.into();
			let rule_description = self
				.guard
				.call_method(&rule, "getDescription", "()Ljava/lang/String;", &[])?
				.l()?;
			let rule_description = self.guard.get_string(&rule_description.into())?.into();

			let suggestion = Suggestion {
				start: start as usize,
				end: end as usize,
				replacements,
				message,
				rule_id,
				rule_description,
			};
			suggestions.push(suggestion);
		}
		Ok(suggestions)
	}
}

pub struct TextBuilder<'a, 'b> {
	text_builder: JObject<'a>,
	env: &'b mut JNIEnv<'a>,
}

impl<'a, 'b> TextBuilder<'a, 'b> {
	pub fn add_text(&mut self, text: &str) -> Result<(), Box<dyn Error>> {
		let text = self.env.new_string(text)?;
		self.env.call_method(
			&self.text_builder,
			"addText",
			"(Ljava/lang/String;)Lorg/languagetool/markup/AnnotatedTextBuilder;",
			&[JValue::Object(&text)],
		)?;
		Ok(())
	}

	pub fn add_markup(&mut self, markup: &str) -> Result<(), Box<dyn Error>> {
		let markup = self.env.new_string(markup)?;
		self.env.call_method(
			&self.text_builder,
			"addMarkup",
			"(Ljava/lang/String;)Lorg/languagetool/markup/AnnotatedTextBuilder;",
			&[JValue::Object(&markup)],
		)?;
		Ok(())
	}

	pub fn add_encoded(&mut self, markup: &str, text: &str) -> Result<(), Box<dyn Error>> {
		let markup = self.env.new_string(markup)?;
		let text = self.env.new_string(text)?;
		self.env.call_method(
			&self.text_builder,
			"addMarkup",
			"(Ljava/lang/String;Ljava/lang/String;)Lorg/languagetool/markup/AnnotatedTextBuilder;",
			&[JValue::Object(&markup), JValue::Object(&text)],
		)?;
		Ok(())
	}

	pub fn finish(self) -> Result<JValueGen<JObject<'a>>, Box<dyn Error>> {
		let annotated_text = self.env.call_method(
			&self.text_builder,
			"build",
			"()Lorg/languagetool/markup/AnnotatedText;",
			&[],
		)?;
		Ok(annotated_text)
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

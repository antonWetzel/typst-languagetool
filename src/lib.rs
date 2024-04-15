mod convert;
mod rules;
mod text_builder;

pub use rules::Rules;
pub use text_builder::TextBuilder;
use typst::syntax::SyntaxNode;

use std::ops::Not;

use jni::{
	objects::{GlobalRef, JObject, JValue, JValueGen},
	InitArgsBuilder, JNIEnv, JavaVM,
};

use crate::convert::convert;

pub struct LanguageTool {
	jvm: JavaVM,
	lang_tool: GlobalRef,
}

fn new_jvm(class_path: &str) -> anyhow::Result<JavaVM> {
	let jvm_args = InitArgsBuilder::new()
		.version(jni::JNIVersion::V8)
		.option(format!("-Djava.class.path={}", class_path))
		.build()?;
	let jvm = JavaVM::new(jvm_args)?;
	Ok(jvm)
}

impl LanguageTool {
	pub fn new(class_path: &str, lang: &str) -> anyhow::Result<Self> {
		let jvm = new_jvm(class_path)?;
		let lang_tool = Self::create_lang_tool(lang, &jvm)?;
		Ok(Self { lang_tool, jvm })
	}

	#[cfg(feature = "bundle-jar")]
	pub fn new_bundled(lang: &str) -> anyhow::Result<Self> {
		let jvm = new_jvm(include!(concat!(env!("OUT_DIR"), "./jar_path.rs")))?;
		let lang_tool = Self::create_lang_tool(lang, &jvm)?;
		Ok(Self { lang_tool, jvm })
	}

	fn create_lang_tool(lang: &str, jvm: &JavaVM) -> anyhow::Result<GlobalRef> {
		let lang_tool = {
			let mut guard = jvm.attach_current_thread()?;
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
			guard.new_global_ref(lang_tool)?
		};

		Ok(lang_tool)
	}

	pub fn change_language(&mut self, lang: &str) -> anyhow::Result<()> {
		self.lang_tool = Self::create_lang_tool(lang, &self.jvm)?;
		Ok(())
	}

	pub fn check_source(&self, text: &str, rules: &Rules) -> anyhow::Result<Vec<Suggestion>> {
		let mut guard = self.jvm.attach_current_thread()?;
		let root = typst::syntax::parse(text);
		let text = convert(&root, rules, &mut guard)?;
		let text = text.finish()?;
		let suggestions = Self::lt_request(&self.lang_tool, text, &mut guard)?;
		Ok(suggestions)
	}

	pub fn check_ast_node(
		&self,
		node: &SyntaxNode,
		rules: &Rules,
	) -> anyhow::Result<Vec<Suggestion>> {
		let mut guard = self.jvm.attach_current_thread()?;
		let text = convert(node, rules, &mut guard)?;
		let text = text.finish()?;
		let suggestions = Self::lt_request(&self.lang_tool, text, &mut guard)?;
		Ok(suggestions)
	}

	pub fn check_document(
		&self,
		root: SyntaxNode,
		rules: &Rules,
	) -> anyhow::Result<Vec<Suggestion>> {
		todo!()
	}

	pub fn allow_words(&self, words: &[impl AsRef<str>]) -> anyhow::Result<()> {
		let mut guard = self.jvm.attach_current_thread()?;
		let rules = guard
			.call_method(
				&self.lang_tool,
				"getAllActiveRules",
				"()Ljava/util/List;",
				&[],
			)?
			.l()?;
		let list = guard.get_list(&rules)?;
		let args = guard.new_object("java/util/ArrayList", "()V", &[])?;
		let args = guard.get_list(&args)?;
		for word in words {
			let word = guard.new_string(word)?;
			args.add(&mut guard, &word)?;
		}

		for i in 0..list.size(&mut guard)? {
			let Some(rule) = list.get(&mut guard, i)? else {
				continue;
			};
			if guard
				.is_instance_of(&rule, "org/languagetool/rules/spelling/SpellingCheckRule")?
				.not()
			{
				continue;
			}

			guard.call_method(
				&rule,
				"acceptPhrases",
				"(Ljava/util/List;)V",
				&[JValue::Object(args.as_ref())],
			)?;
		}
		Ok(())
	}

	pub fn disable_checks(&self, checks: &[impl AsRef<str>]) -> anyhow::Result<()> {
		let mut guard = self.jvm.attach_current_thread()?;
		let args = guard.new_object("java/util/ArrayList", "()V", &[])?;
		let args = guard.get_list(&args)?;
		for check in checks {
			let check = guard.new_string(check)?;
			args.add(&mut guard, &check)?;
		}

		guard.call_method(
			&self.lang_tool,
			"disableRules",
			"(Ljava/util/List;)V",
			&[JValue::Object(args.as_ref())],
		)?;
		Ok(())
	}

	fn lt_request<'a>(
		lang_tool: &JObject<'a>,
		text: JValueGen<JObject<'a>>,
		env: &mut JNIEnv<'a>,
	) -> anyhow::Result<Vec<Suggestion>> {
		let matches = env
			.call_method(
				lang_tool,
				"check",
				"(Lorg/languagetool/markup/AnnotatedText;)Ljava/util/List;",
				&[text.borrow()],
			)?
			.l()?;

		let list = env.get_list(&matches)?;
		let size = list.size(env)?;

		let mut suggestions = Vec::with_capacity(size as usize);

		for i in 0..size {
			let Some(m) = list.get(env, i)? else {
				continue;
			};
			let start = env.call_method(&m, "getFromPos", "()I", &[])?.i()?;
			let end = env.call_method(&m, "getToPos", "()I", &[])?.i()?;

			let message = env
				.call_method(&m, "getMessage", "()Ljava/lang/String;", &[])?
				.l()?;
			let message = env.get_string(&message.into())?.into();

			let replacements = env
				.call_method(&m, "getSuggestedReplacements", "()Ljava/util/List;", &[])?
				.l()?;
			let list = env.get_list(&replacements)?;
			let size = list.size(env)?;
			let mut replacements = Vec::with_capacity(size as usize);
			for i in 0..size {
				let Some(replacement) = list.get(env, i)? else {
					continue;
				};
				let replacement = env.get_string(&replacement.into())?.into();
				replacements.push(replacement);
			}

			let rule = env
				.call_method(&m, "getRule", "()Lorg/languagetool/rules/Rule;", &[])?
				.l()?;
			let rule_id = env
				.call_method(&rule, "getId", "()Ljava/lang/String;", &[])?
				.l()?;
			let rule_id = env.get_string(&rule_id.into())?.into();
			let rule_description = env
				.call_method(&rule, "getDescription", "()Ljava/lang/String;", &[])?
				.l()?;
			let rule_description = env.get_string(&rule_description.into())?.into();

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

#[derive(Debug, Clone)]
pub struct Suggestion {
	pub start: usize,
	pub end: usize,
	pub message: String,
	pub replacements: Vec<String>,
	pub rule_description: String,
	pub rule_id: String,
}

pub struct Position {
	pub utf_8: usize,
	pub line: usize,
	pub column: usize,
}

pub struct TextWithPosition<'a> {
	line: usize,
	column: usize,
	content: StringCursor<'a>,
}

impl<'a> TextWithPosition<'a> {
	pub fn new(content: &'a str) -> Self {
		Self {
			line: 0,
			column: 0,
			content: StringCursor::new(content),
		}
	}

	pub fn get_position(&mut self, char_index: usize, stop_at_newline: bool) -> Position {
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
		Position {
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

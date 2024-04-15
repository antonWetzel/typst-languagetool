mod convert;
mod rules;

#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
mod jni;
#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
pub use jni::LanguageToolJNI;

#[cfg(feature = "remote-server")]
mod remote;
#[cfg(feature = "remote-server")]
pub use remote::LanguageToolRemote;

pub use rules::Rules;

pub trait LanguageTool {
	fn change_language(&mut self, lang: &str) -> anyhow::Result<()>;
	fn allow_words(&mut self, words: &[String]) -> anyhow::Result<()>;
	fn disable_checks(&mut self, checks: &[String]) -> anyhow::Result<()>;
	fn check_source(&self, text: &str, rules: &Rules) -> anyhow::Result<Vec<Suggestion>>;
}

pub fn new_languagetool(
	bundled: bool,
	jar_location: Option<&String>,
	host: Option<&String>,
	port: Option<&String>,
	#[allow(unused)] language: &str,
) -> anyhow::Result<Box<dyn LanguageTool>> {
	let lt = match (bundled, jar_location, host, port) {
		#[cfg(feature = "bundle-jar")]
		(true, None, None, None) => Box::new(LanguageToolJNI::new_bundled(language)?),
		#[cfg(not(feature = "bundle-jar"))]
		(true, None, None, None) => Err(anyhow::anyhow!("Feature 'bundle-jar' is disabled."))?,

		#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
		(false, Some(path), None, None) => Box::new(LanguageToolJNI::new(path, language)?),
		#[cfg(all(not(feature = "bundle-jar"), not(feature = "extern-jar")))]
		(false, Some(_), None, None) => Err(anyhow::anyhow!(
			"Features 'bundle-jar' and 'extern-jar' are disabled."
		))?,

		#[cfg(feature = "remote-server")]
		(false, None, Some(host), Some(port)) => Box::new(LanguageToolRemote::new(host, port, language)?),
		#[cfg(not(feature = "remote-server"))]
		(false, None, Some(_), Some(_)) => Err(anyhow::anyhow!("Feature 'remote-server' is disabled."))?,

		_ => Err(anyhow::anyhow!(
			"Exactly one of 'bundled', 'jar_location' or 'host and port' must be specified."
		))?,
	};
	Ok(lt)
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

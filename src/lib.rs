mod backends;
pub mod convert;
mod rules;

pub use backends::*;
pub use rules::Rules;

#[allow(async_fn_in_trait)]
pub trait LanguageToolBackend {
	async fn change_language(&mut self, lang: &str) -> anyhow::Result<()>;
	async fn allow_words(&mut self, words: &[String]) -> anyhow::Result<()>;
	async fn disable_checks(&mut self, checks: &[String]) -> anyhow::Result<()>;
	async fn check_source(&self, text: &str, rules: &Rules) -> anyhow::Result<Vec<Suggestion>>;
}

pub enum LanguageTool {
	#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
	JNI(jni::LanguageToolJNI),
	#[cfg(feature = "remote-server")]
	Remote(remote::LanguageToolRemote),
}

impl LanguageTool {
	pub fn new(
		bundled: bool,
		jar_location: Option<&String>,
		host: Option<&String>,
		port: Option<&String>,
		#[allow(unused)] language: &str,
	) -> anyhow::Result<Self> {
		let lt = match (bundled, jar_location, host, port) {
			#[cfg(feature = "remote-server")]
			(false, None, Some(host), Some(port)) => {
				Self::Remote(remote::LanguageToolRemote::new(host, port, language)?)
			},
			#[cfg(not(feature = "remote-server"))]
			(false, None, Some(_), Some(_)) => Err(anyhow::anyhow!("Feature 'remote-server' is disabled."))?,

			#[cfg(feature = "bundle-jar")]
			(true, None, None, None) => Self::JNI(jni::LanguageToolJNI::new_bundled(language)?),

			#[cfg(not(feature = "bundle-jar"))]
			(true, None, None, None) => Err(anyhow::anyhow!("Feature 'bundle-jar' is disabled."))?,

			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			(false, Some(path), None, None) => Self::JNI(jni::LanguageToolJNI::new(path, language)?),
			#[cfg(all(not(feature = "bundle-jar"), not(feature = "extern-jar")))]
			(false, Some(_), None, None) => Err(anyhow::anyhow!(
				"Features 'bundle-jar' and 'extern-jar' are disabled."
			))?,

			_ => Err(anyhow::anyhow!(
				"Exactly one of 'bundled', 'jar_location' or 'host and port' must be specified."
			))?,
		};
		Ok(lt)
	}

	pub async fn change_language(&mut self, lang: &str) -> anyhow::Result<()> {
		match self {
			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			Self::JNI(lt) => lt.change_language(lang).await,
			#[cfg(feature = "remote-server")]
			Self::Remote(lt) => lt.change_language(lang).await,
		}
	}
	pub async fn allow_words(&mut self, words: &[String]) -> anyhow::Result<()> {
		match self {
			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			Self::JNI(lt) => lt.allow_words(words).await,
			#[cfg(feature = "remote-server")]
			Self::Remote(lt) => lt.allow_words(words).await,
		}
	}
	pub async fn disable_checks(&mut self, checks: &[String]) -> anyhow::Result<()> {
		match self {
			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			Self::JNI(lt) => lt.disable_checks(checks).await,
			#[cfg(feature = "remote-server")]
			Self::Remote(lt) => lt.disable_checks(checks).await,
		}
	}
	pub async fn check_source(&self, text: &str, rules: &Rules) -> anyhow::Result<Vec<Suggestion>> {
		match self {
			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			Self::JNI(lt) => lt.check_source(text, rules).await,
			#[cfg(feature = "remote-server")]
			Self::Remote(lt) => lt.check_source(text, rules).await,
		}
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

#[derive(Debug, Clone, Copy)]
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

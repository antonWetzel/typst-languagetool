mod convert;
mod languagetool;
mod rules;

pub use languagetool::LanguageTool;
pub use languagetool::Position;
pub use languagetool::Suggestion;
pub use languagetool::TextBuilder;

pub use rules::Rules;

use std::error::Error;

pub fn check(
	lt: &LanguageTool,
	text: &str,
	rules: &Rules,
) -> Result<Vec<Suggestion>, Box<dyn Error>> {
	let root = typst_syntax::parse(&text);
	let text = convert::convert(&root, &rules, lt)?;
	let suggestions = lt.check(text)?;
	Ok(suggestions)
}

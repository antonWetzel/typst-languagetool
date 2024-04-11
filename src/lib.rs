mod convert;
mod languagetool;
mod rules;

pub use languagetool::LanguageTool;
pub use languagetool::Position;
pub use languagetool::Suggestion;
pub use languagetool::TextBuilder;
pub use languagetool::JVM;

pub use rules::Rules;

use std::error::Error;

pub fn check<'a>(
	lt: &mut LanguageTool<'a>,
	text: &str,
	rules: &Rules,
) -> Result<Vec<Suggestion>, Box<dyn Error>> {
	let root = typst_syntax::parse(&text);
	let text = convert::convert(&root, &rules, lt)?;
	let text = text.finish()?;
	let suggestions = lt.check(text)?;
	Ok(suggestions)
}

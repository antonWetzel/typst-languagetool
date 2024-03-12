mod convert;
mod rules;

use languagetool_rust::{
	check::{CheckRequest, Data},
	server::ServerClient,
	CheckResponse,
};
pub use rules::Rules;
use std::{collections::HashSet, error::Error};

pub async fn check(
	client: &ServerClient,
	text: &str,

	language: Option<&str>,
	rules: &Rules,
	max_request_length: usize,
	dict: &HashSet<String>,

	mut action: impl FnMut(CheckResponse, usize),
) -> Result<(), Box<dyn Error>> {
	let root = typst_syntax::parse(&text);
	let data = convert::convert(&root, &rules, max_request_length);

	for items in data {
		let req = CheckRequest::default()
			.with_language(match language {
				Some(value) => String::from(value),
				None => "auto".into(),
			})
			.with_data(Data::from_iter(items.0));

		let mut response = client.check(&req).await?;

		filter_response(&mut response, dict);
		action(response, items.1);
	}
	Ok(())
}

fn filter_response(response: &mut CheckResponse, dict: &HashSet<String>) {
	for m in std::mem::take(&mut response.matches).into_iter() {
		// Only handle misspellings
		if m.rule.issue_type.as_str() != "misspelling" {
			response.matches.push(m);
			continue;
		}
		// Check if the word is contained in the dictionary
		let ctx = &m.context;
		let mut chars = ctx.text.char_indices();
		let start = chars.nth(ctx.offset).map_or(0, |(idx, _)| idx);
		let end = chars
			.nth(ctx.length.wrapping_sub(1))
			.map_or(ctx.text.len(), |(idx, _)| idx);
		let word = &ctx.text[start..end];
		if dict.contains(word) {
			continue;
		}
		response.matches.push(m);
	}
}

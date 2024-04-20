use std::collections::HashSet;

use languagetool_rust::{check::Match, CheckRequest, ServerClient};

use crate::{LanguageToolBackend, Suggestion};

#[derive(Debug)]
pub struct LanguageToolRemote {
	lang: String,
	server_client: ServerClient,
	disabled_categories: Option<Vec<String>>,
	allowed_words: Option<HashSet<String>>,
}

impl LanguageToolRemote {
	pub fn new(hostname: &str, port: &str, lang: &str) -> anyhow::Result<Self> {
		let server_client = ServerClient::new(hostname, port);
		Ok(Self {
			server_client,
			lang: lang.into(),
			disabled_categories: None,
			allowed_words: None,
		})
	}
}

impl LanguageToolBackend for LanguageToolRemote {
	async fn allow_words(&mut self, words: &[String]) -> anyhow::Result<()> {
		self.allowed_words = Some(words.iter().map(|x| x.clone()).collect());
		Ok(())
	}

	async fn change_language(&mut self, lang: &str) -> anyhow::Result<()> {
		self.lang = lang.into();
		Ok(())
	}

	async fn disable_checks(&mut self, checks: &[String]) -> anyhow::Result<()> {
		self.disabled_categories = Some(checks.iter().map(|x| x.clone()).collect());
		Ok(())
	}

	async fn check_text(&self, text: &str) -> anyhow::Result<Vec<crate::Suggestion>> {
		let mut req = CheckRequest::default()
			.with_text(String::from(text))
			.with_language(self.lang.clone());
		req.disabled_rules = self.disabled_categories.clone();

		let response = self.server_client.check(&req).await?;

		let mut suggestions = Vec::with_capacity(response.matches.len());
		for m in response.matches {
			if let Some(allowed) = &self.allowed_words {
				if filter_match(&m, allowed) {
					continue;
				}
			}
			let suggestion = Suggestion {
				start: m.offset,
				end: m.offset + m.length,
				message: m.message,
				rule_description: m.rule.description,
				rule_id: m.rule.id,
				replacements: m.replacements.into_iter().map(|x| x.value).collect(),
			};
			suggestions.push(suggestion);
		}

		Ok(suggestions)
	}
}

fn filter_match(m: &Match, allowed: &HashSet<String>) -> bool {
	if m.context.length == 0 {
		return false;
	}
	let mut iter = m.context.text.char_indices();
	let Some((start, _)) = iter.nth(m.context.offset) else {
		return false;
	};
	let Some((end, _)) = iter.nth(m.context.length - 1) else {
		return false;
	};
	let text = &m.context.text[start..end];
	allowed.contains(text)
}

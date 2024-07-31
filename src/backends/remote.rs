use std::collections::{HashMap, HashSet};

use languagetool_rust::{check::Match, CheckRequest, ServerClient};

use crate::{LanguageToolBackend, Suggestion};

#[derive(Debug)]
pub struct LanguageToolRemote {
	server_client: ServerClient,
	disabled_categories: HashMap<String, Vec<String>>,
	allowed_words: HashMap<String, HashSet<String>>,
}

impl LanguageToolRemote {
	pub fn new(hostname: &str, port: &str) -> anyhow::Result<Self> {
		let server_client = ServerClient::new(hostname, port);
		Ok(Self {
			server_client,
			disabled_categories: HashMap::new(),
			allowed_words: HashMap::new(),
		})
	}
}

impl LanguageToolBackend for LanguageToolRemote {
	async fn allow_words(&mut self, lang: String, words: &[String]) -> anyhow::Result<()> {
		self.allowed_words
			.insert(lang, words.iter().cloned().collect());
		Ok(())
	}

	async fn disable_checks(&mut self, lang: String, checks: &[String]) -> anyhow::Result<()> {
		self.disabled_categories.insert(lang, checks.to_vec());
		Ok(())
	}

	async fn check_text(
		&mut self,
		lang: String,
		text: &str,
	) -> anyhow::Result<Vec<crate::Suggestion>> {
		let disabled_rules = self.disabled_categories.get(&lang).cloned();
		let allowed = self.allowed_words.get(&lang);

		let mut req = CheckRequest::default()
			.with_text(String::from(text))
			.with_language(lang);
		req.disabled_rules = disabled_rules;

		let response = self.server_client.check(&req).await?;

		let mut suggestions = Vec::with_capacity(response.matches.len());
		for m in response.matches {
			if let Some(allowed) = allowed {
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

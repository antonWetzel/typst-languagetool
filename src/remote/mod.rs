use languagetool_rust::{CheckRequest, ServerClient};
use tokio::runtime::Runtime;

use crate::{
	convert::{convert, TextBuilder},
	LanguageTool, Suggestion,
};

pub struct LanguageToolRemote {
	lang: String,
	server_client: ServerClient,
	disabled_categories: Option<Vec<String>>,
	runtime: Runtime,
}

impl LanguageToolRemote {
	pub fn new(hostname: &str, port: &str, lang: &str) -> anyhow::Result<Self> {
		let server_client = ServerClient::new(hostname, port);
		Ok(Self {
			server_client,
			lang: lang.into(),
			disabled_categories: None,
			runtime: Runtime::new()?,
		})
	}
}

impl LanguageTool for LanguageToolRemote {
	fn allow_words(&mut self, words: &[String]) -> anyhow::Result<()> {
		let _ = words;
		eprintln!("Allow Words not supported for Remote LanguageTool");
		Ok(())
	}

	fn change_language(&mut self, lang: &str) -> anyhow::Result<()> {
		self.lang = lang.into();
		Ok(())
	}

	fn disable_checks(&mut self, checks: &[String]) -> anyhow::Result<()> {
		self.disabled_categories = Some(checks.iter().map(|x| x.clone()).collect());
		Ok(())
	}

	fn check_source(
		&self,
		text: &str,
		rules: &crate::Rules,
	) -> anyhow::Result<Vec<crate::Suggestion>> {
		let root = typst::syntax::parse(text);
		let mut text_builder = TextBuilderRemote::new();
		convert(&root, rules, &mut text_builder)?;

		let mut req = CheckRequest::default()
			.with_text(text_builder.converted)
			.with_language(self.lang.clone());
		req.disabled_rules = self.disabled_categories.clone();

		let response = self.runtime.block_on(self.server_client.check(&req))?;

		let mut suggestions = Vec::with_capacity(response.matches.len());
		for m in response.matches {
			let suggestion = Suggestion {
				start: text_builder.map[m.offset],
				end: text_builder.map[m.offset + m.length],
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

pub struct TextBuilderRemote {
	converted: String,
	// todo: don't save position for every char
	map: Vec<usize>,
	position: usize,
}

impl TextBuilderRemote {
	pub fn new() -> Self {
		Self {
			converted: String::new(),
			map: Vec::new(),
			position: 0,
		}
	}
}

impl TextBuilder for TextBuilderRemote {
	fn add_text(&mut self, text: &str) -> anyhow::Result<()> {
		self.converted += text;
		for _ in text.chars() {
			self.map.push(self.position);
			self.position += 1;
		}
		Ok(())
	}

	fn add_markup(&mut self, markup: &str) -> anyhow::Result<()> {
		self.position += markup.chars().count();
		Ok(())
	}

	fn add_encoded(&mut self, markup: &str, text: &str) -> anyhow::Result<()> {
		self.converted += text;
		for _ in text.chars() {
			self.map.push(self.position);
		}
		self.position += markup.chars().count();
		Ok(())
	}
}

use std::collections::HashSet;

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
	allowed_words: Option<HashSet<String>>,
	runtime: Runtime,
}

impl LanguageToolRemote {
	pub fn new(hostname: &str, port: &str, lang: &str) -> anyhow::Result<Self> {
		let server_client = ServerClient::new(hostname, port);
		Ok(Self {
			server_client,
			lang: lang.into(),
			disabled_categories: None,
			allowed_words: None,
			runtime: Runtime::new()?,
		})
	}
}

impl LanguageTool for LanguageToolRemote {
	fn allow_words(&mut self, words: &[String]) -> anyhow::Result<()> {
		self.allowed_words = Some(words.iter().map(|x| x.clone()).collect());
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
			if let Some(allowed) = &self.allowed_words {
				if m.context.length == 0 {
					continue;
				}
				let mut iter = m.context.text.char_indices();
				let (start, _) = iter.nth(m.context.offset).unwrap();
				let (end, _) = iter.nth(m.context.length - 1).unwrap();
				let text = &m.context.text[start..end];
				if allowed.contains(text) {
					continue;
				}
			}
			let suggestion = Suggestion {
				start: text_builder.mapper.source(m.offset),
				end: text_builder.mapper.source(m.offset + m.length),
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

struct Mapper {
	blocks: Vec<Block>,
	lt_position: usize,
	source_position: usize,
	block_index: usize,
}

impl Mapper {
	fn source(&mut self, pos: usize) -> usize {
		while pos < self.lt_position {
			self.block_index -= 1;
			match self.blocks[self.block_index] {
				Block::Text(s) => {
					self.lt_position -= s;
					self.source_position -= s;
				},
				Block::Markup(s) => {
					self.source_position -= s;
				},
				Block::Encoded { text, markup } => {
					self.lt_position -= text;
					self.source_position -= markup;
				},
			}
		}

		loop {
			let diff = pos - self.lt_position;
			match self.blocks[self.block_index] {
				Block::Text(s) => {
					if diff < s {
						return self.source_position + diff;
					}
					self.block_index += 1;
					self.lt_position += s;
					self.source_position += s;
				},
				Block::Markup(s) => {
					self.block_index += 1;
					self.source_position += s;
				},
				Block::Encoded { text, markup } => {
					if diff < text {
						return self.source_position + diff;
					}
					self.block_index += 1;
					self.lt_position += text;
					self.source_position += markup;
				},
			}
		}
	}

	fn add_block(&mut self, block: Block) {
		match (self.blocks.last_mut(), block) {
			(Some(Block::Text(old_s)), Block::Text(s)) => *old_s += s,
			(Some(Block::Markup(old_s)), Block::Markup(s)) => *old_s += s,
			(
				Some(Block::Encoded { text: old_text, markup: old_markup }),
				Block::Encoded { text, markup },
			) => {
				*old_text += text;
				*old_markup += markup
			},
			_ => self.blocks.push(block),
		}
	}
}

#[derive(Debug, Clone, Copy)]
enum Block {
	Text(usize),
	Markup(usize),
	Encoded { text: usize, markup: usize },
}

struct TextBuilderRemote {
	converted: String,
	mapper: Mapper,
}

impl TextBuilderRemote {
	pub fn new() -> Self {
		Self {
			converted: String::new(),
			mapper: Mapper {
				blocks: Vec::new(),
				lt_position: 0,
				source_position: 0,
				block_index: 0,
			},
		}
	}
}

impl TextBuilder for TextBuilderRemote {
	fn add_text(&mut self, text: &str) -> anyhow::Result<()> {
		self.converted += text;
		self.mapper.add_block(Block::Text(text.chars().count()));
		Ok(())
	}

	fn add_markup(&mut self, markup: &str) -> anyhow::Result<()> {
		self.mapper.add_block(Block::Markup(markup.chars().count()));
		Ok(())
	}

	fn add_encoded(&mut self, markup: &str, text: &str) -> anyhow::Result<()> {
		self.converted += text;
		self.mapper.add_block(Block::Encoded {
			text: text.chars().count(),
			markup: markup.chars().count(),
		});
		Ok(())
	}
}

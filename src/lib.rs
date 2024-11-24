mod backends;
pub mod convert;

use std::{collections::HashMap, ops::Range, path::PathBuf};

#[allow(unused_imports)]
pub use backends::*;
use convert::Mapping;
use typst::{
	syntax::{FileId, Source},
	World,
};

#[cfg(not(any(feature = "bundle", feature = "jar", feature = "server",)))]
compile_error!("No backends enabled, the backends can be enabled with feature flags");

#[allow(async_fn_in_trait)]
pub trait LanguageToolBackend {
	async fn allow_words(&mut self, lang: String, words: &[String]) -> anyhow::Result<()>;
	async fn disable_checks(&mut self, lang: String, checks: &[String]) -> anyhow::Result<()>;
	async fn check_text(&mut self, lang: String, text: &str) -> anyhow::Result<Vec<Suggestion>>;
}

#[derive(Debug)]
pub enum LanguageTool {
	#[cfg(any(feature = "bundle", feature = "jar"))]
	JNI(jni::LanguageToolJNI),
	#[cfg(feature = "server")]
	Remote(remote::LanguageToolRemote),
}

impl LanguageTool {
	pub async fn new(options: &LanguageToolOptions) -> anyhow::Result<Self> {
		let mut lt = match &options.backend {
			BackendOptions::None => Err(anyhow::anyhow!(
				"No Languagetool Backend (bundle, jar or server) specified."
			))?,

			#[cfg(feature = "bundle")]
			BackendOptions::Bundle => Self::JNI(jni::LanguageToolJNI::new_bundled()?),

			#[cfg(not(feature = "bundle"))]
			BackendOptions::Bundle => Err(anyhow::anyhow!("Feature 'bundle' is disabled."))?,

			#[cfg(any(feature = "bundle", feature = "jar"))]
			BackendOptions::Jar { jar_location } => Self::JNI(jni::LanguageToolJNI::new(jar_location)?),
			#[cfg(all(not(feature = "bundle"), not(feature = "jar")))]
			BackendOptions::Jar { jar_location: _ } => {
				Err(anyhow::anyhow!("Features 'bundle' and 'jar' are disabled."))?
			},

			#[cfg(feature = "server")]
			BackendOptions::Remote { host, port } => {
				Self::Remote(remote::LanguageToolRemote::new(host, port)?)
			},

			#[cfg(not(feature = "server"))]
			BackendOptions::Remote { host: _, port: _ } => {
				Err(anyhow::anyhow!("Feature 'server' is disabled."))?
			},
		};

		for (lang, dict) in &options.dictionary {
			lt.allow_words(lang.clone(), dict).await?;
		}
		for (lang, checks) in &options.disabled_checks {
			lt.disable_checks(lang.clone(), checks).await?;
		}

		Ok(lt)
	}
}

impl LanguageToolBackend for LanguageTool {
	async fn allow_words(&mut self, lang: String, words: &[String]) -> anyhow::Result<()> {
		match self {
			#[cfg(any(feature = "bundle", feature = "jar"))]
			Self::JNI(lt) => lt.allow_words(lang, words).await,
			#[cfg(feature = "server")]
			Self::Remote(lt) => lt.allow_words(lang, words).await,

			#[allow(unreachable_patterns)]
			_ => unreachable!("{:?} {:?}", lang, words),
		}
	}
	async fn disable_checks(&mut self, lang: String, checks: &[String]) -> anyhow::Result<()> {
		match self {
			#[cfg(any(feature = "bundle", feature = "jar"))]
			Self::JNI(lt) => lt.disable_checks(lang, checks).await,
			#[cfg(feature = "server")]
			Self::Remote(lt) => lt.disable_checks(lang, checks).await,

			#[allow(unreachable_patterns)]
			_ => unreachable!("{:?} {:?}", lang, checks),
		}
	}
	async fn check_text(&mut self, lang: String, text: &str) -> anyhow::Result<Vec<Suggestion>> {
		match self {
			#[cfg(any(feature = "bundle", feature = "jar"))]
			Self::JNI(lt) => lt.check_text(lang, text).await,
			#[cfg(feature = "server")]
			Self::Remote(lt) => lt.check_text(lang, text).await,

			#[allow(unreachable_patterns)]
			_ => unreachable!("{:?} {:?}", lang, text),
		}
	}
}

pub struct FileCollector {
	source: Option<Source>,
	diagnostics: Vec<Diagnostic>,
}

impl FileCollector {
	pub fn new(file_id: Option<FileId>, world: &impl World) -> Self {
		let source = file_id.map(|id| world.source(id).unwrap());
		Self { source, diagnostics: Vec::new() }
	}

	pub fn add(&mut self, world: &impl World, suggestions: &[Suggestion], mapping: &Mapping) {
		let diagnostics = suggestions.iter().filter_map(|suggestion| {
			let locations = mapping.location(suggestion, world, self.source.as_ref());
			if locations.is_empty() {
				return None;
			}
			let dia = Diagnostic {
				locations,
				message: suggestion.message.clone(),
				replacements: suggestion.replacements.clone(),
				rule_description: suggestion.rule_description.clone(),
				rule_id: suggestion.rule_id.clone(),
			};
			Some(dia)
		});
		self.diagnostics.extend(diagnostics)
	}

	pub fn finish(self) -> Vec<Diagnostic> {
		self.diagnostics
	}
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
	pub locations: Vec<(FileId, Range<usize>)>,
	pub message: String,
	pub replacements: Vec<String>,
	pub rule_description: String,
	pub rule_id: String,
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

const DEFAULT_CHUNK_SIZE: usize = 1000;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LanguageToolOptions {
	/// Project Root
	pub root: Option<PathBuf>,
	/// Project Main File
	pub main: Option<PathBuf>,
	/// Size for chunk send to LanguageTool
	pub chunk_size: usize,

	#[serde(flatten)]
	pub backend: BackendOptions,

	/// map for short to long language codes (`en -> en-US`)
	pub languages: HashMap<String, String>,
	/// Additional allowed words
	pub dictionary: HashMap<String, Vec<String>>,
	/// Languagetool rules to ignore (WHITESPACE_RULE, ...)
	pub disabled_checks: HashMap<String, Vec<String>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "backend")]
pub enum BackendOptions {
	#[serde(rename = "none")]
	None,
	#[serde(rename = "bundle")]
	Bundle,
	#[serde(rename = "jar")]
	Jar { jar_location: String },
	#[serde(rename = "server")]
	Remote { host: String, port: String },
}

impl Default for LanguageToolOptions {
	fn default() -> Self {
		Self {
			root: None,
			main: None,
			chunk_size: DEFAULT_CHUNK_SIZE,

			backend: BackendOptions::None,

			languages: HashMap::new(),
			dictionary: HashMap::new(),
			disabled_checks: HashMap::new(),
		}
	}
}

impl LanguageToolOptions {
	pub fn overwrite(mut self, other: Self) -> Self {
		self.dictionary.extend(other.dictionary);
		self.disabled_checks.extend(other.disabled_checks);
		self.languages.extend(other.languages);

		Self {
			root: other.root.or(self.root),
			main: other.main.or(self.main),

			chunk_size: (other.chunk_size != DEFAULT_CHUNK_SIZE)
				.then_some(other.chunk_size)
				.unwrap_or(self.chunk_size),

			backend: (other.backend != BackendOptions::None)
				.then_some(other.backend)
				.unwrap_or(self.backend),

			languages: self.languages,
			dictionary: self.dictionary,
			disabled_checks: self.disabled_checks,
		}
	}
}

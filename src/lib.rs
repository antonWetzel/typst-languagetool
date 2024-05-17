mod backends;
pub mod convert;

use std::ops::{Not, Range};

pub use backends::*;
use convert::Mapping;
use typst::{
	syntax::{FileId, Source},
	World,
};

#[allow(async_fn_in_trait)]
pub trait LanguageToolBackend {
	async fn allow_words(&mut self, lang: String, words: &[String]) -> anyhow::Result<()>;
	async fn disable_checks(&mut self, lang: String, checks: &[String]) -> anyhow::Result<()>;
	async fn check_text(&mut self, lang: String, text: &str) -> anyhow::Result<Vec<Suggestion>>;
}

#[derive(Debug)]
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
	) -> anyhow::Result<Self> {
		let lt = match (bundled, jar_location, host, port) {
			#[cfg(feature = "remote-server")]
			(false, None, Some(host), Some(port)) => {
				Self::Remote(remote::LanguageToolRemote::new(host, port)?)
			},
			#[cfg(not(feature = "remote-server"))]
			(false, None, Some(_), Some(_)) => Err(anyhow::anyhow!("Feature 'remote-server' is disabled."))?,

			#[cfg(feature = "bundle-jar")]
			(true, None, None, None) => Self::JNI(jni::LanguageToolJNI::new_bundled()?),

			#[cfg(not(feature = "bundle-jar"))]
			(true, None, None, None) => Err(anyhow::anyhow!("Feature 'bundle-jar' is disabled."))?,

			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			(false, Some(path), None, None) => Self::JNI(jni::LanguageToolJNI::new(path)?),
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
}

impl LanguageToolBackend for LanguageTool {
	async fn allow_words(&mut self, lang: String, words: &[String]) -> anyhow::Result<()> {
		match self {
			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			Self::JNI(lt) => lt.allow_words(lang, words).await,
			#[cfg(feature = "remote-server")]
			Self::Remote(lt) => lt.allow_words(lang, words).await,
		}
	}
	async fn disable_checks(&mut self, lang: String, checks: &[String]) -> anyhow::Result<()> {
		match self {
			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			Self::JNI(lt) => lt.disable_checks(lang, checks).await,
			#[cfg(feature = "remote-server")]
			Self::Remote(lt) => lt.disable_checks(lang, checks).await,
		}
	}
	async fn check_text(&mut self, lang: String, text: &str) -> anyhow::Result<Vec<Suggestion>> {
		match self {
			#[cfg(any(feature = "bundle-jar", feature = "extern-jar"))]
			Self::JNI(lt) => lt.check_text(lang, text).await,
			#[cfg(feature = "remote-server")]
			Self::Remote(lt) => lt.check_text(lang, text).await,
		}
	}
}

pub struct FileCollector {
	source: Source,
	diagnostics: Vec<Diagnostic>,
}

impl FileCollector {
	pub fn new(file_id: FileId, world: &impl World) -> Self {
		let source = world.source(file_id).unwrap();
		Self { source, diagnostics: Vec::new() }
	}

	pub fn add(&mut self, suggestions: &[Suggestion], mapping: Mapping) {
		let diagnostics = suggestions
			.iter()
			.map(|suggestion| {
				let locations = mapping.location(suggestion, &self.source);
				Diagnostic {
					locations,
					message: suggestion.message.clone(),
					replacements: suggestion.replacements.clone(),
					rule_description: suggestion.rule_description.clone(),
					rule_id: suggestion.rule_id.clone(),
				}
			})
			.filter(|diagnostic| diagnostic.locations.is_empty().not());
		self.diagnostics.extend(diagnostics)
	}

	pub fn finish(self) -> (Source, Vec<Diagnostic>) {
		(self.source, self.diagnostics)
	}
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
	pub locations: Vec<Range<usize>>,
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

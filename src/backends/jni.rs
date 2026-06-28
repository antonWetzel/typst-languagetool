use std::{
	collections::{HashMap, hash_map::Entry},
	ops::Not,
};

use jni::{
	Env, InitArgsBuilder, JavaVM, jni_sig, jni_str,
	objects::{JList, JObject, JString, JValue},
	refs::Global,
};

use crate::{LanguageToolBackend, Suggestion};

#[derive(Debug)]
pub struct LanguageToolJNI {
	jvm: JavaVM,
	data: Data,
}

macro_rules! jni_for {
    (for $p:pat in ($val:ident, $env:ident) $body:block) => {{
		let __iter = $val.iter($env)?;
		while let Some($p) = __iter.next($env)? $body
	}};
}

#[derive(Debug)]
struct Data {
	languages: HashMap<String, Global<JObject<'static>>>,
}

fn new_jvm(class_path: &str) -> anyhow::Result<JavaVM> {
	let jvm_args = InitArgsBuilder::new()
		.version(jni::JNIVersion::V1_8)
		.option(format!("-Djava.class.path={}", class_path))
		.option("--enable-native-access=ALL-UNNAMED")
		.build()?;
	let jvm = JavaVM::new(jvm_args)?;
	Ok(jvm)
}

impl LanguageToolJNI {
	pub fn new(class_path: &str) -> anyhow::Result<Self> {
		let jvm = new_jvm(class_path)?;
		Ok(Self {
			jvm,
			data: Data { languages: HashMap::new() },
		})
	}

	#[cfg(feature = "bundle")]
	pub fn new_bundled() -> anyhow::Result<Self> {
		let path = include!(concat!(env!("OUT_DIR"), "/jar_path.rs"));

		let jvm = new_jvm(path)?;
		Ok(Self {
			jvm,
			data: Data { languages: HashMap::new() },
		})
	}
}

impl LanguageToolBackend for LanguageToolJNI {
	async fn check_text(&mut self, lang: String, text: &str) -> anyhow::Result<Vec<Suggestion>> {
		self.jvm
			.attach_current_thread(|env| self.data.check_text(env, lang, text))
	}

	async fn allow_words(&mut self, lang: String, words: &[String]) -> anyhow::Result<()> {
		self.jvm
			.attach_current_thread(|env| self.data.allow_words(env, lang, words))
	}

	async fn disable_checks(&mut self, lang: String, checks: &[String]) -> anyhow::Result<()> {
		self.jvm
			.attach_current_thread(|env| self.data.disable_checks(env, lang, checks))
	}
}

impl Data {
	fn create_lang_tool(lang: String, env: &mut Env) -> anyhow::Result<Global<JObject<'static>>> {
		let lang_code = env.new_string(lang)?;
		let lang = env.call_static_method(
			jni_str!("org/languagetool/Languages"),
			jni_str!("getLanguageForShortCode"),
			jni_sig!((JString) -> org.languagetool.Language),
			&[JValue::Object(&lang_code)],
		)?;

		let lang_tool = env.new_object(
			jni_str!("org/languagetool/JLanguageTool"),
			jni_sig!((org.languagetool.Language)),
			&[lang.borrow()],
		)?;
		let lang_tool = env.new_global_ref(lang_tool)?;

		Ok(lang_tool)
	}

	fn allow_words(
		&mut self,
		env: &mut Env<'_>,
		lang: String,
		words: &[String],
	) -> anyhow::Result<()> {
		let lang_tool = match self.languages.entry(lang.clone()) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => entry.insert(Self::create_lang_tool(lang, env)?),
		};

		let rules = env
			.call_method(
				lang_tool,
				jni_str!("getAllActiveRules"),
				jni_sig!(() -> JList),
				&[],
			)?
			.into_object()?;
		let list = env.cast_local::<JList>(rules)?;
		let args = env.new_object(jni_str!("java/util/ArrayList"), jni_sig!(()), &[])?;
		let args = env.cast_local::<JList>(args)?;
		for word in words {
			let word = env.new_string(word)?;
			args.add(env, &word)?;
		}

		jni_for!(for rule in (list, env) {
			if env
				.is_instance_of(
					&rule,
					jni_str!("org/languagetool/rules/spelling/SpellingCheckRule"),
				)?
				.not()
			{
				continue;
			}

			env.call_method(
				&rule,
				jni_str!("acceptPhrases"),
				jni_sig!((JList)),
				&[JValue::Object(args.as_ref())],
			)?;
		});
		Ok(())
	}

	fn disable_checks(
		&mut self,
		env: &mut Env,
		lang: String,
		checks: &[String],
	) -> anyhow::Result<()> {
		let args = env.new_object(jni_str!("java/util/ArrayList"), jni_sig!(()), &[])?;
		let args = env.cast_local::<JList>(args)?;
		for check in checks {
			let check = env.new_string(check)?;
			args.add(env, &check)?;
		}
		let lang_tool = match self.languages.entry(lang.clone()) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => entry.insert(Self::create_lang_tool(lang, env)?),
		};
		env.call_method(
			lang_tool,
			jni_str!("disableRules"),
			jni_sig!((JList)),
			&[JValue::Object(args.as_ref())],
		)?;
		Ok(())
	}

	fn check_text(
		&mut self,
		env: &mut Env,
		lang: String,
		text: &str,
	) -> anyhow::Result<Vec<Suggestion>> {
		let text = env.new_string(text)?;
		let lang_tool = match self.languages.entry(lang.clone()) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => entry.insert(Self::create_lang_tool(lang, env)?),
		};
		let suggestions = Self::lt_request(lang_tool, &text, env)?;
		Ok(suggestions)
	}

	fn lt_request<'a>(
		lang_tool: &JObject<'a>,
		text: &JObject<'a>,
		env: &mut Env<'a>,
	) -> anyhow::Result<Vec<Suggestion>> {
		let matches = env
			.call_method(
				lang_tool,
				jni_str!("check"),
				jni_sig!((JString) -> JList),
				&[JValue::Object(text)],
			)?
			.into_object()?;

		let list = env.cast_local::<JList>(matches)?;
		let size = list.size(env)?;

		let mut suggestions = Vec::with_capacity(size as usize);

		jni_for!(for m in (list, env) {
			let start = env
				.call_method(&m, jni_str!("getFromPos"), jni_sig!(() -> i32), &[])?
				.into_int()?;
			let end = env
				.call_method(&m, jni_str!("getToPos"), jni_sig!(() -> i32), &[])?
				.into_int()?;

			let message = env
				.call_method(&m, jni_str!("getMessage"), jni_sig!(() -> JString), &[])?
				.into_object()?;
			let message = env.cast_local::<JString>(message)?.to_string();

			let replacements = env
				.call_method(
					&m,
					jni_str!("getSuggestedReplacements"),
					jni_sig!(() -> JList),
					&[],
				)?
				.into_object()?;
			let list = env.cast_local::<JList>(replacements)?;
			let size = list.size(env)?;
			let mut replacements = Vec::with_capacity(size as usize);

			jni_for!(for replacement in (list, env) {
				let replacement = env.cast_local::<JString>(replacement)?.to_string();
				replacements.push(replacement);
			});

			let rule = env
				.call_method(
					&m,
					jni_str!("getRule"),
					jni_sig!(() -> org.languagetool.rules.Rule),
					&[],
				)?
				.into_object()?;
			let rule_id = env
				.call_method(&rule, jni_str!("getId"), jni_sig!(() -> JString), &[])?
				.into_object()?;
			let rule_id = env.cast_local::<JString>(rule_id)?.to_string();
			let rule_description = env
				.call_method(
					&rule,
					jni_str!("getDescription"),
					jni_sig!(() -> JString),
					&[],
				)?
				.into_object()?;
			let rule_description = env.cast_local::<JString>(rule_description)?.to_string();

			let suggestion = Suggestion {
				start: start as usize,
				end: end as usize,
				replacements,
				message,
				rule_id,
				rule_description,
			};
			suggestions.push(suggestion);
		});
		Ok(suggestions)
	}
}

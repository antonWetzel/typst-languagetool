use std::{
	collections::{HashMap, hash_map::Entry},
	ops::Not,
};

use jni::{
	InitArgsBuilder, JNIEnv, JavaVM,
	objects::{GlobalRef, JObject, JValue},
};

use crate::{LanguageToolBackend, Suggestion};

#[derive(Debug)]
pub struct LanguageToolJNI {
	jvm: JavaVM,
	languages: HashMap<String, GlobalRef>,
}

fn new_jvm(class_path: &str) -> anyhow::Result<JavaVM> {
	let jvm_args = InitArgsBuilder::new()
		.version(jni::JNIVersion::V8)
		.option(format!("-Djava.class.path={}", class_path))
		.build()?;
	let jvm = JavaVM::new(jvm_args)?;
	Ok(jvm)
}

impl LanguageToolJNI {
	pub fn new(class_path: &str) -> anyhow::Result<Self> {
		let jvm = new_jvm(class_path)?;
		Ok(Self { languages: HashMap::new(), jvm })
	}

	pub fn new_bundled() -> anyhow::Result<Self> {
		#[cfg(feature = "bundle")]
		let path = include!(concat!(env!("OUT_DIR"), "/jar_path.rs"));

		#[cfg(not(feature = "bundle"))]
		let path = Err(anyhow::anyhow!("Feature 'bundle-jar' not enabled."))?;

		let jvm = new_jvm(path)?;
		Ok(Self { languages: HashMap::new(), jvm })
	}

	fn create_lang_tool(lang: String, env: &mut JNIEnv) -> anyhow::Result<GlobalRef> {
		let lang_code = env.new_string(lang)?;
		let lang = env.call_static_method(
			"org/languagetool/Languages",
			"getLanguageForShortCode",
			"(Ljava/lang/String;)Lorg/languagetool/Language;",
			&[JValue::Object(&lang_code)],
		)?;

		let lang_tool = env.new_object(
			"org/languagetool/JLanguageTool",
			"(Lorg/languagetool/Language;)V",
			&[lang.borrow()],
		)?;
		let lang_tool = env.new_global_ref(lang_tool)?;

		Ok(lang_tool)
	}

	fn lt_request<'a>(
		lang_tool: &JObject<'a>,
		text: &JObject<'a>,
		env: &mut JNIEnv<'a>,
	) -> anyhow::Result<Vec<Suggestion>> {
		let matches = env
			.call_method(
				lang_tool,
				"check",
				"(Ljava/lang/String;)Ljava/util/List;",
				&[JValue::Object(text)],
			)?
			.l()?;

		let list = env.get_list(&matches)?;
		let size = list.size(env)?;

		let mut suggestions = Vec::with_capacity(size as usize);

		for i in 0..size {
			let Some(m) = list.get(env, i)? else {
				continue;
			};
			let start = env.call_method(&m, "getFromPos", "()I", &[])?.i()?;
			let end = env.call_method(&m, "getToPos", "()I", &[])?.i()?;

			let message = env
				.call_method(&m, "getMessage", "()Ljava/lang/String;", &[])?
				.l()?;
			let message = env.get_string(&message.into())?.into();

			let replacements = env
				.call_method(&m, "getSuggestedReplacements", "()Ljava/util/List;", &[])?
				.l()?;
			let list = env.get_list(&replacements)?;
			let size = list.size(env)?;
			let mut replacements = Vec::with_capacity(size as usize);
			for i in 0..size {
				let Some(replacement) = list.get(env, i)? else {
					continue;
				};
				let replacement = env.get_string(&replacement.into())?.into();
				replacements.push(replacement);
			}

			let rule = env
				.call_method(&m, "getRule", "()Lorg/languagetool/rules/Rule;", &[])?
				.l()?;
			let rule_id = env
				.call_method(&rule, "getId", "()Ljava/lang/String;", &[])?
				.l()?;
			let rule_id = env.get_string(&rule_id.into())?.into();
			let rule_description = env
				.call_method(&rule, "getDescription", "()Ljava/lang/String;", &[])?
				.l()?;
			let rule_description = env.get_string(&rule_description.into())?.into();

			let suggestion = Suggestion {
				start: start as usize,
				end: end as usize,
				replacements,
				message,
				rule_id,
				rule_description,
			};
			suggestions.push(suggestion);
		}
		Ok(suggestions)
	}
}

impl LanguageToolBackend for LanguageToolJNI {
	async fn check_text(&mut self, lang: String, text: &str) -> anyhow::Result<Vec<Suggestion>> {
		let mut guard = self.jvm.attach_current_thread()?;
		let text = guard.new_string(text)?;
		let lang_tool = match self.languages.entry(lang.clone()) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => entry.insert(Self::create_lang_tool(lang, &mut guard)?),
		};
		let suggestions = Self::lt_request(lang_tool, &text, &mut guard)?;
		Ok(suggestions)
	}

	async fn allow_words(&mut self, lang: String, words: &[String]) -> anyhow::Result<()> {
		let mut guard = self.jvm.attach_current_thread()?;
		let lang_tool = match self.languages.entry(lang.clone()) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => entry.insert(Self::create_lang_tool(lang, &mut guard)?),
		};

		let rules = guard
			.call_method(lang_tool, "getAllActiveRules", "()Ljava/util/List;", &[])?
			.l()?;
		let list = guard.get_list(&rules)?;
		let args = guard.new_object("java/util/ArrayList", "()V", &[])?;
		let args = guard.get_list(&args)?;
		for word in words {
			let word = guard.new_string(word)?;
			args.add(&mut guard, &word)?;
		}

		for i in 0..list.size(&mut guard)? {
			let Some(rule) = list.get(&mut guard, i)? else {
				continue;
			};
			if guard
				.is_instance_of(&rule, "org/languagetool/rules/spelling/SpellingCheckRule")?
				.not()
			{
				continue;
			}

			guard.call_method(
				&rule,
				"acceptPhrases",
				"(Ljava/util/List;)V",
				&[JValue::Object(args.as_ref())],
			)?;
		}
		Ok(())
	}

	async fn disable_checks(&mut self, lang: String, checks: &[String]) -> anyhow::Result<()> {
		let mut guard = self.jvm.attach_current_thread()?;
		let args = guard.new_object("java/util/ArrayList", "()V", &[])?;
		let args = guard.get_list(&args)?;
		for check in checks {
			let check = guard.new_string(check)?;
			args.add(&mut guard, &check)?;
		}
		let lang_tool = match self.languages.entry(lang.clone()) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => entry.insert(Self::create_lang_tool(lang, &mut guard)?),
		};
		guard.call_method(
			lang_tool,
			"disableRules",
			"(Ljava/util/List;)V",
			&[JValue::Object(args.as_ref())],
		)?;
		Ok(())
	}
}

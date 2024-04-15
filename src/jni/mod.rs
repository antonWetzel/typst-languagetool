mod text_builder;

pub use text_builder::TextBuilderJNI;

use std::ops::Not;

use jni::{
	objects::{GlobalRef, JObject, JValue, JValueGen},
	InitArgsBuilder, JNIEnv, JavaVM,
};

use crate::{convert::convert, LanguageTool, Rules, Suggestion};

pub struct LanguageToolJNI {
	jvm: JavaVM,
	lang_tool: GlobalRef,
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
	pub fn new(class_path: &str, lang: &str) -> anyhow::Result<Self> {
		let jvm = new_jvm(class_path)?;
		let lang_tool = Self::create_lang_tool(lang, &jvm)?;
		Ok(Self { lang_tool, jvm })
	}

	pub fn new_bundled(lang: &str) -> anyhow::Result<Self> {
		#[cfg(feature = "bundle-jar")]
		let path = include!(concat!(env!("OUT_DIR"), "./jar_path.rs"));

		#[cfg(not(feature = "bundle-jar"))]
		let path = Err(anyhow::anyhow!("Feature 'bundle-jar' not enabled."))?;

		let jvm = new_jvm(path)?;
		let lang_tool = Self::create_lang_tool(lang, &jvm)?;
		Ok(Self { lang_tool, jvm })
	}

	fn create_lang_tool(lang: &str, jvm: &JavaVM) -> anyhow::Result<GlobalRef> {
		let lang_tool = {
			let mut guard = jvm.attach_current_thread()?;
			let lang_code = guard.new_string(lang)?;
			let lang = guard.call_static_method(
				"org/languagetool/Languages",
				"getLanguageForShortCode",
				"(Ljava/lang/String;)Lorg/languagetool/Language;",
				&[JValue::Object(&lang_code)],
			)?;

			let lang_tool = guard.new_object(
				"org/languagetool/JLanguageTool",
				"(Lorg/languagetool/Language;)V",
				&[lang.borrow()],
			)?;
			guard.new_global_ref(lang_tool)?
		};

		Ok(lang_tool)
	}

	fn lt_request<'a>(
		lang_tool: &JObject<'a>,
		text: JValueGen<JObject<'a>>,
		env: &mut JNIEnv<'a>,
	) -> anyhow::Result<Vec<Suggestion>> {
		let matches = env
			.call_method(
				lang_tool,
				"check",
				"(Lorg/languagetool/markup/AnnotatedText;)Ljava/util/List;",
				&[text.borrow()],
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

impl LanguageTool for LanguageToolJNI {
	fn change_language(&mut self, lang: &str) -> anyhow::Result<()> {
		self.lang_tool = Self::create_lang_tool(lang, &self.jvm)?;
		Ok(())
	}

	fn check_source(&self, text: &str, rules: &Rules) -> anyhow::Result<Vec<Suggestion>> {
		let root = typst::syntax::parse(text);
		let mut guard = self.jvm.attach_current_thread()?;
		let mut text_builder = TextBuilderJNI::new(&mut guard)?;
		convert(&root, rules, &mut text_builder)?;
		let text = text_builder.finish()?;
		let suggestions = Self::lt_request(&self.lang_tool, text, &mut guard)?;
		Ok(suggestions)
	}

	fn allow_words(&mut self, words: &[String]) -> anyhow::Result<()> {
		let mut guard = self.jvm.attach_current_thread()?;
		let rules = guard
			.call_method(
				&self.lang_tool,
				"getAllActiveRules",
				"()Ljava/util/List;",
				&[],
			)?
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

	fn disable_checks(&mut self, checks: &[String]) -> anyhow::Result<()> {
		let mut guard = self.jvm.attach_current_thread()?;
		let args = guard.new_object("java/util/ArrayList", "()V", &[])?;
		let args = guard.get_list(&args)?;
		for check in checks {
			let check = guard.new_string(check)?;
			args.add(&mut guard, &check)?;
		}

		guard.call_method(
			&self.lang_tool,
			"disableRules",
			"(Ljava/util/List;)V",
			&[JValue::Object(args.as_ref())],
		)?;
		Ok(())
	}
}

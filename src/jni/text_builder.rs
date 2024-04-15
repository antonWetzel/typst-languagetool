use jni::{
	objects::{JObject, JValue, JValueGen},
	JNIEnv,
};

use crate::convert::TextBuilder;

pub struct TextBuilderJNI<'a, 'b> {
	text_builder: JObject<'a>,
	env: &'b mut JNIEnv<'a>,
}

impl<'a, 'b> TextBuilderJNI<'a, 'b> {
	pub fn new(env: &'b mut JNIEnv<'a>) -> anyhow::Result<Self> {
		let text_builder =
			env.new_object("org/languagetool/markup/AnnotatedTextBuilder", "()V", &[])?;
		Ok(TextBuilderJNI { text_builder, env })
	}

	pub fn finish(self) -> anyhow::Result<JValueGen<JObject<'a>>> {
		let annotated_text = self.env.call_method(
			&self.text_builder,
			"build",
			"()Lorg/languagetool/markup/AnnotatedText;",
			&[],
		)?;
		Ok(annotated_text)
	}
}

impl TextBuilder for TextBuilderJNI<'_, '_> {
	fn add_text(&mut self, text: &str) -> anyhow::Result<()> {
		let text = self.env.new_string(text)?;
		self.env.call_method(
			&self.text_builder,
			"addText",
			"(Ljava/lang/String;)Lorg/languagetool/markup/AnnotatedTextBuilder;",
			&[JValue::Object(&text)],
		)?;
		Ok(())
	}

	fn add_markup(&mut self, markup: &str) -> anyhow::Result<()> {
		let markup = self.env.new_string(markup)?;
		self.env.call_method(
			&self.text_builder,
			"addMarkup",
			"(Ljava/lang/String;)Lorg/languagetool/markup/AnnotatedTextBuilder;",
			&[JValue::Object(&markup)],
		)?;
		Ok(())
	}

	fn add_encoded(&mut self, markup: &str, text: &str) -> anyhow::Result<()> {
		let markup = self.env.new_string(markup)?;
		let text = self.env.new_string(text)?;
		self.env.call_method(
			&self.text_builder,
			"addMarkup",
			"(Ljava/lang/String;Ljava/lang/String;)Lorg/languagetool/markup/AnnotatedTextBuilder;",
			&[JValue::Object(&markup), JValue::Object(&text)],
		)?;
		Ok(())
	}
}

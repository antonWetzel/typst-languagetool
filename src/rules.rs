use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Rules {
	pub functions: HashMap<String, Function>,
	pub arguments: HashMap<String, Argument>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Function {
	pub before: String,
	pub after: String,
	pub after_argument: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Argument {
	pub before: String,
	pub after: String,
}

impl Rules {
	pub fn new() -> Self {
		Self {
			functions: [
				(
					"grid".into(),
					Function {
						before: String::new(),
						after: String::new(),
						after_argument: "\n".into(),
					},
				),
				(
					"table".into(),
					Function {
						before: String::new(),
						after: String::new(),
						after_argument: "\n".into(),
					},
				),
				(
					"header".into(),
					Function {
						before: String::new(),
						after: String::new(),
						after_argument: "\n".into(),
					},
				),
				(
					"cell".into(),
					Function {
						before: "\n".into(),
						after: "\n".into(),
						after_argument: String::new(),
					},
				),
			]
			.into_iter()
			.collect(),
			arguments: [(
				"caption".into(),
				Argument {
					before: "\n\n".into(),
					after: "\n\n".into(),
				},
			)]
			.into_iter()
			.collect(),
		}
	}
}

impl Default for Rules {
	fn default() -> Self {
		Self::new()
	}
}

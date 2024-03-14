use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Rules {
	pub functions: HashMap<String, Replacement>,
	pub arguments: HashMap<String, Replacement>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Replacement {
	pub before: String,
	pub after: String,
}

impl Rules {
	pub fn new() -> Self {
		Self {
			functions: HashMap::new(),
			arguments: [(
				"caption".into(),
				Replacement {
					before: "\n\n".into(),
					after: "\n\n".into(),
				},
			)]
			.into_iter()
			.collect(),
		}
	}
}

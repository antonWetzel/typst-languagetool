use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Rules {
	pub functions: HashMap<String, Function>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Function {
	pub before: String,
	pub after: String,
}

impl Rules {
	pub fn new() -> Self {
		Self { functions: HashMap::new() }
	}
}

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fs::File, io::BufReader};

#[derive(Serialize, Deserialize)]
pub struct Rules {
	pub functions: HashMap<String, Function>,
}

#[derive(Serialize, Deserialize)]
pub struct Function {
	pub before: String,
	pub after: String,
}

impl Rules {
	pub fn new() -> Self {
		Self { functions: HashMap::new() }
	}

	pub fn load(path: &String) -> Result<Self, Box<dyn Error>> {
		let file = File::open(path)?;
		let reader = BufReader::new(file);
		let rules = serde_json::from_reader(reader)?;
		Ok(rules)
	}
}

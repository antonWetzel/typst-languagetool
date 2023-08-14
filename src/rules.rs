use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fs::File, io::BufReader};

#[derive(Serialize, Deserialize)]
pub struct Rules {
	pub functions: HashMap<String, Function>,
}

#[derive(Serialize, Deserialize)]
pub struct Function {
	pub before: String,
	// after: String, // requires recursive convert
}

impl Rules {
	pub fn new() -> Self {
		Self { functions: HashMap::new() }
	}

	pub fn load(path: &String) -> Result<Self, Box<dyn Error>> {
		let file = File::open(path)?;
		let reader = BufReader::new(file);

		// Read the JSON contents of the file as an instance of `User`.
		let rules = serde_json::from_reader(reader)?;

		// Return the `User`.
		Ok(rules)
	}
}

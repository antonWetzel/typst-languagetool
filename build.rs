use std::env;
use std::io::Write;
use std::path::Path;

fn main() {
	println!("cargo::rerun-if-changed=build.rs");

	println!("cargo::rerun-if-changed=pom.xml");
	let command = if cfg!(target_os = "windows") {
		"mvn.cmd"
	} else {
		"mvn" // I hope
	};
	let output = std::process::Command::new(command)
		.arg("dependency:build-classpath")
		.output()
		.unwrap();

	let output = String::from_utf8(output.stdout).unwrap();
	let mut lines = output.lines();
	lines
		.find(|line| line.contains("Dependencies classpath:"))
		.unwrap();

	let path = lines.next().unwrap();

	let out_dir = env::var_os("OUT_DIR").unwrap();
	let dest_path = Path::new(&out_dir).join("class_path.rs");
	let mut dest = std::fs::File::create(dest_path).unwrap();
	dest.write_all(b"const CLASS_PATH: &str = r###\"").unwrap();
	dest.write_all(path.as_bytes()).unwrap();
	dest.write_all(b"\"###;").unwrap();
}

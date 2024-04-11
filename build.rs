use std::ops::Not;

use j4rs::{JvmBuilder, MavenArtifact};

fn main() {
	println!("cargo::rerun-if-changed=build.rs");

	println!("cargo::rerun-if-changed=pom.xml");
	let command = if cfg!(target_os = "windows") {
		"mvn.cmd"
	} else {
		"mvn" // I hope
	};
	let output = std::process::Command::new(command)
		.arg("dependency:list")
		.arg("-DoutputFile=class_path.txt")
		.output()
		.unwrap();
	println!("{:?}", String::from_utf8(output.stdout));
	if output.status.success().not() {
		panic!("Maven failed");
	}

	let content = std::fs::read_to_string("./class_path.txt").unwrap();
	std::fs::remove_file("./class_path.txt").unwrap();

	let jvm = JvmBuilder::new().build().unwrap();
	for line in content.lines() {
		let Some((id, _)) = line.split_once(":compile") else {
			continue;
		};
		let id = id.trim().replace(":jar", "");
		jvm.deploy_artifact(&MavenArtifact::from(id.trim()))
			.unwrap();
	}
}

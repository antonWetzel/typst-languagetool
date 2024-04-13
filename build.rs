fn main() {
	println!("cargo::rerun-if-changed=build.rs");

	#[cfg(feature = "bundle-jar")]
	{
		use std::env;
		use std::io::Write;
		use std::ops::Not;
		use std::path::Path;

		println!("cargo::rerun-if-changed=maven/pom.xml");
		println!("cargo::rerun-if-changed=maven/src/assembly/dep.xml");
		let command = if cfg!(target_os = "windows") {
			"mvn.cmd"
		} else {
			"mvn"
		};
		let output = std::process::Command::new(command)
			.current_dir(std::env::current_dir().unwrap().join("maven"))
			.arg("clean")
			.arg("install")
			.output()
			.unwrap();
		let text = String::from_utf8(output.stdout).unwrap();
		if output.status.success().not() {
			panic!("{}", text);
		}
		let location = text
			.lines()
			.rev()
			.find_map(|line| {
				if line.contains("Installing").not() {
					return None;
				}
				let (_, target) = line.split_once(" to ")?;
				Some(target)
			})
			.unwrap();
		println!("cargo::warning=JAR at {:?}.", location);
		let out_dir = env::var("OUT_DIR").unwrap();
		let dest_path = Path::new(&out_dir).join("jar_path.rs");
		let mut f = std::fs::File::create(&dest_path).unwrap();
		f.write_all(format!("r###\"{}\"###", location).as_bytes())
			.unwrap();
	}
}

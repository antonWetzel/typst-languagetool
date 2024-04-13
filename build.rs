fn main() {
	println!("cargo::rerun-if-changed=build.rs");

	#[cfg(feature = "bundle-jar")]
	{
		use std::env;
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
			.arg("package")
			.current_dir(std::env::current_dir().unwrap().join("maven"))
			.output()
			.unwrap();
		if output.status.success().not() {
			panic!("{}", String::from_utf8(output.stdout).unwrap());
		}

		let out_dir = env::var_os("OUT_DIR").unwrap();
		let dest_path = Path::new(&out_dir).join("languagetool.jar");
		let _ = std::fs::remove_file(&dest_path);
		println!("cargo::warning=Creatin JAR at {}.", dest_path.display());
		std::fs::copy("./maven/target/no-1-jar-with-dependencies.jar", dest_path).unwrap();
	}
}

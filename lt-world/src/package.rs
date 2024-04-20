use std::path::PathBuf;

use typst::diag::{PackageError, PackageResult};
use typst::syntax::package::PackageSpec;

/// Make a package available in the on-disk cache.
/// Just hope the preview packages are already donwloaded.
pub fn prepare_package(spec: &PackageSpec) -> PackageResult<PathBuf> {
	let subdir = format!(
		"typst/packages/{}/{}/{}",
		spec.namespace, spec.name, spec.version
	);

	if let Some(data_dir) = dirs::data_dir() {
		let dir = data_dir.join(&subdir);
		if dir.exists() {
			return Ok(dir);
		}
	}

	if let Some(cache_dir) = dirs::cache_dir() {
		let dir = cache_dir.join(&subdir);
		if dir.exists() {
			return Ok(dir);
		}
	}

	Err(PackageError::NotFound(spec.clone()))
}

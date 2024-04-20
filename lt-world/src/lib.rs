mod fonts;
mod package;

use std::{
	collections::HashMap,
	path::{Path, PathBuf},
};

use chrono::{DateTime, Datelike, FixedOffset, Local, Utc};
use comemo::Prehashed;
use fonts::FontManager;
use typst::{
	diag::{FileError, FileResult},
	eval::Tracer,
	foundations::{Dict, Value},
	model::Document,
	syntax::{FileId, Source, VirtualPath},
	text::Font,
	Library, World,
};

#[derive(Debug)]
pub struct LtWorld {
	library: Prehashed<Library>,
	now: DateTime<Utc>,
	main: FileId,
	root: PathBuf,
	font_manager: FontManager,
	shadow_files: HashMap<FileId, Source>,
}

impl LtWorld {
	pub fn new(main: PathBuf, root: Option<PathBuf>) -> Self {
		let root = if let Some(root) = root {
			root
		} else {
			main.parent().unwrap().to_path_buf()
		};

		let main = main.strip_prefix(&root).unwrap();
		let main = FileId::new(None, VirtualPath::new(main));

		let mut inputs = Dict::new();
		inputs.insert("spellcheck".into(), Value::Bool(true));

		Self {
			library: Prehashed::new(Library::builder().with_inputs(inputs).build()),
			now: chrono::Utc::now(),
			font_manager: FontManager::new(),
			main,
			root,
			shadow_files: HashMap::new(),
		}
	}

	pub fn root(&self) -> &Path {
		&self.root
	}

	pub fn file_id(&self, path: &Path) -> FileId {
		let path = path.strip_prefix(&self.root).unwrap();
		FileId::new(None, VirtualPath::new(path))
	}

	pub fn path(&self, file_id: FileId) -> typst::diag::FileResult<PathBuf> {
		let path = if let Some(spec) = file_id.package() {
			crate::package::prepare_package(spec)?.join(file_id.vpath().as_rootless_path())
		} else {
			self.root.join(file_id.vpath().as_rootless_path())
		};

		Ok(path)
	}

	pub fn use_shadow_file(&mut self, path: &Path, text: String) {
		let file_id = self.file_id(path);
		self.shadow_files
			.insert(file_id, Source::new(file_id, text));
	}

	pub fn shadow_file(&mut self, path: &Path) -> Option<&mut Source> {
		let file_id = self.file_id(path);
		self.shadow_files.get_mut(&file_id)
	}

	pub fn use_original_file(&mut self, path: &Path) {
		let file_id = self.file_id(path);
		self.shadow_files.remove(&file_id);
	}

	pub fn compile(&self) -> Option<Document> {
		let mut tracer = Tracer::new();
		typst::compile(self, &mut tracer).ok()
	}
}

impl World for LtWorld {
	fn library(&self) -> &Prehashed<Library> {
		&self.library
	}

	fn today(&self, offset: Option<i64>) -> Option<typst::foundations::Datetime> {
		let with_offset = match offset {
			None => self.now.with_timezone(&Local).fixed_offset(),
			Some(hours) => {
				let seconds = i32::try_from(hours).ok()?.checked_mul(3600)?;
				self.now.with_timezone(&FixedOffset::east_opt(seconds)?)
			},
		};

		typst::foundations::Datetime::from_ymd(
			with_offset.year(),
			with_offset.month().try_into().ok()?,
			with_offset.day().try_into().ok()?,
		)
	}

	fn book(&self) -> &Prehashed<typst::text::FontBook> {
		self.font_manager.book()
	}

	fn main(&self) -> typst::syntax::Source {
		self.source(self.main).unwrap()
	}

	fn source(&self, id: FileId) -> typst::diag::FileResult<typst::syntax::Source> {
		if let Some(source) = self.shadow_files.get(&id) {
			return Ok(source.clone());
		}

		let path = self.path(id)?;

		let Ok(text) = std::fs::read_to_string(&path) else {
			return Err(FileError::NotFound(path));
		};
		Ok(Source::new(id, text))
	}

	fn file(&self, id: FileId) -> FileResult<typst::foundations::Bytes> {
		let path = self.path(id)?;

		let Ok(bytes) = std::fs::read(&path) else {
			return Err(FileError::NotFound(path));
		};
		Ok(bytes.into())
	}

	fn font(&self, index: usize) -> Option<Font> {
		self.font_manager.get(index)
	}
}

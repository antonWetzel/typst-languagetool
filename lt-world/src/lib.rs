use std::{
	collections::HashMap,
	ops::Deref,
	path::{Path, PathBuf},
};

use chrono::{DateTime, Datelike, FixedOffset, Local, Utc};
use typst::{
	Library, LibraryExt, World,
	diag::{FileError, FileResult, SourceResult},
	foundations::{Dict, Value},
	layout::PagedDocument,
	syntax::{FileId, Source, VirtualPath},
	text::Font,
	utils::LazyHash,
};
use typst_kit::{
	download::Downloader,
	fonts::{FontSlot, Fonts},
	package::PackageStorage,
};

#[derive(Debug)]
pub struct LtWorld {
	library: LazyHash<Library>,
	now: DateTime<Utc>,

	packages: PackageStorage,

	fonts: Vec<FontSlot>,
	font_book: LazyHash<typst::text::FontBook>,
	shadow_files: HashMap<FileId, Source>,
	root: PathBuf,
}

pub struct LtWorldRunning<'a> {
	world: &'a LtWorld,
	main: FileId,
}

impl LtWorld {
	pub fn new(root: PathBuf) -> Self {
		let mut inputs = Dict::new();
		inputs.insert("spellcheck".into(), Value::Bool(true));
		let root = root.canonicalize().unwrap();

		let fonts = Fonts::searcher()
			.include_embedded_fonts(true)
			.include_system_fonts(true)
			.search();

		Self {
			library: LazyHash::new(Library::builder().with_inputs(inputs).build()),
			now: chrono::Utc::now(),

			packages: PackageStorage::new(None, None, Downloader::new("typst-languagetool")),

			font_book: LazyHash::new(fonts.book),
			fonts: fonts.fonts,
			root,
			shadow_files: HashMap::new(),
		}
	}

	pub fn root(&self) -> &Path {
		&self.root
	}

	pub fn file_id(&self, path: &Path) -> Option<FileId> {
		let path = path.canonicalize().unwrap();
		let path = path.strip_prefix(&self.root).ok()?;
		let id = FileId::new(None, VirtualPath::new(path));
		Some(id)
	}

	pub fn use_shadow_file(&mut self, path: &Path, text: String) {
		let Some(file_id) = self.file_id(path) else {
			return;
		};
		self.shadow_files
			.insert(file_id, Source::new(file_id, text));
	}

	pub fn shadow_file(&mut self, path: &Path) -> Option<&mut Source> {
		let file_id = self.file_id(path)?;
		self.shadow_files.get_mut(&file_id)
	}

	pub fn use_original_file(&mut self, path: &Path) {
		let Some(file_id) = self.file_id(path) else {
			return;
		};
		self.shadow_files.remove(&file_id);
	}

	pub fn path(&self, file_id: FileId) -> typst::diag::FileResult<PathBuf> {
		let path = if let Some(spec) = file_id.package() {
			self.packages
				.prepare_package(&spec, &mut Progress)?
				.join(file_id.vpath().as_rootless_path())
		} else {
			self.root.join(file_id.vpath().as_rootless_path())
		};

		Ok(path)
	}

	pub fn with_main(&self, main: PathBuf) -> LtWorldRunning<'_> {
		let main = VirtualPath::new(
			main.canonicalize()
				.unwrap()
				.strip_prefix(&self.root)
				.unwrap(),
		);
		LtWorldRunning {
			world: &self,
			main: FileId::new(None, main),
		}
	}
}

impl Deref for LtWorldRunning<'_> {
	type Target = LtWorld;

	fn deref(&self) -> &Self::Target {
		self.world
	}
}

impl LtWorldRunning<'_> {
	pub fn compile(&self) -> SourceResult<PagedDocument> {
		typst::compile(self).output
	}
}

impl World for LtWorldRunning<'_> {
	fn library(&self) -> &LazyHash<Library> {
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

	fn book(&self) -> &LazyHash<typst::text::FontBook> {
		&self.font_book
	}

	fn main(&self) -> FileId {
		self.main
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
		Ok(typst::foundations::Bytes::new(bytes))
	}

	fn font(&self, index: usize) -> Option<Font> {
		self.fonts[index].get()
	}
}

struct Progress;

impl typst_kit::download::Progress for Progress {
	fn print_start(&mut self) {}

	fn print_progress(&mut self, _state: &typst_kit::download::DownloadState) {}

	fn print_finish(&mut self, _state: &typst_kit::download::DownloadState) {}
}

use std::{
	collections::HashMap,
	ops::Deref,
	path::{Path, PathBuf},
};

use typst::{
	Library, LibraryExt, World,
	diag::{FileError, FileResult, SourceResult},
	engine::{Route, Sink, Traced},
	foundations::{Content, Duration},
	syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
	text::Font,
	utils::LazyHash,
};
use typst_kit::{
	datetime::Time, downloader::SystemDownloader, files::FsRoot, fonts::FontStore,
	packages::SystemPackages,
};

pub struct LtWorld {
	library: LazyHash<Library>,
	now: Time,

	packages: SystemPackages,
	root: FsRoot,

	fonts: FontStore,
	shadow_files: HashMap<FileId, Source>,
}

pub struct LtWorldRunning<'a> {
	world: &'a LtWorld,
	main: FileId,
}

impl LtWorld {
	pub fn new(root: PathBuf) -> Self {
		let root = root.canonicalize().unwrap();

		let mut fonts = FontStore::new();
		fonts.extend(typst_kit::fonts::embedded());
		fonts.extend(typst_kit::fonts::system());

		Self {
			library: LazyHash::new(Library::builder().build()),
			now: Time::system(),

			packages: SystemPackages::new(SystemDownloader::new("typst-languagetool")),

			fonts,
			root: FsRoot::new(root),
			shadow_files: HashMap::new(),
		}
	}

	pub fn root(&self) -> &Path {
		self.root.path()
	}

	pub fn file_id(&self, path: &Path) -> Option<FileId> {
		let path = path.canonicalize().unwrap();
		let path = VirtualPath::virtualize(self.root.path(), &path).ok()?;
		let id = RootedPath::new(VirtualRoot::Project, path).intern();
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
		match file_id.root() {
			VirtualRoot::Package(spec) => self.packages.obtain(spec)?.resolve(file_id.vpath()),
			VirtualRoot::Project => self.root.resolve(file_id.vpath()),
		}
	}

	pub fn with_main(&self, main: PathBuf) -> LtWorldRunning<'_> {
		let main = self.file_id(&main).unwrap();
		LtWorldRunning { world: self, main }
	}
}

impl Deref for LtWorldRunning<'_> {
	type Target = LtWorld;

	fn deref(&self) -> &Self::Target {
		self.world
	}
}

impl LtWorldRunning<'_> {
	pub fn compile(&self) -> SourceResult<Content> {
		use typst::comemo::Track;

		let mut sink = Sink::new();
		let world = (self as &dyn World).track();

		let main = world.main();
		let main = world.source(main).expect("source exist");

		let content = typst_eval::eval(
			world,
			&self.library,
			Traced::default().track(),
			sink.track_mut(),
			Route::default().track(),
			&main,
		)?
		.content();

		Ok(content)
	}
}

impl World for LtWorldRunning<'_> {
	fn library(&self) -> &LazyHash<Library> {
		&self.library
	}

	fn today(&self, offset: Option<Duration>) -> Option<typst::foundations::Datetime> {
		self.now.today(offset)
	}

	fn book(&self) -> &LazyHash<typst::text::FontBook> {
		self.fonts.book()
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
		self.fonts.font(index)
	}
}

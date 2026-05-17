use std::{
	collections::HashMap,
	ops::{ControlFlow, Deref, Not},
	path::{Path, PathBuf},
};

use chrono::{DateTime, Datelike, FixedOffset, Local, Utc};
use typst::{
	Library, LibraryExt, ROUTINES, World,
	comemo::Tracked,
	diag::{FileError, FileResult, SourceResult},
	engine::{Engine, Route, Sink, Traced},
	foundations::{
		Chainable, Content, Dict, Field, IntoValue, NativeElement, SequenceElem, Set, ShowSet,
		StyleChain, StyledElem, Styles, Target, TargetElem, Value,
	},
	introspection::Introspector,
	layout::{PagedDocument, resolve::Header},
	math::EquationElem,
	model::{FigureElem, HeadingElem, ParbreakElem, TableElem},
	syntax::{FileId, Source, VirtualPath},
	text::{Font, SpaceElem, TextElem},
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

	pub fn compile_2(&self) -> SourceResult<()> {
		use typst::comemo::Track;

		let mut sink = Sink::new();
		let world = (self as &dyn World).track();

		let main = world.main();
		let main = world.source(main).expect("source exist");

		let content = typst_eval::eval(
			&ROUTINES,
			world,
			Traced::default().track(),
			sink.track_mut(),
			Route::default().track(),
			&main,
		)?
		.content();

		let mut collector = TestCollector { text: String::new() };
		collector.iter_content(&content, StyleChain::default());
		panic!("{}", collector.text);

		Ok(())
	}
}

struct TestCollector {
	text: String,
}

impl TestCollector {
	pub fn add_break(&mut self) {
		self.text += "\n\n";
	}

	pub fn iter_content(&mut self, content: &Content, style: StyleChain) {
		if let Some(styled) = content.to_packed::<StyledElem>() {
			let style = style.chain(&styled.styles);
			self.iter_content(&styled.child, style);
		} else if let Some(text) = content.to_packed::<TextElem>() {
			let lang = style.get(TextElem::lang);
			let region = style.get(TextElem::region);
			self.text += text.text.as_str();
			// println!("{:?}+{:?}: {}", lang, region, text.text);
		} else if let Some(heading) = content.to_packed::<HeadingElem>() {
			let _level = heading.resolve_level(style);
			// chunking based on this level
			self.iter_content(&heading.body, style);
		} else if let Some(sequence) = content.to_packed::<SequenceElem>() {
			for child in sequence.children.iter() {
				self.iter_content(child, style);
			}
		} else if let Some(_space) = content.to_packed::<SpaceElem>() {
			// ?
		} else if let Some(_parbreak) = content.to_packed::<ParbreakElem>() {
			self.add_break();
		} else if let Some(figure) = content.to_packed::<FigureElem>() {
			if let Some(caption) = figure.caption.get_ref(style) {
				self.iter_content(&caption.body, style);
			}
			self.iter_content(&figure.body, style);
		} else if let Some(_equation) = content.to_packed::<EquationElem>() {
			// ?
		} else {
			for (_key, field) in content.fields() {
				self.iter_value(&field, style);
			}
			if self.text.ends_with("\n\n").not() {
				self.add_break();
			}
		}
	}

	pub fn iter_value(&mut self, value: &Value, style: StyleChain) {
		match value {
			Value::Content(content) => {
				self.iter_content(content, style);
			},
			Value::Array(array) => {
				for value in array.iter() {
					self.iter_value(value, style);
				}
			},
			// symbol?
			_ => {},
		}
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

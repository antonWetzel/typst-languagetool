use std::{path::PathBuf, sync::OnceLock};

use comemo::Prehashed;
use fontdb::Database;
use typst::text::{Font, FontBook, FontInfo};

#[derive(Debug)]
pub struct FontSlot {
	path: PathBuf,
	index: u32,
	font: OnceLock<Option<Font>>,
}

impl FontSlot {
	pub fn get(&self) -> Option<Font> {
		self.font
			.get_or_init(|| {
				let data = std::fs::read(&self.path).ok()?.into();
				Font::new(data, self.index)
			})
			.clone()
	}
}

#[derive(Debug)]
pub struct FontManager {
	book: Prehashed<FontBook>,
	fonts: Vec<FontSlot>,
}

impl FontManager {
	pub fn new() -> Self {
		let mut book = FontBook::new();
		let mut fonts = Vec::new();

		let mut db = Database::new();
		db.load_system_fonts();

		for face in db.faces() {
			let path = match &face.source {
				fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => path,
				fontdb::Source::Binary(_) => continue,
			};

			let info = db
				.with_face_data(face.id, FontInfo::new)
				.expect("database must contain this font");

			if let Some(info) = info {
				book.push(info);
				fonts.push(FontSlot {
					path: path.clone(),
					index: face.index,
					font: OnceLock::new(),
				});
			}
		}

		for data in typst_assets::fonts() {
			let buffer = typst::foundations::Bytes::from_static(data);
			for (i, font) in Font::iter(buffer).enumerate() {
				book.push(font.info().clone());
				fonts.push(FontSlot {
					path: PathBuf::new(),
					index: i as u32,
					font: OnceLock::from(Some(font)),
				});
			}
		}

		Self { book: Prehashed::new(book), fonts }
	}

	pub fn book(&self) -> &Prehashed<FontBook> {
		&self.book
	}

	pub fn get(&self, index: usize) -> Option<Font> {
		self.fonts[index].get()
	}
}

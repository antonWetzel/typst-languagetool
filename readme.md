# typst-languagetool

Spellcheck typst files with LanguageTool.

## Overview

1. compile the document
1. extract text content
1. check text with languagetool
1. map results back to the source 

## Use special styling for spellchecking

```typst
// use styling for spellcheck only in the spellchecker
// keep the correct styling in pdf or preview
// should be called after the template
#show: lt()

// use styling for spellcheck in pdf or preview
// should be called after the template
#show: lt(overwrite: true) 

#let lt(overwrite: false) = {
	if not sys.inputs.at("spellcheck", default: overwrite) {
		return (doc) => doc
	}
	return (doc) => {
		show math.equation.where(block: false): it => [0]
		show math.equation.where(block: true): it => []
		show bibliography: it => []
		show par: set par(justify: false, leading: 0.65em)
		set page(height: auto)
		show block: it => it.body
		show page: set page(numbering: none)
		show heading: it => if it.level <= 3 {
			pagebreak() + it
		} else {
			it
		}
		doc
	}
}
```

## Language Selection

The compiled document contains the text language, but not the region.
```typst
#set text(
    lang: "de", // included
    region: "DE", // lost
)
```
The text language is used to determine the region code ("de-DE", ...).
If another region is desired, it can be specified in the language parameter.

## LanguageTool Backend

- different LanguageTool backends can be used to check the text
- atleast one backend must be enabled for `cargo install ...` with `--features=<backend>`
- one backend must be selected for `typst-languagetool ...` with the required flags

### Bundled

- typst-languagetool starts a LanguageTool instance with JNI
- requires maven and the executable is not portable
- add feature `bundle-jar`
- specify flag `--bundled`

### External JAR

- typst-languagetool starts a LanguageTool instance with JNI
- requires JAR with languagetool
- add feature  `external-jar`
- specify flag `jar_location=<path>`

### Remote Server

- typst-languagetool connects to a running LanguageTool server
- add feature `remote-server`
- specify flags `host=<host>` and `port=<port>`

## Usage

- terminal
	- install command line interface (CLI)
		- `cargo install --git=https://github.com/antonWetzel/typst-languagetool cli --features=...`
	- Check on time or watch for changes
		- `typst-languagetool check ...`
		- `typst-languagetool watch ...`
	- Path to check
		- `typst-languagetool watch --path=<directory or file>`
		- `typst-languagetool check --path=<file>`
	- Different main file can be used
		- defaults to path
		- `--main=<file>`
	- Project root can be changed
		- defaults to main parent folder
		- `--root=<path>`
- vs-codium/vs-code
	- install language server protocal (LSP)
		- `cargo install --git=https://github.com/antonWetzel/typst-languagetool lsp --features=...`
	- install generic lsp (`editors/vscodium/generic-lsp/generic-lsp-0.0.1.vsix`)
	- configure options (see below)
	- hints should appear
		- first check takes longer
- neovim
	- install language server protocal (LSP)
		- `cargo install --git=https://github.com/antonWetzel/typst-languagetool lsp --features=...`
    - copy the `editors/nvim/typst.lua` file in the `ftplugin/` folder (should be in the nvim config path)
	- configure options in `init_option` (see below)
    - create a `main.typst` file and include your typst files inside if needed
	- hints should appear (if not use `set filetype=typst` to force the type)
		- first check takes longer


## LSP Options

```rust
/// Additional allowed words for language codes
dictionary: HashMap<String, Vec<String>>,
/// Languagetool rules to ignore (WHITESPACE_RULE, ...) for language codes
disabled_checks: HashMap<String, Vec<String>>,

/// preferred language codes
languages: Vec<String>,

/// use bundled languagetool
bundled: bool,
/// use external JAR for languagetool
jar_location: Option<String>,
/// host for remote languagetool
host: Option<String>,
/// port for remote languagetool
port: Option<String>,

/// Size for chunk send to LanguageTool
chunk_size: usize,
/// Duration to wait for additional changes before checking the file
/// Leave empty to only check on open and save
on_change: Option<std::time::Duration>,

/// Project Root
root: Option<PathBuf>,
/// Project Main File
main: Option<PathBuf>,
```

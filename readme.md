# typst-languagetool

Spellcheck typst files with LanguageTool.

## Overview

1. compile the document
1. extract text content
1. check text with languagetool
1. map results back to the source 

## Disable Hyphenation

- The text extraction can't handle hyphenation yet
- With the line below the hyphenation can be disabled only for the spellchecker

```typst
#set par(justify: not sys.inputs.at("spellcheck", default: false))
```

## LanguageTool Backend

- different LanguageTool backends can be used to check the text

### Bundled

- requires maven
- add feature `bundle-jar`
- specify `--bundled`

### External JAR

- requires JAR with languagetool
- add feature `external-jar`
- specify `jar_location=...`

### Remote Server

- add feature `remote-server`
- specify `host=...` and `port=...`

## Usage

- terminal
	- install command line interface (CLI)
		- `cargo install --git=https://github.com/antonWetzel/typst-languagetool cli features=...`
	- Check on time or watch for changes
		- `typst-languagetool check ...`
		- `typst-languagetool watch ...`
	- Path to check
		- `typst-languagetool watch --path=<directory or file>`
		- `typst-languagetool cehck --path=<file>`
	- Different main file can be used
		- defaults to path
		- `--main=<file>`
	- Project root can be changed
		- defaults to main parent folder
		- `--root=<path>`
- vs-codium/vs-code
	- install language server protocal (LSP)
		- `cargo install --git=https://github.com/antonWetzel/typst-languagetool lsp features=...`
	- install generic lsp (`editors/vscodium/generic-lsp/generic-lsp-0.0.1.vsix`)
	- configure options (see below)
	- hints should appear
		- first check takes longer

## LSP Options

```rust
/// Language Code like "en-US"
language: String,
/// Additional allowed words
dictionary: Vec<String>,
/// Languagetool rules to ignore (WHITESPACE_RULE, ...)
disabled_checks: Vec<String>,

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
main: PathBuf,
```
# typst-languagetool

Spellcheck typst files with LanguageTool.

# TODO: UPDATE FOR DOCUMENT CHECK
 
## Done

- only use text from typst files
	- please open an issue if some code/math creates errors
- print results with line and columns
- pretty feedback
- lsp implementation

## Languagetool

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
	- install command line interface (CLI) version with `cargo install --git=https://github.com/antonWetzel/typst-languagetool cli features=...`
	- Check single file
		- `typst-languagetool check <path>`
	- Watch directory
		- `typst-languagetool watch <directory>`
- vs-codium/vs-code
	- install language server protocal (LSP) version with `cargo install --git=https://github.com/antonWetzel/typst-languagetool lsp features=...`
	- install generic lsp (`editors/vscodium/generic-lsp/generic-lsp-0.0.1.vsix`)
	- configure settings
	- save `<file>.typ`
	- hints should appear
		- first check takes longer

## LSP Options

```rust
language: String, // Language Code like "en-US"
rules: Rules, // Replacements rules, see 'src/rules.rs' for definition
dictionary: Vec<String>, // Additional allowed words
disabled_checks: Vec<String>, // Languagetool rules to ignore (WHITESPACE_RULE, ...)
bundled: bool, // use bundled languagetool
jar_location: Option<String>, // use external JAR for languagetool
host: Option<String>, // host for remote languagetool
port: Option<String>, // port for remote languagetool
```
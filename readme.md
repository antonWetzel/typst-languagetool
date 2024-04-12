# typst-languagetool

Spellcheck typst files with LanguageTool.

## Done

- only use text from typst files
	- please open an issue if some code/math creates errors
- print results with line and columns
- pretty feedback
- lsp implementation

## Usage


- install java
- install maven
- terminal
	- install command line interface (CLI) version with `cargo install --git=https://github.com/antonWetzel/typst-languagetool cli`
	- `typst-languagetool check ...` in root directory
- vs-codium/vs-code
	- install language server protocal (LSP) version with `cargo install --git=https://github.com/antonWetzel/typst-languagetool lsp`
	- install generic lsp (`editors/vscodium/generic-lsp/generic-lsp-0.0.1.vsix`)
	- configure settings
- save `<file>.typ`
- hints should appear
	- first check takes longer

## LSP Options

```rust
language: String // Language Code like "en-US"
rules: Rules, // Replacements rules, see 'src/rules.rs' for definition
dictionary: Vec<String>, // Additional allowed words
disabled_checks: Vec<String>, // Languagetool rules to ignore (WHITESPACE_RULE, ...)
```
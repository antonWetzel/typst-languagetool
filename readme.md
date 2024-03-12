# typst-languagetool

Spellcheck typst files with a local LanguageTool-Server.

## Done

- only use text from typst files
	- please open an issue if some code/math creates errors
- print results with line and columns
- pretty feedback
- lsp implementation

## Usage


- install java
- download server from <https://dev.languagetool.org/http-server.html>
- terminal
	- install command line interface (CLI) version with `cargo install --git=https://github.com/antonWetzel/typst-languagetool cli`
	- start server (see download website)
	- `typst-languagetool --language=...` in root directory
- vs-codium/vs-code
	- install language server protocal (LSP) version with `cargo install --git=https://github.com/antonWetzel/typst-languagetool lsp`
	- install generic lsp (`editors/vscodium/generic-lsp/generic-lsp-0.0.1.vsix`)
	- configure settings
- save `<file>.typ`
- hints should appear ~1 sec. later

## LSP Options

```rust
language: Option<String> // Language Code like "en-US", empty for auto detect
host: String // Host of the LanguageTool Server
port: String, // Port of the LanguageTool Server
request_length: usize, // length of a request to the LanguageTool Server
rules: Rules, // Replacements rules, see 'src/rules.rs' for definition
dictionary: HashSet<String>, // Always allowed words
local_languagetool_folder: Option<PathBuf>, // Folder with the LanguageTool jar file, starts the server if provided
```
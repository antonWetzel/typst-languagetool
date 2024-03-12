# typst-languagetool

Spellcheck typst files with a local LanguageTool-Server.

## Done

- only use text from typst files
	- please open an issue if some code/math creates errors
- print results with line and columns
- pretty feedback
- lsp implementation

## Usage

- install with `cargo install --git=https://github.com/antonWetzel/typst-languagetool`
- install java
- download server from <https://dev.languagetool.org/http-server.html>
- vs-codium/vs-code
	- install generic lsp (`editors/vscodium/generic-lsp/generic-lsp-0.0.1.vsix`)
	- configure settings
- terminal
	- start server (see download website)
	- `typst-languagetool --language=...` in root directory
- save `<file>.typ`
- hints should appear ~1 sec. later

# typst-languagetool

Spellcheck typst files with a local LanguageTool-Server.

## Done

- only use text from typst files
	- please open an issue if some code/math creates error
- print results with line and columns
- vs-codium/vs-code problem-matcher to show hints in the file
- pretty feedback

## Usage

- install with `cargo install --git=https://github.com/antonWetzel/typst-languagetool`
- install java
- download server from <https://dev.languagetool.org/http-server.html>
- vs-codium/vs-code
	- start server (see `tasks.json`)
	- start problem matcher (see `tasks.json`)
- terminal
	- start server (see download website)
	- `typst-lt --language=...` in root directory
- save `<file>.typ`
- hints should appear ~1 sec. later

## To-do

- no file watcher mode
- allow remote server
- vs-codium/vs-code extension
- choose used rules
- additional allowed words
- check all files in folder, not just the last saved

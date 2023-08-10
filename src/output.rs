use std::{io::stdout, io::Write, path::PathBuf, str::Chars};

use languagetool_rust::CheckResponse;

pub fn output(file: &PathBuf, start: &mut Position, response: &CheckResponse, total: usize) {
    let mut last = 0;
    let mut out = stdout().lock();
    for info in &response.matches {
        start.advance(info.offset - last);
        let mut end = start.clone();
        end.advance(info.length);
        writeln!(
            out,
            "{} {}:{}-{}:{} info {}",
            file.display(),
            start.line,
            start.column,
            end.line,
            end.column,
            info.message,
        )
        .unwrap();
        last = info.offset;
    }
    start.advance(total - last);
}

#[derive(Clone)]
pub struct Position<'a> {
    line: usize,
    column: usize,
    content: Chars<'a>,
}

impl<'a> Position<'a> {
    pub fn new(content: &'a str) -> Self {
        Self {
            line: 1,
            column: 1,
            content: content.chars(),
        }
    }

    fn advance(&mut self, amount: usize) {
        for _ in 0..amount {
            match self.content.next().unwrap() {
                '\n' => {
                    self.line += 1;
                    self.column = 1;
                }
                _ => {
                    self.column += 1;
                }
            }
        }
    }
}

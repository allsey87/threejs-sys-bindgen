use std::io::{BufWriter, Write};
use std::collections::HashMap;

pub struct WasmBindgenWriter<W: Write> {
    indentation: usize,
    output: BufWriter<W>
}

impl<W> WasmBindgenWriter<W> where W: Write {
    pub fn new(w: W) -> WasmBindgenWriter<W> {
        WasmBindgenWriter {
            indentation: 0,
            output: BufWriter::new(w),
        }
    }

    pub fn writeln(&mut self, line: &str) -> std::io::Result<()> {
        let indentation = "    ".repeat(self.indentation);
        writeln!(&mut self.output,
                 "{}{}",
                 indentation,
                 line)
    }

    pub fn write_use_statements(&mut self, statements: &HashMap<String, Vec<String>>) -> std::io::Result<()> {
        for (path, symbols) in statements {
            let symbols = symbols.iter().fold(String::new(), |acc, sym| {
                format!("{}, {}", acc, sym)
            });
            writeln!(&mut self.output,
                     "use {}::{{{}}};",
                     path,
                     symbols.trim_start_matches(", "))?;

        }
        Ok(())
    }
}


use std::io::{BufWriter, Write};
use std::collections::HashMap;

pub struct FunctionDesc {
    attributes: Vec<(String, Option<String>)>,
    name: String,
    arguments: Vec<(String, String)>,
    return_type: Option<String>,
}

impl FunctionDesc {
    pub fn new(attributes: Vec<(String, Option<String>)>,
               name: String,
               arguments: Vec<(String, String)>,
               return_type: Option<String>) -> FunctionDesc {
        FunctionDesc {
            attributes: attributes,
            name: name,
            arguments: arguments,
            return_type: return_type,
        }
    }
}

pub struct ClassDesc {
    class_name: String,
    attributes: Vec<(String, Option<String>)>,
    methods: Vec<FunctionDesc>
}

impl ClassDesc {
    pub fn new(class_name : String,
               attributes: Vec<(String, Option<String>)>,
               methods: Vec<FunctionDesc>) -> ClassDesc {
        ClassDesc {
            class_name: class_name,
            attributes: attributes,
            methods: methods
        }
    }
}

pub struct ModuleDesc {
    attributes: Vec<(String, Option<String>)>,
    class: ClassDesc,
}

impl ModuleDesc {
    pub fn new(attributes: Vec<(String, Option<String>)>,
               class: ClassDesc) -> ModuleDesc {
        ModuleDesc {
            attributes: attributes,
            class: class
        }
    }
}

pub struct Writer<W: Write> {
    indentation: usize,
    output: BufWriter<W>
}

impl<W> Writer<W> where W: Write {
    pub fn new(w: W) -> Writer<W> {
        Writer {
            indentation: 0,
            output: BufWriter::new(w),
        }
    }

    pub fn set_indentation(&mut self, indentation: usize) {
        self.indentation = indentation;
    }

    pub fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        let indentation = "    ".repeat(self.indentation);
        writeln!(&mut self.output,
                 "{}{}",
                 indentation,
                 line)
    }

    pub fn write_export(&mut self, attributes: &[(String, Option<String>)]) -> std::io::Result<()> {
        if attributes.is_empty() {
            self.write_line("#[wasm_bindgen]")
        }
        else {
            let attributes = attributes.iter().fold(String::new(), |mut res, attr| {
                if !res.is_empty() {
                    res.push_str(", ");
                }
                res.push_str(&attr.0);
                if let Some(assignment) = &attr.1 {
                    res.push_str(" = ");
                    res.push_str(assignment);
                }
                res
            });
            self.write_line(&format!("#[wasm_bindgen({})]", attributes))
        }
    }

    pub fn write_function(&mut self, function: &FunctionDesc) -> std::io::Result<()> {
        self.write_export(&function.attributes)?;
        let arguments = function.arguments.iter().fold(String::new(), |mut res, arg| {
            if !res.is_empty() {
                res.push_str(", ");
            }
            res.push_str(&arg.0);
            res.push_str(": ");
            res.push_str(&arg.1);
            res
        });
        let mut fn_str = format!("pub fn {}({})", function.name, arguments);
        if let Some(return_type) = &function.return_type {
            fn_str.push_str(" -> ");
            fn_str.push_str(return_type);
        }
        fn_str.push_str(";");
        self.write_line(&fn_str)
    }

    pub fn write_class(&mut self, class: &ClassDesc) -> std::io::Result<()> {
        self.write_export(&class.attributes)?;
        let class_decl = format!("pub type {};", class.class_name);
        self.write_line(&class_decl)?;
        /* write class methods */
        for function in &class.methods {
            self.write_function(function)?;
        }
        Ok(())
    }

    pub fn write_module(&mut self, module: &ModuleDesc) -> std::io::Result<()> {
        self.write_export(&module.attributes)?;
        self.write_line("extern \"C\" {")?;
        self.set_indentation(1);
        self.write_class(&module.class)?;
        self.set_indentation(0);
        self.write_line("}")?;
        Ok(())
    }

    pub fn write_imports(&mut self, statements: &HashMap<String, Vec<String>>) -> std::io::Result<()> {
        let mut imports = Vec::with_capacity(statements.len());
        for (path, symbols) in statements {
            let mut symbols = symbols.clone();
            symbols.sort_by(|a, b| a.cmp(b));
            match symbols.len() {
                0 => {},
                1 => {
                    imports.push(format!("use {}::{};", path, symbols[0]));
                },
                _ => {
                    let symbols = symbols.iter().fold(String::new(), |mut res, sym| {
                        if !res.is_empty() {
                            res.push_str(", ");
                        }
                        res.push_str(sym);
                        res
                    });
                    imports.push(format!("use {}::{{{}}};", path, symbols));
                }
            }
        }
        imports.sort_by(|a, b| a.cmp(b));
        for import in imports {
            self.write_line(&import)?;
        }
        Ok(())
    }
}


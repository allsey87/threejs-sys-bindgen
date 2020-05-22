use std::io::{BufWriter, Write};
use std::collections::HashMap;

pub struct FunctionDesc<'a> {
    attributes: Vec<(&'a str, Option<&'a str>)>,
    name: &'a str,
    arguments: Vec<&'a str>,
    return_type: Option<&'a str>,
}

impl<'a> FunctionDesc<'a> {
    pub fn new(attributes: Vec<(&'a str, Option<&'a str>)>,
               name: &'a str,
               arguments: Vec<&'a str>,
               return_type: Option<&'a str>) -> FunctionDesc<'a> {
        FunctionDesc {
            attributes: attributes,
            name: name,
            arguments: arguments,
            return_type: return_type,
        }
    }
}

pub struct ClassDesc<'a> {
    class_name: &'a str,
    super_class_name: &'a str,
    methods: Vec<FunctionDesc<'a>>
}

impl<'a> ClassDesc<'a> {
    pub fn new(class_name : &'a str,
               super_class_name: &'a str,
               methods: Vec<FunctionDesc<'a>>) -> ClassDesc<'a> {
        ClassDesc {
            class_name: class_name,
            super_class_name: super_class_name,
            methods: methods
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

    pub fn writeln(&mut self, line: &str) -> std::io::Result<()> {
        let indentation = "    ".repeat(self.indentation);
        writeln!(&mut self.output,
                 "{}{}",
                 indentation,
                 line)
    }

    pub fn write_function(&mut self, function: &FunctionDesc) -> std::io::Result<()> {
        let attributes = function.attributes.iter().fold(String::new(), |mut res, attr| {
            if !res.is_empty() {
                res.push_str(", ");
            }
            res.push_str(attr.0);
            if let Some(assignment) = attr.1 {
                res.push_str(" = ");
                res.push_str(assignment);
            }
            res
        });
        self.writeln(&format!("#[wasm_bindgen({})]", attributes))?;
        let arguments = function.arguments.iter().fold(String::new(), |mut res, arg| {
            if !res.is_empty() {
                res.push_str(", ");
            }
            res.push_str(arg);
            res
        });
        let mut fn_str = format!("pub fn {}({})", function.name, arguments);
        if let Some(return_type) = function.return_type {
            fn_str.push_str(" -> ");
            fn_str.push_str(return_type);
        }
        fn_str.push_str(";");
        self.writeln(&fn_str)
    }

    pub fn write_class(&mut self, class: &ClassDesc) -> std::io::Result<()> {
        /* 
        //test 
        let attributes = vec![("method", None), ("js_name", Some("someJsFunction"))];
        let name = "some_js_function";
        let arguments = vec!["this: &Object3D", "that: u64", "other: &str"];
        let return_type = Some("&str");
        let fnd = FunctionDesc::new(attributes, name, arguments, return_type);
        self.write_function(&fnd)?;
        */
        for function in &class.methods {
            self.write_function(function)?;
        }
        Ok(())
    }

    pub fn write_use_statements(&mut self, statements: &HashMap<String, Vec<String>>) -> std::io::Result<()> {
        for (path, symbols) in statements {
            let symbols = symbols.iter().fold(String::new(), |acc, sym| {
                format!("{}, {}", acc, sym)
            });
            writeln!(&mut self.output,
                     "use {}::{{{}}};",
                     path,
                     symbols.trim_start_matches(", "))?; // this required?

        }
        Ok(())
    }
}


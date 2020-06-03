use std::io::{BufWriter, Write};
use std::collections::HashMap;

pub enum TypeDesc {
    RsSelf,
    RsF64,
    RsBool,
    RsStr,
    RsI64,
    RsStruct(String),
}

impl ToString for TypeDesc {
    fn to_string(&self) -> String {
        match self {
            TypeDesc::RsSelf => String::from("Self"),
            TypeDesc::RsF64 => String::from("f64"),
            TypeDesc::RsBool => String::from("bool"),
            TypeDesc::RsStr => String::from("str"),
            TypeDesc::RsI64 => String::from("i64"),
            TypeDesc::RsStruct(identifier) => identifier.clone(),
        }
    }
}

pub struct ParamDesc {
    type_desc: TypeDesc,
    reference: bool,
    optional: bool,
}

impl ParamDesc {
    pub fn new(type_desc : TypeDesc, reference : bool, optional: bool) -> ParamDesc {
        ParamDesc {
            type_desc: type_desc,
            reference: reference,
            optional: optional,
        }
    }
}

pub struct FunctionDesc {
    attributes: Vec<(String, Option<String>)>,
    name: String,
    arguments: Vec<(String, ParamDesc)>,
    return_type: Option<ParamDesc>,
}

impl FunctionDesc {
    pub fn new(attributes: Vec<(String, Option<String>)>,
               name: String,
               arguments: Vec<(String, ParamDesc)>,
               return_type: Option<ParamDesc>) -> FunctionDesc {
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

// TODO intermediate representation of use statements?
pub struct UseDesc {
    path: Vec<String>,
    symbols: Vec<String>
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

    pub fn write_function(&mut self, function: &FunctionDesc, class: Option<&ClassDesc>) -> std::io::Result<()> {
        self.write_export(&function.attributes)?;
        let arguments = function.arguments.iter().fold(String::new(), |mut res, arg| {
            if !res.is_empty() {
                res.push_str(", ");
            }
            let rs_type = {
                if let TypeDesc::RsSelf = arg.1.type_desc {
                    if let Some(class) = class {
                        class.class_name.clone()
                    }
                    else {
                        panic!("write_function requires the class for methods");
                    }
                }
                else {
                    arg.1.type_desc.to_string()
                }
            };
            let arg = match (arg.1.reference, arg.1.optional) {
                (false, false) => format!("{}: {}", arg.0, rs_type),
                (false, true) => format!("{}: Option<{}>", arg.0, rs_type),
                (true, false) => format!("{}: &{}", arg.0, rs_type),
                (true, true) => format!("{}: &Option<{}>", arg.0, rs_type),
            };
            res.push_str(&arg);
            res
        });
        let mut fn_str = format!("pub fn {}({})", function.name, arguments);
        if let Some(rt) = &function.return_type {
            fn_str.push_str(" -> ");
            let rs_type = {
                if let TypeDesc::RsSelf = rt.type_desc {
                    if let Some(class) = class {
                        class.class_name.clone()
                    }
                    else {
                        panic!("write_function requires the class for methods");
                    }
                }
                else {
                    rt.type_desc.to_string()
                }
            };           
            let rt = match (rt.reference, rt.optional) {
                (false, false) => format!("{}", rs_type),
                (false, true) => format!("Option<{}>", rs_type),
                (true, false) => format!("&{}", rs_type),
                (true, true) => format!("&Option<{}>", rs_type),
            };
            fn_str.push_str(&rt);
        }
        fn_str.push(';');
        self.write_line(&fn_str)
    }

    pub fn write_class(&mut self, class: &ClassDesc) -> std::io::Result<()> {
        self.write_export(&class.attributes)?;
        let class_decl = format!("pub type {};", class.class_name);
        self.write_line(&class_decl)?;
        /* write class methods */
        for function in &class.methods {
            self.write_function(function, Some(class))?;
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


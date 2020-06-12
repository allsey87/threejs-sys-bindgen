use std::io::{BufWriter, Write};
use std::collections::HashMap;

pub enum TypeDesc {
    TsAny,
    TsBoolean,
    TsNull,
    TsNumber,
    TsString,
    TsThis,
    TsVoid,
    TsArray(Box<TypeDesc>),
    TsClass(String),
    TsUnion(Vec<TypeDesc>),
}

/*  implementing this trait on this type seems like it will
    not make sense in the long run since the required string
    depends on the context (return type vs parameter etc) */
impl ToString for TypeDesc {
    fn to_string(&self) -> String {
        match self {
            TypeDesc::TsAny => String::from("JsValue"),
            TypeDesc::TsBoolean => String::from("bool"),
            TypeDesc::TsNull => panic!("Can not convert TsNull to string"),
            TypeDesc::TsNumber => String::from("f64"),
            // it may be more ergonomic to just use &str or string here
            // &str is not an option since it is not supported by Option<&str>
            // two options are Option<String> and Option<JsString>
            TypeDesc::TsString => String::from("String"),
            TypeDesc::TsThis => panic!("Can not convert TsThis to string!"),
            TypeDesc::TsVoid => panic!("Can not convert TsVoid to string!"),            
            TypeDesc::TsArray(inner_type) => {
                if let TypeDesc::TsNumber = **inner_type {
                    /* it seems more efficient to just pass a slice here */
                    /* the glue code wraps the wasm memory buffer in a Float64Array for us */
                    String::from("&[f64]")
                }
                else {
                    String::from("js_sys::Array")
                }
            },
            TypeDesc::TsClass(identifier) => identifier.clone(),
            TypeDesc::TsUnion(_) => panic!("Can not convert TsUnion to string"),
        }
    }
}

pub struct ParamDesc {
    pub type_desc: TypeDesc,
    pub reference: bool,
    pub optional: bool,
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
    pub attributes: Vec<(String, Option<String>)>,
    pub name: String,
    pub arguments: Vec<(String, ParamDesc)>,
    pub return_type: Option<ParamDesc>,
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
    pub class_name: String,
    pub attributes: Vec<(String, Option<String>)>,
    pub methods: Vec<FunctionDesc>
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
    pub attributes: Vec<(String, Option<String>)>,
    pub class: ClassDesc,
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
                if let TypeDesc::TsThis = arg.1.type_desc {
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
                if let TypeDesc::TsThis = rt.type_desc {
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


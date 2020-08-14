use std::io::{self, BufWriter, Write};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use std::convert::TryFrom;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TypeDesc {
    Any,
    Boolean,
    Null,
    Number,
    String,
    This,
    Void,
    Undefined,
    Array(Box<TypeDesc>),
    Function(Vec<(String, TypeDesc)>, Option<Box<TypeDesc>>),
    Class(String),
    Union(Vec<TypeDesc>),
    Unimplemented,
}

impl<'a> TryFrom<&'a TypeDesc> for &'a str {
    type Error = &'static str;

    fn try_from(type_desc: &'a TypeDesc) -> Result<Self, Self::Error> {
        match type_desc {
            TypeDesc::Any => Ok("JsValue"),
            TypeDesc::Boolean => Ok("bool"),
            TypeDesc::Null => Err("cannot convert from null"),
            TypeDesc::Number => Ok("f64"),
            // it may be more ergonomic to just use &str or string here
            // &str is not an option since it is not supported by Option<&str>
            // two options are Option<String> and Option<JsString>
            TypeDesc::String => Ok("String"),
            TypeDesc::This => Err("cannot convert from this"),
            TypeDesc::Void => Err("cannot convert from void"),
            TypeDesc::Undefined => Err("cannot convert from undefined"),
            TypeDesc::Unimplemented => Err("type not implemented"),
            TypeDesc::Array(inner_type) => {
                if let TypeDesc::Number = **inner_type {
                    /* it seems more efficient to just pass a slice here */
                    /* the glue code wraps the wasm memory buffer in a Float64Array for us */
                    Ok("&[f64]")
                }
                else {
                    Ok("js_sys::Array")
                }
            },
            TypeDesc::Function(_, _) => Ok("js_sys::Function"),
            TypeDesc::Class(identifier) => Ok(&identifier),
            TypeDesc::Union(_) => Err("cannot convert from union"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ParamDesc {
    #[serde(rename = "type")]
    pub type_desc: TypeDesc,
    pub reference: bool,
    pub optional: bool,
}

impl ParamDesc {
    pub fn new(type_desc : TypeDesc,
               reference : bool,
               optional: bool) -> ParamDesc {
        ParamDesc {
            type_desc: type_desc,
            reference: reference,
            optional: optional,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FunctionDesc {
    #[serde(default)]
    pub attributes: Vec<(String, Option<String>)>,
    #[serde(default)]
    pub name: String, // Option<String>
    #[serde(default)]
    pub arguments: Vec<(String, ParamDesc)>,
    #[serde(default)]
    pub returns: Option<ParamDesc>,
}

impl FunctionDesc {
    pub fn new(attributes: Vec<(String, Option<String>)>,
               name: String,
               arguments: Vec<(String, ParamDesc)>,
               returns: Option<ParamDesc>) -> FunctionDesc {
        FunctionDesc {
            attributes: attributes,
            name: name,
            arguments: arguments,
            returns: returns,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClassDesc {
    pub name: String,
    #[serde(default)]
    pub attributes: Vec<(String, Option<String>)>,
    #[serde(default)]
    pub methods: Vec<FunctionDesc>
}

impl ClassDesc {
    pub fn new(name : String,
               attributes: Vec<(String, Option<String>)>,
               methods: Vec<FunctionDesc>) -> ClassDesc {
        ClassDesc {
            name: name,
            attributes: attributes,
            methods: methods
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
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

pub struct Writer<W: Write> {
    indentation: usize,
    output: BufWriter<W>,
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

    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        let indentation = "    ".repeat(self.indentation);
        writeln!(&mut self.output,
                 "{}{}",
                 indentation,
                 line)
    }

    pub fn write_export(&mut self, attributes: &[(String, Option<String>)]) -> io::Result<()> {
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

    pub fn write_function(&mut self, function: &FunctionDesc, class_name: Option<&str>) -> io::Result<()> {
        self.write_export(&function.attributes)?;
        let arguments = function.arguments
            .iter()
            .try_fold::<_, _, io::Result<String>>(String::new(), |mut res, arg| {
            if !res.is_empty() {
                res.push_str(", ");
            }
            let rs_type = if let TypeDesc::This = arg.1.type_desc {
                class_name.ok_or(
                    io::Error::new(io::ErrorKind::Other,
                                   "write_function requires the class for methods"))
            }
            else {
                <&str>::try_from(&arg.1.type_desc)
                    .map_err(|_e| io::Error::new(io::ErrorKind::Other, 
                                                 format!("Cannot convert parameter {} of {}",
                                                         arg.0,
                                                         function.name)))
            }?;
            let arg = match (arg.1.reference, arg.1.optional) {
                (false, false) => format!("{}: {}", arg.0, rs_type),
                (false, true) => format!("{}: Option<{}>", arg.0, rs_type),
                (true, false) => format!("{}: &{}", arg.0, rs_type),
                (true, true) => format!("{}: &Option<{}>", arg.0, rs_type),
            };
            res.push_str(&arg);
            Ok(res)
        })?;
        let mut fn_str = format!("pub fn {}({})", function.name, arguments);
        if let Some(rt) = &function.returns {
            let rs_type = match rt.type_desc {
                TypeDesc::This => {
                    class_name.ok_or(
                        io::Error::new(io::ErrorKind::Other,
                                       "write_function requires the class for methods"))?
                },
                _ => {
                    <&str>::try_from(&rt.type_desc)
                        .map_err(|_e| io::Error::new(io::ErrorKind::Other, 
                                                    format!("Cannot convert return type of {}",
                                                            function.name)))?
                }
            };           
            let rt = match (rt.reference, rt.optional) {
                (false, false) => format!(" -> {}", rs_type),
                (false, true) => format!(" -> Option<{}>", rs_type),
                (true, false) => format!(" -> &{}", rs_type),
                (true, true) => format!(" -> &Option<{}>", rs_type),
            };
            fn_str.push_str(&rt);
        }
        fn_str.push(';');
        self.write_line(&fn_str)
    }

    pub fn write_class(&mut self, class: ClassDesc) -> io::Result<()> {
        self.write_export(&class.attributes)?;
        let class_decl = format!("pub type {};", class.name);
        self.write_line(&class_decl)?;
        /* write class methods */
        for function in &class.methods {
            self.write_function(function, Some(&class.name))?;
        }
        Ok(())
    }

    pub fn write_module(&mut self, module: ModuleDesc) -> io::Result<()> {
        self.write_export(&module.attributes)?;
        self.write_line("extern \"C\" {")?;
        self.set_indentation(1);
        self.write_class(module.class)?;
        self.set_indentation(0);
        self.write_line("}")?;
        Ok(())
    }

    pub fn write_imports(&mut self, mut statements: HashMap<String, Vec<String>>) -> io::Result<()> {
        let mut imports = Vec::with_capacity(statements.len());
        for (path, mut symbols) in statements.drain() {
            symbols.sort_unstable_by(|a, b| a.cmp(b));
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
        imports.sort_unstable_by(|a, b| a.cmp(b));
        for import in imports {
            self.write_line(&import)?;
        }
        Ok(())
    }
}


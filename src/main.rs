//use inflector::Inflector;

use std::fs::File;
use std::path::Path;
use std::collections::HashMap;

mod swc;
mod wasm_bindgen_writer;

fn main() -> std::io::Result<()> {
    let module = swc::parse_module(Path::new("threejs/core/Object3D.d.ts")).expect("Unable to parse module");

    let output = File::create("Object3D.rs").expect("Unable to create file");
    let mut writer = wasm_bindgen_writer::WasmBindgenWriter::new(output);
    
    let imports_grouped = process_imports(&module);
    writer.write_use_statements(&imports_grouped)?;

    writer.writeln("use wasm_bindgen::prelude::*;")?;

    //write!(&mut output, "#[wasm_bindgen(module = \"{}\")]\n", "threejs/core/Object3D.js");
    //write!(&mut output, "extern \"C\" {{\n");

    for item in &module.body {
        match item {
            swc::ast::ModuleItem::ModuleDecl(declaration) => {
                match declaration {
                    swc::ast::ModuleDecl::ExportDecl(export) => {
                        let export_decl = &export.decl;
                        match export_decl {
                            swc::ast::Decl::Class(class_declaration) => {
                                if let Some(super_class) = &class_declaration.class.super_class {
                                    if let swc::ast::Expr::Ident(_ident) = &**super_class {
                                        //writeln!(&mut output, "#[wasm_bindgen(extends = {})]", ident.sym);    
                                    }
                                }
                                let _this_type : &str = &class_declaration.ident.sym;
                                //writeln!(&mut output, "pub type {};", this_type);
                                //println!("{:?}", class_declaration.class.body);
                                for class_member in &class_declaration.class.body {
                                    match class_member {
                                        swc::ast::ClassMember::Constructor(_constructor) => {
                                            //writeln!(&mut output, "#[wasm_bindgen(constructor)]");
                                            // TODO handle arguments (multiple constructors?)                                                
                                            //writeln!(&mut output, "pub fn new() -> {};", this_type);
                                        },
                                        swc::ast::ClassMember::Method(class_method) => {
                                            if class_method.kind == swc::ast::MethodKind::Method {
                                                let function = &class_method.function;
                                                if let swc::ast::PropName::Ident(_ident) = &class_method.key {
                                                    /*
                                                    writeln!(&mut output,
                                                                "#[wasm_bindgen(method, js_name = {})]",
                                                                ident.sym);
                                                    */
                                                    /* handle arguments */
                                                    let mut arguments = String::new();
                                                    for param in &function.params {
                                                        if let swc::ast::Pat::Ident(ident) = &param.pat {
                                                            if let Some(type_ann) = &ident.type_ann {
                                                                let ts_type = &*type_ann.type_ann;
                                                                let argument = 
                                                                    format!("{}: {},",
                                                                            ident.sym,
                                                                            ts_to_rust_type_signature(ts_type));
                                                                arguments.push_str(&argument);
                                                            }
                                                        }
                                                    }
                                                    /*
                                                    writeln!(&mut output,
                                                            "pub fn {}({});",
                                                            ident.sym.to_snake_case(), arguments);
                                                    */
                                                }
                                                else {
                                                    panic!("unimplemented PropName");
                                                }
                                                
                                                if let Some(_ts_return) = &function.return_type {
                                                    //println!("ts return type = {}", "yupr");
                                                }
                                            }
                                            else {
                                                panic!("setters and getters not implemented yet!")
                                            }
                                            
                                        },
                                        _ => ()
                                    }
                                }

                            },
                            _ => ()
                        }
                        //println!("{:?}", export);
                    },
                    _ => ()
                }
            },
            _ => ()
        }
    }
    Ok(())
}

fn process_imports(module: &swc_ecma_ast::Module) -> HashMap<String, Vec<String>> {
    /* get imports */
    let mut imports = Vec::new();
    for item in &module.body {
        if let swc::ast::ModuleItem::ModuleDecl(declaration) = item {
            if let swc::ast::ModuleDecl::Import(import) = declaration {
                let mut symbols = Vec::new();
                for import_specifier in &import.specifiers {
                    if let swc::ast::ImportSpecifier::Named(named_import_specifier) = import_specifier {
                        symbols.push(named_import_specifier.local.sym.as_ref());
                    }
                }
                if symbols.len() != 1 {
                    eprintln!("warning: multiple symbols for imports unhandled");
                }
                let source : &str = &import.src.value;
                imports.push((source, symbols));
            }
        }
    }
    /* map for grouping the imports together */
    let mut imports_grouped: HashMap<String, Vec<String>> =
        HashMap::with_capacity(imports.len());
    /* group and convert import paths */
    for mut import in imports {
        if let Some(symbol) = import.1.pop() {
            let path = import.0.split('/').fold(String::new(), |path, part| {
                match part {
                    "." => format!("{}::self", path),
                    ".." => format!("{}::super", path),
                    _ => format!("{}::{}", path, part)
                }
            });
            let path = path.replace("self::super", "super");
            let path = path.trim_start_matches(':');
            let path_parts : Vec<&str> =
                path.rsplitn(2, "::")
                    .collect();
            if let Some((path_symbol, path)) = path_parts.split_first() {
                if *path_symbol == symbol {
                    if let Some(path) = path.first() {
                        imports_grouped.entry((*path).to_owned())
                                       .and_modify(|symbols| {
                                           symbols.push(symbol.to_owned())
                                       }).or_insert(vec![symbol.to_owned()]);
                    }
                }
            }
        }
    }
    imports_grouped
}

fn ts_to_rust_type_signature(ts_type: &swc_ecma_ast::TsType) -> String {
    match ts_type {
        swc::ast::TsType::TsTypeRef(ts_type_ref) => {
            if let swc::ast::TsEntityName::Ident(ident) = &ts_type_ref.type_name {
                format!("&{}", ident.sym)
            }
            else {
                "".to_owned()
            }
        },
        swc::ast::TsType::TsKeywordType(ts_keyword_type) => {
            match ts_keyword_type.kind {
                swc::ast::TsKeywordTypeKind::TsNumberKeyword => "f64",
                swc::ast::TsKeywordTypeKind::TsBooleanKeyword => "bool",
                swc::ast::TsKeywordTypeKind::TsStringKeyword => "&str",
                swc::ast::TsKeywordTypeKind::TsBigIntKeyword => "i64",
                _ => ""
            }.to_owned()
        },
        _ => "".to_owned()
    }
}

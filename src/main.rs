use inflector::Inflector;

use std::fs::File;
use std::path::Path;
use std::collections::HashMap;

mod swc;
mod wb;

// https://github.community/t5/How-to-use-Git-and-GitHub/How-can-I-download-a-specific-folder-from-a-GitHub-repo/td-p/88

fn main() -> std::io::Result<()> {
    let module = swc::parse_module(Path::new("threejs/core/Object3D.d.ts")).expect("Unable to parse module");

    let output = File::create("Object3D.rs").expect("Unable to create file");
    let mut writer = wb::Writer::new(output);
    
    let imports = process_imports(&module);
    writer.write_imports(&imports)?;

    writer.write_line("\nuse wasm_bindgen::prelude::*;\n")?;

    for item in &module.body {
        if let swc_ecma_ast::ModuleItem::ModuleDecl(declaration) = item {
            if let swc_ecma_ast::ModuleDecl::ExportDecl(export) = declaration {
                if let swc_ecma_ast::Decl::Class(class_declaration) = &export.decl {
                    let mod_attributes = 
                        vec![(String::from("module"), Some(String::from("\"threejs/core/Object3D.js\"")))];
                    let mod_class = process_class(class_declaration);
                    writer.write_module(&wb::ModuleDesc::new(mod_attributes, mod_class))?;
                }
            }
        }
    }
  
    /*
    let fn_attributes = vec![("method", None), ("js_name", Some("someJsFunction"))];
    let fn_name = "some_js_function";
    let fn_arguments = vec![("this","&SomeClass"), ("that","u64")];
    let fn_return_type = Some("&str");
    let fn_desc = wb::FunctionDesc::new(fn_attributes, fn_name, fn_arguments, fn_return_type);
    
    let cls_attributes = vec![("extends", Some("SomeOtherClass"))];
    let cls_name = "SomeClass";
    let cls_methods = vec![fn_desc];
    let cls_desc = wb::ClassDesc::new(cls_name, cls_attributes, cls_methods);

    let mod_attributes = vec![("module", Some("\"threejs/core/Object3D.js\""))];
    let mod_desc = wb::ModuleDesc::new(mod_attributes, cls_desc);

    writer.write_module(&mod_desc)?;
    */
    Ok(())
}

fn process_class(class_declaration: &swc_ecma_ast::ClassDecl) -> wb::ClassDesc {
    let cls_name = class_declaration.ident.sym.to_string();
    let mut cls_attributes = Vec::new();
    let mut cls_methods = Vec::new();
    /* handle super class */
    if let Some(class) = &class_declaration.class.super_class {
        if let swc_ecma_ast::Expr::Ident(ident) = &**class {
            cls_attributes.push((String::from("extends"), Some(ident.sym.to_string())));
        }
    }
    /* handle methods */
    for class_member in &class_declaration.class.body {
        match class_member {
            swc_ecma_ast::ClassMember::Constructor(_constructor) => {
                let fn_attributes = vec![(String::from("constructor"), None)];
                let fn_name = String::from("new");
                let fn_arguments = vec![];
                let fn_return_type = Some(cls_name.clone());
                let fn_desc = 
                    wb::FunctionDesc::new(
                        fn_attributes,
                        fn_name,
                        fn_arguments,
                        fn_return_type);
                cls_methods.push(fn_desc);
            },
            swc_ecma_ast::ClassMember::Method(class_method) => {
                if class_method.kind == swc_ecma_ast::MethodKind::Method {
                    let function = &class_method.function;
                    if let swc_ecma_ast::PropName::Ident(ident) = &class_method.key {
                        let fn_attributes =
                            vec![(String::from("method"), None), 
                                 (String::from("js_name"), Some(ident.sym.to_string()))];
                        let fn_name = ident.sym.to_snake_case();
                        let fn_arguments = vec![];
                        let fn_return_type = None;
                        let fn_desc = 
                            wb::FunctionDesc::new(fn_attributes,
                                                  fn_name,
                                                  fn_arguments,
                                                  fn_return_type);
                        cls_methods.push(fn_desc);
                        /* handle arguments */
                        for param in &function.params {
                            if let swc_ecma_ast::Pat::Ident(ident) = &param.pat {
                                if let Some(type_ann) = &ident.type_ann {
                                    let _ts_type = &*type_ann.type_ann;
                                }
                            }
                        }
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
    wb::ClassDesc::new(cls_name, cls_attributes, cls_methods)
}

fn _ts_to_rust_type_signature(ts_type: &swc_ecma_ast::TsType, this_type: Option<&str>) -> String {
    match ts_type {
        swc_ecma_ast::TsType::TsTypeRef(ts_type_ref) => {
            if let swc_ecma_ast::TsEntityName::Ident(ident) = &ts_type_ref.type_name {
                format!("&{}", ident.sym)
            }
            else {
                "".to_owned()
            }
        },
        swc_ecma_ast::TsType::TsKeywordType(ts_keyword_type) => {
            match ts_keyword_type.kind {
                swc_ecma_ast::TsKeywordTypeKind::TsNumberKeyword => "f64",
                swc_ecma_ast::TsKeywordTypeKind::TsBooleanKeyword => "bool",
                swc_ecma_ast::TsKeywordTypeKind::TsStringKeyword => "&str",
                swc_ecma_ast::TsKeywordTypeKind::TsBigIntKeyword => "i64",
                _ => ""
            }.to_owned()
        },
        swc_ecma_ast::TsType::TsThisType(_) => {
            if let Some(class_name) = this_type {
                class_name
            }
            else {
                panic!("cannot resolve this type without class");
            }.to_owned()
        },
        _ => "".to_owned()
    }
}

fn process_imports(module: &swc_ecma_ast::Module) -> HashMap<String, Vec<String>> {
    /* get imports */
    let mut imports = Vec::new();
    for item in &module.body {
        if let swc_ecma_ast::ModuleItem::ModuleDecl(declaration) = item {
            if let swc_ecma_ast::ModuleDecl::Import(import) = declaration {
                let mut symbols = Vec::new();
                for import_specifier in &import.specifiers {
                    if let swc_ecma_ast::ImportSpecifier::Named(named_import_specifier) = import_specifier {
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
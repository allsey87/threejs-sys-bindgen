use inflector::Inflector;

use std::fs::File;
use std::path::Path;
use std::collections::HashMap;

mod swc;
mod wb;

// https://github.community/t5/How-to-use-Git-and-GitHub/How-can-I-download-a-specific-folder-from-a-GitHub-repo/td-p/88

// TODOs
// start walking directories
// handle references
// handle optional arguments (done)
// handle complex types, arrays, callbacks

// for the generator library : use build script to pull in the ts files
// for the output library: use build script to pull in the js files

fn main() -> std::io::Result<()> {
    let module = swc::parse_module(Path::new("threejs/core/InstancedBufferAttribute.d.ts")).expect("Unable to parse module");

    let output = File::create("InstancedBufferAttribute.rs").expect("Unable to create file");
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
    Ok(())
}

fn process_type(ts_type: &swc_ecma_ast::TsType) -> wb::TypeDesc {
    match ts_type {
        swc_ecma_ast::TsType::TsTypeRef(ts_type_ref) => {
            if let swc_ecma_ast::TsEntityName::Ident(ident) = &ts_type_ref.type_name {
                wb::TypeDesc::RsStruct(ident.sym.to_string())
            }
            else {
                panic!("TsTypeRef identifer missing");
            }
        },
        swc_ecma_ast::TsType::TsKeywordType(ts_keyword_type) => {
            match ts_keyword_type.kind {
                swc_ecma_ast::TsKeywordTypeKind::TsNumberKeyword =>
                    wb::TypeDesc::RsF64,
                swc_ecma_ast::TsKeywordTypeKind::TsBooleanKeyword =>
                    wb::TypeDesc::RsBool,
                swc_ecma_ast::TsKeywordTypeKind::TsStringKeyword => 
                    wb::TypeDesc::RsStr,
                swc_ecma_ast::TsKeywordTypeKind::TsBigIntKeyword =>
                    wb::TypeDesc::RsI64,
                _ => {
                    panic!("Unimplemented TsKeywordType");
                }
            }
        },
        swc_ecma_ast::TsType::TsThisType(_) => {
            wb::TypeDesc::RsSelf
        },
        _ => {
            panic!("Unimplemented TsType");
        }
    }
}

fn process_parameter(parameter: &swc_ecma_ast::Param) -> (String, wb::ParamDesc) {
    if let swc_ecma_ast::Pat::Ident(identifier) = &parameter.pat {
        if let Some(ts_type) = &identifier.type_ann {
            let name = identifier.sym.to_snake_case();
            let type_desc = process_type(&ts_type.type_ann);
            (name, wb::ParamDesc::new(type_desc, false, identifier.optional))
        }
        else {
            panic!("Type annotation missing")
        }
    }
    else {
        panic!("Parameter did not have an identifier")
    }
}

fn process_function(name: &str,
                    attributes: Vec<(String, Option<String>)>,
                    parameters: &[&swc_ecma_ast::Param],
                    return_type: &Option<&swc_ecma_ast::TsType>) -> wb::FunctionDesc {
    /* process the parameters */
    let fn_arguments : Vec<(String, wb::ParamDesc)> = 
        parameters.iter().map(|p| process_parameter(p)).collect();
    /* process return type */
    let fn_return_type = match return_type {
        /* hack: to be resolved once all ts types are implemented and we know how to handle references */
        Some(return_type) => Some(wb::ParamDesc::new(process_type(return_type), false, false)),
        None => None,
    };
    wb::FunctionDesc::new(attributes,
                          name.to_owned(),
                          fn_arguments,
                          fn_return_type)
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
            swc_ecma_ast::ClassMember::Constructor(constructor) => {
                let fn_attributes = vec![(String::from("constructor"), None)];
                let fn_name = String::from("new");
                let fn_parameters : Vec<&swc_ecma_ast::Param> = constructor.params
                    .iter()
                    .filter_map(|p| match p {
                        swc_ecma_ast::ParamOrTsParamProp::Param(param) => Some(param),
                        _ => None,
                    })
                    .collect();
                let fn_return_type = Some(
                    &swc_ecma_ast::TsType::TsThisType(
                        swc_ecma_ast::TsThisType {
                            span: swc_common::DUMMY_SP
                        }
                    )
                );
                let fn_desc = process_function(
                    &fn_name,
                    fn_attributes,
                    &fn_parameters,
                    &fn_return_type);
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
                        let mut fn_parameters = Vec::with_capacity(function.params.len());
                        for param in &function.params {
                            fn_parameters.push(param);
                        }
                        let fn_return_type = match &function.return_type {
                            Some(fn_return_type) => {
                                Some(&*fn_return_type.type_ann)
                            },
                            None => None
                        };
                        let fn_desc =
                            process_function(&fn_name,
                                             fn_attributes,
                                             &fn_parameters,
                                             &fn_return_type);
                        cls_methods.push(fn_desc);
                    }
                    else {
                        panic!("unimplemented PropName");
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

// This function is doing both scanning of the AST and formatting
// TODO: Move to the string generation into the wb module
// TODO: Create some intermediate type such as UseDesc that has a vector of symbols and a path
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
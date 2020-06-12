use inflector::Inflector;
use std::{fs, io, path, vec, collections::HashMap};

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

#[derive(Hash, Eq, PartialEq, Debug)]
struct Key(String, String);

struct BindingsTargetIterator(vec::Vec<fs::ReadDir>);

impl BindingsTargetIterator {
    fn new<P: AsRef<path::Path>>(start_path: P) -> io::Result<BindingsTargetIterator> {
        let mut paths = vec::Vec::new();
        paths.push(fs::read_dir(start_path)?);
        Ok(BindingsTargetIterator(paths))
    }
}

impl Iterator for BindingsTargetIterator {
    type Item = io::Result<path::PathBuf>;


    //type Item = io::Result<(path::PathBuf, path::PathBuf, path::PathBuf)>;

    fn next(&mut self) -> Option<Self::Item> {
        let paths = &mut self.0;
        while let Some(mut current_path) = paths.pop() {
            if let Some(entry) = current_path.next() {
                paths.push(current_path);
                match entry {
                    Ok(entry) => {
                        let entry_path = entry.path();
                        if entry_path.is_dir() {
                            match fs::read_dir(entry_path) {
                                Ok(child_path) => {
                                    paths.push(child_path);
                                    continue;
                                }
                                Err(error) => {
                                    return Some(Err(error));
                                }
                            }
                        }
                        else {
                            if let Some(extension) = entry_path.extension() {
                                if extension == "ts" {
                                    return Some(Ok(entry_path))
                                }
                            }
                        }
                    }
                    Err(error) => {
                        return Some(Err(error));
                    }
                }
            }
        }
        return None;
    }
}

fn main() -> std::io::Result<()> {
    if let Ok(iterator) = BindingsTargetIterator::new("threejs/math/") {
        for ts_path in iterator {
            let mut ts_path = ts_path?;
            let ts_module = swc::parse_module(&ts_path)?;
            let ts_file_name = 
                ts_path.file_name()
                       .and_then(|f| f.to_str())
                       .ok_or(io::Error::new(io::ErrorKind::Other,
                            "could not convert ts filename to string"))?
                       .to_owned();
            ts_path.pop();
            let rs_path = path::Path::new("bindings").join(&ts_path);

            fs::create_dir_all(&rs_path)?;
            let js_path = ts_path.join(ts_file_name.replace(".d.ts", ".js"));
            let rs_path = rs_path.join(ts_file_name.replace(".d.ts", ".rs"));

            let mut writer = wb::Writer::new(fs::File::create(rs_path)?);          
            let imports = process_imports(&ts_module);
            writer.write_imports(&imports)?;
            writer.write_line("\nuse wasm_bindgen::prelude::*;\n")?;

            for item in &ts_module.body {
                if let swc_ecma_ast::ModuleItem::ModuleDecl(declaration) = item {
                    if let swc_ecma_ast::ModuleDecl::ExportDecl(export) = declaration {
                        if let swc_ecma_ast::Decl::Class(class_declaration) = &export.decl {
                            let js_module_str = 
                                js_path.to_str()
                                    .ok_or(io::Error::new(io::ErrorKind::Other,
                                           "could not convert js filename to string"))?
                                    .to_owned();
                            let mod_attributes = 
                                vec![(String::from("module"), Some(js_module_str))];
                            let mod_class = process_class(class_declaration);
                            writer.write_module(&wb::ModuleDesc::new(mod_attributes, mod_class))?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_type(ts_type: &swc_ecma_ast::TsType) -> wb::TypeDesc {
    match ts_type {
        swc_ecma_ast::TsType::TsArrayType(ts_array_type) => {
            let inner = process_type(&ts_array_type.elem_type);
            wb::TypeDesc::TsArray(Box::new(inner))
        },
        swc_ecma_ast::TsType::TsTypeRef(ts_type_ref) => {
            if let swc_ecma_ast::TsEntityName::Ident(ident) = &ts_type_ref.type_name {
                /* handle interfaces? */
                if ident.sym.eq_str_ignore_ascii_case("ArrayLike") {
                    if let Some(params) = &ts_type_ref.type_params {
                        let param = &params.params[0];
                        let inner = process_type(&*param);
                        wb::TypeDesc::TsArray(Box::new(inner))
                    }
                    else {
                        panic!("ArrayLike without type params?")
                    }
                }
                else {
                    wb::TypeDesc::TsClass(ident.sym.to_string())
                }
            }
            else {
                panic!("TsTypeRef identifer missing");
            }
        },
        swc_ecma_ast::TsType::TsKeywordType(ts_keyword_type) => {
            match ts_keyword_type.kind {
                swc_ecma_ast::TsKeywordTypeKind::TsNumberKeyword =>
                    wb::TypeDesc::TsNumber,
                swc_ecma_ast::TsKeywordTypeKind::TsNullKeyword => 
                    wb::TypeDesc::TsNull,                    
                swc_ecma_ast::TsKeywordTypeKind::TsBooleanKeyword =>
                    wb::TypeDesc::TsBoolean,
                swc_ecma_ast::TsKeywordTypeKind::TsStringKeyword => 
                    wb::TypeDesc::TsString,
                swc_ecma_ast::TsKeywordTypeKind::TsAnyKeyword => 
                    wb::TypeDesc::TsAny,
                swc_ecma_ast::TsKeywordTypeKind::TsVoidKeyword =>
                    wb::TypeDesc::TsVoid,
                _ => {
                    panic!(format!("TsKeywordType::{:?} is not implemented", ts_keyword_type));
                }
            }
        },
        swc_ecma_ast::TsType::TsThisType(_) => {
            wb::TypeDesc::TsThis
        },
        // TODO: special case, return null or something => Option<Something>
        // General case: generate x different functions?
        // e.g., constructor( color?: Color | string | number );
        // constructor( r: number, g: number, b: number );
        // Color::from(other), Color::from("red"), Color::from(0x124235u16), Color::with_components(r,g,b)
        // Create a configuration file that lists the classes/paths to be searched and bindings to be formed
        // import statements could be used to figure out dependencies?
        // add quirks to configuration file for removing optional_ prefixes, to_vector_3 to_vector3
        // create high level example and start increasing complexity on a need-be basis
        swc_ecma_ast::TsType::TsUnionOrIntersectionType(variant) => {
            match variant {
                swc_ecma_ast::TsUnionOrIntersectionType::TsUnionType(ts_union_type) => {
                    let mut ts_types = Vec::with_capacity(ts_union_type.types.len());
                    for ts_type in &ts_union_type.types {
                        ts_types.push(process_type(&**ts_type));
                    }
                    wb::TypeDesc::TsUnion(ts_types)
                },
                swc_ecma_ast::TsUnionOrIntersectionType::TsIntersectionType(ts_intersection_type) => {
                    panic!(format!("TsIntersectionType::{:?} is not implemented", ts_intersection_type));
                }
            }
        },

        _ => {
            panic!(format!("TsType::{:?} is not implemented", ts_type));
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
            panic!("Missing type annotation for parameter")
        }
    }
    else {
        panic!("Missing identifier for parameter")
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
    if let Some(return_type) = return_type {
        let mut return_type = process_type(&return_type);
        let mut optional = false;
        /* handle special option case */
        if let wb::TypeDesc::TsUnion(union) = &mut return_type {
            if let [_, wb::TypeDesc::TsNull] = &union[..] {
                return_type = union.remove(0);
                optional = true;
            }
        }
        let return_param = wb::ParamDesc::new(return_type, false, optional);
        wb::FunctionDesc::new(attributes,
            name.to_owned(),
            fn_arguments,
            Some(return_param))
    }
    else {
        wb::FunctionDesc::new(attributes,
            name.to_owned(),
            fn_arguments,
            None)
    }
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
                let mut fn_desc = process_function(
                    &fn_name,
                    fn_attributes,
                    &fn_parameters,
                    &None);
                let fn_return_type = wb::ParamDesc::new(wb::TypeDesc::TsThis, false, false);
                fn_desc.return_type = Some(fn_return_type);
                cls_methods.push(fn_desc);
            },
            swc_ecma_ast::ClassMember::Method(class_method) => {
                if class_method.kind == swc_ecma_ast::MethodKind::Method {
                    let function = &class_method.function;
                    if let swc_ecma_ast::PropName::Ident(ident) = &class_method.key {
                        /* check override */
                        // let override: Vec<wb::FunctionDesc> = overrides[("class", "method")]
                        let fn_name = ident.sym.to_snake_case();
                        let mut fn_attributes = vec![(String::from("method"), None)];
                        if ident.sym.to_string() != fn_name {
                            fn_attributes.push((String::from("js_name"), Some(ident.sym.to_string()))); 
                        }
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
                        let mut fn_desc =
                            process_function(&fn_name,
                                             fn_attributes,
                                             &fn_parameters,
                                             &fn_return_type);
                        let this_param = wb::ParamDesc::new(wb::TypeDesc::TsThis, true, false);
                        fn_desc.arguments.insert(0, (String::from("this"), this_param));
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
use inflector::Inflector;
use std::{fs, io, path, vec, collections::HashMap};
use serde::{Serialize, Deserialize};
mod swc;
mod wb;

// https://github.community/t5/How-to-use-Git-and-GitHub/How-can-I-download-a-specific-folder-from-a-GitHub-repo/td-p/88
// for the generator library : use build script to pull in the ts files
// for the output library: use build script to pull in the js files

// TODOs
// start walking directories
// handle references
// handle optional arguments (done)
// handle complex types, arrays, callbacks

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum OverrideMode {
    Skip,
    Override,
}

#[derive(Serialize, Deserialize, Debug)]
struct ClassOverride {
    mode: OverrideMode,
    #[serde(default)]
    methods: HashMap<String, Vec<wb::FunctionDesc>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ModuleOverride {
    mode: OverrideMode,
    #[serde(default)]
    classes: HashMap<String, ClassOverride>,
}

struct BindingsTargetIterator(vec::Vec<fs::ReadDir>);

impl BindingsTargetIterator {
    fn new<P: AsRef<path::Path>>(start_path: P) -> io::Result<BindingsTargetIterator> {
        let mut paths = vec::Vec::new();
        paths.push(fs::read_dir(start_path)?);
        Ok(BindingsTargetIterator(paths))
    }
}


/* implements a depth-first search for ts files */
impl Iterator for BindingsTargetIterator {
    type Item = io::Result<path::PathBuf>;

    fn next(&mut self) -> Option<Self::Item> {
        let paths = &mut self.0;
        while let Some(mut current_path) = paths.pop() {
            if let Some(entry) = current_path.next() {
                /* since the iterator gave us another item, push
                  the current path back on to the stack of paths*/
                paths.push(current_path);
                if let Ok(entry) = entry {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        let child_path = fs::read_dir(entry_path);
                        /* add the child path to the stack and start again */
                        if let Ok(child_path) = child_path {
                            paths.push(child_path);
                            continue;
                        }
                        else if let Err(error) = child_path {
                            return Some(Err(error));
                        }
                    }
                    else if let Some(extension) = entry_path.extension() {
                        if extension == "ts" {
                            return Some(Ok(entry_path))
                        }
                    }
                }
                else {
                    return Some(Err(entry.unwrap_err()));
                }
            }
        }
        return None;
    }
}

fn main() -> std::io::Result<()> {
    let matches = clap::App::new("threejs-bindgen")
    .version("1.0")
    .author("Michael Allwright <allsey87@gmail.com>")
    .about("Generate Rust bindings for the Three.js library")
    .arg(clap::Arg::with_name("overrides")
        .help("Set the overrides directory")
        .long("overrides")
        .short("o")
        .takes_value(true)
        .value_name("DIR"))
    .arg(clap::Arg::with_name("paths")
        .help("The paths to search")
        .required(true)
        .multiple(true))
    .get_matches();

    // new features of overrides
    // 1. mark classes that are not to be bound
    // 2. all objects inside one bindings file per module
    // 3. for methods with the same name, handle the only use once case
    let mut overrides : HashMap<String, ModuleOverride> = HashMap::new();
    
    if let Some(override_dir) = matches.value_of("overrides") {
        for override_entry in fs::read_dir(override_dir)? {
            let override_path = override_entry?.path();
            if override_path
                .extension()
                .map_or(false, |ext| ext == "yaml") {
                let override_filestem = override_path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or(io::Error::new(io::ErrorKind::Other,
                         "could not convert filestem to string"))?
                    .to_owned();
                let override_file = fs::File::open(&override_path)?;           
                let module_override = 
                    serde_yaml::from_reader::<_, ModuleOverride>(override_file)
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                overrides.insert(override_filestem, module_override);
            }
        }
    }

    /* TODO remove this once overrides work correctly? */
    let paths : Vec<&str> = matches.values_of("paths").unwrap().collect();

    for path in paths {
        if let Ok(iterator) = BindingsTargetIterator::new(path) {
            for ts_path in iterator {
                let ts_path = ts_path?;
                /* extract the typescript module name from the path */
                let ts_module_name = ts_path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .and_then(|f| f.strip_suffix(".d.ts"))
                    .ok_or(io::Error::new(io::ErrorKind::Other,
                           "could not convert typescript file to a module name"))?
                    .to_owned();
                /* check if we have any overrides defined for this module */
                let mod_overrides = overrides.get(&ts_module_name);
                /* check if we should skip generating bindings for this module */
                if let Some(mod_overrides) = mod_overrides {
                    if let OverrideMode::Skip = mod_overrides.mode {
                        continue;
                    }
                }
                /* generate the AST and get the comments from the typescript */
                let (ts_module, ts_comments) = swc::parse_module(&ts_path)?;
                /* get the current directory */
                let ts_dir = ts_path
                    .parent()
                    .ok_or(io::Error::new(io::ErrorKind::Other,
                           "could not get the typescript directory"))?;
                /* create the path to the javascript module */
                let js_path = ts_dir.join(format!("{}.js", ts_module_name));
                /* create the path to the rust binding */
                let rs_path = path::Path::new("bindings")
                    .join(ts_dir)
                    .join(format!("{}.rs", ts_module_name));
                /* create (all parts of) the directory for the rust bindings output */
                fs::create_dir_all(&rs_path)?;
                /* create the module writer */
                let mut writer = wb::Writer::new(fs::File::create(rs_path)?);
                let imports = process_imports(&ts_module);
                writer.write_imports(&imports)?;
                writer.write_line("\nuse wasm_bindgen::prelude::*;\n")?;
                /* process the components of the typescript module's body */
                for item in &ts_module.body {
                    if let swc_ecma_ast::ModuleItem::ModuleDecl(declaration) = item {
                        if let swc_ecma_ast::ModuleDecl::ExportDecl(export) = declaration {
                            /* skip over deprecated export declarations */
                            if ts_comments
                                .take_leading_comments(export.span.lo())
                                .and_then(|mut v| v.pop())
                                .map_or(false, |c| c.text.contains("@deprecated")) {
                                    continue;
                            }
                            if let swc_ecma_ast::Decl::Class(class_declaration) = &export.decl {
                                let cls_overrides = mod_overrides
                                    .and_then(|m| m.classes.get(&class_declaration.ident.sym.to_string()));
                                // TODO find a more idomatic way to express this
                                if let Some(cls_overrides) = cls_overrides {
                                    if let OverrideMode::Skip = cls_overrides.mode {
                                        continue;
                                    }
                                }
                                let mod_attributes =
                                    vec![(String::from("module"), 
                                          js_path.to_str().and_then(|s| Some(s.to_owned())))];
                                // TODO pass in class overrides here
                                //
                                
                                let mod_class = process_class(class_declaration, &ts_comments);
                                writer.write_module(&wb::ModuleDesc::new(mod_attributes, mod_class))?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_type(ts_type: &swc_ecma_ast::TsType)
    -> Result<wb::TypeDesc, String> {
    match ts_type {
        swc_ecma_ast::TsType::TsArrayType(ts_array_type) => {
            let inner = process_type(&ts_array_type.elem_type)?;
            Ok(wb::TypeDesc::TsArray(Box::new(inner)))
        },
        swc_ecma_ast::TsType::TsTypeRef(ts_type_ref) => {
            if let swc_ecma_ast::TsEntityName::Ident(ident) = &ts_type_ref.type_name {
                /* handle interfaces? */
                if ident.sym.eq_str_ignore_ascii_case("ArrayLike") {
                    if let Some(params) = &ts_type_ref.type_params {
                        let param = &params.params[0];
                        let inner = process_type(&*param)?;
                        Ok(wb::TypeDesc::TsArray(Box::new(inner)))
                    }
                    else {
                        Err("ArrayLike without type annotations".to_owned())
                    }
                }
                else {
                    Ok(wb::TypeDesc::TsClass(ident.sym.to_string()))
                }
            }
            else {
                Err("TsType without an identifer".to_owned())
            }
        },
        swc_ecma_ast::TsType::TsKeywordType(ts_keyword_type) => {
            match ts_keyword_type.kind {
                swc_ecma_ast::TsKeywordTypeKind::TsNumberKeyword =>
                    Ok(wb::TypeDesc::TsNumber),
                swc_ecma_ast::TsKeywordTypeKind::TsNullKeyword => 
                    Ok(wb::TypeDesc::TsNull),
                swc_ecma_ast::TsKeywordTypeKind::TsBooleanKeyword =>
                    Ok(wb::TypeDesc::TsBoolean),
                swc_ecma_ast::TsKeywordTypeKind::TsStringKeyword => 
                    Ok(wb::TypeDesc::TsString),
                swc_ecma_ast::TsKeywordTypeKind::TsAnyKeyword => 
                    Ok(wb::TypeDesc::TsAny),
                swc_ecma_ast::TsKeywordTypeKind::TsVoidKeyword =>
                    Ok(wb::TypeDesc::TsVoid),
                swc_ecma_ast::TsKeywordTypeKind::TsUndefinedKeyword =>
                    Ok(wb::TypeDesc::TsUndefined),
                _ => {
                    Err(format!("cannot process TsKeywordType::{:?}", ts_keyword_type.kind))
                }
            }
        },
        swc_ecma_ast::TsType::TsThisType(_) => {
            Ok(wb::TypeDesc::TsThis)
        },
        swc_ecma_ast::TsType::TsUnionOrIntersectionType(variant) => {
            match variant {
                swc_ecma_ast::TsUnionOrIntersectionType::TsUnionType(ts_union_type) => {
                    let mut ts_types = Vec::with_capacity(ts_union_type.types.len());
                    for ts_type in &ts_union_type.types {
                        ts_types.push(process_type(&**ts_type)?);
                    }
                    Ok(wb::TypeDesc::TsUnion(ts_types))
                },
                swc_ecma_ast::TsUnionOrIntersectionType::TsIntersectionType(ts_intersection_type) => {
                    Err(format!("cannot process TsIntersectionType::{{{:?}}}", ts_intersection_type.types))
                }
            }
        },
        swc_ecma_ast::TsType::TsFnOrConstructorType(fn_or_constructor) => {
            if let swc_ecma_ast::TsFnOrConstructorType::TsFnType(function) = fn_or_constructor {
                let fn_parameters = function.params
                    .iter()
                    .try_fold(Vec::with_capacity(function.params.len()), |mut vec, param| {
                        if let swc_ecma_ast::TsFnParam::Ident(ident) = param {
                            if let Some(type_ann) = &ident.type_ann {
                                let fn_param_type_desc = process_type(&type_ann.type_ann)?;
                                vec.push((ident.sym.to_snake_case(), fn_param_type_desc));
                                Ok(vec)
                            }
                            else {
                                Err(format!("TsFnOrConstructorType: {}", ident.sym))
                            }
                        }
                        else {
                            Err(format!("TsFnOrConstructorType"))
                        }
                    })?;
                let fn_return_type = Box::new(process_type(&function.type_ann.type_ann)?);
                Ok(wb::TypeDesc::TsFunction(fn_parameters, Some(fn_return_type)))
            }
            else {
                Err(format!("cannot process TsType::{:?}", ts_type))
            }
        },
        _ => {
            Ok(wb::TypeDesc::Unimplemented)
        }
    }
}

fn process_parameter(parameter: &swc_ecma_ast::Param)
    -> Result<(String, wb::ParamDesc), String> {
    if let swc_ecma_ast::Pat::Ident(identifier) = &parameter.pat {
        if let Some(ts_type) = &identifier.type_ann {
            let name = identifier.sym.to_snake_case();
            let type_desc = process_type(&ts_type.type_ann)?;
            Ok((name, wb::ParamDesc::new(type_desc, false, identifier.optional)))
        }
        else {
            Err("cannot process parameter without type annotation".to_owned())
        }
    }
    else {
        Err(format!("cannot process parameter without identifier {:?}", parameter))
    }
}

fn process_function(name: &str,
                    attributes: Vec<(String, Option<String>)>,
                    parameters: &[&swc_ecma_ast::Param],
                    return_type: &Option<&swc_ecma_ast::TsType>)
    -> Result<wb::FunctionDesc, String> {
    /* process the parameters */
    let fn_arguments = parameters
        .iter()
        .map(|p| process_parameter(p))
        .collect::<Result<Vec<_>, _>>()?;
    /* process return type */
    if let Some(return_type) = return_type {
        let mut return_type = process_type(&return_type)?;
        // TODO do not filter out TsVoid here
        if matches!(return_type, wb::TypeDesc::TsVoid) {
            Ok(wb::FunctionDesc::new(attributes,
                name.to_owned(),
                fn_arguments,
                None))
        }
        else {
            let mut optional = false;
            /* handle special option case */
            if let wb::TypeDesc::TsUnion(union) = &mut return_type {
                match &union[..] {
                    [_, wb::TypeDesc::TsNull] | [_, wb::TypeDesc::TsUndefined] => {
                        return_type = union.remove(0);
                        optional = true;
                    },
                    _ => {}
                }
            }
            let return_param = wb::ParamDesc::new(return_type, false, optional);
            Ok(wb::FunctionDesc::new(attributes,
                name.to_owned(),
                fn_arguments,
                Some(return_param)))
        }
    }
    else {
        Ok(wb::FunctionDesc::new(attributes,
            name.to_owned(),
            fn_arguments,
            None))
    }
}

fn process_class(class_declaration: &swc_ecma_ast::ClassDecl, 
                 comments: &swc_common::comments::Comments) -> wb::ClassDesc {
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
                let fn_parameters : Vec<&swc_ecma_ast::Param> = constructor
                    .params
                    .iter()
                    .filter_map(|p| match p {
                        swc_ecma_ast::ParamOrTsParamProp::Param(param) => Some(param),
                        _ => None,
                    })
                    .collect();
                let fn_desc = 
                    process_function(&fn_name, fn_attributes, &fn_parameters, &None);
                match fn_desc {
                    Ok(mut fn_desc) => {
                        let fn_return_type = 
                            wb::ParamDesc::new(wb::TypeDesc::TsThis, false, false);
                            fn_desc.returns = Some(fn_return_type);
                        cls_methods.push(fn_desc);
                    },
                    Err(error) => {
                        panic!(format!("Error processing {}::{}: {}", cls_name, fn_name, error));
                    }
                }
            },
            swc_ecma_ast::ClassMember::Method(class_method) => {
                if class_method.kind == swc_ecma_ast::MethodKind::Method {
                    let fn_deprecated = comments
                        .take_leading_comments(class_method.span.lo())
                        .and_then(|mut v| v.pop())
                        .and_then(|c| Some(c.text.contains("@deprecated")))
                        .unwrap_or(false);
                    if fn_deprecated {
                        continue;
                    }
                    let function = &class_method.function;
                    if let swc_ecma_ast::PropName::Ident(ident) = &class_method.key {
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
                        let fn_desc =
                            process_function(&fn_name, fn_attributes, &fn_parameters, &fn_return_type);
                        match fn_desc {
                            Ok(mut fn_desc) => {
                                let this_param = wb::ParamDesc::new(wb::TypeDesc::TsThis, true, false);
                                fn_desc.arguments.insert(0, (String::from("this"), this_param));
                                cls_methods.push(fn_desc);
                            },
                            Err(error) => {
                                panic!(format!("Error processing {}::{}: {}", cls_name, fn_name, error));
                            }
                        }
                    }
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
use swc_common::{errors::{ColorConfig, Handler},SourceMap,};
use swc_ecma_parser::{lexer::Lexer, Parser, Session, SourceFileInput, Syntax, TsConfig};
use std::path::Path;
use std::sync::Arc;

pub use swc_ecma_ast as ast;

pub fn parse_module(path: &Path) -> Result<swc_ecma_ast::Module, String> {
    let source_map: Arc<SourceMap> = Default::default();
    let handler =
        Handler::with_tty_emitter(ColorConfig::Auto,
                                  true,
                                  false,
                                  Some(source_map.clone()));
    let session = Session { handler: &handler };
    let source = source_map
        .load_file(path)
        .map_err(|error| {
            if let Some(path) = path.to_str() {
                format!("{}: {}", path, error.to_string())
            }
            else {
                format!("{}", error.to_string())
            }
        })?;
    let lexer = Lexer::new(
        session,
        Syntax::Typescript(TsConfig {dts: true, ..Default::default()}),
        Default::default(),
        SourceFileInput::from(&*source),
        None,
    );
    let mut parser = Parser::new_from(session, lexer);
    parser
        .parse_module()
        .map_err(|error| {
            if let Some(path) = path.to_str() {
                format!("{}: {}", path, error.message())
            }
            else {
                format!("{}", error.message())
            }
        })
}


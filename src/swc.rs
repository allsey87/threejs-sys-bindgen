use swc_common::{comments, errors::{ColorConfig, Handler}, SourceMap};
use swc_ecma_parser::{lexer::Lexer, Parser, Session, SourceFileInput, Syntax, TsConfig};
use std::{io, path, sync};

pub fn parse_module(path: &path::Path) -> 
Result<(swc_ecma_ast::Module, swc_common::comments::Comments), io::Error> {
    let source_map: sync::Arc<SourceMap> = Default::default();
    let handler =
        Handler::with_tty_emitter(ColorConfig::Auto,
                                  true,
                                  false,
                                  Some(source_map.clone()));
    let session = Session { handler: &handler };
    let source = source_map.load_file(path)?;
    let comments : comments::Comments = Default::default();
    let lexer = Lexer::new(
        session,
        Syntax::Typescript(TsConfig {dts: true, ..Default::default()}),
        Default::default(),
        SourceFileInput::from(&*source),
        Some(&comments),
    );
    let mut parser = Parser::new_from(session, lexer);
    parser
        .parse_module()
        .map_err(|error| {
            io::Error::new(io::ErrorKind::Other,
            format!("{:?}: {}", path.to_str(), error.message()))
        })
        .and_then(|m| Ok((m, comments)))
}


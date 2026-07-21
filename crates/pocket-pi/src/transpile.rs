//! Runtime TypeScript → JavaScript, the native op behind Pocket Pi's plugin
//! loader.
//!
//! Real pi loads its extensions — TypeScript files — at runtime through `jiti`
//! (`jiti` + `typescript`, a heavy Node toolchain). Pocket Pi has no Node, so
//! per the runtime's own rule ("heavy Node deps become native ops") it strips
//! types with **oxc**, a pure-Rust compiler, and hands the resulting JS to
//! QuickJS. Only type-erasure + light syntax lowering is needed: QuickJS is
//! ES2023, so we keep modern syntax and just remove the TypeScript.

use oxc_allocator::Allocator;
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{TransformOptions, Transformer};
use std::path::Path;

/// Transpile a TypeScript source string to JavaScript. `filename` only informs
/// the source-type detection (`.ts` / `.tsx`) and diagnostics.
pub fn transpile_ts(filename: &str, source: &str) -> Result<String, String> {
    let allocator = Allocator::default();
    let path = Path::new(filename);
    let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::ts());

    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked || !parsed.diagnostics.is_empty() {
        let msgs: Vec<String> = parsed.diagnostics.iter().map(|d| d.to_string()).collect();
        return Err(format!("TypeScript parse error: {}", msgs.join("; ")));
    }

    let mut program = parsed.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let result = Transformer::new(&allocator, path, &TransformOptions::default())
        .build_with_scoping(scoping, &mut program);
    if !result.diagnostics.is_empty() {
        let msgs: Vec<String> = result.diagnostics.iter().map(|d| d.to_string()).collect();
        return Err(format!("TypeScript transform error: {}", msgs.join("; ")));
    }

    Ok(Codegen::new().build(&program).code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_types_keeps_logic() {
        let js = transpile_ts(
            "p.ts",
            "type N = number;\nexport default (a: N, b: N): N => a + b;",
        )
        .unwrap();
        assert!(js.contains("export default"), "got: {js}");
        assert!(!js.contains(": N"), "types not stripped: {js}");
        assert!(!js.contains("type N"), "type alias not stripped: {js}");
    }

    #[test]
    fn reports_parse_errors() {
        let err = transpile_ts("p.ts", "export default function( {{{ ").unwrap_err();
        assert!(err.contains("parse"), "got: {err}");
    }
}

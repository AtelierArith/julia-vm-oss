use std::collections::HashMap;

use crate::compile::precompile::{deserialize_base_cache, serialize_base_cache};

use super::{cached_base_from_serialized, clear_cache, compile_base_functions_from_source};

struct ExplicitCachedBaseCleanup10969;

impl Drop for ExplicitCachedBaseCleanup10969 {
    fn drop(&mut self) {
        clear_cache();
    }
}

fn run_with_explicit_cached_base_10969(source: &str) -> Result<String, String> {
    clear_cache();
    let _cleanup = ExplicitCachedBaseCleanup10969;
    let fresh_base = compile_base_functions_from_source().map_err(|err| err.to_string())?;
    let bytes = serialize_base_cache(
        &fresh_base.compiled,
        &fresh_base.method_tables,
        &fresh_base.closure_captures,
        &fresh_base.inference_results,
    )
    .map_err(|err| err.to_string())?;
    let restored = deserialize_base_cache(&bytes).map_err(|err| err.to_string())?;
    let cached_base = cached_base_from_serialized(restored, "issue-10969-test");

    let program = crate::pipeline::parse_and_lower(source).map_err(|err| err.to_string())?;
    let output = crate::compile::compile_core_program_internal(
        &program,
        &HashMap::new(),
        &HashMap::new(),
        crate::compile::CompilerCacheInput {
            precompiled_base: Some(&cached_base.compiled),
            method_tables: Some(&cached_base.method_tables),
            closure_captures: Some(&cached_base.closure_captures),
            inference_results: Some(&cached_base.inference_results),
            ..Default::default()
        },
    )
    .map_err(|err| err.to_string())?;
    crate::test_runtime::run_compiled_program_raw(output.compiled, 42)
        .map_err(|err| err.to_string())
}

pub(super) fn cached_base_parametric_inner_origin_normalizes_rational_10969() -> Result<(), String>
{
    let source = r#"
dynamic_rational_10969(x::T) where {T<:Integer} = Rational{T}(x, x + x)
r = dynamic_rational_10969(2)
println(r.num)
println(r.den)
"#;
    assert_eq!(run_with_explicit_cached_base_10969(source)?, "1\n2\n");
    Ok(())
}

//! `subset_julia_vm_lowering` — Julia CST to Core IR lowering.
//!
//! This crate is the shared lowering layer below both the bytecode compiler
//! and interpreter VM. VM-backed macro execution and embedded source lookup
//! are installed through narrow traits by the integration crate, keeping this
//! crate free of upward compiler/VM dependencies (Issue #9090).

pub use subset_julia_vm_ir::{error, span};
pub use subset_julia_vm_types::{inference_core, ir, types};

pub mod expr_heads;
pub mod include;
pub mod lowering;
pub mod module_names;
pub mod parser;

/// Static stdlib-name classification used while lowering calls.
pub mod stdlib {
    /// Returns whether `name` is a bundled stdlib module understood by the
    /// lowering layer. Source retrieval remains integration-owned.
    pub fn is_stdlib_module(name: &str) -> bool {
        matches!(
            name,
            "Statistics"
                | "Test"
                | "Random"
                | "InteractiveUtils"
                | "Dates"
                | "LinearAlgebra"
                | "Iterators"
                | "Broadcast"
                | "Printf"
                | "Base64"
        )
    }
}

// A bounded parse/lower seam for lowering's own unit tests. Production
// whole-program loading, prelude merging, and package loading stay in the
// integration crate's `pipeline` module.
#[cfg(test)]
pub mod pipeline {
    use std::path::PathBuf;

    use crate::ir::core::Program;
    use crate::lowering::{IncludeContext, Lowering, LoweringWithInclude};
    use crate::parser::Parser;

    pub fn parse_source(src: &str) -> Result<Program, String> {
        let mut parser = Parser::new().map_err(|err| err.to_string())?;
        let outcome = parser.parse(src).map_err(|err| err.to_string())?;
        Lowering::new(src)
            .lower(outcome)
            .map_err(|err| format!("{err:?}"))
    }

    pub fn parse_and_lower(src: &str) -> Result<Program, String> {
        parse_source(src)
    }

    pub fn parse_source_with_include(
        src: &str,
        base_dir: Option<PathBuf>,
    ) -> Result<Program, String> {
        let mut parser = Parser::new().map_err(|err| err.to_string())?;
        let outcome = parser.parse(src).map_err(|err| err.to_string())?;
        LoweringWithInclude::new(src, IncludeContext::new(base_dir))
            .lower(outcome)
            .map_err(|err| format!("{err:?}"))
    }
}

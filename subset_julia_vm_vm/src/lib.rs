//! SubsetJuliaVM bytecode interpreter.

#![deny(clippy::print_stderr)]

pub use subset_julia_vm_bytecode as bytecode;
pub use subset_julia_vm_bytecode::{builtins, intrinsics, rng, runtime_constants, runtime_types};
pub use subset_julia_vm_ir::{error, span};
pub use subset_julia_vm_lowering::{expr_heads, include, lowering, module_names, parser};
pub use subset_julia_vm_types::{inference_core, ir, promotion, types};

pub mod host;
pub mod plotting;
pub mod register_vm;
pub mod vm;

pub mod cancel {
    pub fn is_requested() -> bool {
        crate::host::get().is_some_and(|host| host.is_cancel_requested())
    }
}

pub mod julia {
    pub mod packages {
        pub fn get_package_file(normalized_path: &str) -> Option<&'static str> {
            crate::host::get().and_then(|host| host.package_file(normalized_path))
        }
    }
}

#[cfg(test)]
pub mod api {
    pub use subset_julia_vm::api::*;
}

#[cfg(test)]
pub mod test_runtime {
    use subset_julia_vm_bytecode::CompiledProgram;

    fn lower(source: &str) -> crate::ir::core::Program {
        subset_julia_vm::macro_runtime::install();
        let mut parser = crate::parser::Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = crate::lowering::Lowering::new(source);
        lowering.lower(parsed).expect("lower source")
    }

    pub fn compile_source_with_cache(source: &str) -> CompiledProgram {
        subset_julia_vm_compile::compile::integration_support::compile_with_cache(&lower(source))
            .expect("compile source")
    }

    pub fn compile_core_source(source: &str) -> CompiledProgram {
        subset_julia_vm_compile::compile::integration_support::compile_core_program(&lower(source))
            .expect("compile source")
    }
}

#[cfg(test)]
pub mod compile {
    pub use subset_julia_vm_compile::compile::ssa_ir;

    pub mod test_helpers {
        use crate::ir::core::Expr;
        use crate::span::Span;

        pub fn zero_span() -> Span {
            Span::new(0, 0, 0, 0, 0, 0)
        }

        pub fn var_expr(name: &str) -> Expr {
            Expr::Var(name.to_string().into(), zero_span())
        }

        pub fn call_expr(function: &str, args: Vec<Expr>) -> Expr {
            Expr::Call {
                function: function.to_string().into(),
                args,
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span: zero_span(),
            }
        }
    }
}

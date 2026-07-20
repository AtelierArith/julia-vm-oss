//! Julia Core IR to SubsetJuliaVM bytecode compiler.

#![deny(clippy::print_stderr)]

pub use subset_julia_vm_bytecode as bytecode;
pub use subset_julia_vm_bytecode::runtime_types;
pub use subset_julia_vm_bytecode::{builtins, intrinsics, runtime_constants};
pub use subset_julia_vm_ir::{error, span};
pub use subset_julia_vm_lowering::{expr_heads, lowering, module_names, parser};
pub use subset_julia_vm_types::{inference_core, ir, promotion, types};

pub mod compile;
pub mod host;

pub fn get_prelude_program() -> Option<&'static ir::core::Program> {
    host::get().and_then(|host| host.prelude_program())
}

pub mod base_loader {
    pub fn get_base_program() -> Option<&'static crate::ir::core::Program> {
        crate::host::get().and_then(|host| host.base_program())
    }
}

pub mod julia {
    pub mod base {
        pub fn exported_names() -> &'static [String] {
            crate::host::get()
                .map(|host| host.base_exported_names())
                .unwrap_or(&[])
        }

        pub fn is_exported(name: &str) -> bool {
            crate::host::get().is_some_and(|host| host.base_is_exported(name))
        }

        pub fn declares_module(name: &str) -> bool {
            crate::host::get().is_some_and(|host| host.base_declares_module(name))
        }
    }
}

pub mod stdlib_loader {
    pub fn load_stdlib_modules(
        usings: &[crate::ir::core::UsingImport],
    ) -> Vec<crate::ir::core::Module> {
        crate::host::get()
            .map(|host| host.load_stdlib_modules(usings))
            .unwrap_or_default()
    }
}

pub mod base {
    pub fn get_prelude() -> String {
        crate::host::get()
            .map(|host| host.prelude_source())
            .unwrap_or_default()
    }
}

pub mod stdlib {
    pub fn get_stdlib_package(name: &str) -> Option<crate::host::PackageSource> {
        crate::host::get().and_then(|host| host.stdlib_package(name))
    }
}

pub mod packages {
    pub fn get_bundled_package(name: &str) -> Option<crate::host::PackageSource> {
        crate::host::get().and_then(|host| host.bundled_package(name))
    }
}

pub mod pipeline {
    use std::path::PathBuf;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum SoftScopeMode {
        Lenient,
        Strict,
    }

    pub fn parse_and_lower(source: &str) -> Result<crate::ir::core::Program, String> {
        parse_and_lower_with_base_dir_mode(source, None, SoftScopeMode::Lenient, None)
    }

    pub fn parse_and_lower_with_base_dir_mode(
        source: &str,
        base_dir: Option<PathBuf>,
        mode: SoftScopeMode,
        _script_path: Option<&str>,
    ) -> Result<crate::ir::core::Program, String> {
        #[cfg(test)]
        subset_julia_vm::macro_runtime::install();
        let host = crate::host::get().ok_or_else(|| "compile host is not installed".to_string())?;
        host.parse_and_lower(source, base_dir, mode == SoftScopeMode::Strict)
    }
}

#[cfg(test)]
pub mod api {
    pub fn compile_and_run_str(src: &str, seed: u64) -> f64 {
        let Ok(program) = crate::pipeline::parse_and_lower(src) else {
            return f64::NAN;
        };
        let Ok(compiled) = crate::compile::integration_support::compile_with_cache(&program) else {
            return f64::NAN;
        };
        if crate::test_runtime::run_compiled_program(compiled, seed).is_ok() {
            0.0
        } else {
            f64::NAN
        }
    }
}

#[cfg(test)]
pub mod vm_bytecode_file {
    pub use subset_julia_vm::vm_bytecode_file::*;
}

#[cfg(test)]
pub mod test_runtime {
    use subset_julia_vm_bytecode::CompiledProgram;

    pub fn run_compiled_program(compiled: CompiledProgram, seed: u64) -> Result<String, String> {
        run_compiled_program_raw(compiled, seed).map_err(|error| format!("run error: {error:?}"))
    }

    pub fn run_compiled_program_raw(
        compiled: CompiledProgram,
        seed: u64,
    ) -> Result<String, subset_julia_vm_bytecode::VmError> {
        subset_julia_vm::macro_runtime::install();
        let mut vm = subset_julia_vm::vm::Vm::new_program(
            compiled,
            subset_julia_vm_bytecode::rng::StableRng::new(seed),
        );
        vm.run()?;
        Ok(vm.get_output().to_string())
    }
}

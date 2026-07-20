//! Integration services required by cache generation and source loading.

use crate::ir::core::{Module, Program, UsingImport};
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub struct PackageSource {
    pub project_toml: &'static str,
    pub source: &'static str,
}

pub trait CompileHost: Sync {
    fn prelude_program(&self) -> Option<&'static Program>;
    fn base_program(&self) -> Option<&'static Program>;
    fn base_exported_names(&self) -> &'static [String];
    fn base_is_exported(&self, name: &str) -> bool;
    fn base_declares_module(&self, name: &str) -> bool;
    fn load_stdlib_modules(&self, usings: &[UsingImport]) -> Vec<Module>;
    fn prelude_source(&self) -> String;
    fn stdlib_package(&self, name: &str) -> Option<PackageSource>;
    fn bundled_package(&self, name: &str) -> Option<PackageSource>;
    fn parse_and_lower(
        &self,
        source: &str,
        base_dir: Option<PathBuf>,
        strict_soft_scope: bool,
    ) -> Result<Program, String>;
}

static HOST: OnceLock<&'static dyn CompileHost> = OnceLock::new();

pub fn install(host: &'static dyn CompileHost) {
    let _ = HOST.set(host);
}

pub fn get() -> Option<&'static dyn CompileHost> {
    #[cfg(test)]
    install(&TEST_HOST);
    HOST.get().copied()
}

#[cfg(test)]
struct TestHost;

#[cfg(test)]
impl CompileHost for TestHost {
    fn prelude_program(&self) -> Option<&'static Program> {
        subset_julia_vm::get_prelude_program()
    }

    fn base_program(&self) -> Option<&'static Program> {
        subset_julia_vm::base_loader::get_base_program()
    }

    fn base_exported_names(&self) -> &'static [String] {
        subset_julia_vm::macro_runtime::compile_host_base_exported_names()
    }

    fn base_is_exported(&self, name: &str) -> bool {
        subset_julia_vm::macro_runtime::compile_host_base_is_exported(name)
    }

    fn base_declares_module(&self, name: &str) -> bool {
        subset_julia_vm::macro_runtime::compile_host_base_declares_module(name)
    }

    fn load_stdlib_modules(&self, usings: &[UsingImport]) -> Vec<Module> {
        subset_julia_vm::stdlib_loader::load_stdlib_modules(usings)
    }

    fn prelude_source(&self) -> String {
        subset_julia_vm::base::get_prelude()
    }

    fn stdlib_package(&self, name: &str) -> Option<PackageSource> {
        subset_julia_vm::stdlib::get_stdlib_package(name).map(|package| PackageSource {
            project_toml: package.project_toml,
            source: package.source,
        })
    }

    fn bundled_package(&self, name: &str) -> Option<PackageSource> {
        subset_julia_vm::packages::get_bundled_package(name).map(|package| PackageSource {
            project_toml: package.project_toml,
            source: package.source,
        })
    }

    fn parse_and_lower(
        &self,
        source: &str,
        base_dir: Option<PathBuf>,
        strict_soft_scope: bool,
    ) -> Result<Program, String> {
        let mode = if strict_soft_scope {
            subset_julia_vm::pipeline::SoftScopeMode::Strict
        } else {
            subset_julia_vm::pipeline::SoftScopeMode::Lenient
        };
        subset_julia_vm::pipeline::parse_and_lower_with_base_dir_mode(source, base_dir, mode, None)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
static TEST_HOST: TestHost = TestHost;

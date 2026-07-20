//! Host-facing compiler cache operations.
//!
//! This facade keeps CLI/FFI/Web integration code from depending on the
//! `compile::cache` implementation module directly.

use crate::ir::core::Program;
use subset_julia_vm_bytecode::CompiledProgram;

use super::CResult;

pub fn compile_core_program(program: &Program) -> CResult<CompiledProgram> {
    super::compile_core_program(program)
}

pub fn compile_with_cache(program: &Program) -> CResult<CompiledProgram> {
    super::compile_with_cache(program)
}

pub fn begin_warm_start_prefetch() {
    super::cache::begin_warm_start_prefetch();
}

pub fn warm_base_cache() {
    super::cache::warm_base_cache();
}

pub fn warm_start_compile_cache() {
    begin_warm_start_prefetch();
    warm_base_cache();
}

pub fn base_cache_debug_status() -> super::cache::BaseCacheDebugStatus {
    super::cache::base_cache_debug_status()
}

pub fn is_compile_cache_initialized() -> bool {
    super::cache::is_cache_initialized()
}

pub fn clear_compile_cache() {
    super::cache::clear_cache();
}

pub fn clear_non_base_compile_cache() {
    super::cache::clear_non_base_cache();
}

pub fn clear_program_compile_cache() {
    super::cache::clear_program_cache();
}

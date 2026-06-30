//! Pipeline logic for parsing and lowering Julia source code.
//!
//! This module handles the transformation pipeline:
//! Julia source → Parser → CST → Lowering → Core IR

use crate::error::{SyntaxError, UnsupportedFeature};
use crate::ir::core::{MetaAnnotation, Program, Stmt, BASE_USER_MAIN_BOUNDARY_META};
use crate::julia::base;
use crate::loader::{LoadError, LoaderConfig, PackageLoader};
use crate::lowering::{Lowering, LoweringWithInclude};
use crate::parser::Parser;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Error variants produced by the parse-and-lower pipeline.
#[derive(Debug)]
pub enum PipelineError {
    /// Source code failed to parse.
    Parse(SyntaxError),
    /// Lowering to Core IR failed due to an unsupported feature.
    Lower(UnsupportedFeature),
    /// Loading a stdlib/package referenced by `using` failed.
    Load(LoadError),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Parse(e) => write!(f, "parse error: {}", e),
            PipelineError::Lower(e) => write!(f, "lowering error: {:?}", e),
            PipelineError::Load(e) => write!(f, "load error: {}", e),
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

/// Result of parsing and lowering Julia source code.
pub type PipelineResult = Result<Program, PipelineError>;

const PRELUDE_PROGRAM_CACHE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct SerializedPreludeProgram {
    version: u32,
    source_hash: String,
    program: Program,
}

struct PersistentPreludeLock {
    path: PathBuf,
}

impl Drop for PersistentPreludeLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Cached prelude program (parsed and lowered once per process).
static PRELUDE_PROGRAM: Lazy<Option<Program>> = Lazy::new(load_prelude_program);

/// Get the Base program (used by compile_core_program)
pub fn get_prelude_program() -> Option<&'static Program> {
    PRELUDE_PROGRAM.as_ref()
}

fn compute_prelude_source_hash() -> String {
    // Include the same compiler/VM source fingerprint as the Base bytecode
    // cache. The prelude Program cache stores lowered IR, so lowering changes
    // can invalidate it even when the Julia prelude source is unchanged
    // (Issue #7544).
    static HASH: Lazy<String> = Lazy::new(|| {
        let mut hasher = sha2::Sha256::new();
        hasher.update(crate::compile::precompile::compute_prelude_hash().as_bytes());
        hasher.update(b"\0");
        hasher.update(env!("SJULIA_BASE_CACHE_BUILD_HASH").as_bytes());
        format!("{:x}", hasher.finalize())
    });
    HASH.clone()
}

fn persistent_prelude_cache_disabled() -> bool {
    env::var("SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE").is_ok()
}

fn workspace_target_dir() -> PathBuf {
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir);
    }

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("target"))
        .unwrap_or_else(|| PathBuf::from("target"))
}

fn persistent_prelude_cache_path() -> PathBuf {
    let hash = compute_prelude_source_hash();
    workspace_target_dir().join(format!("sjulia_prelude_program_{hash}.bin"))
}

fn parse_prelude_from_source() -> Option<Program> {
    let prelude_src = base::get_prelude();
    parse_source(&prelude_src).ok()
}

fn serialize_prelude_program(program: &Program) -> Result<Vec<u8>, String> {
    let cache = SerializedPreludeProgram {
        version: PRELUDE_PROGRAM_CACHE_VERSION,
        source_hash: compute_prelude_source_hash(),
        program: program.clone(),
    };
    bincode::serialize(&cache).map_err(|e| format!("Prelude serialization failed: {}", e))
}

fn deserialize_prelude_program(bytes: &[u8]) -> Result<Program, String> {
    let cache: SerializedPreludeProgram = bincode::deserialize(bytes)
        .map_err(|e| format!("Prelude deserialization failed: {}", e))?;

    if cache.version != PRELUDE_PROGRAM_CACHE_VERSION {
        return Err(format!(
            "Prelude cache version mismatch: expected {}, got {}",
            PRELUDE_PROGRAM_CACHE_VERSION, cache.version
        ));
    }
    if cache.source_hash != compute_prelude_source_hash() {
        return Err("Prelude source hash mismatch".to_string());
    }

    Ok(cache.program)
}

fn embedded_prelude_program_bytes() -> Option<&'static [u8]> {
    #[cfg(has_embedded_prelude_program)]
    {
        Some(include_bytes!(env!("SJULIA_PRELUDE_PROGRAM_CACHE_PATH")))
    }
    #[cfg(not(has_embedded_prelude_program))]
    {
        None
    }
}

fn load_embedded_prelude_program() -> Option<Program> {
    let bytes = embedded_prelude_program_bytes()?;
    deserialize_prelude_program(bytes).ok()
}

fn read_persistent_prelude_cache(path: &Path) -> Option<Program> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return None,
        Err(_) => return None,
    };

    match deserialize_prelude_program(&bytes) {
        Ok(program) => Some(program),
        Err(_) => {
            let _ = fs::remove_file(path);
            None
        }
    }
}

fn acquire_persistent_prelude_lock(cache_path: &Path) -> Option<PersistentPreludeLock> {
    let lock_path = cache_path.with_extension("lock");
    let stale_after = Duration::from_secs(20 * 60);

    for _ in 0..1200 {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => {
                return Some(PersistentPreludeLock { path: lock_path });
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                if let Ok(metadata) = fs::metadata(&lock_path) {
                    let is_stale = metadata
                        .modified()
                        .ok()
                        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                        .is_some_and(|age| age > stale_after);
                    if is_stale {
                        let _ = fs::remove_file(&lock_path);
                    }
                }
                if cache_path.exists() {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }

    None
}

fn write_persistent_prelude_cache(path: &Path, program: &Program) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let Ok(bytes) = serialize_prelude_program(program) else {
        return;
    };

    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    if fs::write(&tmp_path, bytes).is_err() {
        return;
    }
    if fs::rename(&tmp_path, path).is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
}

fn load_prelude_program() -> Option<Program> {
    if persistent_prelude_cache_disabled() {
        return parse_prelude_from_source();
    }

    if let Some(program) = load_embedded_prelude_program() {
        return Some(program);
    }

    let cache_path = persistent_prelude_cache_path();
    if let Some(program) = read_persistent_prelude_cache(&cache_path) {
        return Some(program);
    }

    let Some(_lock) = acquire_persistent_prelude_lock(&cache_path) else {
        return read_persistent_prelude_cache(&cache_path).or_else(parse_prelude_from_source);
    };

    if let Some(program) = read_persistent_prelude_cache(&cache_path) {
        return Some(program);
    }

    let program = parse_prelude_from_source()?;
    write_persistent_prelude_cache(&cache_path, &program);
    Some(program)
}

/// Generate and serialize the parsed/lowered prelude Program for build-time embedding.
pub fn generate_prelude_program_cache() -> Result<Vec<u8>, String> {
    let program = parse_prelude_from_source().ok_or("Failed to parse and lower prelude")?;
    serialize_prelude_program(&program)
}

/// Parse source code without prelude merging (used for prelude itself)
pub fn parse_source(src: &str) -> PipelineResult {
    let mut parser = Parser::new().map_err(|e| {
        PipelineError::Parse(SyntaxError::parse_failed(format!(
            "Parser initialization failed: {}",
            e
        )))
    })?;

    let outcome = parser.parse(src).map_err(PipelineError::Parse)?;

    let mut lowering = Lowering::new(src);
    lowering.lower(outcome).map_err(PipelineError::Lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prelude_program_cache_roundtrips() {
        let bytes = generate_prelude_program_cache().expect("generate prelude cache");
        let program = deserialize_prelude_program(&bytes).expect("deserialize prelude cache");
        assert!(
            !program.functions.is_empty(),
            "prelude cache should contain lowered Base functions"
        );
        assert_eq!(
            program.base_function_count, 0,
            "raw prelude Program is not merged into a user Program yet"
        );
    }
}

/// Parse and lower Julia source code using tree-sitter pipeline.
/// Merges prelude functions and structs with user code.
pub fn parse_and_lower(src: &str) -> PipelineResult {
    parse_and_lower_with_base_dir(src, None)
}

/// Parse and lower Julia source code with include support.
/// The base_dir is used to resolve relative paths in include() calls.
pub fn parse_and_lower_with_base_dir(src: &str, base_dir: Option<PathBuf>) -> PipelineResult {
    use crate::compile::profile;

    // Parse user source code with include support
    let mut user_program = profile::time_immediate("pipeline.parse_user", || {
        parse_source_with_include(src, base_dir)
    })?;

    // Force the prelude Lazy first so the merge timing below reflects only the
    // per-run merge cost, not the one-time prelude deserialize (Issue #6348).
    let prelude_ref =
        profile::time_immediate("pipeline.prelude_program_load", || PRELUDE_PROGRAM.as_ref());

    // Merge prelude program (structs first, then functions)
    if let Some(prelude) = prelude_ref {
        profile::time_immediate("pipeline.merge_prelude", || {
            merge_prelude_into_user_program(prelude, &mut user_program);
        });
    }

    let existing_modules: std::collections::HashSet<String> = user_program
        .modules
        .iter()
        .map(|m| m.name.clone())
        .collect();
    let usings_to_load: Vec<crate::ir::core::UsingImport> = user_program
        .usings
        .iter()
        .filter(|u| !u.is_relative && !existing_modules.contains(&u.module))
        .cloned()
        .collect();

    // Load stdlib/packages referenced by `using` statements that are not defined inline.
    let mut package_loader = PackageLoader::new(LoaderConfig::from_env());
    let loaded_modules = package_loader
        .load_for_usings(&usings_to_load)
        .map_err(PipelineError::Load)?;

    for module in loaded_modules {
        if !existing_modules.contains(&module.name) {
            user_program.modules.push(module);
        }
    }

    Ok(user_program)
}

/// Merge the (process-wide cached) prelude `Program` into a freshly lowered
/// user `Program`: structs, functions (exact-signature replacement, Issue
/// #2719), abstract types, and main-block statements with the Base/user
/// boundary meta marker.
fn merge_prelude_into_user_program(prelude: &Program, user_program: &mut Program) {
    // Helper function to get method signature (name + parameter types)
    // This allows multiple dispatch - same name with different parameter types
    fn get_method_signature(func: &crate::ir::core::Function) -> String {
        let param_types: Vec<String> = func
            .params
            .iter()
            .map(|p| p.effective_type().to_string())
            .collect();
        format!("{}({})", func.name, param_types.join(", "))
    }

    // Collect user method signatures to avoid conflicts (for Base extensions)
    let user_method_sigs: std::collections::HashSet<_> = user_program
        .functions
        .iter()
        .map(get_method_signature)
        .collect();

    // Collect user struct names to avoid conflicts
    let user_struct_names: std::collections::HashSet<String> = user_program
        .structs
        .iter()
        .map(|s| s.name.clone())
        .collect();

    // Merge structs (prelude first, but skip if user defines same name)
    let mut all_structs: Vec<crate::ir::core::StructDef> = prelude
        .structs
        .iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .cloned()
        .collect();
    all_structs.extend(std::mem::take(&mut user_program.structs));
    user_program.structs = all_structs;

    // Merge functions:
    // Filter by exact signature to support multiple dispatch (Issue #2719).
    // User-defined methods only replace base methods with the SAME signature,
    // preserving all other overloads. This matches Julia semantics where
    // defining a new method adds to (or replaces an exact match in) the
    // method table, never removing unrelated overloads.
    let mut all_functions: Vec<crate::ir::core::Function> = prelude
        .functions
        .iter()
        .filter(|f| !user_method_sigs.contains(&get_method_signature(f)))
        .cloned()
        .collect();
    // Track base function count BEFORE adding user functions
    let base_function_count = all_functions.len();
    all_functions.extend(std::mem::take(&mut user_program.functions));
    user_program.functions = all_functions;
    user_program.base_function_count = base_function_count;

    // Merge abstract types (prelude first, skip if user defines same name)
    let user_abstract_type_names: std::collections::HashSet<String> = user_program
        .abstract_types
        .iter()
        .map(|a| a.name.clone())
        .collect();
    let mut all_abstract_types: Vec<crate::ir::core::AbstractTypeDef> = prelude
        .abstract_types
        .iter()
        .filter(|a| !user_abstract_type_names.contains(a.name.as_str()))
        .cloned()
        .collect();
    all_abstract_types.extend(std::mem::take(&mut user_program.abstract_types));
    user_program.abstract_types = all_abstract_types;

    // Merge Base/prelude modules before user modules so bundled Base submodules
    // such as `Base.Order` participate in module metadata and initialization.
    let mut all_modules: Vec<crate::ir::core::Module> = prelude.modules.clone();
    all_modules.extend(std::mem::take(&mut user_program.modules));
    user_program.modules = all_modules;

    // Merge main blocks: prelude main block first (defines globals like `im`, const arrays, etc.)
    // then user program main block follows.
    // This ensures prelude const definitions are available to all functions.
    let mut merged_main_stmts = prelude.main.stmts.clone();
    merged_main_stmts.push(Stmt::Meta {
        annotation: MetaAnnotation {
            name: BASE_USER_MAIN_BOUNDARY_META.to_string(),
            args: Vec::new(),
        },
        span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
    });
    merged_main_stmts.extend(std::mem::take(&mut user_program.main.stmts));
    user_program.main = crate::ir::core::Block {
        stmts: merged_main_stmts,
        span: user_program.main.span,
    };
}

/// Parse source code with include support.
pub fn parse_source_with_include(src: &str, base_dir: Option<PathBuf>) -> PipelineResult {
    let mut parser = Parser::new().map_err(|e| {
        PipelineError::Parse(SyntaxError::parse_failed(format!(
            "Parser initialization failed: {}",
            e
        )))
    })?;

    let outcome = parser.parse(src).map_err(PipelineError::Parse)?;

    let mut lowering = LoweringWithInclude::with_base_dir(src, base_dir);
    lowering.lower(outcome).map_err(PipelineError::Lower)
}

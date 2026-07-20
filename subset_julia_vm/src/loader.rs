// Issue #10906 (Phase 1c of #10869): the package loader's persistent
// `.ji.json` cache-load boundary — zero real unwrap_used/expect_used sites
// in production code (every match is inside the cfg(test) module, which
// carries an explicit allow). `read_cache` already collapses every parse/
// version/schema failure to `None` (a cache miss) via `.ok()?`, never
// panicking.
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{SyntaxError, UnsupportedFeature};
use crate::ir::core::{
    Block, Expr, Function, InnerConstructor, Literal, Module, Program, Stmt, StructDef,
    TypeAliasDef, UsingImport,
};
use crate::lowering::LoweringWithInclude;
use crate::packages;
use crate::parser::Parser;
use crate::span::Span;
use crate::stdlib;
use crate::types::TypeExpr;

/// Persistent package-loader cache format version.
///
/// Bump this whenever the **serialized shape or semantics** of a cached
/// [`Module`] changes so that older `.ji.json` entries are invalidated rather
/// than silently reused (Issue #7921). A `source_hash` match alone is not
/// enough: it tracks only the package *source*, not the lowering/metadata that
/// produced the cached `Module`. The companion [`module_schema_fingerprint`]
/// folds the metadata schema into the cache key automatically, so adding or
/// reshaping a `Module` metadata field (e.g. the type-alias / module-binding
/// entries such as `PolynomialElem` / `MatrixElem`) also invalidates stale
/// entries even if this constant is not bumped.
///
/// History: 9 -> 10 in Issue #7921 (cached `Module` metadata now carries
/// type-alias / module-binding entries that older caches lacked).
/// 12 -> 13: import lowering now normalizes `import A.B.C` as a selective
/// import of `C` from `A.B`, so cached package modules must be rebuilt.
/// 13 -> 14: `@inbounds` tuple-index assignment lowering now preserves both
/// RHS values before explicit `setindex!` calls (Issue #8366/#8370), so cached
/// package modules containing that macro shape must be rebuilt.
/// 14 -> 15: `let` tuple destructuring now introduces destructured bindings in
/// the body scope (Issue #8403/#8408), so cached QuadGK modules compiled with
/// stale `let (s0, si) = ...` lowering must be rebuilt.
/// 15 -> 16: static type-object literals now lower to `Literal::DataType`
/// instead of `typeof("TypeName")`, so cached packages must rebuild to avoid
/// stale string/type sentinel semantics (Issue #9741).
/// 17 -> 18: `InnerConstructor.is_explicit_parametric` (bare `Type{Foo}` vs
/// explicit `Type{Foo{T}}` constructor-self identity) is now load-bearing for
/// dispatch: the `has_where_params()` fallback that used to compensate for a
/// wrong/missing value was removed (Issue #10962/#10974). Cache entries
/// written before this field was reliably populated for every constructor
/// shape silently defaulted it to `false` via `#[serde(default)]` — the
/// `module_schema_fingerprint` probe below did not include a representative
/// `StructDef`/`InnerConstructor`, so that drift was never caught by the
/// fingerprint alone (`packages_data_structures_binary_max_heap_8509`
/// regressed this way against a stale on-disk `.ji.json`; root-caused and
/// fixed as Issue #11004). Bump forces a rebuild; the probe below is also
/// extended to cover this shape going forward.
/// 18 -> 19: definition spans carry a serialized evaluation ordinal so
/// constructor last-definition-wins semantics remain comparable across
/// included files (Issue #11028), and inner constructors with positional
/// defaults now lower reduced-
/// arity forwarding stubs; source-identical package caches must be rebuilt to
/// gain those additional methods (Issues #11003/#11019). The schema probe now
/// includes a representative `Function` as well as `StructDef`.
/// 19 -> 20: annotated optional keywords now lower a two-phase default-
/// materialization / type-assertion prologue. The serialized `Function` shape
/// is unchanged, so the schema fingerprint cannot distinguish an old body from
/// a newly lowered one; rebuild source-identical package caches (Issue #11154).
/// 20 -> 21: `UsingImport.is_import` distinguishes Julia's `import` from
/// `using`. Older package caches deserialize the new field as `false`, silently
/// turning `import M` into `using M`; include a representative import in the
/// schema probe and force source-identical caches to rebuild (Issue #11216).
/// 21 -> 22: `using`/`import` spans consume serialized evaluation ordinals so
/// independently lowered package Modules can be inserted at source chronology
/// rather than after the whole user Program (Issues #11036/#11128).
/// 23 -> 24: lowering-generated callables carry an explicit private-helper
/// provenance marker; cached zero-order helpers would otherwise be mistaken
/// for source functions (Issue #11685).
/// 24 -> 25: bound-form callable-struct receivers are marked structurally at
/// lowering time — the synthesized `self` parameter name carries
/// `CALLABLE_SELF_BOUND_MARKER` and the runtime's
/// `callable_struct_needs_self` trusts that marker instead of guessing from
/// arity (Issues #11386/#11553). A stale cached package Module still carries
/// the unmarked parameter name, so its bound callables would silently stop
/// receiving the prepended receiver (`MethodError: no method matching
/// __callable_<Type>(...)`); force source-identical caches to rebuild.
const CACHE_VERSION: u32 = 25;

#[derive(Debug, Clone)]
pub enum LoadPathEntry {
    Stdlib,
    /// Embedded bundled third-party packages (resolved via [`packages::get_bundled_package`]).
    Packages,
    Path(PathBuf),
}

#[derive(Debug, Clone)]
pub struct LoaderConfig {
    pub load_path: Vec<LoadPathEntry>,
    pub cache_dir: Option<PathBuf>,
}

impl LoaderConfig {
    pub fn from_env() -> Self {
        let load_path = load_path_from_env();
        let cache_dir = cache_dir_from_env();
        Self {
            load_path,
            cache_dir,
        }
    }
}

#[derive(Debug)]
pub enum LoadError {
    ModuleNotFound {
        module: String,
    },
    InvalidProject {
        module: String,
        message: String,
    },
    InvalidPackageLayout {
        module: String,
        reason: String,
    },
    ParserInit {
        module: String,
        message: String,
    },
    ParseError {
        module: String,
        error: String,
    },
    LowerError {
        module: String,
        error: String,
    },
    CircularDependency {
        module: String,
        cycle: Vec<String>,
    },
    IoError {
        module: String,
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::ModuleNotFound { module } => {
                write!(f, "module '{}' not found in LOAD_PATH", module)
            }
            LoadError::InvalidProject { module, message } => {
                write!(f, "invalid Project.toml for {}: {}", module, message)
            }
            LoadError::InvalidPackageLayout { module, reason } => {
                write!(f, "invalid package layout for {}: {}", module, reason)
            }
            LoadError::ParserInit { module, message } => {
                write!(f, "parser init failed for {}: {}", module, message)
            }
            LoadError::ParseError { module, error } => {
                write!(f, "parse error in {}: {}", module, error)
            }
            LoadError::LowerError { module, error } => {
                write!(f, "lowering error in {}: {}", module, error)
            }
            LoadError::CircularDependency { module, cycle } => {
                write!(
                    f,
                    "circular dependency while loading {}: {:?}",
                    module, cycle
                )
            }
            LoadError::IoError {
                module,
                path,
                message,
            } => {
                write!(
                    f,
                    "I/O error for {} at {}: {}",
                    module,
                    path.display(),
                    message
                )
            }
        }
    }
}

impl std::error::Error for LoadError {}

#[derive(Debug, Clone)]
struct ResolvedPackage {
    // Name is intentionally retained for diagnostics and future metadata output.
    #[allow(dead_code)]
    name: String,
    project_toml: String,
    source: String,
    base_dir: Option<PathBuf>,
    is_stdlib_origin: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedModule {
    version: u32,
    vm_version: String,
    target: String,
    /// Fingerprint of the serialized `Module` metadata schema (Issue #7921).
    ///
    /// Defaults to empty for caches written before this field existed; an empty
    /// or mismatched fingerprint forces a cache miss in [`read_cache`], so a
    /// stale entry that predates a `Module` metadata-shape change is not reused.
    #[serde(default)]
    schema_fingerprint: String,
    module_name: String,
    source_hash: String,
    module: Module,
}

#[derive(Debug, Deserialize)]
struct ProjectToml {
    // Optional package name is parsed for compatibility even if not always used.
    #[allow(dead_code)]
    name: Option<String>,
    deps: Option<HashMap<String, String>>,
}

pub struct PackageLoader {
    config: LoaderConfig,
    loaded: HashMap<String, Module>,
    dependencies: HashMap<String, Vec<DependencyAnchor>>,
    load_order: Vec<String>,
    loading_stack: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DependencyAnchor {
    module: String,
    definition_order: u64,
}

impl PackageLoader {
    pub fn new(config: LoaderConfig) -> Self {
        Self {
            config,
            loaded: HashMap::new(),
            dependencies: HashMap::new(),
            load_order: Vec::new(),
            loading_stack: Vec::new(),
        }
    }

    pub fn load_for_usings(&mut self, usings: &[UsingImport]) -> Result<Vec<Module>, LoadError> {
        for using in usings {
            if should_load_module(&using.module) {
                self.load_module(&using.module)?;
            }
        }

        let mut modules = Vec::new();
        for name in &self.load_order {
            if let Some(module) = self.loaded.get(name) {
                modules.push(module.clone());
            }
        }
        Ok(modules)
    }

    /// Load every external top-level import into `program` at its stamped
    /// evaluation position. Each requested package is first composed with its
    /// dependencies at the package-local stamped imports, then the whole
    /// fragment is inserted at the caller's import (Issues #11036 and #11128).
    pub fn load_into_program(&mut self, program: &mut Program) -> Result<(), LoadError> {
        let mut using_indices: Vec<usize> = program
            .usings
            .iter()
            .enumerate()
            .filter_map(|(index, using)| (!using.is_relative).then_some(index))
            .collect();
        using_indices.sort_by_key(|index| (program.usings[*index].span.definition_order, *index));

        let mut existing_modules: HashSet<String> = program
            .modules
            .iter()
            .map(|module| module.name.clone())
            .collect();
        let mut chronology = program.definition_order_cursor();
        for using_index in using_indices {
            let using = program.usings[using_index].clone();
            if existing_modules.contains(&using.module) || !should_load_module(&using.module) {
                continue;
            }
            self.load_module(&using.module)?;
            let mut included = HashSet::new();
            let mut fragment =
                self.compose_loaded_fragment(&using.module, &existing_modules, &mut included);
            // Keep the established dependency-first DFS vector order used by
            // compilation and module initialization. Definition ordinals carry
            // the finer source chronology independently; changing both signals
            // at once perturbs unrelated package dispatch and inference.
            fragment.modules.sort_by_key(|module| {
                self.load_order
                    .iter()
                    .position(|name| name == &module.name)
                    .unwrap_or(usize::MAX)
            });
            let anchor = program.usings[using_index].span.definition_order;
            chronology.insert_fragment_after(program, anchor, &mut fragment);
            for module in fragment.modules.drain(..) {
                existing_modules.insert(module.name.clone());
                program.modules.push(module);
            }
        }
        Ok(())
    }

    fn compose_loaded_fragment(
        &self,
        module: &str,
        existing_modules: &HashSet<String>,
        included: &mut HashSet<String>,
    ) -> Program {
        if existing_modules.contains(module) || !included.insert(module.to_string()) {
            return empty_program();
        }
        let Some(root) = self.loaded.get(module) else {
            return empty_program();
        };

        let mut fragment = empty_program();
        fragment.modules.push(root.clone());
        let mut chronology = fragment.definition_order_cursor();
        let mut inserted_width = 0u64;
        for dependency in self.dependencies.get(module).into_iter().flatten() {
            let mut dependency_fragment =
                self.compose_loaded_fragment(&dependency.module, existing_modules, included);
            let anchor = dependency.definition_order.saturating_add(inserted_width);
            let inserted_end =
                chronology.insert_fragment_after(&mut fragment, anchor, &mut dependency_fragment);
            inserted_width = inserted_width.saturating_add(inserted_end.saturating_sub(anchor));
            fragment.modules.append(&mut dependency_fragment.modules);
        }

        // Dependencies must be initialized before the package that imports
        // them, even when the package has definitions before its first import.
        let root = fragment.modules.remove(0);
        fragment.modules.push(root);
        fragment
    }

    fn load_module(&mut self, module: &str) -> Result<(), LoadError> {
        if self.loaded.contains_key(module) {
            return Ok(());
        }
        if self.loading_stack.contains(&module.to_string()) {
            let mut cycle = self.loading_stack.clone();
            cycle.push(module.to_string());
            return Err(LoadError::CircularDependency {
                module: module.to_string(),
                cycle,
            });
        }

        self.loading_stack.push(module.to_string());

        let resolved = resolve_package(module, &self.config)?;
        let source_hash = compute_source_hash(
            &resolved.project_toml,
            &resolved.source,
            resolved.base_dir.as_deref(),
        );

        let mut module_value = if let Some(cached) = read_cache(&self.config, module, &source_hash)
        {
            cached
        } else {
            let program =
                parse_module_source(module, &resolved.source, resolved.base_dir.as_ref())?;
            let module_value = extract_module(module, program)?;

            if let Err(e) = write_cache(&self.config, module, &source_hash, &module_value) {
                use std::io::Write;
                let _ = writeln!(
                    std::io::stderr(),
                    "[loader] cache write failed for {}: {}",
                    module,
                    e
                );
            }

            module_value
        };
        module_value.mark_as_package_origin();
        if resolved.is_stdlib_origin {
            // PackageLoader inserts stdlibs into `Program.modules` before the
            // compiler derives module provenance. Preserve the same Base/stdlib
            // constructor identity as the compiler's fallback stdlib loader;
            // bundled and filesystem packages deliberately remain source-owned.
            module_value.mark_structs_as_base_origin();
        }
        let project_deps = parse_project_deps(module, &resolved.project_toml)?;
        let mut dependencies = Vec::new();
        collect_module_usings(&module_value, &mut dependencies);
        let body_dep_names: HashSet<String> = dependencies
            .iter()
            .map(|dependency| dependency.module.clone())
            .collect();
        // Preserve the established dependency-first DFS module order: sorted
        // Project.toml dependencies are loaded before body-only imports. The
        // source-anchor ordering below is a separate semantic chronology and
        // must not reorder compiler/module initialization vectors.
        for dep in &project_deps {
            if should_load_module(dep) {
                self.load_module(dep)?;
            }
        }
        for dependency in &dependencies {
            if should_load_module(&dependency.module) {
                self.load_module(&dependency.module)?;
            }
        }
        for dep in project_deps {
            if !body_dep_names.contains(&dep) {
                dependencies.push(DependencyAnchor {
                    module: dep,
                    definition_order: 0,
                });
            }
        }
        dependencies.sort_by(|left, right| {
            (left.definition_order, left.module.as_str())
                .cmp(&(right.definition_order, right.module.as_str()))
        });
        let mut seen_dependencies = HashSet::new();
        dependencies.retain(|dependency| seen_dependencies.insert(dependency.module.clone()));

        // Fresh lowering and `.ji.json` restore must leave the thread-local
        // nominal registry identical. Commit the reconstructed declarations
        // only after dependency loading succeeds so a failed load leaves no
        // partial package registration behind (Issue #11280).
        register_module_nominal_types(&module_value);
        self.dependencies.insert(module.to_string(), dependencies);
        self.loaded.insert(module.to_string(), module_value);
        self.load_order.push(module.to_string());
        self.loading_stack.pop();
        Ok(())
    }
}

fn should_load_module(module: &str) -> bool {
    // Skip Base, Core, Main, Pkg and their submodules (Base.MathConstants, etc.)
    if matches!(module, "Base" | "Core" | "Main" | "Pkg") {
        return false;
    }
    // Skip Base.* submodules (e.g., Base.MathConstants, Base.Math)
    if module.starts_with("Base.") {
        return false;
    }
    true
}

fn parse_module_source(
    module: &str,
    source: &str,
    base_dir: Option<&PathBuf>,
) -> Result<Program, LoadError> {
    let mut parser = Parser::new().map_err(|e| LoadError::ParserInit {
        module: module.to_string(),
        message: e.to_string(),
    })?;

    let outcome = parser.parse(source).map_err(|e| LoadError::ParseError {
        module: module.to_string(),
        error: format_syntax_error(&e),
    })?;

    let source_file = base_dir.map(|dir| dir.join(format!("{module}.jl")));
    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    crate::macro_runtime::install();
    let mut lowering = LoweringWithInclude::new_with_file(
        source,
        crate::lowering::IncludeContext::new(base_dir.cloned()),
        source_file,
    );
    lowering.lower(outcome).map_err(|e| LoadError::LowerError {
        module: module.to_string(),
        error: format_lower_error(&e),
    })
}

fn extract_module(module: &str, program: Program) -> Result<Module, LoadError> {
    if !program.functions.is_empty() {
        return Err(LoadError::InvalidPackageLayout {
            module: module.to_string(),
            reason: "top-level functions are not allowed in package files".to_string(),
        });
    }
    if !program.structs.is_empty() {
        return Err(LoadError::InvalidPackageLayout {
            module: module.to_string(),
            reason: "top-level structs are not allowed in package files".to_string(),
        });
    }
    if !program.usings.is_empty() {
        return Err(LoadError::InvalidPackageLayout {
            module: module.to_string(),
            reason: "top-level using/import statements are not allowed in package files"
                .to_string(),
        });
    }
    if !program
        .main
        .stmts
        .iter()
        .all(is_ignorable_package_entry_stmt)
    {
        return Err(LoadError::InvalidPackageLayout {
            module: module.to_string(),
            reason: "top-level statements are not allowed in package files".to_string(),
        });
    }

    let mut matches: Vec<Module> = program
        .modules
        .into_iter()
        .filter(|m| m.name == module)
        .collect();

    if matches.is_empty() {
        return Err(LoadError::InvalidPackageLayout {
            module: module.to_string(),
            reason: format!("module '{}' not found", module),
        });
    }

    if matches.len() > 1 {
        return Err(LoadError::InvalidPackageLayout {
            module: module.to_string(),
            reason: "multiple modules with the same name found".to_string(),
        });
    }

    Ok(matches.remove(0))
}

fn is_ignorable_package_entry_stmt(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::Expr {
            expr: Expr::Literal(Literal::Nothing | Literal::Str(_), _),
            ..
        }
    )
}

fn collect_module_usings(module: &Module, out: &mut Vec<DependencyAnchor>) {
    // Module.usings retains whether an import was relative. Executable
    // Stmt::Using markers deliberately carry only the canonical module name,
    // so reading dependencies from the body would mistake an internal import
    // such as `import ..LinearAlgebra` for an external self-dependency.
    for using in &module.usings {
        if !using.is_relative && should_load_module(&using.module) {
            out.push(DependencyAnchor {
                module: using.module.clone(),
                definition_order: using.span.definition_order,
            });
        }
    }
    for submodule in &module.submodules {
        collect_module_usings(submodule, out);
    }
}

fn register_module_nominal_types(module: &Module) {
    fn register(module: &Module, prefix: &str) {
        let module_path = if prefix.is_empty() {
            module.name.clone()
        } else {
            format!("{prefix}.{}", module.name)
        };

        for definition in &module.structs {
            crate::types::register_type_name(&format!("{module_path}.{}", definition.name));
        }
        for definition in &module.abstract_types {
            crate::types::register_type_name(&format!("{module_path}.{}", definition.name));
        }
        for definition in &module.primitive_types {
            crate::types::register_type_name(&format!("{module_path}.{}", definition.name));
        }
        for submodule in &module.submodules {
            register(submodule, &module_path);
        }
    }

    register(module, "");
}

fn empty_program() -> Program {
    Program {
        abstract_types: Vec::new(),
        primitive_types: Vec::new(),
        type_aliases: Vec::new(),
        structs: Vec::new(),
        functions: Vec::new(),
        base_function_count: 0,
        modules: Vec::new(),
        usings: Vec::new(),
        macros: Vec::new(),
        enums: Vec::new(),
        main: Block {
            stmts: Vec::new(),
            span: Span::new(0, 0, 0, 0, 0, 0),
        },
    }
}

fn parse_project_deps(module: &str, project_toml: &str) -> Result<Vec<String>, LoadError> {
    let parsed: ProjectToml =
        toml::from_str(project_toml).map_err(|e| LoadError::InvalidProject {
            module: module.to_string(),
            message: e.to_string(),
        })?;

    let mut deps: Vec<String> = parsed.deps.unwrap_or_default().into_keys().collect();
    deps.sort();
    Ok(deps)
}

fn resolve_package(module: &str, config: &LoaderConfig) -> Result<ResolvedPackage, LoadError> {
    for entry in &config.load_path {
        match entry {
            LoadPathEntry::Stdlib => {
                if let Some(pkg) = stdlib::get_stdlib_package(module) {
                    return Ok(ResolvedPackage {
                        name: module.to_string(),
                        project_toml: pkg.project_toml.to_string(),
                        source: pkg.source.to_string(),
                        base_dir: None,
                        is_stdlib_origin: true,
                    });
                }
            }
            LoadPathEntry::Packages => {
                if let Some(pkg) = packages::get_bundled_package(module) {
                    // Use a virtual path so that include() calls inside the package
                    // resolve via get_package_include() on all platforms (incl. iOS/WASM).
                    let virtual_dir =
                        PathBuf::from(format!("{}/{}/src", packages::VIRTUAL_PKG_PREFIX, module));
                    return Ok(ResolvedPackage {
                        name: module.to_string(),
                        project_toml: pkg.project_toml.to_string(),
                        source: pkg.source.to_string(),
                        base_dir: Some(virtual_dir),
                        is_stdlib_origin: false,
                    });
                }
            }
            LoadPathEntry::Path(root) => {
                let pkg_root = root.join(module);
                let src_dir = pkg_root.join("src");
                let project_path = pkg_root.join("Project.toml");
                let source_path = src_dir.join(format!("{}.jl", module));

                if project_path.exists() && source_path.exists() {
                    let project_toml = read_file(module, &project_path)?;
                    let source = read_file(module, &source_path)?;
                    // base_dir points to src/ so that include("helpers.jl") inside
                    // the package resolves relative to the source file's directory.
                    return Ok(ResolvedPackage {
                        name: module.to_string(),
                        project_toml,
                        source,
                        base_dir: Some(src_dir),
                        is_stdlib_origin: false,
                    });
                }
            }
        }
    }

    Err(LoadError::ModuleNotFound {
        module: module.to_string(),
    })
}

fn read_file(module: &str, path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|e| LoadError::IoError {
        module: module.to_string(),
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

fn compute_source_hash(project_toml: &str, source: &str, base_dir: Option<&Path>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_toml.as_bytes());
    hasher.update(b"\n--source-tree--\n");
    let mut visited = HashSet::new();
    hash_source_tree(&mut hasher, source, base_dir, &mut visited);
    let digest = hasher.finalize();
    format!("{:x}", digest)
}

fn hash_source_tree(
    hasher: &mut Sha256,
    source: &str,
    base_dir: Option<&Path>,
    visited: &mut HashSet<PathBuf>,
) {
    hasher.update(b"\n--source--\n");
    hasher.update(source.as_bytes());

    for include_path in extract_literal_include_paths(source) {
        hasher.update(b"\n--include-path--\n");
        hasher.update(include_path.as_bytes());

        let resolved = resolve_include_path_for_hash(base_dir, &include_path);
        if !visited.insert(resolved.clone()) {
            continue;
        }

        match crate::include::read_include_file(&resolved) {
            Ok(included) => {
                hasher.update(b"\n--include-content--\n");
                hash_source_tree(hasher, &included, resolved.parent(), visited);
            }
            Err(_) => {
                // Keep cache lookup conservative. Lowering will report the real
                // include error later, but the unresolved path still participates
                // in the hash so distinct sources do not share cache entries.
                hasher.update(b"\n--include-unresolved--\n");
                hasher.update(resolved.to_string_lossy().as_bytes());
            }
        }
    }
}

fn resolve_include_path_for_hash(base_dir: Option<&Path>, include_path: &str) -> PathBuf {
    let path = PathBuf::from(include_path);
    if path.is_absolute() {
        path
    } else if let Some(base_dir) = base_dir {
        base_dir.join(path)
    } else {
        path
    }
}

fn extract_literal_include_paths(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut paths = Vec::new();
    let mut i = 0;

    while let Some(relative) = source[i..].find("include") {
        let include_start = i + relative;
        let include_end = include_start + "include".len();

        let prev_is_ident = include_start
            .checked_sub(1)
            .and_then(|idx| bytes.get(idx))
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
        let next_is_ident = bytes
            .get(include_end)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
        if prev_is_ident || next_is_ident {
            i = include_end;
            continue;
        }

        let mut j = include_end;
        while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
            j += 1;
        }
        if bytes.get(j) != Some(&b'(') {
            i = include_end;
            continue;
        }
        j += 1;
        while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
            j += 1;
        }
        if bytes.get(j) != Some(&b'"') {
            i = include_end;
            continue;
        }
        j += 1;

        let mut path = String::new();
        let mut escaped = false;
        while let Some(&b) = bytes.get(j) {
            j += 1;
            if escaped {
                path.push(b as char);
                escaped = false;
                continue;
            }
            if b == b'\\' {
                escaped = true;
                continue;
            }
            if b == b'"' {
                paths.push(path);
                break;
            }
            path.push(b as char);
        }

        i = j;
    }

    paths
}

fn cache_dir_from_env() -> Option<PathBuf> {
    if let Ok(val) = env::var("SUBSETJULIA_CACHE_DIR") {
        if !val.trim().is_empty() {
            return Some(PathBuf::from(val));
        }
    }

    if cfg!(any(target_os = "ios", target_arch = "wasm32")) {
        return None;
    }

    Some(env::temp_dir().join("subset_julia_vm_cache"))
}

fn load_path_from_env() -> Vec<LoadPathEntry> {
    match env::var("SUBSETJULIA_LOAD_PATH").or_else(|_| env::var("JULIA_LOAD_PATH")) {
        Ok(env_val) => parse_load_path(&env_val),
        Err(_) => default_load_path(),
    }
}

fn default_load_path() -> Vec<LoadPathEntry> {
    vec![LoadPathEntry::Stdlib, LoadPathEntry::Packages]
}

fn parse_load_path(raw: &str) -> Vec<LoadPathEntry> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let mut entries = Vec::new();

    for part in raw.split(separator) {
        let token = part.trim();
        if token.is_empty() {
            entries.push(LoadPathEntry::Stdlib);
            continue;
        }
        if token == "@stdlib" {
            entries.push(LoadPathEntry::Stdlib);
        } else if token == "@packages" {
            entries.push(LoadPathEntry::Packages);
        } else {
            entries.push(LoadPathEntry::Path(PathBuf::from(token)));
        }
    }

    if entries.is_empty() {
        entries.push(LoadPathEntry::Stdlib);
    }

    entries
}

fn cache_path(config: &LoaderConfig, module: &str, hash: &str) -> Option<PathBuf> {
    let cache_dir = config.cache_dir.as_ref()?;
    let name = sanitize_name(module);
    Some(cache_dir.join(format!("{}.{}.ji.json", name, hash)))
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn read_cache(config: &LoaderConfig, module: &str, hash: &str) -> Option<Module> {
    let path = cache_path(config, module, hash)?;
    let data = fs::read_to_string(path).ok()?;
    let cached: CachedModule = serde_json::from_str(&data).ok()?;

    if cached.version != CACHE_VERSION {
        return None;
    }
    if cached.vm_version != env!("CARGO_PKG_VERSION") {
        return None;
    }
    // Reject entries whose `Module` metadata schema differs from the running
    // binary's (Issue #7921). Caches written before this field existed have an
    // empty fingerprint and so are treated as stale.
    if cached.schema_fingerprint != module_schema_fingerprint() {
        return None;
    }
    if cached.target != cache_target() {
        return None;
    }
    if cached.module_name != module {
        return None;
    }
    if cached.source_hash != hash {
        return None;
    }

    Some(cached.module)
}

fn write_cache(
    config: &LoaderConfig,
    module: &str,
    hash: &str,
    module_value: &Module,
) -> Result<(), LoadError> {
    let path = match cache_path(config, module, hash) {
        Some(p) => p,
        None => return Ok(()),
    };

    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Err(LoadError::IoError {
                module: module.to_string(),
                path: parent.to_path_buf(),
                message: e.to_string(),
            });
        }
    }

    let cached = CachedModule {
        version: CACHE_VERSION,
        vm_version: env!("CARGO_PKG_VERSION").to_string(),
        target: cache_target(),
        schema_fingerprint: module_schema_fingerprint(),
        module_name: module.to_string(),
        source_hash: hash.to_string(),
        module: module_value.clone(),
    };

    let json = serde_json::to_string(&cached).map_err(|e| LoadError::IoError {
        module: module.to_string(),
        path: path.clone(),
        message: e.to_string(),
    })?;

    fs::write(&path, json).map_err(|e| LoadError::IoError {
        module: module.to_string(),
        path: path.clone(),
        message: e.to_string(),
    })
}

fn cache_target() -> String {
    format!("{}-{}", env::consts::OS, env::consts::ARCH)
}

/// Fingerprint of the serialized [`Module`] metadata schema (Issue #7921).
///
/// The persistent loader cache stores a lowered `Module` per package. A cache
/// entry is keyed by `source_hash`, but that only tracks the package *source* —
/// not the lowering/metadata that produced the cached `Module`. When the lowered
/// `Module` metadata shape gains new entries (e.g. type-alias / module-binding
/// metadata such as `PolynomialElem` / `MatrixElem`), an older cache entry on
/// the same source would otherwise be reused even though it lacks the newer
/// metadata, yielding `isdefined(Pkg, :PolynomialElem) == false`.
///
/// To invalidate such entries automatically, this hashes the JSON serialization
/// of a canonical probe `Module`. Serde emits **every** field name even for
/// empty collections, so adding or removing a top-level `Module` field changes
/// the fingerprint. The probe also populates `type_aliases` with one
/// representative [`TypeAliasDef`], so reshaping that nested metadata type (the
/// kind of change that triggered #7921) is captured too. The result is folded
/// into the cache key alongside [`CACHE_VERSION`], so a metadata-shape change
/// invalidates stale entries even when the constant is not bumped.
fn module_schema_fingerprint() -> String {
    let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
    let probe = Module {
        name: String::new(),
        is_bare: false,
        is_package_origin: false,
        is_base_origin: false,
        functions: vec![Function {
            name: String::new(),
            params: Vec::new(),
            kwparams: Vec::new(),
            type_params: Vec::new(),
            return_type: None,
            body: Block {
                stmts: Vec::new(),
                span: dummy_span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span,
            new_struct_name: None,
        }],
        structs: vec![StructDef {
            name: String::new(),
            is_mutable: false,
            type_params: Vec::new(),
            parent_type: None,
            fields: Vec::new(),
            is_base_origin: false,
            // One representative inner constructor with the self-family
            // marker set, so any future shape change to `StructDef` or
            // `InnerConstructor` (including `is_explicit_parametric` itself,
            // Issue #10962/#10974/#11004) changes this fingerprint and
            // invalidates stale `.ji.json` entries instead of silently
            // reusing them with `#[serde(default)]`-filled defaults.
            inner_constructors: vec![InnerConstructor {
                params: Vec::new(),
                kwparams: Vec::new(),
                type_params: Vec::new(),
                is_explicit_parametric: true,
                explicit_type_parameter_names: vec!["T".to_string()],
                explicit_type_arguments: vec![TypeExpr::TypeVar("T".to_string())],
                body: Block {
                    stmts: Vec::new(),
                    span: dummy_span,
                },
                span: dummy_span,
            }],
            // Struct-body `global` helpers are moved into `functions` during
            // lowering, so a serialized `StructDef` always carries an empty
            // list here (Issue #11005).
            global_new_helpers: Vec::new(),
            span: dummy_span,
        }],
        abstract_types: Vec::new(),
        primitive_types: Vec::new(),
        type_aliases: vec![TypeAliasDef {
            name: String::new(),
            target_type: String::new(),
            params: Vec::new(),
            span: dummy_span,
        }],
        submodules: Vec::new(),
        usings: vec![UsingImport {
            module: "Probe".to_string(),
            is_import: true,
            symbols: None,
            is_relative: false,
            relative_level: 0,
            alias_bindings: Vec::new(),
            span: dummy_span,
        }],
        macros: Vec::new(),
        exports: Vec::new(),
        publics: Vec::new(),
        body: Block {
            stmts: Vec::new(),
            span: dummy_span,
        },
        span: dummy_span,
    };

    // Serialization cannot fail for this fixed in-memory value; fall back to a
    // constant marker so a hypothetical error still yields a stable fingerprint.
    let json = serde_json::to_string(&probe).unwrap_or_else(|_| "schema-error".to_string());
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn format_syntax_error(error: &SyntaxError) -> String {
    match error {
        SyntaxError::ErrorNodes(issues) => issues
            .first()
            .map(|issue| issue.text.clone())
            .unwrap_or_else(|| "unknown syntax error".to_string()),
        SyntaxError::ParseFailed(msg) => msg.clone(),
    }
}

fn format_lower_error(error: &UnsupportedFeature) -> String {
    format!("{:?}", error)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_load_path_stdlib() {
        let entries = parse_load_path("@stdlib");
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0], LoadPathEntry::Stdlib));
    }

    #[test]
    fn test_parse_load_path_packages() {
        let entries = parse_load_path("@packages");
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0], LoadPathEntry::Packages));
    }

    #[test]
    fn test_parse_load_path_stdlib_and_packages() {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let entries = parse_load_path(&format!("@stdlib{separator}@packages"));
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], LoadPathEntry::Stdlib));
        assert!(matches!(entries[1], LoadPathEntry::Packages));
    }

    #[test]
    fn test_default_load_path_is_platform_independent() {
        let entries = default_load_path();
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], LoadPathEntry::Stdlib));
        assert!(matches!(entries[1], LoadPathEntry::Packages));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("Base.Random"), "Base_Random");
    }

    #[test]
    fn test_extract_literal_include_paths() {
        let source = r#"
            include("types.jl")
            notinclude("ignored.jl")
            include ( "api.jl" )
        "#;
        assert_eq!(
            extract_literal_include_paths(source),
            vec!["types.jl".to_string(), "api.jl".to_string()]
        );
    }

    #[test]
    fn test_extract_module_allows_noop_package_header_statement() {
        let source = r#"@doc raw"""
Demo package docs.
"""
module Demo
export loaded
loaded() = true
end
"#;
        let program = parse_module_source("Demo", source, None).expect("lower package source");
        let module = extract_module("Demo", program).expect("extract module");

        assert_eq!(module.name, "Demo");
        assert_eq!(module.exports, vec!["loaded"]);
    }

    #[test]
    fn test_extract_module_allows_top_level_docstring_header() {
        let source = r#""Demo package docs."
module Demo
export loaded
loaded() = true
end
"#;
        let program = parse_module_source("Demo", source, None).expect("lower package source");
        let module = extract_module("Demo", program).expect("extract module");

        assert_eq!(module.name, "Demo");
        assert_eq!(module.exports, vec!["loaded"]);
    }

    #[test]
    fn test_extract_module_rejects_effectful_package_header_statement() {
        let source = r#"println("loading")
module Demo
export loaded
loaded() = true
end
"#;
        let program = parse_module_source("Demo", source, None).expect("lower package source");
        let err = extract_module("Demo", program).expect_err("reject effectful header");

        assert!(
            matches!(
                err,
                LoadError::InvalidPackageLayout { ref reason, .. }
                    if reason == "top-level statements are not allowed in package files"
            ),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_source_hash_includes_included_file_content() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let included_path = dir.path().join("api.jl");
        fs::write(&included_path, "f() = 1\n").expect("write include");

        let source = r#"module Demo
include("api.jl")
end
"#;
        let first = compute_source_hash("", source, Some(dir.path()));
        fs::write(&included_path, "f() = 2\n").expect("rewrite include");
        let second = compute_source_hash("", source, Some(dir.path()));

        assert_ne!(first, second);
    }

    fn empty_module(name: &str) -> Module {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        Module {
            name: name.to_string(),
            is_bare: false,
            is_package_origin: false,
            is_base_origin: false,
            functions: Vec::new(),
            structs: Vec::new(),
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            type_aliases: Vec::new(),
            submodules: Vec::new(),
            usings: Vec::new(),
            macros: Vec::new(),
            exports: Vec::new(),
            publics: Vec::new(),
            body: Block {
                stmts: Vec::new(),
                span: dummy_span,
            },
            span: dummy_span,
        }
    }

    fn nominal_struct_11280(name: &str) -> StructDef {
        StructDef {
            name: name.to_string(),
            is_mutable: false,
            is_base_origin: false,
            type_params: Vec::new(),
            parent_type: None,
            fields: Vec::new(),
            inner_constructors: Vec::new(),
            global_new_helpers: Vec::new(),
            span: Span::new(0, 0, 0, 0, 0, 0),
        }
    }

    fn nominal_parametric_struct_11280(name: &str) -> StructDef {
        StructDef {
            type_params: vec![crate::types::TypeParam::new("T".to_string())],
            ..nominal_struct_11280(name)
        }
    }

    fn cached_nominal_module_11280(package: &str) -> Module {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let nested = Module {
            structs: vec![nominal_struct_11280("NestedStructReplay11280")],
            ..empty_module("NestedReplay11280")
        };
        Module {
            structs: vec![
                nominal_struct_11280("StructReplay11280"),
                nominal_parametric_struct_11280("ParametricReplay11280"),
            ],
            abstract_types: vec![crate::ir::core::AbstractTypeDef {
                name: "AbstractReplay11280".to_string(),
                parent: None,
                type_params: Vec::new(),
                span,
            }],
            primitive_types: vec![crate::ir::core::PrimitiveTypeDef {
                name: "PrimitiveReplay11280".to_string(),
                parent: None,
                bits: 8,
                span,
            }],
            submodules: vec![nested],
            ..empty_module(package)
        }
    }

    #[test]
    fn restored_module_replays_qualified_nominal_declarations_11280(
    ) -> Result<(), Box<dyn std::error::Error>> {
        const PACKAGE: &str = "NominalReplayPkg11280";
        const FAMILIES: &[&str] = &[
            "StructReplay11280",
            "ParametricReplay11280",
            "AbstractReplay11280",
            "PrimitiveReplay11280",
            "NestedStructReplay11280",
        ];

        let temp = tempfile::tempdir()?;
        let package_root = temp.path().join(PACKAGE);
        let source_dir = package_root.join("src");
        let cache_dir = temp.path().join("cache");
        fs::create_dir_all(&source_dir)?;
        let project_toml = format!(
            "name = \"{PACKAGE}\"\n\
             uuid = \"11280000-0000-0000-0000-000000000000\"\n\
             version = \"0.1.0\"\n"
        );
        let source = format!("module {PACKAGE}\nend\n");
        fs::write(package_root.join("Project.toml"), &project_toml)?;
        fs::write(source_dir.join(format!("{PACKAGE}.jl")), &source)?;

        let config = LoaderConfig {
            load_path: vec![LoadPathEntry::Path(temp.path().to_path_buf())],
            cache_dir: Some(cache_dir),
        };
        let hash = compute_source_hash(&project_toml, &source, Some(&source_dir));
        write_cache(
            &config,
            PACKAGE,
            &hash,
            &cached_nominal_module_11280(PACKAGE),
        )?;

        for family in FAMILIES {
            crate::types::register_type_name(&format!("Base.{family}"));
            assert!(
                !crate::types::has_qualified_nominal_family_collision(family),
                "the package owner must be absent before cache restore: {family}"
            );
        }

        let mut loader = PackageLoader::new(config);
        loader.load_module(PACKAGE)?;

        for family in FAMILIES {
            assert!(
                crate::types::has_qualified_nominal_family_collision(family),
                "cache restore must register the qualified package owner: {family}"
            );
        }

        let loaded = loader
            .loaded
            .get(PACKAGE)
            .ok_or_else(|| std::io::Error::other("cached module was not committed"))?;
        assert_eq!(loaded.structs[0].name, "StructReplay11280");
        assert_eq!(loaded.structs[1].type_params[0].name, "T");
        assert_eq!(
            loaded.submodules[0].structs[0].name,
            "NestedStructReplay11280"
        );
        Ok(())
    }

    fn cache_config(dir: &Path) -> LoaderConfig {
        LoaderConfig {
            load_path: vec![LoadPathEntry::Stdlib],
            cache_dir: Some(dir.to_path_buf()),
        }
    }

    #[test]
    fn test_module_schema_fingerprint_is_deterministic() {
        // The fingerprint must be stable across calls so a cache written by one
        // process is accepted by another with the same Module schema.
        assert_eq!(module_schema_fingerprint(), module_schema_fingerprint());
        assert!(!module_schema_fingerprint().is_empty());
    }

    /// Issue #10962/#10974/#11004: `InnerConstructor.is_explicit_parametric`
    /// (bare `Type{Foo}` vs explicit `Type{Foo{T}}` constructor-self
    /// identity) used to be backstopped by a `has_where_params()` dispatch
    /// fallback, so a wrong/missing value silently self-healed. That
    /// fallback was removed once the field became load-bearing, which
    /// exposed a real bug: the `module_schema_fingerprint` probe carried no
    /// representative `StructDef`/`InnerConstructor`, so an on-disk
    /// `.ji.json` cache entry written before this field was reliably
    /// populated for every constructor shape kept matching the fingerprint
    /// and got reused with `#[serde(default)]`-filled `false` values
    /// (`packages_data_structures_binary_max_heap_8509` regressed exactly
    /// this way against a real stale cache). This test pins that the probe
    /// extended below is actually sensitive to struct/inner-constructor shape
    /// by comparing against the pre-fix probe shape (no representative
    /// struct at all) — the two must hash differently so old entries are
    /// invalidated by the fingerprint alone, independent of `CACHE_VERSION`.
    #[test]
    fn test_schema_fingerprint_covers_struct_inner_constructor_shape_10962() {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let pre_fix_probe = Module {
            name: String::new(),
            is_bare: false,
            is_package_origin: false,
            is_base_origin: false,
            functions: Vec::new(),
            // Pre-fix shape: no representative struct/inner-constructor.
            structs: Vec::new(),
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            type_aliases: vec![TypeAliasDef {
                name: String::new(),
                target_type: String::new(),
                params: Vec::new(),
                span: dummy_span,
            }],
            submodules: Vec::new(),
            usings: Vec::new(),
            macros: Vec::new(),
            exports: Vec::new(),
            publics: Vec::new(),
            body: Block {
                stmts: Vec::new(),
                span: dummy_span,
            },
            span: dummy_span,
        };
        let pre_fix_json = serde_json::to_string(&pre_fix_probe).expect("serialize pre-fix probe");
        let mut hasher = Sha256::new();
        hasher.update(pre_fix_json.as_bytes());
        let pre_fix_fingerprint = format!("{:x}", hasher.finalize());

        assert_ne!(
            pre_fix_fingerprint,
            module_schema_fingerprint(),
            "the schema-fingerprint probe must change when a representative \
             StructDef/InnerConstructor is added, so cache entries written \
             against the narrower pre-fix probe shape are invalidated"
        );
    }

    #[test]
    fn test_cache_roundtrip_hits_with_matching_schema() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config = cache_config(dir.path());
        let module = empty_module("Roundtrip");
        let hash = "deadbeef";

        write_cache(&config, "Roundtrip", hash, &module).expect("write cache");
        let read = read_cache(&config, "Roundtrip", hash);
        assert_eq!(read.as_ref().map(|m| m.name.as_str()), Some("Roundtrip"));
    }

    /// Issues #7921/#11019: cache entries with an obsolete schema fingerprint
    /// or cache version must be treated as stale rather than silently reused.
    #[test]
    fn test_stale_cache_with_mismatched_schema_or_version_is_rejected() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config = cache_config(dir.path());
        let module = "StaleSchema";
        let hash = "cafef00d";

        let stale = CachedModule {
            version: CACHE_VERSION,
            vm_version: env!("CARGO_PKG_VERSION").to_string(),
            target: cache_target(),
            // Empty fingerprint == produced before the metadata-shape change.
            schema_fingerprint: String::new(),
            module_name: module.to_string(),
            source_hash: hash.to_string(),
            module: empty_module(module),
        };
        let path = cache_path(&config, module, hash).expect("cache path");
        fs::write(&path, serde_json::to_string(&stale).expect("serialize")).expect("write stale");

        // Without the fingerprint guard this would return Some(..) and reuse the
        // stale metadata; with the guard it must miss.
        assert!(read_cache(&config, module, hash).is_none());

        // Version 18 predates inner-constructor identity/default-stub metadata.
        let stale_old_version = CachedModule {
            version: 18,
            schema_fingerprint: module_schema_fingerprint(),
            ..stale
        };
        fs::write(
            &path,
            serde_json::to_string(&stale_old_version).expect("serialize"),
        )
        .expect("write stale");
        assert!(read_cache(&config, module, hash).is_none());
    }

    /// Issue #10906 (Phase 1c of #10869): malformed/truncated `.ji.json`
    /// bytes on disk (partial write, disk corruption) must fall back to a
    /// cache miss (`None`), never panic — `read_cache`'s `.ok()?` chain
    /// already collapses every parse failure this way; this proves it holds
    /// for genuinely invalid JSON and truncated JSON, not just
    /// well-formed-but-stale entries (the case the test above covers).
    #[test]
    fn test_malformed_and_truncated_cache_json_is_a_cache_miss_not_a_panic() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config = cache_config(dir.path());
        let module = "Malformed10906";
        let hash = "badc0ffee";
        let path = cache_path(&config, module, hash).expect("cache path");

        // Not JSON at all.
        fs::write(&path, b"this is not valid json at all, oops").expect("write garbage");
        assert!(
            read_cache(&config, module, hash).is_none(),
            "garbage bytes must be a cache miss, not a panic"
        );

        // Valid JSON, but truncated mid-object (a partial write).
        let module_value = empty_module(module);
        let cached = CachedModule {
            version: CACHE_VERSION,
            vm_version: env!("CARGO_PKG_VERSION").to_string(),
            target: cache_target(),
            schema_fingerprint: module_schema_fingerprint(),
            module_name: module.to_string(),
            source_hash: hash.to_string(),
            module: module_value,
        };
        let full_json = serde_json::to_string(&cached).expect("serialize");
        let truncated = &full_json[..full_json.len() / 2];
        fs::write(&path, truncated).expect("write truncated json");
        assert!(
            read_cache(&config, module, hash).is_none(),
            "truncated JSON must be a cache miss, not a panic"
        );
    }
}

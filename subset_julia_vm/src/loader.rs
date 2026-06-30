use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{SyntaxError, UnsupportedFeature};
use crate::ir::core::{Block, Expr, Literal, Module, Program, Stmt, TypeAliasDef, UsingImport};
use crate::lowering::LoweringWithInclude;
use crate::packages;
use crate::parser::Parser;
use crate::span::Span;
use crate::stdlib;

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
const CACHE_VERSION: u32 = 15;

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
    load_order: Vec<String>,
    loading_stack: Vec<String>,
}

impl PackageLoader {
    pub fn new(config: LoaderConfig) -> Self {
        Self {
            config,
            loaded: HashMap::new(),
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

        let module_value = if let Some(cached) = read_cache(&self.config, module, &source_hash) {
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
        let deps = parse_project_deps(module, &resolved.project_toml)?;
        for dep in deps {
            if should_load_module(&dep) {
                self.load_module(&dep)?;
            }
        }

        let mut body_usings = HashSet::new();
        collect_module_usings(&module_value, &mut body_usings);
        for dep in body_usings {
            if should_load_module(&dep) {
                self.load_module(&dep)?;
            }
        }

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

fn collect_module_usings(module: &Module, out: &mut HashSet<String>) {
    for stmt in &module.body.stmts {
        if let Stmt::Using { module, .. } = stmt {
            out.insert(module.clone());
        }
    }
    for submodule in &module.submodules {
        collect_module_usings(submodule, out);
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
    let env_val = env::var("SUBSETJULIA_LOAD_PATH")
        .or_else(|_| env::var("JULIA_LOAD_PATH"))
        .unwrap_or_else(|_| "@stdlib:@packages".to_string());

    parse_load_path(&env_val)
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
        functions: Vec::new(),
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
        let entries = parse_load_path("@stdlib:@packages");
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

    /// Issue #7921: a cache entry that matches version / vm_version / target /
    /// source_hash but predates a `Module` metadata-shape change (here simulated
    /// by an empty `schema_fingerprint`, as written by binaries before the field
    /// existed) must be treated as stale rather than silently reused.
    #[test]
    fn test_stale_cache_with_mismatched_schema_is_rejected() {
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

        // A differently shaped (non-empty but wrong) fingerprint is also rejected.
        let stale_wrong = CachedModule {
            schema_fingerprint: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            ..stale
        };
        fs::write(
            &path,
            serde_json::to_string(&stale_wrong).expect("serialize"),
        )
        .expect("write stale");
        assert!(read_cache(&config, module, hash).is_none());
    }
}

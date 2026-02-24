use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{SyntaxError, UnsupportedFeature};
use crate::ir::core::{Module, Program, Stmt, UsingImport};
use crate::lowering::LoweringWithInclude;
use crate::parser::Parser;
use crate::stdlib;

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub enum LoadPathEntry {
    Stdlib,
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
        let source_hash = compute_source_hash(&resolved.project_toml, &resolved.source);

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

    let mut lowering = LoweringWithInclude::with_base_dir(source, base_dir.cloned());
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
    if !program.main.stmts.is_empty() {
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
            LoadPathEntry::Path(root) => {
                let pkg_root = root.join(module);
                let project_path = pkg_root.join("Project.toml");
                let source_path = pkg_root.join("src").join(format!("{}.jl", module));

                if project_path.exists() && source_path.exists() {
                    let project_toml = read_file(module, &project_path)?;
                    let source = read_file(module, &source_path)?;
                    return Ok(ResolvedPackage {
                        name: module.to_string(),
                        project_toml,
                        source,
                        base_dir: Some(pkg_root),
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

fn compute_source_hash(project_toml: &str, source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_toml.as_bytes());
    hasher.update(b"\n--\n");
    hasher.update(source.as_bytes());
    let digest = hasher.finalize();
    format!("{:x}", digest)
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
        .unwrap_or_else(|_| "@stdlib".to_string());

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
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("Base.Random"), "Base_Random");
    }
}

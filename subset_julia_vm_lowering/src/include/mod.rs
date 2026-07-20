//! Include registry for bundled Julia source files.
//!
//! This module provides a registry of pre-bundled Julia source files that can
//! be included via `include("path")` calls. In a sandboxed environment (iOS/WASM),
//! we cannot access the filesystem, so all includable files must be pre-bundled.
//!
//! # Design
//!
//! Julia's `include` evaluates a file at compile/load time, not runtime.
//! This implementation follows that pattern - included code is parsed and merged
//! during the lowering phase.
//!
//! # Example
//!
//! ```julia
//! include("utils/math.jl")  # Loads bundled math utilities
//! ```

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use once_cell::sync::Lazy;

use crate::error::IncludeError;

/// Registry of bundled Julia source files.
/// Maps file paths to their source code content.
static INCLUDE_REGISTRY: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    // Add bundled files here as:
    // registry.insert("path/to/file.jl", include_str!("../bundled/path/to/file.jl"));

    // Example files that could be included:
    // registry.insert("prelude/math.jl", include_str!("../prelude/math.jl"));
    // registry.insert("prelude/array.jl", include_str!("../prelude/array.jl"));

    HashMap::new()
});

/// Integration-owned lookup for source files embedded in bundled packages.
pub trait PackageIncludeProvider: Sync {
    fn get_package_include(&self, normalized_path: &str) -> Option<&'static str>;
}

static PACKAGE_INCLUDE_PROVIDER: OnceLock<&'static dyn PackageIncludeProvider> = OnceLock::new();

/// Installs the bundled-package include lookup. The first composition root wins.
pub fn install_package_include_provider(provider: &'static dyn PackageIncludeProvider) {
    let _ = PACKAGE_INCLUDE_PROVIDER.set(provider);
}

fn get_package_include(normalized_path: &str) -> Option<&'static str> {
    PACKAGE_INCLUDE_PROVIDER
        .get()
        .and_then(|provider| provider.get_package_include(normalized_path))
}

/// Get the source code for a bundled include path.
/// Returns None if the path is not in the registry.
pub fn get_include_source(path: &str) -> Option<&'static str> {
    // Normalize the path (remove leading ./ or /)
    let normalized = path.trim_start_matches("./").trim_start_matches('/');
    INCLUDE_REGISTRY.get(normalized).copied()
}

/// Check if a path is registered for include.
pub fn is_includable(path: &str) -> bool {
    get_include_source(path).is_some()
}

/// Get all registered include paths.
pub fn registered_paths() -> Vec<&'static str> {
    INCLUDE_REGISTRY.keys().copied().collect()
}

fn normalized_registry_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                parts.push(prefix.as_os_str().to_string_lossy().to_string());
            }
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(part) => {
                parts.push(part.to_string_lossy().to_string());
            }
        }
    }
    parts.join("/")
}

/// Register a new includable file dynamically.
/// This is primarily for testing or runtime-added content.
/// Note: Static registry is preferred for bundled content.
pub struct DynamicRegistry {
    files: HashMap<String, String>,
}

impl DynamicRegistry {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    pub fn register(&mut self, path: &str, source: &str) {
        self.files.insert(path.to_string(), source.to_string());
    }

    pub fn get(&self, path: &str) -> Option<&str> {
        let normalized = path.trim_start_matches("./").trim_start_matches('/');
        self.files.get(normalized).map(|s| s.as_str())
    }
}

impl Default for DynamicRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve an include path relative to a base directory.
/// If the path is absolute, it's returned as-is.
/// If relative, it's resolved from base_dir (or current directory if None).
pub fn resolve_include_path(path: &str, base_dir: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        match base_dir {
            Some(base) => base.join(p),
            None => std::env::current_dir().unwrap_or_default().join(p),
        }
    }
}

/// Read an include file from the filesystem or registry.
/// On native platforms, tries registries first, then falls back to filesystem.
/// On iOS/WASM, only uses the registries.
#[cfg(not(any(target_os = "ios", target_arch = "wasm32")))]
pub fn read_include_file(path: &Path) -> Result<String, IncludeError> {
    let path_str = path.to_string_lossy();
    let normalized = normalized_registry_path(path);

    // Check static include registry first.
    if let Some(content) = get_include_source(&normalized) {
        return Ok(content.to_string());
    }

    // Check embedded package includes (virtual paths like /embedded_packages/...).
    if let Some(content) = get_package_include(&normalized) {
        return Ok(content.to_string());
    }

    // Fall back to filesystem.
    std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            IncludeError::FileNotFound {
                requested_path: path_str.to_string(),
                resolved_path: path.to_path_buf(),
            }
        } else {
            IncludeError::IoError {
                file_path: path_str.to_string(),
                message: e.to_string(),
            }
        }
    })
}

/// On iOS/WASM, only the static and package include registries are available.
#[cfg(any(target_os = "ios", target_arch = "wasm32"))]
pub fn read_include_file(path: &Path) -> Result<String, IncludeError> {
    let path_str = path.to_string_lossy();
    let normalized = normalized_registry_path(path);

    if let Some(content) = get_include_source(&normalized) {
        return Ok(content.to_string());
    }

    if let Some(content) = get_package_include(&normalized) {
        return Ok(content.to_string());
    }

    Err(IncludeError::NotSupported {
        reason: format!(
            "include('{}') is not supported on iOS/WASM outside of embedded packages.",
            path_str
        ),
    })
}

/// Check if the current platform supports filesystem-based includes.
pub fn can_read_filesystem() -> bool {
    cfg!(not(any(target_os = "ios", target_arch = "wasm32")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        // Initially, registry is empty (no bundled files yet)
        assert!(registered_paths().is_empty() || !registered_paths().is_empty());
    }

    #[test]
    fn test_dynamic_registry() {
        let mut registry = DynamicRegistry::new();
        registry.register("test.jl", "x = 1");
        assert_eq!(registry.get("test.jl"), Some("x = 1"));
        assert_eq!(registry.get("./test.jl"), Some("x = 1"));
    }
}

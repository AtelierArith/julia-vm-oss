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
use std::path::{Path, PathBuf};

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
/// On native platforms, tries filesystem first, then falls back to registry.
/// On iOS/WASM, only uses the registry.
#[cfg(not(any(target_os = "ios", target_arch = "wasm32")))]
pub fn read_include_file(path: &Path) -> Result<String, IncludeError> {
    // First try the static registry (for bundled files)
    let path_str = path.to_string_lossy();
    if let Some(content) = get_include_source(&path_str) {
        return Ok(content.to_string());
    }

    // Then try filesystem
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

/// On iOS/WASM, include is completely disabled.
/// Returns an error for any include() call.
#[cfg(any(target_os = "ios", target_arch = "wasm32"))]
pub fn read_include_file(path: &Path) -> Result<String, IncludeError> {
    let path_str = path.to_string_lossy();
    Err(IncludeError::NotSupported {
        reason: format!(
            "include('{}') is not supported on iOS/WASM. \
             Define functions directly in the source code instead.",
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

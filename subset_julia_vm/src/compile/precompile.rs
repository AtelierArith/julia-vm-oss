//! Precompiled Base cache serialization.
//!
//! Provides save/load for the `SerializedBaseCache`, which contains
//! all data needed to skip Base compilation at startup.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

use crate::vm::CompiledProgram;

use super::MethodTable;

/// Version of the cache format. Increment on breaking changes.
const CACHE_VERSION: u32 = 1;

/// Serialized Base cache containing all precompiled data.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SerializedBaseCache {
    pub(crate) version: u32,
    /// SHA-256 of get_prelude() source — detects stale caches
    pub(crate) source_hash: String,
    pub(crate) compiled: CompiledProgram,
    pub(crate) method_tables: HashMap<String, MethodTable>,
    pub(crate) closure_captures: HashMap<String, HashSet<String>>,
    /// Promotion rules extracted from method_tables: (type1, type2, result)
    pub(crate) promotion_rules: Vec<(String, String, String)>,
}

/// Compute SHA-256 of the prelude source for staleness detection.
pub(crate) fn compute_prelude_hash() -> String {
    let prelude_src = crate::base::get_prelude();
    let mut hasher = Sha256::new();
    hasher.update(prelude_src.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Serialize the Base cache to bytes.
pub(crate) fn serialize_base_cache(
    compiled: &CompiledProgram,
    method_tables: &HashMap<String, MethodTable>,
    closure_captures: &HashMap<String, HashSet<String>>,
) -> Result<Vec<u8>, String> {
    // Use the promotion rules already registered in the thread-local registry (Issue #3025).
    // The old approach (extract_promotion_rules_for_cache) used method_table return types
    // which are always ValueType::Any after inference, yielding zero rules.
    // The new approach reads from the registry populated by extract_promotion_rules_from_ir
    // during compile_base_functions().
    let promotion_rules = super::promotion::get_all_promotion_rules();

    let cache = SerializedBaseCache {
        version: CACHE_VERSION,
        source_hash: compute_prelude_hash(),
        compiled: compiled.clone(),
        method_tables: method_tables.clone(),
        closure_captures: closure_captures.clone(),
        promotion_rules,
    };

    bincode::serialize(&cache).map_err(|e| format!("Serialization failed: {}", e))
}

/// Deserialize and validate a Base cache from bytes.
pub(crate) fn deserialize_base_cache(bytes: &[u8]) -> Result<SerializedBaseCache, String> {
    let cache: SerializedBaseCache =
        bincode::deserialize(bytes).map_err(|e| format!("Deserialization failed: {}", e))?;

    if cache.version != CACHE_VERSION {
        return Err(format!(
            "Cache version mismatch: expected {}, got {}",
            CACHE_VERSION, cache.version
        ));
    }

    let current_hash = compute_prelude_hash();
    if cache.source_hash != current_hash {
        return Err("Source hash mismatch: cache was built with different prelude".to_string());
    }

    Ok(cache)
}

/// Generate and serialize the Base cache in one step.
/// This is the main entry point for `--precompile-base`.
/// Triggers Base compilation, exports the cache, and serializes to bytes.
pub fn generate_base_cache() -> Result<Vec<u8>, String> {
    use crate::compile::compile_with_cache;

    // Parse a trivial program to trigger Base compilation via cache
    let program = match crate::pipeline::parse_and_lower("true") {
        Ok(p) => p,
        Err(_) => return Err("Failed to parse trivial program".to_string()),
    };

    // Compile to populate Base cache
    compile_with_cache(&program).map_err(|e| format!("Base compilation failed: {}", e))?;

    // Export cached data
    let (compiled, method_tables, closure_captures) =
        super::cache::export_base_cache().ok_or("Base cache not populated after compilation")?;

    serialize_base_cache(&compiled, &method_tables, &closure_captures)
}


#[cfg(test)]
mod tests {
    use super::*;

    // ── compute_prelude_hash ───────────────────────────────────────────────────

    #[test]
    fn test_prelude_hash_is_64_hex_chars() {
        let hash = compute_prelude_hash();
        assert_eq!(
            hash.len(),
            64,
            "SHA-256 digest should be 64 hex characters, got {} chars: {}",
            hash.len(),
            hash
        );
    }

    #[test]
    fn test_prelude_hash_is_lowercase_hex() {
        let hash = compute_prelude_hash();
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "Hash should be lowercase hex, got: {}",
            hash
        );
    }

    #[test]
    fn test_prelude_hash_is_deterministic() {
        let hash1 = compute_prelude_hash();
        let hash2 = compute_prelude_hash();
        assert_eq!(hash1, hash2, "compute_prelude_hash() must be deterministic");
    }

    // ── deserialize_base_cache error paths ────────────────────────────────────

    #[test]
    fn test_deserialize_empty_bytes_returns_error() {
        let result = deserialize_base_cache(&[]);
        assert!(
            result.is_err(),
            "Deserializing empty bytes should return Err"
        );
    }

    #[test]
    fn test_deserialize_garbage_bytes_returns_error() {
        let garbage = b"not a valid bincode blob at all!!!!";
        let result = deserialize_base_cache(garbage);
        assert!(
            result.is_err(),
            "Deserializing garbage bytes should return Err"
        );
    }

    // ── serialize → deserialize round-trip ────────────────────────────────────

    #[test]
    fn test_serialize_deserialize_roundtrip_empty_program() {
        use crate::vm::CompiledProgram;
        use std::collections::HashMap;

        let program = CompiledProgram {
            code: Vec::new(),
            functions: Vec::new(),
            struct_defs: Vec::new(),
            abstract_types: Vec::new(),
            show_methods: Vec::new(),
            entry: 0,
            specializable_functions: Vec::new(),
            compile_context: None,
            base_function_count: 0,
            global_slot_names: Vec::new(),
            global_slot_count: 0,
        };

        let bytes = serialize_base_cache(&program, &HashMap::new(), &HashMap::new())
            .expect("serialization of empty program should succeed");
        assert!(!bytes.is_empty(), "serialized bytes must be non-empty");

        // Round-trip: version and hash both match (same process, same prelude)
        let result = deserialize_base_cache(&bytes);
        assert!(
            result.is_ok(),
            "round-trip of empty program should succeed: {:?}",
            result
        );

        let cache = result.unwrap();
        assert_eq!(
            cache.version, CACHE_VERSION,
            "deserialized version should match CACHE_VERSION"
        );
        assert!(
            cache.compiled.functions.is_empty(),
            "empty program should have no functions"
        );
        assert!(
            cache.method_tables.is_empty(),
            "empty method_tables should round-trip correctly"
        );
        assert!(
            cache.promotion_rules.is_empty(),
            "promotion_rules should be empty when registry is unpopulated"
        );
    }

    // ── version mismatch detection ─────────────────────────────────────────────

    #[test]
    fn test_version_mismatch_returns_error() {
        use crate::vm::CompiledProgram;
        use std::collections::HashMap;

        // Build a cache with a wrong version number
        let wrong_version_cache = SerializedBaseCache {
            version: CACHE_VERSION + 1,
            source_hash: compute_prelude_hash(),
            compiled: CompiledProgram {
                code: Vec::new(),
                functions: Vec::new(),
                struct_defs: Vec::new(),
                abstract_types: Vec::new(),
                show_methods: Vec::new(),
                entry: 0,
                specializable_functions: Vec::new(),
                compile_context: None,
                base_function_count: 0,
                global_slot_names: Vec::new(),
                global_slot_count: 0,
            },
            method_tables: HashMap::new(),
            closure_captures: HashMap::new(),
            promotion_rules: Vec::new(),
        };

        let bytes = bincode::serialize(&wrong_version_cache)
            .expect("serialization should succeed even with wrong version");

        let result = deserialize_base_cache(&bytes);
        assert!(result.is_err(), "wrong version should return Err");

        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("version mismatch"),
            "Error message should mention 'version mismatch': {}",
            err_msg
        );
    }
}

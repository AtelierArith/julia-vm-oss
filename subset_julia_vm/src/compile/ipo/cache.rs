//! Inference result caching for performance.
//!
//! This module provides a cache for storing and retrieving inference results,
//! avoiding redundant type inference for the same function with the same
//! argument types.

use crate::compile::lattice::types::LatticeType;
use std::collections::HashMap;

/// Cache for inference results.
///
/// The cache maps (function_id, argument_types) to inferred return types.
/// This is particularly useful for polymorphic functions that are called
/// with different argument types.
#[derive(Debug, Clone)]
pub struct InferenceCache {
    /// Map from (function_id, arg_types_hash) to return type
    cache: HashMap<CacheKey, LatticeType>,
}

/// Key for the inference cache.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    func_id: usize,
    // Simplified: In a full implementation, we'd hash the arg types
    // For now, we just cache by function ID (monomorphic assumption)
    arg_types_hash: u64,
}

impl InferenceCache {
    /// Create a new empty inference cache.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Get a cached return type for a function with given argument types.
    ///
    /// # Arguments
    ///
    /// * `func_id` - The function ID
    /// * `arg_types` - The argument types (currently unused in simplified version)
    ///
    /// # Returns
    ///
    /// The cached return type if available, or None otherwise.
    pub fn get(&self, func_id: usize, _arg_types: &[LatticeType]) -> Option<&LatticeType> {
        let key = CacheKey {
            func_id,
            arg_types_hash: 0, // Simplified: ignore arg types
        };
        self.cache.get(&key)
    }

    /// Insert a return type into the cache.
    ///
    /// # Arguments
    ///
    /// * `func_id` - The function ID
    /// * `arg_types` - The argument types (currently unused in simplified version)
    /// * `return_type` - The inferred return type
    pub fn insert(
        &mut self,
        func_id: usize,
        _arg_types: Vec<LatticeType>,
        return_type: LatticeType,
    ) {
        let key = CacheKey {
            func_id,
            arg_types_hash: 0, // Simplified: ignore arg types
        };
        self.cache.insert(key, return_type);
    }

    /// Check if a result is cached for a given function.
    pub fn contains(&self, func_id: usize, _arg_types: &[LatticeType]) -> bool {
        let key = CacheKey {
            func_id,
            arg_types_hash: 0,
        };
        self.cache.contains_key(&key)
    }

    /// Clear the cache.
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get the number of cached entries.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for InferenceCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;

    #[test]
    fn test_cache_basic() {
        let mut cache = InferenceCache::new();

        let func_id = 0;
        let arg_types = vec![];
        let return_type = LatticeType::Concrete(ConcreteType::Int64);

        // Insert
        cache.insert(func_id, arg_types.clone(), return_type.clone());

        // Get
        let cached = cache.get(func_id, &arg_types);
        assert_eq!(cached, Some(&return_type));
    }

    #[test]
    fn test_cache_miss() {
        let cache = InferenceCache::new();

        let func_id = 0;
        let arg_types = vec![];

        let cached = cache.get(func_id, &arg_types);
        assert_eq!(cached, None);
    }

    #[test]
    fn test_cache_multiple_functions() {
        let mut cache = InferenceCache::new();

        let return_type1 = LatticeType::Concrete(ConcreteType::Int64);
        let return_type2 = LatticeType::Concrete(ConcreteType::Float64);

        cache.insert(0, vec![], return_type1.clone());
        cache.insert(1, vec![], return_type2.clone());

        assert_eq!(cache.get(0, &[]), Some(&return_type1));
        assert_eq!(cache.get(1, &[]), Some(&return_type2));
    }

    #[test]
    fn test_cache_overwrite() {
        let mut cache = InferenceCache::new();

        let func_id = 0;
        let return_type1 = LatticeType::Concrete(ConcreteType::Int64);
        let return_type2 = LatticeType::Concrete(ConcreteType::Float64);

        cache.insert(func_id, vec![], return_type1);
        cache.insert(func_id, vec![], return_type2.clone());

        // Should get the latest value
        assert_eq!(cache.get(func_id, &[]), Some(&return_type2));
    }

    #[test]
    fn test_cache_contains() {
        let mut cache = InferenceCache::new();

        assert!(!cache.contains(0, &[]));

        cache.insert(0, vec![], LatticeType::Concrete(ConcreteType::Int64));

        assert!(cache.contains(0, &[]));
        assert!(!cache.contains(1, &[]));
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = InferenceCache::new();

        cache.insert(0, vec![], LatticeType::Concrete(ConcreteType::Int64));
        cache.insert(1, vec![], LatticeType::Concrete(ConcreteType::Float64));

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }
}

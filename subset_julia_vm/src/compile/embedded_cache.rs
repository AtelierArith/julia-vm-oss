//! Embedded precompiled Base cache.
//!
//! When built with `SJULIA_BASE_CACHE` environment variable,
//! the cache bytes are included at compile time via `include_bytes!`.
//! Otherwise, this module provides a `None` fallback.

use super::precompile::SerializedBaseCache;

/// Try to load the embedded Base cache.
/// Returns `None` if no cache was embedded at build time,
/// or if validation fails (version/hash mismatch).
pub(crate) fn load_embedded_cache() -> Option<SerializedBaseCache> {
    let bytes = embedded_cache_bytes()?;
    match super::precompile::deserialize_base_cache(bytes) {
        Ok(cache) => Some(cache),
        Err(e) => {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[Warning] Embedded Base cache invalid: {}. Falling back to runtime compilation.",
                e
            );
            None
        }
    }
}

/// Get the raw embedded cache bytes, if present.
fn embedded_cache_bytes() -> Option<&'static [u8]> {
    #[cfg(has_embedded_base_cache)]
    {
        Some(include_bytes!(env!("SJULIA_BASE_CACHE_PATH")))
    }
    #[cfg(not(has_embedded_base_cache))]
    {
        None
    }
}

//! Embedded precompiled Base cache.
//!
//! When built with `SJULIA_BASE_CACHE` environment variable,
//! the cache bytes are included at compile time via `include_bytes!`.
//! Otherwise, this module provides a `None` fallback.

// Issue #10906 (Phase 1c of #10869): cache-load boundary — zero real
// unwrap_used/expect_used sites (no code change needed; every load failure
// already degrades to `None` / runtime compilation).
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::precompile::SerializedBaseCache;

/// Try to load the embedded Base cache.
/// Returns `None` if no cache was embedded at build time,
/// or if validation fails (version/hash/fingerprint mismatch).
///
/// The embedded cache (iOS/`SJULIA_BASE_CACHE`) is generated in `build.sh` by
/// a `sjulia --precompile-base` binary built from the same source tree, so the
/// header fingerprints — including the enum variant fingerprint (Issue #8626)
/// — match by construction. Validation still runs so an accidentally embedded
/// stale/foreign cache degrades to runtime compilation instead of misdecoding.
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
pub(crate) fn embedded_cache_bytes() -> Option<&'static [u8]> {
    #[cfg(has_embedded_base_cache)]
    {
        Some(include_bytes!(env!("SJULIA_BASE_CACHE_PATH")))
    }
    #[cfg(not(has_embedded_base_cache))]
    {
        None
    }
}

/// Get the raw embedded preloaded-package cache bytes, if present
/// (Issue #9189/#9230). Built with `SJULIA_PRELOAD_CACHE` set (see `build.rs`
/// / `build.sh`), so iOS/WASM — which have no writable disk for the
/// persistent-file tier — still benefit from the whole-closure preload cache.
/// Validation (version / source hash / enum fingerprint) runs in
/// `preload_cache::deserialize_preload_cache` against the build-time
/// `PRELOAD_PACKAGES` configuration, so an accidentally stale embed degrades to
/// runtime compilation rather than misdecoding.
pub(crate) fn embedded_preload_cache_bytes() -> Option<&'static [u8]> {
    #[cfg(has_embedded_preload_cache)]
    {
        Some(include_bytes!(env!("SJULIA_PRELOAD_CACHE_PATH")))
    }
    #[cfg(not(has_embedded_preload_cache))]
    {
        None
    }
}

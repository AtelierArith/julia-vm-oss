//! Preloaded-package bytecode cache (Issue #9189).
//!
//! STATUS (Issue #9189/#9245/#9254): fully landed and ACTIVE — the "Stage 1/2/3"
//! narrative below is the original phased-implementation plan and is kept for
//! historical context. `PRELOAD_PACKAGES` is build configuration supplied via
//! `SJULIA_PRELOAD_PACKAGES`, the consumer is wired into `build_method_tables` /
//! `compile_functions`, and the cache is embedded for iOS/WASM via
//! `build.rs` + `build.sh` (`--precompile-packages`). Programs whose non-Base
//! function layout matches the bare-`using` generation get their package
//! functions spliced (compile ~307ms -> ~53ms). One known gap remains: a lifted
//! main lambda (the #9158 Surface sample) deactivates the gate fail-safe — see
//! `PRELOAD_PACKAGES`'s doc comment for the mechanism and why the obvious fix
//! reintroduces #9254.
//!
//! `using Plots; using LinearAlgebra` costs ~300ms in cold-start profiling
//! (`SJULIA_COMPILE_PROFILE=1`), almost entirely in `compile.emit_functions`
//! (inference + codegen) — NOT parse/lower, which is already cached by
//! `loader.rs`'s `.ji.json` module cache. The ~4975 true Base/prelude
//! functions skip this cost via the frozen `CachedBase`/`BASE_CACHE` fast
//! path (`cache.rs`, `pipeline_ctx.rs::compile_functions`), but that fast
//! path only covers the flat `program.functions[0..base_function_count]`
//! prefix. Bundled-package functions live inside `Module.functions` (nested)
//! and never pass through it, so every process start re-infers and
//! re-emits their bytecode from scratch.
//!
//! Unlike `CachedBase`, a bundled package is only supposed to be *visible*
//! once the user's program actually `using`s it — Base has no such on/off
//! switch, so this cache cannot simply extend `base_function_count` /
//! `cached_base_len` (a single always-present positional prefix) without
//! inventing a new visibility-gating layer on top of it. Instead this is an
//! **additive, hashmap-keyed** cache: `(module_path, function signature) ->
//! compiled body`, consulted only for modules that actually end up in a
//! program's `all_modules` this run (i.e. only for packages the program
//! actually `using`s — the same gate `loader.rs::should_load_module`
//! already enforces today). An unused preloaded package is therefore never
//! looked up here, so it stays completely inert: no new dormancy mechanism
//! is required, unlike a design that always folds the superset into the
//! frozen prefix and gates visibility separately.
//!
//! Cached function bodies are stored 0-based and relocated with
//! `relocate_jumps` when spliced into a real compile's code buffer — the
//! same primitive `compile_functions`/`compile_module_recursive`/
//! `compile_main` already use to append a freshly-compiled chunk at its
//! final absolute position (Issue #8192's `install_specialized_body` does
//! the identical trick for runtime specialization), so this is not a novel
//! risk to the bytecode format.
//!
//! **Stage 1**: cache data structures, generation (`generate_preload_cache_for`),
//! and serialization/versioning/fingerprint gating.
//!
//! **Stage 2 (in progress)**: wire consumption into `build_method_tables`
//! (the lookup, keyed off its already-resolved `params` local — *not* a
//! standalone from-raw-IR key derivation, see `signature_key_for_resolved_params`'s
//! doc comment for why two earlier attempts at that were silently wrong) and
//! `compile_functions` (skip `CoreCompiler::new_for_function` + inference on a
//! hit, splice the cached body in during `finalize`, after both peephole
//! passes — not folded into the existing `reused_base` prefix, since a
//! module's position in the code buffer varies per run, unlike Base's fixed
//! position 0). `get_or_init_preload_cache` below provides a persistent,
//! disk-backed cache (mirrors `pipeline.rs`'s prelude-Program cache) so a
//! *single cold CLI process* benefits, not just repeated in-process compiles.
//!
//! **Stage 3 (hand-off)**: populate `PRELOAD_PACKAGES` from explicit build
//! configuration; embed the generated cache for iOS/WASM the same way
//! `SJULIA_BASE_CACHE`/`SJULIA_PRELUDE_PROGRAM_CACHE` are embedded
//! (`embedded_cache.rs` — no writable disk at runtime there, so the
//! persistent-file tier alone doesn't help iOS); add a `--precompile-packages`
//! CLI entry point wiring into `build.sh`; and re-verify with the full
//! isolation + perf + nextest + AoT gates described in the Issue. Issue #10160
//! intentionally stopped the earlier default sample-union auto-detection
//! because that union does not match real samples' exact package closure/order.
//!
//! No package name appears in any compile/dispatch branch (Design Principle
//! #8) — `PRELOAD_PACKAGES` is the only package-specific surface, and it is
//! plain config read generically by the (future) consumer, the same way
//! `base_cache_schema_files.txt` is a config manifest rather than inline
//! logic. The lookup mechanism itself works for any module (a nice side
//! effect: it would also speed up Base's own submodules, not just bundled
//! packages), satisfying Design Principle #10.

// Issue #10906 (Phase 1c of #10869): the preloaded-package bytecode
// cache-load boundary — zero real unwrap_used/expect_used sites in
// production code (every match is inside the cfg(test) module, which
// carries an explicit allow).
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::cell::Cell;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::bytecode::{FunctionInfo, Instr};

use super::utils::relocate_jumps;

/// Packages to fold into the preloaded bytecode cache (Issue #9189 Stage 3).
/// `LinearAlgebra` + `Plots` are the issue's target (`using Plots; using
/// LinearAlgebra` cold-start cost); `SciMLBase` is Plots' one transitive
/// dependency (`packages/Plots/Project.toml`; `packages/SciMLBase` itself has
/// no further deps — verified against both Project.toml `[deps]` and each
/// package's own top-level `using`/`import` statements, since `loader.rs`
/// resolves both). Kept as a plain config list so the *lookup* mechanism
/// never has to branch on a package name (Design Principle #8).
///
/// **Root cause of the earlier ~57-failure activation (Issue #9230, CONFIRMED
/// and fixed)**: enabling `["LinearAlgebra", "Plots", "SciMLBase"]` reproduced
/// `MethodError: Any is ambiguous. Multiple candidates matched with equal
/// specificity: Any(::Any)` on `norm` (and every fixture downstream of
/// LinearAlgebra). Two symptoms: (1) `UndefVarError` on kwarg-derived locals —
/// the cache-hit path didn't copy `kwparams[].slot` (only assigned by
/// `finalize`'s slotize loop, which a hit skips); now copied. (2) the
/// ambiguous-`Any` dispatch — root-caused via a runtime tie-candidate dump: the
/// cache was generated PER-PACKAGE and WITHOUT the Base cache, so the captured
/// bodies' frozen absolute call-target indices came from a different function
/// layout than the base-cache compile that consumes them, and `relocate_jumps`
/// fixes jump offsets but NOT call-target indices — so `norm`'s spliced body
/// dispatched a `Vector{Float64}` to four unrelated 1-arg lambdas that happened
/// to occupy those indices. sjulia's caches rely on LAYOUT IDENTITY, not
/// relocation (`refresh_cached_base_dispatch_candidates` / the Base cache both
/// assume base indices are never moved).
///
/// **The fix restores layout identity** (like the Base cache):
/// `generate_preload_cache_for` compiles the WHOLE closure as one
/// `using P1\nusing P2\n...` preamble WITH the Base cache, and
/// `build_method_tables` reuses the cache only when a program's **entire
/// non-Base function region** matches the cached `closure_layout`. A spliced
/// module body's frozen call targets reference Base functions AND non-Base
/// functions/closures — including the trailing lifted Base closures
/// (`_rstrip_eq_pred`, broadcast `fused`/`sel`, `__lambda_nested_*`) the
/// two-region split (Issue #9245) leaves after the package region — so that
/// whole region must align. Issue #9245 first narrowed the layout to the
/// package region only (`base_function_count..first_user_function_idx`) to keep
/// the gate active when a lifted lambda / user function trails the region; but
/// a user/main lifted lambda is inserted at the FRONT of that trailing block
/// and shifts every Base closure by one, so a frozen index that meant
/// `_rstrip_eq_pred` at generation dispatched to its shifted neighbor at
/// consumption — the #9254 iOS Surface sample (`surface(x, y, (x,y) -> …)` after
/// `using Plots; using LinearAlgebra`) silently rendered a 2-D line. The layout
/// now spans the whole non-Base region (Issue #9254), so any interposed user
/// function or lifted lambda deactivates the gate and falls back to a normal
/// (still-correct, base-cache-backed) compile — fail-safe. Programs with no
/// lifted lambda (`plot([1,2,3])`, `plot(sin)`) keep the whole non-Base region
/// identical to generation and stay on the fast spliced path. Any mismatch
/// (different package set / load order too) also falls back.
///
/// Populated in the order a target program `using`s the packages. The generated
/// `closure_layout` must match the consuming program's load order, or the gate
/// deactivates.
///
/// **Known remaining gap (Issue #9189, measured)**: the #9158 Surface sample
/// `surface(x, y, (x,y) -> …)` lifts its main lambda to a top-level `__lambda_*`
/// USER function placed at `first_user_function_idx`, which shifts every
/// following non-Base function by one and deactivates this gate — so the sample
/// still pays a full ~266 ms compile (fail-safe correct, just slow). The obvious
/// fix (relocate the deterministic trailing Base-body closures ahead of user
/// functions) was prototyped and REINTRODUCES the #9254 wrong-output bug: struct
/// constructors are appended to `function_infos` after `all_functions` (see
/// `pipeline_ctx.rs::register_inner_constructors`), so their absolute indices
/// still shift and spliced package bodies that construct `Series`/`Plot`
/// mis-dispatch. A complete fix must place ALL user-introduced functions after
/// ALL deterministic functions (struct-ctor region included), not just relocate
/// the trailing Base closures — tracked as follow-up #9477.
/// Comma-separated package list to fold into the embedded preload bytecode cache.
///
/// `build.sh` passes an explicitly supplied `SJULIA_PRELOAD_PACKAGES` value to
/// both cache generation and the final FFI build. When the variable is unset or
/// empty, the consumer stays inert and build.sh skips generation/embed (Issue
/// #10160). Keeping the list outside Rust source avoids package-name compile
/// shortcuts while preserving the layout identity contract.
pub(crate) const PRELOAD_PACKAGES: &str = match option_env!("SJULIA_PRELOAD_PACKAGES") {
    Some(packages) => packages,
    None => "",
};

pub fn parse_preload_package_list(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect()
}

fn compile_time_preload_packages() -> Vec<&'static str> {
    parse_preload_package_list(PRELOAD_PACKAGES)
}

thread_local! {
    /// Prevent cache generation from recursively trying to consume the cache
    /// it has not finished producing yet (Issue #10196).
    static PRELOAD_CACHE_GENERATION_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct PreloadCacheGenerationGuard;

impl PreloadCacheGenerationGuard {
    fn enter() -> Self {
        PRELOAD_CACHE_GENERATION_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self
    }
}

impl Drop for PreloadCacheGenerationGuard {
    fn drop(&mut self) {
        PRELOAD_CACHE_GENERATION_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

fn preload_cache_generation_in_progress() -> bool {
    PRELOAD_CACHE_GENERATION_DEPTH.with(|depth| depth.get() != 0)
}

fn preload_cache_consumer_enabled(packages: &[&str]) -> bool {
    !packages.is_empty() && !preload_cache_generation_in_progress()
}

/// Bumped whenever `CachedPreloadFunction`/`CachedPreloadModule`/
/// `SerializedPreloadCache` change shape OR the `closure_layout` semantics
/// change (an old-semantics layout must not be matched by the new gate).
///
/// v2 (Issue #9230): whole-closure generation + `closure_layout` gate.
/// v3 (Issue #9254): `closure_layout` spans the FULL non-Base region (package
/// region + trailing Base closures), not just the package region, so a user
/// lambda that shifts the trailing Base closures deactivates the gate.
/// v4 (Issue #9784): `FunctionInfo` persists helper provenance/definition order
/// and `Instr` appends `CreateResolvedClosure`; stale v3 bodies have a different
/// wire shape and must be rejected before body decoding.
const PRELOAD_CACHE_VERSION: u32 = 4;

/// A single module-scoped function's compiled body, stored 0-based so it can
/// be relocated (via `relocate_jumps`) to wherever it lands in a real
/// compile's code buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedPreloadFunction {
    /// `entry`/`code_start`/`code_end` are 0-based against `body`, not a real
    /// program's code buffer — the Stage 2 consumer must rebase them (and
    /// `body`'s jump targets) to the function's actual final position.
    pub function_info: FunctionInfo,
    pub body: Vec<Instr>,
}

/// One preload-listed module's captured functions, keyed by the same
/// `"name(ParamType1,ParamType2)"` signature string `merge_prelude_into_user_program`
/// / `merge_with_precompiled_base` already use to disambiguate overloads.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CachedPreloadModule {
    pub functions: HashMap<String, CachedPreloadFunction>,
}

/// On-disk/embedded shape of the whole preload cache.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SerializedPreloadCache {
    pub version: u32,
    /// SHA-256 over each preload package's bundled/stdlib source + Project.toml,
    /// plus the compiler build fingerprint — analogous to
    /// `pipeline::compute_prelude_source_hash` / `precompile::compute_base_cache_hash`.
    /// Editing a preloaded package's `.jl` source invalidates this cache.
    pub source_hash: String,
    pub enum_variant_fingerprint: String,
    /// The whole preload closure's non-Base function layout in global-function-index
    /// order (`(module_path, bare IR name)` per index), captured from the single
    /// combined `using P1\nusing P2\n...` compile that produced this cache
    /// (Issue #9230). This spans the ENTIRE non-Base region — package functions,
    /// module closures, AND the trailing lifted Base closures — because a spliced
    /// body's frozen call targets reach all of them (Issue #9254; #9245's
    /// package-region-only narrowing let a user lambda shift the trailing Base
    /// closures undetected). A consuming compile only reuses the cache when its
    /// own non-Base function prefix *starts with* this layout, guaranteeing every
    /// spliced body's frozen absolute function index still resolves to the same
    /// function (layout identity — no relocation, mirroring the Base cache).
    pub closure_layout: Vec<(Option<String>, String)>,
    /// Module path (e.g. `"LinearAlgebra"`, `"Plots.RecipesBase"`) -> captured functions.
    pub modules: HashMap<String, CachedPreloadModule>,
}

/// In-memory view of a validated preload cache: the two pieces the consuming
/// compile needs (Issue #9230). The `closure_layout` gate and the `modules`
/// bodies are always set together, so `CompilerCacheInput`'s `preload_cache`
/// and `preload_closure_layout` are populated from this pair as a unit.
#[derive(Debug)]
pub(crate) struct PreloadCacheData {
    pub modules: HashMap<String, CachedPreloadModule>,
    pub closure_layout: Vec<(Option<String>, String)>,
}

impl From<SerializedPreloadCache> for PreloadCacheData {
    fn from(cache: SerializedPreloadCache) -> Self {
        PreloadCacheData {
            modules: cache.modules,
            closure_layout: cache.closure_layout,
        }
    }
}

/// Build the `"name(ParamType1,ParamType2)"` key stored by
/// `generate_preload_cache_for` (from a bare IR name + the compiled
/// `FunctionInfo::param_julia_types`).
///
/// Always takes an explicit bare `name` rather than reading
/// `FunctionInfo.name`: for any module-scoped function, `FunctionInfo.name`
/// is *always* module-qualified (`format!("{}.{}", module_path, func.name)`,
/// see `pipeline_ctx.rs`'s `function_name` construction) so `methods`/
/// reflection can disambiguate — but the bare IR `Function.name` is what a
/// Stage 2 lookup site (before that qualification happens) sees.
///
/// **Correctness note (Issue #9189)**: `param_julia_types` is *not* a raw
/// echo of `func.params[i].effective_type()`. `build_method_tables`
/// (pipeline_ctx.rs ~L1830) resolves each parameter type through
/// `qualify_type_for_module` (bare `UpperHessenberg` -> module-qualified
/// `LinearAlgebra.UpperHessenberg` when the struct is module-local),
/// `resolve_abstract_type`, and `resolve_type_alias` *before* it becomes
/// `param_julia_types` — two independent early attempts at a standalone
/// "derive the key straight from raw IR" helper each produced a key that
/// silently never matched (caught by a roundtrip consistency test against
/// the real `LinearAlgebra` module, not by inspection). The Stage 2 lookup
/// therefore must not try to re-derive this resolution independently; it
/// reuses `build_method_tables`'s own already-resolved `params: Vec<(String,
/// JuliaType)>` local (pipeline_ctx.rs ~L1830-1842) directly, via
/// `signature_key_for_resolved_params` below, so there is exactly one
/// implementation of the resolution, not two copies to keep in sync.
fn signature_key_from_types(name: &str, param_types: &[crate::types::JuliaType]) -> String {
    let param_strs: Vec<String> = param_types.iter().map(|ty| ty.to_string()).collect();
    format!("{}({})", name, param_strs.join(","))
}

/// Same key, from `build_method_tables`'s already-resolved `params: Vec<(String,
/// JuliaType)>` (pipeline_ctx.rs ~L1830) — the Stage 2 lookup call site. Must
/// stay byte-identical to what `generate_preload_cache_for` stores.
pub(crate) fn signature_key_for_resolved_params(
    name: &str,
    params: &[(String, crate::types::JuliaType)],
) -> String {
    let param_types: Vec<crate::types::JuliaType> =
        params.iter().map(|(_, ty)| ty.clone()).collect();
    signature_key_from_types(name, &param_types)
}

/// Look up a preload package's bundled/stdlib source for hashing. Mirrors the
/// resolution order `loader.rs::resolve_package` uses for the default
/// `LoaderConfig` (stdlib first, then bundled packages).
fn preload_package_source(name: &str) -> Option<(&'static str, &'static str)> {
    if let Some(pkg) = crate::stdlib::get_stdlib_package(name) {
        return Some((pkg.project_toml, pkg.source));
    }
    if let Some(pkg) = crate::packages::get_bundled_package(name) {
        return Some((pkg.project_toml, pkg.source));
    }
    None
}

fn compute_preload_source_hash(names: &[&str]) -> String {
    let mut sorted: Vec<&str> = names.to_vec();
    sorted.sort_unstable();

    let mut hasher = sha2::Sha256::new();
    for name in sorted {
        hasher.update(name.as_bytes());
        hasher.update(b"\0");
        if let Some((project_toml, source)) = preload_package_source(name) {
            hasher.update(project_toml.as_bytes());
            hasher.update(b"\0");
            hasher.update(source.as_bytes());
        }
        hasher.update(b"\0\0");
    }
    hasher.update(b"\0compiler\0");
    hasher.update(super::precompile::compiler_build_fingerprint().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate the preload cache for an explicit package list (test/tooling
/// entry point; `generate_preload_cache()` below is the real one, driven by
/// `PRELOAD_PACKAGES`).
///
/// For each package, compiles a synthetic `using <name>` program through the
/// ordinary pipeline (so Base merge + `PackageLoader` dependency resolution
/// happen exactly as they would for real user code) and slices the module's
/// own compiled function bodies out of the result using
/// `CoreCompileOutput::module_function_infos` (Issue #9189).
pub fn generate_preload_cache_for(names: &[&str]) -> Result<Vec<u8>, String> {
    let _generation_guard = PreloadCacheGenerationGuard::enter();

    // Issue #9230: the cache is CONSUMED in compiles that use the real Base
    // cache AND must be reused with layout identity (no per-body relocation).
    // So generation (a) uses the same Base cache and (b) compiles the WHOLE
    // preload closure as ONE `using P1\nusing P2\n...` preamble. That single
    // compile fixes every preload function's global index in the closure
    // layout; a consuming compile only splices when its non-Base function
    // prefix matches `closure_layout` (see the gate in `build_method_tables`),
    // so the captured bodies' frozen absolute call-target indices stay valid
    // (`relocate_jumps` fixes jump offsets but NOT call-target indices — a
    // per-package or no-Base-cache compile would misalign them, which is the
    // `MethodError: Any is ambiguous` on `norm` this fixes).
    crate::compile::cache::warm_base_cache();
    let base = crate::compile::cache::export_base_cache();
    let base_function_count = base
        .as_ref()
        .map(|(compiled, ..)| compiled.functions.len())
        .unwrap_or(0);

    let src: String = names.iter().map(|name| format!("using {name}\n")).collect();
    let program = crate::pipeline::parse_and_lower(&src)
        .map_err(|e| format!("preload cache: failed to load closure {names:?}: {e}"))?;
    let cache_input = match &base {
        Some((compiled, method_tables, closure_captures, inference_results)) => {
            super::CompilerCacheInput {
                precompiled_base: Some(compiled),
                method_tables: Some(method_tables),
                closure_captures: Some(closure_captures),
                inference_results: Some(inference_results.as_slice()),
                ..Default::default()
            }
        }
        None => super::CompilerCacheInput::default(),
    };
    let output = super::compile_core_program_internal(
        &program,
        &HashMap::new(),
        &HashMap::new(),
        cache_input,
    )
    .map_err(|e| format!("preload cache: failed to compile closure {names:?}: {e:?}"))?;

    // Capture every non-Base module function's finalized body, keyed by
    // (module path, signature). Their frozen call targets are already in the
    // closure layout, so no per-body relocation is needed once the consuming
    // compile's `closure_layout` gate confirms the same prefix.
    let mut modules: HashMap<String, CachedPreloadModule> = HashMap::new();
    for (module_path, func_info_indices) in &output.module_function_infos {
        let cached_module = modules.entry(module_path.clone()).or_default();
        for (idx, bare_name) in func_info_indices {
            if *idx < base_function_count {
                // Base function — always layout-identical, never captured here.
                continue;
            }
            let Some(fi) = output.compiled.functions.get(*idx) else {
                continue;
            };
            let (start, end) = (fi.code_start, fi.code_end);
            if start >= end || end > output.compiled.code.len() {
                // Nothing to cache (e.g. a stub with no body); leave it for the
                // ordinary compile path to handle on activation.
                continue;
            }

            let mut body = output.compiled.code[start..end].to_vec();
            relocate_jumps(&mut body, start, 0);

            let mut function_info = (**fi).clone();
            function_info.entry = 0;
            function_info.code_start = 0;
            function_info.code_end = body.len();

            // Keyed by the *bare* IR name (see `module_function_infos`'s doc
            // comment) — `function_info.name` is module-qualified for every
            // module-scoped function and would never match the as-yet-uncompiled
            // IR `Function.name` a Stage 2 lookup sees.
            let key = signature_key_from_types(bare_name, &function_info.param_julia_types);
            cached_module.functions.insert(
                key,
                CachedPreloadFunction {
                    function_info,
                    body,
                },
            );
        }
    }

    let cache = SerializedPreloadCache {
        version: PRELOAD_CACHE_VERSION,
        source_hash: compute_preload_source_hash(names),
        enum_variant_fingerprint: super::precompile::enum_variant_fingerprint(),
        closure_layout: output.nonbase_layout,
        modules,
    };
    bincode::serialize(&cache).map_err(|e| format!("Preload cache serialization failed: {}", e))
}

/// Generate the preload cache for the configured package list. Driven by
/// `--precompile-packages` and `build.sh`'s embedded-cache step.
pub fn generate_preload_cache() -> Result<Vec<u8>, String> {
    let packages = compile_time_preload_packages();
    generate_preload_cache_for(&packages)
}

/// Deserialize and validate a preload cache against the package list that
/// should have produced it (mirrors `pipeline::deserialize_prelude_program` /
/// `precompile::deserialize_base_cache`'s discard-and-regenerate discipline).
pub(crate) fn deserialize_preload_cache(
    bytes: &[u8],
    names: &[&str],
) -> Result<SerializedPreloadCache, String> {
    // `version` is the first fixed-width bincode field. Read it independently
    // so an old cache whose remaining struct/enum shape is incompatible gets a
    // clean version rejection rather than failing halfway through body decode.
    let encoded_version: u32 = bincode::deserialize_from(&mut std::io::Cursor::new(bytes))
        .map_err(|e| format!("Preload cache version decode failed: {}", e))?;
    if encoded_version != PRELOAD_CACHE_VERSION {
        return Err(format!(
            "Preload cache version mismatch: expected {}, got {}",
            PRELOAD_CACHE_VERSION, encoded_version
        ));
    }

    let cache: SerializedPreloadCache = bincode::deserialize(bytes)
        .map_err(|e| format!("Preload cache deserialization failed: {}", e))?;

    debug_assert_eq!(cache.version, encoded_version);
    if cache.source_hash != compute_preload_source_hash(names) {
        return Err("Preload cache source hash mismatch".to_string());
    }
    if cache.enum_variant_fingerprint != super::precompile::enum_variant_fingerprint() {
        return Err("Preload cache enum variant fingerprint mismatch".to_string());
    }

    Ok(cache)
}

fn persistent_preload_cache_disabled() -> bool {
    std::env::var("SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELOAD_CACHE").is_ok()
}

/// Mirrors `pipeline.rs::workspace_target_dir` (not reused directly: that one
/// is private to `pipeline.rs`).
fn workspace_target_dir() -> std::path::PathBuf {
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        return std::path::PathBuf::from(target_dir);
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("target"))
        .unwrap_or_else(|| std::path::PathBuf::from("target"))
}

fn persistent_preload_cache_path(names: &[&str]) -> std::path::PathBuf {
    let hash = compute_preload_source_hash(names);
    workspace_target_dir().join(format!("sjulia_preload_cache_{hash}.bin"))
}

struct PersistentPreloadCacheLock {
    path: std::path::PathBuf,
}

impl Drop for PersistentPreloadCacheLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Mirrors `pipeline.rs::acquire_persistent_prelude_lock`'s create-new-file
/// mutual exclusion + staleness handling (kept independent since the two
/// caches invalidate/regenerate on unrelated schedules).
fn acquire_persistent_preload_cache_lock(
    cache_path: &std::path::Path,
) -> Option<PersistentPreloadCacheLock> {
    let lock_path = cache_path.with_extension("lock");
    let stale_after = std::time::Duration::from_secs(20 * 60);

    for _ in 0..1200 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => return Some(PersistentPreloadCacheLock { path: lock_path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if let Ok(metadata) = std::fs::metadata(&lock_path) {
                    let is_stale = metadata
                        .modified()
                        .ok()
                        .and_then(|modified| {
                            std::time::SystemTime::now().duration_since(modified).ok()
                        })
                        .is_some_and(|age| age > stale_after);
                    if is_stale {
                        let _ = std::fs::remove_file(&lock_path);
                    }
                }
                if cache_path.exists() {
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
    None
}

fn read_persistent_preload_cache(
    path: &std::path::Path,
    names: &[&str],
) -> Option<PreloadCacheData> {
    let bytes = std::fs::read(path).ok()?;
    match deserialize_preload_cache(&bytes, names) {
        Ok(cache) => Some(cache.into()),
        Err(_) => {
            // Stale/foreign file under this hash — remove and regenerate,
            // same discard-and-regenerate discipline as the Base/prelude caches.
            let _ = std::fs::remove_file(path);
            None
        }
    }
}

fn write_persistent_preload_cache(path: &std::path::Path, bytes: &[u8]) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    if std::fs::write(&tmp_path, bytes).is_err() {
        return;
    }
    if std::fs::rename(&tmp_path, path).is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
}

fn load_or_generate_preload_cache(names: &[&str]) -> Option<PreloadCacheData> {
    // Issue #9230: prefer the embedded cache. iOS/WASM have no writable disk for
    // the persistent-file tier, so this is the only tier that reaches them; on a
    // host it still avoids regenerating. A stale/foreign embed fails validation
    // in `deserialize_preload_cache` and falls through to the disk/generation
    // tiers below (mirrors `embedded_cache::load_embedded_cache`).
    if let Some(bytes) = super::embedded_cache::embedded_preload_cache_bytes() {
        if let Ok(cache) = deserialize_preload_cache(bytes, names) {
            return Some(cache.into());
        }
    }

    if persistent_preload_cache_disabled() {
        let bytes = generate_preload_cache_for(names).ok()?;
        return deserialize_preload_cache(&bytes, names)
            .ok()
            .map(PreloadCacheData::from);
    }

    let cache_path = persistent_preload_cache_path(names);
    if let Some(modules) = read_persistent_preload_cache(&cache_path, names) {
        return Some(modules);
    }

    let Some(_lock) = acquire_persistent_preload_cache_lock(&cache_path) else {
        // Another process is writing it (or the retry budget ran out); use
        // whatever it finished, or fall back to an in-process compile rather
        // than block indefinitely.
        if let Some(modules) = read_persistent_preload_cache(&cache_path, names) {
            return Some(modules);
        }
        let bytes = generate_preload_cache_for(names).ok()?;
        return deserialize_preload_cache(&bytes, names)
            .ok()
            .map(PreloadCacheData::from);
    };

    if let Some(modules) = read_persistent_preload_cache(&cache_path, names) {
        return Some(modules);
    }

    let bytes = generate_preload_cache_for(names).ok()?;
    write_persistent_preload_cache(&cache_path, &bytes);
    deserialize_preload_cache(&bytes, names)
        .ok()
        .map(PreloadCacheData::from)
}

thread_local! {
    /// Process-local handle to the (persistent-file-backed) preload cache, so
    /// repeated compiles in the same process (REPL, benches, nextest) don't
    /// re-read/re-deserialize it every time. Sibling of `cache.rs`'s
    /// `BASE_CACHE` thread-local.
    static PRELOAD_CACHE: std::cell::RefCell<Option<std::rc::Rc<PreloadCacheData>>> =
        const { std::cell::RefCell::new(None) };
}

/// Get (loading/generating once per process if needed) the preload cache's
/// module bodies + closure layout. Returns `None` immediately, with zero
/// filesystem/compile work, whenever `PRELOAD_PACKAGES` is empty — the
/// default-safe path when the cache is disabled.
pub(crate) fn get_or_init_preload_cache() -> Option<std::rc::Rc<PreloadCacheData>> {
    let packages = compile_time_preload_packages();
    if !preload_cache_consumer_enabled(&packages) {
        return None;
    }
    PRELOAD_CACHE.with(|cache| {
        if let Some(existing) = cache.borrow().as_ref() {
            return Some(std::rc::Rc::clone(existing));
        }
        let data = load_or_generate_preload_cache(&packages)?;
        let rc = std::rc::Rc::new(data);
        *cache.borrow_mut() = Some(std::rc::Rc::clone(&rc));
        Some(rc)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn preload_generation_disables_recursive_cache_consumption_issue_10196() {
        let packages = ["Plots"];
        assert!(preload_cache_consumer_enabled(&packages));

        {
            let _outer = PreloadCacheGenerationGuard::enter();
            assert!(!preload_cache_consumer_enabled(&packages));
            {
                let _inner = PreloadCacheGenerationGuard::enter();
                assert!(!preload_cache_consumer_enabled(&packages));
            }
            assert!(!preload_cache_consumer_enabled(&packages));
        }

        assert!(preload_cache_consumer_enabled(&packages));
    }

    #[test]
    fn preload_package_list_parser_trims_empty_entries_issue_9947() {
        assert_eq!(
            parse_preload_package_list(" Alpha, Beta ,,Gamma "),
            vec!["Alpha", "Beta", "Gamma"]
        );
    }

    #[test]
    fn preload_cache_generation_covers_every_target_package() {
        // Exercises the target list explicitly (not the currently-empty
        // `PRELOAD_PACKAGES`, see its doc comment) via `generate_preload_cache_for`:
        // generation itself must succeed and capture at least one function for
        // every configured package. This still passes today — the blocked bug
        // is in *using* the cache broadly, not in generating it.
        let names = ["LinearAlgebra", "Plots", "SciMLBase"];
        let bytes = generate_preload_cache_for(&names).expect("generate preload cache");
        let cache = deserialize_preload_cache(&bytes, &names).expect("deserialize cache");
        for &name in &names {
            let module = cache
                .modules
                .get(name)
                .unwrap_or_else(|| panic!("{name} should be captured in the preload cache"));
            assert!(
                !module.functions.is_empty(),
                "{name} should contribute at least one cached function body"
            );
        }
    }

    #[test]
    fn preload_cache_roundtrips_for_a_bundled_stdlib_module() {
        let names = ["LinearAlgebra"];
        let bytes = generate_preload_cache_for(&names).expect("generate preload cache");
        let cache = deserialize_preload_cache(&bytes, &names).expect("deserialize preload cache");

        assert_eq!(cache.version, PRELOAD_CACHE_VERSION);
        let module = cache
            .modules
            .get("LinearAlgebra")
            .expect("LinearAlgebra module should be captured");
        assert!(
            !module.functions.is_empty(),
            "LinearAlgebra should contribute at least one cached function body"
        );
        for cached in module.functions.values() {
            assert_eq!(cached.function_info.code_start, 0);
            assert_eq!(cached.function_info.code_end, cached.body.len());
            assert_eq!(cached.function_info.entry, 0);
        }
    }

    // NOTE (Issue #9189 Stage 2): an earlier version of this module had a
    // standalone `ir_signature_key(func: &Function) -> String` here, tested
    // against a real `LinearAlgebra` compile for consistency with the cached
    // keys. That test caught two independent bugs in quick succession
    // (`FunctionInfo.name` is module-qualified, and `param_julia_types` is
    // resolved through `qualify_type_for_module`/`resolve_abstract_type`/
    // `resolve_type_alias` — neither of which a standalone from-raw-IR
    // function can replicate without duplicating `build_method_tables`'s own
    // resolution logic). Rather than attempt a third, still-fragile
    // reimplementation, the Stage 2 lookup instead reuses
    // `build_method_tables`'s already-resolved `params: Vec<(String,
    // JuliaType)>` local directly via `signature_key_for_resolved_params`
    // (see its doc comment) — there is exactly one implementation of the
    // resolution, so no separate consistency test is needed at this layer.
    // Correctness of the wired-up lookup is instead proven end-to-end below.

    /// Compile `src` via `compile_core_program_internal` with an explicit
    /// `CompilerCacheInput` (no Base cache either way, to isolate exactly the
    /// preload-cache effect), run it, and return captured stdout.
    fn compile_and_run_with_cache_input(
        src: &str,
        cache_input: super::super::CompilerCacheInput<'_>,
    ) -> String {
        let program = crate::pipeline::parse_and_lower(src).expect("parse/lower");
        let output = super::super::compile_core_program_internal(
            &program,
            &HashMap::new(),
            &HashMap::new(),
            cache_input,
        )
        .expect("compile");
        crate::test_runtime::run_compiled_program(output.compiled, 42).expect("vm run")
    }

    /// Issue #9230: with the REAL Base cache active on both sides (the
    /// configuration every `compile_with_cache` caller actually uses), a
    /// preload-cache-active compile of a `using LinearAlgebra` program that
    /// calls `norm` must produce the SAME output as the ordinary compile.
    ///
    /// Before the fix, `generate_preload_cache_for` compiled packages WITHOUT
    /// the Base cache (`CompilerCacheInput::default()`), so the captured bodies
    /// embedded absolute function indices from a from-source Base layout;
    /// spliced into a base-cache compile they pointed at unrelated functions,
    /// surfacing as `MethodError: Any is ambiguous` on `norm`. Generation now
    /// uses the same Base cache, restoring the layout identity sjulia's caches
    /// rely on (`refresh_cached_base_dispatch_candidates` / the Base cache both
    /// assume base indices are unchanged, never relocated).
    #[test]
    fn preload_cache_with_real_base_cache_matches_normal_compile() {
        use super::super::CompilerCacheInput;
        crate::compile::cache::warm_base_cache();
        let (base_compiled, base_mt, base_cc, base_inf) =
            crate::compile::cache::export_base_cache().expect("base cache warmed");

        let names = ["LinearAlgebra"];
        let bytes = generate_preload_cache_for(&names).expect("gen preload");
        let cache = deserialize_preload_cache(&bytes, &names).expect("deser preload");

        let src = "using LinearAlgebra\nprintln(norm([1.0, 2.0, 3.0]))\n";
        let program = crate::pipeline::parse_and_lower(src).expect("parse/lower");

        let run = |preload: Option<&SerializedPreloadCache>| -> Result<(String, usize), String> {
            let out = super::super::compile_core_program_internal(
                &program,
                &HashMap::new(),
                &HashMap::new(),
                CompilerCacheInput {
                    precompiled_base: Some(&base_compiled),
                    method_tables: Some(&base_mt),
                    closure_captures: Some(&base_cc),
                    inference_results: Some(base_inf.as_slice()),
                    preload_cache: preload.map(|c| &c.modules),
                    preload_closure_layout: preload.map(|c| c.closure_layout.as_slice()),
                    ..Default::default()
                },
            )
            .map_err(|e| format!("compile error: {e:?}"))?;
            let spliced = out.preload_spliced_count;
            let output = crate::test_runtime::run_compiled_program(out.compiled, 42)?;
            Ok((output, spliced))
        };

        let (off, off_spliced) = run(None).expect("baseline (base cache, no preload) must succeed");
        let (on, on_spliced) = run(Some(&cache)).expect("preload-active compile must succeed");
        assert_eq!(
            off, "3.7416573867739413\n",
            "sanity: norm([1,2,3]) == sqrt(14)"
        );
        assert_eq!(
            on, off,
            "preload-active compile must match the ordinary compile"
        );
        assert_eq!(off_spliced, 0, "no preload cache -> nothing spliced");
        assert!(
            on_spliced > 0,
            "the closure_layout gate must ACTIVATE and splice preloaded bodies, \
             not silently fall back to a normal compile"
        );
    }

    /// Issue #9230: the whole-closure generation + `closure_layout` gate must
    /// make a MULTI-package consumption work — the exact case per-package
    /// generation broke. `Random` is a light stdlib package that loads *ahead*
    /// of `LinearAlgebra`, shifting its function indices; with the old
    /// per-package (or no-Base) generation, `norm`'s spliced body's frozen
    /// intra-package call targets pointed at the wrong (shifted) functions and
    /// dispatch died with `MethodError: Any is ambiguous`. Generating the whole
    /// `using Random; using LinearAlgebra` closure at once + gating on the
    /// matching prefix keeps every frozen index valid with no relocation.
    #[test]
    fn preload_cache_reuses_multi_package_closure_layout() {
        use super::super::CompilerCacheInput;
        crate::compile::cache::warm_base_cache();
        let (base_compiled, base_mt, base_cc, base_inf) =
            crate::compile::cache::export_base_cache().expect("base cache warmed");

        let names = ["Random", "LinearAlgebra"];
        let bytes = generate_preload_cache_for(&names).expect("gen preload");
        let cache = deserialize_preload_cache(&bytes, &names).expect("deser preload");

        let src = "using Random\nusing LinearAlgebra\nprintln(norm([1.0, 2.0, 3.0]))\n";
        let program = crate::pipeline::parse_and_lower(src).expect("parse/lower");

        let run = |preload: Option<&SerializedPreloadCache>| -> Result<(String, usize), String> {
            let out = super::super::compile_core_program_internal(
                &program,
                &HashMap::new(),
                &HashMap::new(),
                CompilerCacheInput {
                    precompiled_base: Some(&base_compiled),
                    method_tables: Some(&base_mt),
                    closure_captures: Some(&base_cc),
                    inference_results: Some(base_inf.as_slice()),
                    preload_cache: preload.map(|c| &c.modules),
                    preload_closure_layout: preload.map(|c| c.closure_layout.as_slice()),
                    ..Default::default()
                },
            )
            .map_err(|e| format!("compile error: {e:?}"))?;
            let spliced = out.preload_spliced_count;
            let output = crate::test_runtime::run_compiled_program(out.compiled, 42)?;
            Ok((output, spliced))
        };

        let (off, _) = run(None).expect("multi-package baseline must succeed");
        let (on, on_spliced) =
            run(Some(&cache)).expect("multi-package preload-active compile must succeed");
        assert_eq!(
            off, "3.7416573867739413\n",
            "sanity: norm([1,2,3]) == sqrt(14)"
        );
        assert_eq!(
            on, off,
            "multi-package preload-active compile must match the ordinary compile"
        );
        assert!(
            on_spliced > 0,
            "the gate must ACTIVATE for a matching multi-package closure and splice \
             LinearAlgebra bodies at their shifted (but layout-aligned) indices"
        );
    }

    /// Issue #9254 (regression guard): a program that lifts an anonymous main
    /// lambda — the iOS Surface sample's shape,
    /// `surface(x, y, (x, y) -> sinc(norm([x, y])))` — must FAIL-SAFE, i.e. the
    /// `closure_layout` gate must DEACTIVATE and the program must still produce
    /// the ordinary-compile output.
    ///
    /// sjulia lifts that lambda to a `__lambda_*` function that lands at the
    /// FRONT of the trailing inline-function block, right before the lifted Base
    /// closures (`__lambda_nested_*`, `_rstrip_eq_pred`, broadcast `fused`/`sel`,
    /// …). It shifts every one of those Base closures by one. A spliced package
    /// body's frozen absolute call target that meant a Base closure at
    /// generation then points one slot off at consumption — the #9254 bug, where
    /// `surface` silently rendered a 2-D line. #9245 narrowed `closure_layout` to
    /// the package region only, which let this shift go UNDETECTED and the gate
    /// stayed (unsoundly) active. #9254 restores the full-non-Base-region layout,
    /// so the shift now diverges the gate and it deactivates — a normal,
    /// base-cache-backed compile that is always correct. (A perf follow-up may
    /// relocate the deterministic trailing Base closures into the gated region so
    /// the fast spliced path survives a main lambda, but correctness comes first.)
    #[test]
    fn preload_cache_deactivates_for_a_main_lambda_program_9254() {
        use super::super::CompilerCacheInput;
        crate::compile::cache::warm_base_cache();
        let (base_compiled, base_mt, base_cc, base_inf) =
            crate::compile::cache::export_base_cache().expect("base cache warmed");

        let names = ["LinearAlgebra"];
        let bytes = generate_preload_cache_for(&names).expect("gen preload");
        let cache = deserialize_preload_cache(&bytes, &names).expect("deser preload");

        // An ANONYMOUS main-body lambda passed as a higher-order-function
        // argument that calls a preloaded function — mirroring the #9158/#9254
        // Surface sample's `surface(x, y, (x, y) -> sinc(norm([x, y])))`. The
        // lambda lifts to a `__lambda_*` that lands at the FRONT of the trailing
        // inline block and shifts the lifted Base closures behind it, so the
        // full-non-Base-region `closure_layout` gate (#9254) must DEACTIVATE
        // (fail-safe) — output still correct, just no splice.
        let src = "using LinearAlgebra\nprintln(map(v -> norm(v), [[1.0, 2.0, 3.0]]))\n";
        let program = crate::pipeline::parse_and_lower(src).expect("parse/lower");

        let run = |preload: Option<&SerializedPreloadCache>| -> Result<(String, usize), String> {
            let out = super::super::compile_core_program_internal(
                &program,
                &HashMap::new(),
                &HashMap::new(),
                CompilerCacheInput {
                    precompiled_base: Some(&base_compiled),
                    method_tables: Some(&base_mt),
                    closure_captures: Some(&base_cc),
                    inference_results: Some(base_inf.as_slice()),
                    preload_cache: preload.map(|c| &c.modules),
                    preload_closure_layout: preload.map(|c| c.closure_layout.as_slice()),
                    ..Default::default()
                },
            )
            .map_err(|e| format!("compile error: {e:?}"))?;
            let spliced = out.preload_spliced_count;
            let output = crate::test_runtime::run_compiled_program(out.compiled, 42)?;
            Ok((output, spliced))
        };

        let (off, _) = run(None).expect("baseline must succeed");
        let (on, on_spliced) = run(Some(&cache)).expect("main-lambda preload compile must succeed");
        assert!(
            off.contains("3.7416573867739413"),
            "sanity: norm([1,2,3]) == sqrt(14) should appear, got {off:?}"
        );
        assert_eq!(
            on, off,
            "main-lambda preload compile must still match the ordinary compile \
             (fail-safe correctness)"
        );
        assert_eq!(
            on_spliced, 0,
            "Issue #9254: the gate must DEACTIVATE for a `using ...; <main + \
             anonymous lambda arg>` program (the iOS Surface-sample shape) — the \
             lifted lambda shifts the trailing Base closures, so splicing frozen \
            indices would mis-dispatch; fall back to a normal compile instead"
        );
    }

    /// Issue #9646: a user top-level struct shifts concrete struct type_ids
    /// relative to the preload-cache generation compile. Spliced package bodies
    /// contain frozen `NewStruct(type_id, ...)` operands, so a matching function
    /// `closure_layout` is not enough: the gate must fail-safe when root source
    /// introduces any struct definition.
    #[test]
    fn preload_cache_deactivates_for_user_top_level_struct_9646() {
        use super::super::CompilerCacheInput;
        crate::compile::cache::warm_base_cache();
        let (base_compiled, base_mt, base_cc, base_inf) =
            crate::compile::cache::export_base_cache().expect("base cache warmed");

        let names = ["LinearAlgebra"];
        let bytes = generate_preload_cache_for(&names).expect("gen preload");
        let cache = deserialize_preload_cache(&bytes, &names).expect("deser preload");

        let src = r#"
using LinearAlgebra
struct MyS9646
    x::Float64
end
F = lu([4.0 3.0; 6.0 3.0])
println(typeof(F))
"#;
        let program = crate::pipeline::parse_and_lower(src).expect("parse/lower");

        let run = |preload: Option<&SerializedPreloadCache>| -> Result<(String, usize), String> {
            let out = super::super::compile_core_program_internal(
                &program,
                &HashMap::new(),
                &HashMap::new(),
                CompilerCacheInput {
                    precompiled_base: Some(&base_compiled),
                    method_tables: Some(&base_mt),
                    closure_captures: Some(&base_cc),
                    inference_results: Some(base_inf.as_slice()),
                    preload_cache: preload.map(|c| &c.modules),
                    preload_closure_layout: preload.map(|c| c.closure_layout.as_slice()),
                    ..Default::default()
                },
            )
            .map_err(|e| format!("compile error: {e:?}"))?;
            let spliced = out.preload_spliced_count;
            let output = crate::test_runtime::run_compiled_program(out.compiled, 42)?;
            Ok((output, spliced))
        };

        let (off, off_spliced) = run(None).expect("baseline must succeed");
        let (on, on_spliced) = run(Some(&cache)).expect("user-struct preload compile must succeed");
        assert_eq!(
            off_spliced, 0,
            "no preload cache should never report spliced bodies"
        );
        assert!(
            // Issue #11365: with `using LinearAlgebra` the exported LU leaf is
            // Main-visible, so display prints the bare upstream form `LU`.
            off.contains("LU"),
            "sanity: lu should construct a LinearAlgebra.LU value, got {off:?}"
        );
        assert_eq!(
            on, off,
            "preload-active compile must match the ordinary compile"
        );
        assert_eq!(
            on_spliced, 0,
            "Issue #9646: a user top-level struct shifts struct type_ids, so the \
             preload cache must deactivate even when the function closure_layout \
             prefix still matches"
        );
    }

    #[test]
    fn preload_cache_hit_produces_identical_output_to_a_normal_compile() {
        // Issue #9189 Stage 2 end-to-end proof: a real `using LinearAlgebra`
        // program, compiled once with the preload cache absent (the
        // CompilerCacheInput::default() path every existing caller still
        // takes) and once with it active, must produce byte-identical
        // output — proving the whole chain (build_method_tables's lookup,
        // compile_functions's skip, finalize's splice) is behaviorally
        // transparent, not just "doesn't crash".
        let src = "using LinearAlgebra\nprintln(dot([1, 2, 3], [4, 5, 6]))\n";

        let baseline =
            compile_and_run_with_cache_input(src, super::super::CompilerCacheInput::default());

        let names = ["LinearAlgebra"];
        let bytes = generate_preload_cache_for(&names).expect("generate preload cache");
        let cache = deserialize_preload_cache(&bytes, &names).expect("deserialize preload cache");
        let with_cache = compile_and_run_with_cache_input(
            src,
            super::super::CompilerCacheInput {
                preload_cache: Some(&cache.modules),
                ..Default::default()
            },
        );

        assert_eq!(baseline, "32\n", "sanity: dot([1,2,3],[4,5,6]) == 32");
        assert_eq!(
            with_cache, baseline,
            "preload-cache-active compile must produce identical output to the ordinary compile"
        );
    }

    #[test]
    fn unused_preload_package_leaves_a_program_byte_identical() {
        // Isolation proof: a program that does NOT `using` a preload-listed
        // package must be unaffected by the preload cache being active —
        // dormancy falls out of `all_functions` never containing that
        // package's functions in the first place (loader.rs only loads
        // modules a program actually `using`s), not a new gate this
        // mechanism had to add.
        let src = "println(1 + 1)\n";

        let baseline =
            compile_and_run_with_cache_input(src, super::super::CompilerCacheInput::default());

        let names = ["LinearAlgebra"];
        let bytes = generate_preload_cache_for(&names).expect("generate preload cache");
        let cache = deserialize_preload_cache(&bytes, &names).expect("deserialize preload cache");
        let with_cache = compile_and_run_with_cache_input(
            src,
            super::super::CompilerCacheInput {
                preload_cache: Some(&cache.modules),
                ..Default::default()
            },
        );

        assert_eq!(baseline, "2\n");
        assert_eq!(with_cache, baseline);
    }

    #[test]
    fn compile_with_cache_auto_activates_preload_cache_for_real() {
        // Issue #9189: `compile::compile_with_cache` — the actual production
        // entry point every caller (CLI, REPL, api.rs) goes through — must
        // produce correct output for both a preload-listed package and an
        // unrelated program with zero explicit wiring, whether or not
        // `get_or_init_preload_cache()` finds a non-empty `PRELOAD_PACKAGES`
        // to auto-activate. Currently exercises the "empty list, cache
        // inert" path (see `PRELOAD_PACKAGES`'s doc comment for why); once
        // that's repopulated, this test starts exercising real auto-activation
        // again with no changes needed.
        let program = crate::pipeline::parse_and_lower(
            "using LinearAlgebra\nprintln(dot([1, 2, 3], [4, 5, 6]))\n",
        )
        .expect("parse/lower");
        let compiled = super::super::compile_with_cache(&program).expect("compile_with_cache");
        let output = crate::test_runtime::run_compiled_program(compiled, 42).expect("vm run");
        assert_eq!(output, "32\n");

        let program = crate::pipeline::parse_and_lower("println(1 + 1)\n").expect("parse/lower");
        let compiled = super::super::compile_with_cache(&program).expect("compile_with_cache");
        let output = crate::test_runtime::run_compiled_program(compiled, 42).expect("vm run");
        assert_eq!(output, "2\n");
    }

    #[test]
    fn preload_cache_version_mismatch_rejected() {
        let names = ["LinearAlgebra"];
        // A stale cache may have an incompatible body shape. Its leading wire
        // version must be rejected before attempting to decode that body.
        let stale_header_result = bincode::serialize(&(PRELOAD_CACHE_VERSION - 1));
        assert!(stale_header_result.is_ok(), "stale header must encode");
        let stale_header_only = match stale_header_result {
            Ok(bytes) => bytes,
            Err(_) => return,
        };
        let stale_result = deserialize_preload_cache(&stale_header_only, &names);
        assert!(
            stale_result.is_err(),
            "stale header must be rejected before body decode"
        );
        let err = match stale_result {
            Err(error) => error,
            Ok(_) => return,
        };
        assert!(
            err.contains("version mismatch"),
            "expected pre-decode version rejection, got: {}",
            err
        );

        let bytes = generate_preload_cache_for(&names).expect("generate preload cache");
        let mut cache: SerializedPreloadCache =
            bincode::deserialize(&bytes).expect("decode for corruption");
        cache.version = PRELOAD_CACHE_VERSION + 1;
        let corrupted = bincode::serialize(&cache).expect("reserialize corrupted cache");

        let err = deserialize_preload_cache(&corrupted, &names)
            .expect_err("mismatched version must be rejected");
        assert!(
            err.contains("version mismatch"),
            "expected version-mismatch rejection, got: {}",
            err
        );
    }

    #[test]
    fn preload_cache_source_hash_mismatch_rejected() {
        // Validating against a *different* package list than the one that
        // produced the bytes must fail the source-hash gate — this is the
        // same mechanism that invalidates the cache when a preloaded
        // package's `.jl` source is edited.
        let bytes = generate_preload_cache_for(&["LinearAlgebra"]).expect("generate");
        let err = deserialize_preload_cache(&bytes, &["Random"])
            .expect_err("mismatched package list must be rejected");
        assert!(
            err.contains("source hash mismatch"),
            "expected source-hash rejection, got: {}",
            err
        );
    }
}

//! Type comparison and subtype checking.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::inference_core::{CoreSubtypeEngine, CoreType, CoreTypeVar};
use crate::rng::RngLike;
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Check if a struct is an instance of a user-defined abstract type.
    /// Uses the pre-computed `type_ancestors` map (Issue #3356) for O(1) lookup.
    pub(in crate::vm) fn check_isa_with_abstract_resolved(
        &self,
        struct_name_opt: &Option<String>,
        target_type: &str,
    ) -> bool {
        let struct_name = match struct_name_opt {
            Some(name) => name,
            None => return false,
        };

        if let Some(ancestor_list) = self.type_ancestors.get(struct_name.as_str()) {
            return ancestor_list.iter().any(|a| a == target_type);
        }

        false
    }

    /// Check if left_type <: right_type (left is a subtype of right).
    ///
    /// This is the **runtime** (string-based) counterpart of
    /// `JuliaType::is_subtype_of()` in `types/julia_type.rs` (compile-time,
    /// enum-based). Both delegate the built-in type hierarchy to the shared
    /// `CoreSubtypeEngine`, so there is a single source of truth — new types
    /// belong in `inference_core`, not here (Issue #2494 / #5921). The
    /// agreement is locked by `test_check_subtype_parity_with_julia_type`.
    pub(in crate::vm) fn check_subtype(&self, left_type: &str, right_type: &str) -> bool {
        // Exact match
        if left_type == right_type {
            return true;
        }

        // Any is the top type - everything is a subtype of Any
        if right_type == "Any" {
            return true;
        }

        // Union{} (Bottom) is the bottom type - it is a subtype of everything.
        // NOTE: `Nothing` (the type of `nothing`) is a normal concrete singleton
        // DataType, NOT the bottom type. It is only `<:` to its actual supertypes
        // (Any, Nothing itself, and Unions/abstract types that contain it).
        // Conflating it with `Union{}` made `Nothing <: T` true for every T
        // (Issue #5257). The CoreType structured path below handles `Nothing`
        // correctly, so just let it fall through.
        if left_type == "Union{}" {
            return true;
        }

        // Native RNG values (Value::Rng) report their concrete type as one of
        // "Xoshiro" / "StableRNG" / "MersenneTwister" / "TaskLocalRNG". These
        // are not declared Julia
        // structs (they are VM-native), so the struct-hierarchy engine cannot
        // know they are subtypes of the abstract `AbstractRNG`. Teach the
        // runtime string path that relation directly so a `rng::AbstractRNG`
        // parameter (or `x isa AbstractRNG`) matches a Value::Rng argument
        // (Issues #7230, #7231).
        if matches!(right_type, "AbstractRNG" | "Random.AbstractRNG")
            && matches!(
                left_type,
                "Xoshiro"
                    | "StableRNG"
                    | "MersenneTwister"
                    | "TaskLocalRNG"
                    | "Random.Xoshiro"
                    | "StableRNGs.StableRNG"
                    | "Random.MersenneTwister"
                    | "Random.TaskLocalRNG"
            )
        {
            return true;
        }

        // A `typeof(v)` annotation names a *binding* `v`, while a closure
        // value reports its mangled definition-site singleton
        // `typeof(parent#anon)` (Issue #9106). Resolve the annotation's inner
        // name through the current global binding so the two spellings of the
        // same singleton type compare equal.
        if self.typeof_singleton_names_match(left_type, right_type) {
            return true;
        }

        // The shared `CoreSubtypeEngine` (over the VM's `struct_hierarchy`) is the
        // single source of truth for every `<:` decision (Issue #5915). It covers
        // the built-in numeric/range/container lattice, unions, tuple covariance,
        // `Type{T}`, parametric struct invariance, user-declared nominal chains
        // (including user-name `<:` built-in abstract, e.g. `struct Money <: Real`),
        // and `where`-form existentials/foralls — all the cases the old
        // hand-rolled nominal-chain / `type_ancestors` / Union-decomposition /
        // tuple-walk fallbacks used to recover separately. `from_julia_name`
        // re-parses the rendered `where` surface syntax into a `UnionAll`, and the
        // module-prefix-insensitive bare-family match (`Diagonal{Float64} <:
        // Diagonal`) is the engine's `base_type_name` comparison. The
        // authoritative gate (`core_type_subtype_is_authoritative`) only suppresses
        // the engine's answer when neither side is a known type, in which case the
        // relation is `false` anyway — so the engine result IS the answer.
        self.engine_subtype(left_type, right_type)
    }

    /// Route a `<:` query through the shared `CoreSubtypeEngine`, supplying the
    /// VM's declared `struct_hierarchy` so user nominal chains resolve. This is
    /// the single subtype authority for the runtime string path (Issue #5915).
    fn engine_subtype(&self, left_type: &str, right_type: &str) -> bool {
        let l = self.classify_subtype_left_operand(left_type);
        let r = self.classify_subtype_operand(right_type);
        CoreSubtypeEngine::with_hierarchy(&self.struct_hierarchy).is_subtype(&l, &r)
    }

    /// Classify a bare type-name operand for the subtype engine.
    ///
    /// `CoreType::from_julia_name` is context-free and treats short
    /// uppercase[+digits] names (e.g. `A2`, `P`, `S3`) as *type variables*. When
    /// such a name is actually a *registered* user struct/abstract type, it must
    /// be a nominal `Named` type instead — otherwise `RM <: A2` reduces to
    /// `Named <: free-typevar` (spuriously `true`) and `A2 <: Rot` to
    /// `free-typevar <: Named` (spuriously `false`) (Issue #8092). The runtime
    /// has the program's `struct_hierarchy`, so it can tell a declared type from
    /// a genuine typevar where `from_julia_name` cannot.
    pub(in crate::vm) fn classify_subtype_operand(&self, name: &str) -> CoreType {
        let ct = CoreType::from_julia_name(name);
        if matches!(ct, CoreType::TypeVar(_)) && self.struct_hierarchy.contains_name(name) {
            return CoreType::Named(name.to_string());
        }
        ct
    }

    /// Classify the left-hand side of `<:`. Bare registered parametric families
    /// (for example the canonicalized value of `MyVec{T} where T`) need to keep
    /// their declared type-parameter slots. Otherwise the hierarchy walk sees
    /// only a parameter-free nominal family and cannot substitute `T` into the
    /// declared parent template `Wrapper{T}` when checking against
    /// `Wrapper{S} where S` (Issue #9563).
    ///
    /// This projection is intentionally left-only: a bare parametric family on
    /// the right remains the broad family supertype (`x isa BatchIntegrand`),
    /// not an invariant fully-declared schema requiring every parameter slot.
    fn classify_subtype_left_operand(&self, name: &str) -> CoreType {
        if let Some(parametric_family) = self.declared_parametric_family_core(name) {
            return parametric_family;
        }
        self.classify_subtype_operand(name)
    }

    fn declared_parametric_family_core(&self, name: &str) -> Option<CoreType> {
        if name.contains('{') || name.contains(" where ") {
            return None;
        }
        let entry = self.struct_hierarchy.entry(name)?;
        let type_params = entry.type_params();
        if type_params.is_empty() {
            return None;
        }
        Some(CoreType::Struct {
            name: name.to_string(),
            params: type_params
                .iter()
                .map(|param| CoreType::TypeVar(CoreTypeVar::unscoped(param.clone())))
                .collect(),
        })
    }

    /// Structured counterpart of [`Self::check_subtype`] for operands that are
    /// already [`CoreType`]s (Issue #6502 slice 2): same fast paths (equality,
    /// `Any` top, `Union{}` bottom), same engine, same `struct_hierarchy` —
    /// without re-parsing rendered type names per query.
    pub(in crate::vm) fn check_subtype_core(&self, left: &CoreType, right: &CoreType) -> bool {
        if left == right || matches!(right, CoreType::Any) || matches!(left, CoreType::Bottom) {
            return true;
        }
        // Resolve `typeof(binding)` singleton spellings against the current
        // global binding, mirroring the string path above (Issue #9106).
        if let (
            CoreType::Struct {
                name: left_name,
                params: left_params,
            },
            CoreType::Struct {
                name: right_name,
                params: right_params,
            },
        ) = (left, right)
        {
            if left_params.is_empty()
                && right_params.is_empty()
                && self.typeof_singleton_names_match(left_name, right_name)
            {
                return true;
            }
        }
        CoreSubtypeEngine::with_hierarchy(&self.struct_hierarchy).is_subtype(left, right)
    }

    /// Do two `typeof(...)` singleton type names denote the same callable
    /// singleton once bindings are resolved? (Issue #9106)
    ///
    /// A `::typeof(f)` method annotation carries the *source-level binding
    /// name* `f`, while a closure value's runtime type embeds the mangled
    /// definition-site function name (`typeof(make_fn#anon)`), and a
    /// function-valued alias `g = f` reports `typeof(f)`. Resolve each
    /// side's inner name through the current global bindings to the
    /// canonical callable name before comparing. Returns `false` for
    /// non-`typeof(...)` operands (callers fall through to the engine).
    pub(in crate::vm) fn typeof_singleton_names_match(&self, left: &str, right: &str) -> bool {
        fn strip_typeof(name: &str) -> Option<&str> {
            name.strip_prefix("typeof(")?.strip_suffix(')')
        }
        let (Some(left_inner), Some(right_inner)) = (strip_typeof(left), strip_typeof(right))
        else {
            return false;
        };
        let left_canonical = self.canonical_callable_name(left_inner);
        let right_canonical = self.canonical_callable_name(right_inner);
        left_canonical == right_canonical
    }

    /// Resolve a callable binding name to its canonical function name: a
    /// closure-valued global resolves to the closure's mangled definition-site
    /// name, a function-valued global to the function's own name. Names
    /// without a callable global binding (including already-mangled closure
    /// names) resolve to themselves.
    fn canonical_callable_name<'n>(&self, name: &'n str) -> std::borrow::Cow<'n, str> {
        match self.get_global(name) {
            Some(crate::vm::value::Value::Closure(cv)) => {
                std::borrow::Cow::Owned(cv.singleton_identity().encoded_name())
            }
            Some(crate::vm::value::Value::Function(f)) => {
                std::borrow::Cow::Owned(f.singleton_identity().encoded_name())
            }
            _ => std::borrow::Cow::Borrowed(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rng::StableRng;
    use crate::types::StructHierarchy;
    use crate::vm::Vm;

    /// Helper: create a minimal VM for testing check_subtype.
    fn make_vm() -> Vm<StableRng> {
        Vm::new(vec![], StableRng::new(0))
    }

    /// Test-only bridge that pins the shared `CoreSubtypeEngine`'s answer for a
    /// rendered type-name pair. After Issue #5915 wave 3 the engine is the single
    /// `<:` authority (the runtime `check_subtype` delegates straight to it), so
    /// these existing structured-family assertions now exercise the engine
    /// directly. The `Option<bool>` shape is preserved so the original
    /// `assert_eq!(..., Some(true/false))` expectations stay literal: the engine
    /// always produces a definite answer over the supplied hierarchy.
    fn check_core_structured_subtype_with_hierarchy(
        hierarchy: &StructHierarchy,
        left_type: &str,
        right_type: &str,
    ) -> Option<bool> {
        use crate::inference_core::{CoreSubtypeEngine, CoreType};
        Some(CoreSubtypeEngine::with_hierarchy(hierarchy).is_subtype(
            &CoreType::from_julia_name(left_type),
            &CoreType::from_julia_name(right_type),
        ))
    }

    fn check_core_structured_subtype(left_type: &str, right_type: &str) -> Option<bool> {
        check_core_structured_subtype_with_hierarchy(&StructHierarchy::new(), left_type, right_type)
    }

    /// Verify concrete signed integer types are subtypes of Signed, Integer, Real, Number.
    #[test]
    fn test_check_subtype_signed_integers() {
        let vm = make_vm();
        for ty in &["Int8", "Int16", "Int32", "Int64", "Int128"] {
            assert!(vm.check_subtype(ty, "Signed"), "{ty} should be <: Signed");
            assert!(vm.check_subtype(ty, "Integer"), "{ty} should be <: Integer");
            assert!(vm.check_subtype(ty, "Real"), "{ty} should be <: Real");
            assert!(vm.check_subtype(ty, "Number"), "{ty} should be <: Number");
            // Signed integers are NOT Unsigned or AbstractFloat
            assert!(
                !vm.check_subtype(ty, "Unsigned"),
                "{ty} should NOT be <: Unsigned"
            );
            assert!(
                !vm.check_subtype(ty, "AbstractFloat"),
                "{ty} should NOT be <: AbstractFloat"
            );
        }
    }

    /// Verify BigInt subtype limitations.
    /// In Julia, BigInt <: Signed <: Integer <: Real <: Number, but adding
    /// any BigInt subtype relationship causes dispatch regressions in
    /// convert/promote paths. BigInt relies on exact-match dispatch.
    #[test]
    fn test_check_subtype_bigint() {
        let vm = make_vm();
        // BigInt <: Signed <: Integer <: Real <: Number (Issue #2492)
        assert!(
            vm.check_subtype("BigInt", "Signed"),
            "BigInt should be <: Signed"
        );
        assert!(
            vm.check_subtype("BigInt", "Integer"),
            "BigInt should be <: Integer"
        );
        assert!(
            vm.check_subtype("BigInt", "Real"),
            "BigInt should be <: Real"
        );
        assert!(
            vm.check_subtype("BigInt", "Number"),
            "BigInt should be <: Number"
        );
        // Negative cases
        assert!(
            !vm.check_subtype("BigInt", "Unsigned"),
            "BigInt should NOT be <: Unsigned"
        );
        assert!(
            !vm.check_subtype("BigInt", "AbstractFloat"),
            "BigInt should NOT be <: AbstractFloat"
        );
        // BigInt <: BigInt (reflexive)
        assert!(
            vm.check_subtype("BigInt", "BigInt"),
            "BigInt should be <: BigInt"
        );
        // BigInt <: Any
        assert!(vm.check_subtype("BigInt", "Any"), "BigInt should be <: Any");
    }

    #[test]
    fn test_check_subtype_array_unionall_vector_alias() {
        let vm = make_vm();
        assert!(vm.check_subtype("Vector{Int64}", "Array{T} where T"));
        assert!(vm.check_subtype("Matrix{Float64}", "Array{T} where T"));
        assert!(vm.check_subtype("Array{Bool, 3}", "Array{T} where T"));
        assert!(vm.check_subtype("Vector{Int64}", "Array{<:Real}"));
        assert!(!vm.check_subtype("Vector{String}", "Array{<:Real}"));
        assert!(!vm.check_subtype("Vector{Int64}", "Array{Real}"));
        assert!(vm.check_subtype("Array{Float64, 1}", "Vector{Float64}"));
        assert!(!vm.check_subtype("Array{Float64, 2}", "Vector{Float64}"));

        assert_eq!(
            check_core_structured_subtype("Vector{Int64}", "Array{T} where T"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Matrix{Float64}", "Array{T} where T"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Array{Bool, 3}", "Array{T} where T"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Vector{Int64}", "Array{<:Real}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Vector{String}", "Array{<:Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Vector{Int64}", "Array{Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Array{Float64, 1}", "Vector{Float64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Array{Float64, 2}", "Vector{Float64}"),
            Some(false)
        );
    }

    /// Verify all concrete unsigned integer types are subtypes of Unsigned, Integer, Real, Number.
    #[test]
    fn test_check_subtype_unsigned_integers() {
        let vm = make_vm();
        for ty in &["UInt8", "UInt16", "UInt32", "UInt64", "UInt128"] {
            assert!(
                vm.check_subtype(ty, "Unsigned"),
                "{ty} should be <: Unsigned"
            );
            assert!(vm.check_subtype(ty, "Integer"), "{ty} should be <: Integer");
            assert!(vm.check_subtype(ty, "Real"), "{ty} should be <: Real");
            assert!(vm.check_subtype(ty, "Number"), "{ty} should be <: Number");
            // Unsigned integers are NOT Signed or AbstractFloat
            assert!(
                !vm.check_subtype(ty, "Signed"),
                "{ty} should NOT be <: Signed"
            );
            assert!(
                !vm.check_subtype(ty, "AbstractFloat"),
                "{ty} should NOT be <: AbstractFloat"
            );
        }
    }

    /// Verify Bool <: Integer <: Real <: Number, but NOT Signed or Unsigned.
    #[test]
    fn test_check_subtype_bool() {
        let vm = make_vm();
        assert!(
            vm.check_subtype("Bool", "Integer"),
            "Bool should be <: Integer"
        );
        assert!(vm.check_subtype("Bool", "Real"), "Bool should be <: Real");
        assert!(
            vm.check_subtype("Bool", "Number"),
            "Bool should be <: Number"
        );
        // In Julia, Bool is NOT <: Signed and NOT <: Unsigned
        assert!(
            !vm.check_subtype("Bool", "Signed"),
            "Bool should NOT be <: Signed"
        );
        assert!(
            !vm.check_subtype("Bool", "Unsigned"),
            "Bool should NOT be <: Unsigned"
        );
        assert!(
            !vm.check_subtype("Bool", "AbstractFloat"),
            "Bool should NOT be <: AbstractFloat"
        );
    }

    /// Verify all concrete float types are subtypes of AbstractFloat, Real, Number.
    #[test]
    fn test_check_subtype_floats() {
        let vm = make_vm();
        for ty in &["Float16", "Float32", "Float64", "BigFloat"] {
            assert!(
                vm.check_subtype(ty, "AbstractFloat"),
                "{ty} should be <: AbstractFloat"
            );
            assert!(vm.check_subtype(ty, "Real"), "{ty} should be <: Real");
            assert!(vm.check_subtype(ty, "Number"), "{ty} should be <: Number");
            // Floats are NOT Integer, Signed, or Unsigned
            assert!(
                !vm.check_subtype(ty, "Integer"),
                "{ty} should NOT be <: Integer"
            );
            assert!(
                !vm.check_subtype(ty, "Signed"),
                "{ty} should NOT be <: Signed"
            );
            assert!(
                !vm.check_subtype(ty, "Unsigned"),
                "{ty} should NOT be <: Unsigned"
            );
        }
    }

    /// Verify abstract type chain relationships.
    #[test]
    fn test_check_subtype_abstract_chain() {
        let vm = make_vm();
        // Signed <: Integer <: Real <: Number
        assert!(vm.check_subtype("Signed", "Integer"));
        assert!(vm.check_subtype("Signed", "Real"));
        assert!(vm.check_subtype("Signed", "Number"));
        // Unsigned <: Integer <: Real <: Number
        assert!(vm.check_subtype("Unsigned", "Integer"));
        assert!(vm.check_subtype("Unsigned", "Real"));
        assert!(vm.check_subtype("Unsigned", "Number"));
        // Integer <: Real <: Number
        assert!(vm.check_subtype("Integer", "Real"));
        assert!(vm.check_subtype("Integer", "Number"));
        // AbstractFloat <: Real <: Number
        assert!(vm.check_subtype("AbstractFloat", "Real"));
        assert!(vm.check_subtype("AbstractFloat", "Number"));
        // Real <: Number
        assert!(vm.check_subtype("Real", "Number"));
    }

    /// Verify reflexive property: T <: T for all types.
    #[test]
    fn test_check_subtype_reflexive() {
        let vm = make_vm();
        for ty in &[
            "Int8",
            "Int16",
            "Int32",
            "Int64",
            "Int128",
            "BigInt",
            "UInt8",
            "UInt16",
            "UInt32",
            "UInt64",
            "UInt128",
            "Bool",
            "Float16",
            "Float32",
            "Float64",
            "BigFloat",
            "Integer",
            "Signed",
            "Unsigned",
            "Real",
            "Number",
            "AbstractFloat",
            "String",
            "Any",
        ] {
            assert!(vm.check_subtype(ty, ty), "{ty} should be <: {ty}");
        }
    }

    /// Bare `Diagonal` method params match module-qualified runtime instances.
    #[test]
    fn test_check_subtype_bare_diagonal_family() {
        let vm = make_vm();
        assert!(vm.check_subtype("LinearAlgebra.Diagonal{Float64}", "Diagonal"));
        assert!(vm.check_subtype("Diagonal{Float64}", "Diagonal"));
    }

    /// Verify everything is <: Any, and that `Nothing` (a concrete singleton
    /// type) is NOT a subtype of unrelated concrete/abstract types. Only
    /// `Union{}` (Bottom) is `<:` everything (Issue #5257).
    #[test]
    fn test_check_subtype_any_and_nothing() {
        let vm = make_vm();
        for ty in &["Int64", "Float64", "Bool", "String", "Integer", "Number"] {
            assert!(vm.check_subtype(ty, "Any"), "{ty} should be <: Any");
            // Issue #5257: Nothing is NOT bottom; it is only <: its real supertypes.
            assert!(
                !vm.check_subtype("Nothing", ty),
                "Nothing should NOT be <: {ty}"
            );
            // Union{} (Bottom) IS a subtype of everything.
            assert!(
                vm.check_subtype("Union{}", ty),
                "Union{{}} should be <: {ty}"
            );
        }
        // Nothing's actual supertype relationships (upstream Julia 1.12):
        assert!(vm.check_subtype("Nothing", "Any"), "Nothing <: Any");
        assert!(vm.check_subtype("Nothing", "Nothing"), "Nothing <: Nothing");
        assert!(
            vm.check_subtype("Nothing", "Union{Nothing, Int64}"),
            "Nothing <: Union{{Nothing, Int64}}"
        );
        assert!(
            !vm.check_subtype("Nothing", "Int64"),
            "Nothing should NOT be <: Int64"
        );
        // Missing is likewise a concrete singleton, not bottom.
        assert!(
            !vm.check_subtype("Missing", "Int64"),
            "Missing should NOT be <: Int64"
        );
        // Sanity: unrelated concrete pairs.
        assert!(
            !vm.check_subtype("Int64", "Float64"),
            "Int64 should NOT be <: Float64"
        );
        assert!(
            !vm.check_subtype("Int64", "Nothing"),
            "Int64 should NOT be <: Nothing"
        );
    }

    /// Verify Complex and Rational subtype relationships.
    #[test]
    fn test_check_subtype_complex_rational() {
        // Issue #5157/#5920: the Complex/Rational hierarchy is derived from
        // the VM's shared StructHierarchy.
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Complex", Some("Number".to_string()), vec!["T".to_string()]);
        hierarchy.insert("Rational", Some("Real".to_string()), vec!["T".to_string()]);
        let mut vm = make_vm();
        vm.struct_hierarchy = hierarchy.clone();
        // Complex <: Number but NOT <: Real
        assert!(vm.check_subtype("Complex", "Number"));
        assert!(vm.check_subtype("Complex{Float64}", "Number"));
        assert!(vm.check_subtype("Complex{Int64}", "Number"));
        assert!(!vm.check_subtype("Complex", "Real"));
        assert!(!vm.check_subtype("Complex{Float64}", "Real"));
        // Rational <: Real <: Number
        assert!(vm.check_subtype("Rational{Int64}", "Real"));
        assert!(vm.check_subtype("Rational{Int64}", "Number"));
        assert!(vm.check_subtype("Rational{Int32}", "Real"));
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Complex{Float64}", "Number"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Complex{Float64}", "Real"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Rational{Int64}", "Real"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Rational{Int64}", "Integer"),
            Some(false)
        );
    }

    #[test]
    fn test_check_subtype_parametric_parent_hierarchy_pairs() {
        // Issues #5615/#5882: `Pairs{K,V,I,A}` should inherit the declared
        // `AbstractDict{K,V}` parent with K/V substituted from the child.
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "Pairs",
            Some("AbstractDict{K,V}".to_string()),
            vec![
                "K".to_string(),
                "V".to_string(),
                "I".to_string(),
                "A".to_string(),
            ],
        );

        let mut vm = make_vm();
        vm.struct_hierarchy = hierarchy.clone();

        assert!(vm.check_subtype("Pairs{Symbol,Int64,Any,Any}", "AbstractDict"));
        assert!(vm.check_subtype("Pairs{Symbol,Int64,Any,Any}", "AbstractDict{Symbol,Int64}"));
        assert!(!vm.check_subtype("Pairs{Symbol,Int64,Any,Any}", "AbstractDict{Symbol,Any}"));
        assert!(!vm.check_subtype("Pairs{Symbol,Int64,Any,Any}", "AbstractDict{Any,Int64}"));
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(
                &hierarchy,
                "Pairs{Symbol,Int64,Any,Any}",
                "AbstractDict{Symbol,Int64}"
            ),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(
                &hierarchy,
                "Pairs{Symbol,Int64,Any,Any}",
                "AbstractDict{Symbol,Any}"
            ),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(
                &hierarchy,
                "Tuple{Pairs{Symbol,Int64,Any,Any}}",
                "Tuple{AbstractDict{Symbol,Any}}"
            ),
            Some(false)
        );
    }

    #[test]
    fn runtime_where_right_uses_vm_struct_hierarchy_issue_5920() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "MyVec",
            Some("Wrapper{T}".to_string()),
            vec!["T".to_string()],
        );

        let mut vm = make_vm();
        vm.struct_hierarchy = hierarchy;

        assert!(vm.check_subtype("MyVec{Int64}", "Wrapper{S} where S"));
        assert!(!vm.check_subtype("MyVec{Int64}", "Wrapper{Real}"));
    }

    #[test]
    fn runtime_bare_parametric_family_uses_declared_params_issue_9563() {
        let mut vm = make_vm();
        vm.struct_hierarchy
            .insert("Wrapper", Some("Any".to_string()), vec!["S".to_string()]);
        vm.struct_hierarchy.insert(
            "MyVec",
            Some("Wrapper{T}".to_string()),
            vec!["T".to_string()],
        );

        assert!(vm.check_subtype("MyVec", "Wrapper{S} where S"));
    }

    #[test]
    fn runtime_bare_parametric_family_right_stays_family_supertype_issue_9563() {
        let mut vm = make_vm();
        vm.struct_hierarchy.insert(
            "QuadGK.BatchIntegrand",
            Some("Any".to_string()),
            vec![
                "Y".to_string(),
                "X".to_string(),
                "Ty".to_string(),
                "Tx".to_string(),
                "F".to_string(),
            ],
        );

        assert!(vm.check_subtype("QuadGK.BatchIntegrand{Float64, Float64}", "BatchIntegrand"));
    }

    /// Issue #5915 wave 3: the `CoreSubtypeEngine` is now the single `<:`
    /// authority — the runtime `check_subtype` delegates straight to it with no
    /// legacy nominal-chain / `type_ancestors` fallback. This pins the engine
    /// coverage that the retired fallbacks used to recover separately, all
    /// verified against upstream `julia` 1.12:
    ///   struct Money <: Real            → Money <: Real / Number      true
    ///   abstract type Currency <: Number→ Currency <: Number          true
    ///   struct MyVec{T} <: Wrapper{T}   → MyVec{Int} <: Wrapper{S}@S  true
    ///                                     MyVec{Int} <: Wrapper{Real}  false
    #[test]
    fn test_check_subtype_engine_is_sole_authority_issue_5915() {
        let mut vm = make_vm();
        // Non-parametric user struct / abstract whose declared parent is a
        // BUILT-IN abstract: the `(Named, Abstract)` engine arm (added in this
        // wave) walks the registered chain into the numeric lattice.
        vm.struct_hierarchy
            .insert("Money", Some("Real".to_string()), Vec::new());
        vm.struct_hierarchy
            .insert("Currency", Some("Number".to_string()), Vec::new());
        // A parametric user struct declaring a parametric abstract parent: the
        // existential-right match (`MyVec{Int} <: Wrapper{S} where S`) walks the
        // substituted parent `Wrapper{Int}` and binds `S`.
        vm.struct_hierarchy.insert(
            "MyVec",
            Some("Wrapper{T}".to_string()),
            vec!["T".to_string()],
        );
        vm.struct_hierarchy
            .insert("Wrapper", Some("Any".to_string()), vec!["T".to_string()]);

        assert!(vm.check_subtype("Money", "Real"));
        assert!(vm.check_subtype("Money", "Number"));
        assert!(!vm.check_subtype("Money", "AbstractFloat"));
        assert!(vm.check_subtype("Currency", "Number"));
        assert!(!vm.check_subtype("Currency", "Real"));
        // Through containers / tuples (covariant element walk via the engine).
        assert!(vm.check_subtype("Tuple{Money}", "Tuple{Number}"));
        assert!(vm.check_subtype("Tuple{Money, Currency}", "Tuple{Real, Number}"));
        // Existential parametric parent.
        assert!(vm.check_subtype("MyVec{Int64}", "Wrapper{S} where S"));
        assert!(!vm.check_subtype("MyVec{Int64}", "Wrapper{Real}"));
        // Unknown names stay authoritatively false (no fallback resurrects them).
        assert!(!vm.check_subtype("Mystery", "Real"));
        assert!(!vm.check_subtype("Money", "Mystery"));
    }

    #[test]
    fn test_check_subtype_shared_core_structured_families() {
        let vm = make_vm();

        assert!(vm.check_subtype("Vector{Int64}", "AbstractVector"));
        assert!(vm.check_subtype("Matrix{Float64}", "AbstractMatrix"));
        assert!(vm.check_subtype("Array{Float64, 1}", "AbstractVector"));
        assert!(vm.check_subtype("Array{Float64, 2}", "AbstractMatrix"));
        assert!(!vm.check_subtype("Matrix{Float64}", "AbstractVector"));
        assert!(!vm.check_subtype("Vector{Int64}", "AbstractMatrix"));
        assert!(!vm.check_subtype("Array{Float64}", "AbstractVector"));
        assert!(!vm.check_subtype("Array{Float64}", "AbstractMatrix"));
        assert!(!vm.check_subtype("Vector{Int64}", "Vector{Real}"));
        assert!(!vm.check_subtype("Vector{Int64}", "Array{Real}"));
        assert!(vm.check_subtype(
            "SubArray{Int64,1,Array{Int64},Tuple{UnitRange{Int64}},true}",
            "AbstractArray{Int64,1}"
        ));
        assert!(vm.check_subtype("Dict{String, Int64}", "AbstractDict"));
        assert!(vm.check_subtype("Dict{String, Int64}", "AbstractDict{String, T} where T"));
        assert!(!vm.check_subtype("Dict{String, Int64}", "AbstractDict{Symbol, T} where T"));
        assert!(!vm.check_subtype("Dict{String, Int64}", "Dict{String, Any}"));
        assert!(vm.check_subtype("Set{Int64}", "Set"));
        assert!(vm.check_subtype("Set{Int64}", "AbstractSet"));
        assert!(vm.check_subtype("Set{Int64}", "AbstractSet{T} where T"));
        assert!(!vm.check_subtype("Set{String}", "AbstractSet{T<:Real}"));
        assert!(!vm.check_subtype("Set{Int64}", "Set{Real}"));
        assert!(vm.check_subtype("UnitRange", "AbstractUnitRange"));
        assert!(vm.check_subtype("OneTo", "AbstractUnitRange"));
        assert!(vm.check_subtype("UnitRange", "OrdinalRange"));
        assert!(vm.check_subtype("StepRange", "OrdinalRange"));
        assert!(vm.check_subtype("AbstractUnitRange", "OrdinalRange"));
        assert!(vm.check_subtype("UnitRange", "AbstractRange"));
        assert!(vm.check_subtype("StepRange", "AbstractRange"));
        assert!(vm.check_subtype("StepRangeLen", "AbstractRange"));
        assert!(vm.check_subtype("LinRange", "AbstractRange"));
        assert!(!vm.check_subtype("LogRange", "AbstractRange"));
        assert!(vm.check_subtype("UnitRange{Int64}", "AbstractUnitRange"));
        assert!(vm.check_subtype("StepRangeLen{Float64}", "AbstractRange"));
        assert!(vm.check_subtype(
            "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}",
            "AbstractRange"
        ));
        assert!(vm.check_subtype("AbstractRange", "AbstractVector"));
        assert!(vm.check_subtype("AbstractRange", "AbstractArray"));
        assert!(vm.check_subtype("OrdinalRange", "AbstractRange"));
        assert!(vm.check_subtype("OrdinalRange", "AbstractVector"));
        assert!(vm.check_subtype("UnitRange{Int64}", "AbstractVector{Int64}"));
        assert!(vm.check_subtype("UnitRange{Int64}", "AbstractArray{Int64,1}"));
        assert!(!vm.check_subtype("UnitRange{Int64}", "AbstractVector{Integer}"));
        assert!(!vm.check_subtype("UnitRange{Int64}", "Array{Int64,1}"));
        assert!(vm.check_subtype("LogRange{Float64}", "AbstractVector{Float64}"));
        assert!(vm.check_subtype("LogRange{Float64}", "AbstractArray{Float64,1}"));
        assert!(!vm.check_subtype("LogRange{Float64}", "AbstractRange"));
        assert!(vm.check_subtype("IOBuffer", "IO"));
        assert!(!vm.check_subtype("IOBuffer", "Number"));
    }

    #[test]
    fn test_check_subtype_core_gate_handles_authoritative_runtime_pairs() {
        let mut vm = make_vm();
        // The engine resolves user nominal chains through `struct_hierarchy`
        // (which production always populates alongside `type_ancestors` from the
        // same program — Issue #5915 wave 3), so register the declared parents
        // there. `Vehicle` is registered so `Dog <: Vehicle` is authoritatively
        // false rather than unknown.
        vm.struct_hierarchy
            .insert("Animal", Some("Any".to_string()), Vec::new());
        vm.struct_hierarchy
            .insert("Dog", Some("Animal".to_string()), Vec::new());
        vm.struct_hierarchy
            .insert("Vehicle", Some("Any".to_string()), Vec::new());
        vm.type_ancestors
            .insert("Dog".to_string(), vec!["Animal".to_string()]);

        assert_eq!(
            check_core_structured_subtype("Vector{Int64}", "Vector{Real}"),
            Some(false)
        );
        let subarray = "SubArray{Int64, 1, Vector{Int64}, Tuple{UnitRange{Int64}}, true}";
        let reshaped =
            "ReshapedArray{Int64, 2, SubArray{Int64, 1, Vector{Int64}, Tuple{UnitRange{Int64}}, true}, Tuple}";
        assert_eq!(
            check_core_structured_subtype(subarray, "AbstractVector{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype(subarray, "DenseVector{Int64}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype(reshaped, "AbstractMatrix{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype(reshaped, "DenseMatrix{Int64}"),
            Some(false)
        );
        // Without a struct hierarchy the engine has no parent data for these
        // user names, so it is authoritatively false (the old `None`
        // "not-authoritative" gate is retired — Issue #5915 wave 3); the
        // hierarchy-aware positives are asserted further below.
        assert_eq!(
            check_core_structured_subtype("Tuple{Dog}", "Tuple{Animal}"),
            Some(false)
        );
        assert_eq!(check_core_structured_subtype("Dog", "Animal"), Some(false));
        assert_eq!(
            check_core_structured_subtype("Box{Int64}", "Animal"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{Int64, String}", "Tuple{Real, Any}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{String}", "Tuple{Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{Int64, Float64}", "Tuple{Integer, Real}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{Int64, String}", "Tuple{Integer, Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{Int64, String}", "Tuple"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{Vararg{Int64}}", "Tuple"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Signed", "Number"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Signed", "AbstractFloat"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Int64", "AbstractFloat"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("AbstractVector", "AbstractMatrix"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Array{Float64, 1}", "AbstractVector"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Array{Float64}", "AbstractVector"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Matrix{Float64}", "AbstractVector"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Vector{Int64}", "AbstractArray"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Matrix{Float64}", "AbstractArray"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Array{Float64, 1}", "AbstractVector"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Array{Float64, 2}", "AbstractMatrix"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Vector{Int64}", "AbstractVector{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Vector{Float64}", "AbstractVector{Int64}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Matrix{Int64}", "AbstractMatrix{Float64}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Array{Float64, 2}", "AbstractVector{Float64}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Vector{Int64}", "AbstractVector{T} where T"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Dict{String, Int64}", "AbstractDict{String, Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Dict{String, Int64}", "AbstractDict{String, T} where T"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Dict{String, Int64}", "AbstractDict{Symbol, T} where T"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Dict{String, Int64}", "AbstractDict{String, Any}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Set{Int64}", "AbstractSet{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Set{Int64}", "AbstractSet{T} where T"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Set{String}", "AbstractSet{T<:Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Set{String}", "AbstractSet{T} where T<:Real"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Set{Int64}", "AbstractSet{Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange", "AbstractUnitRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("OneTo", "AbstractUnitRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange", "OrdinalRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("StepRange", "OrdinalRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("AbstractUnitRange", "OrdinalRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange", "AbstractRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("StepRange", "AbstractRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("StepRangeLen", "AbstractRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("LinRange", "AbstractRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("LogRange", "AbstractRange"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange{Int64}", "AbstractUnitRange{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange{Int64}", "AbstractUnitRange{Integer}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange{Int64}", "AbstractRange{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange{Int64}", "AbstractRange{Integer}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("StepRangeLen{Float64}", "AbstractRange{Float64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("StepRangeLen{Float64}", "AbstractRange{Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("AbstractRange", "AbstractVector"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("AbstractRange", "AbstractArray"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("OrdinalRange", "AbstractRange"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("OrdinalRange", "AbstractVector"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange{Int64}", "AbstractVector{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange{Int64}", "AbstractArray{Int64,1}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange{Int64}", "AbstractVector{Integer}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("UnitRange{Int64}", "Array{Int64,1}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("LogRange{Float64}", "AbstractVector{Float64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("LogRange{Float64}", "AbstractArray{Float64,1}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("LogRange{Float64}", "AbstractRange"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("LogRange{Float64}", "AbstractRange{Float64}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("RefValue{Int64}", "Ref{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("RefValue{Int64}", "Ref{T} where T"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("RefValue{String}", "Ref{T<:Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("RefValue{String}", "Ref{T} where T<:Real"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("RefValue{Int64}", "Ref{Real}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Ref{Int64}", "Ref{Real}"),
            Some(false)
        );
        assert_eq!(check_core_structured_subtype("IOBuffer", "IO"), Some(true));
        assert_eq!(
            check_core_structured_subtype("IOBuffer", "Number"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Type{Int64}", "Type{Int64}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Type{Int64}", "Type{Integer}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Type{Int64}", "Type{_<:Integer}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Type{String}", "Type{_<:Integer}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Type{Vector{Int64}}", "Type{_<:AbstractVector}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Type{Matrix{Int64}}", "Type{_<:AbstractVector}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Union{Int64, String}", "Union{Real, AbstractString}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Union{Int64, String}", "Union{AbstractFloat, Symbol}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Int64", "Union{AbstractFloat, String}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Union{Int64, String}", "Real"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype(
                "Tuple{Dict{String, Int64}}",
                "Tuple{AbstractDict{String, Any}}"
            ),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{Set{Int64}}", "Tuple{AbstractSet{Int64}}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{Vector{Int64}}", "Tuple{AbstractVector{Real}}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype(
                "Tuple{UnitRange{Int64}}",
                "Tuple{AbstractVector{Integer}}"
            ),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("Tuple{RefValue{Int64}}", "Tuple{Ref{Real}}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype(
                "@NamedTuple{a::Int64, b::String}",
                "@NamedTuple{a::Int, b::String}"
            ),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype(
                "@NamedTuple{a::Int64, b::String}",
                "@NamedTuple{a::Integer, b::String}"
            ),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype(
                "@NamedTuple{a::Int64, b::String}",
                "@NamedTuple{x::Int64, b::String}"
            ),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype(
                "Tuple{@NamedTuple{a::Int64}}",
                "Tuple{@NamedTuple{a::Integer}}"
            ),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype(
                "NamedTuple{(:a, :b), Tuple{Int64, String}}",
                "@NamedTuple{a::Int64, b::String}"
            ),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype(
                "NamedTuple{(:a, :b), Tuple{Int64, String}}",
                "NamedTuple{(:a, :b), Tuple{Integer, String}}"
            ),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype(
                "NamedTuple{(:a, :b), Tuple{Int64, String}}",
                "NamedTuple{(:x, :b)}"
            ),
            Some(false)
        );

        let mut hierarchy = StructHierarchy::new();
        for (name, parent) in [
            ("Animal", Some("Any")),
            ("Mammal", Some("Animal")),
            ("Vehicle", Some("Any")),
            ("Dog", Some("Mammal")),
            ("Cat", Some("Animal")),
            ("Box", Some("Animal")),
        ] {
            hierarchy.insert(name, parent.map(str::to_string), Vec::new());
        }
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Dog", "Animal"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Cat", "Mammal"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Tuple{Dog}", "Tuple{Animal}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(
                &hierarchy,
                "Tuple{Dog}",
                "Tuple{Vehicle}"
            ),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Box{Int64}", "Animal"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(&hierarchy, "Box{Int64}", "Vehicle"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(
                &hierarchy,
                "Tuple{Box{Int64}}",
                "Tuple{Animal}"
            ),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype_with_hierarchy(
                &hierarchy,
                "Tuple{Tuple{Dog}, Int64}",
                "Tuple{Tuple{Animal}, Real}"
            ),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("BitVector", "AbstractVector{Bool}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("BitVector", "AbstractVector{Any}"),
            Some(false)
        );
        assert_eq!(
            check_core_structured_subtype("BitArray{3}", "AbstractArray{Bool,3}"),
            Some(true)
        );
        assert_eq!(
            check_core_structured_subtype("Vector{Bool}", "BitVector"),
            Some(false)
        );

        assert!(vm.check_subtype("Vector{Int64}", "Vector{T} where T"));
        assert!(vm.check_subtype("Foo{Int64}", "Foo"));
        assert!(vm.check_subtype("Tuple{Foo{Int64}}", "Tuple{Foo}"));
        assert!(vm.check_subtype("Signed", "Number"));
        assert!(!vm.check_subtype("Signed", "AbstractFloat"));
        assert!(!vm.check_subtype("Int64", "AbstractFloat"));
        assert!(!vm.check_subtype("AbstractVector", "AbstractMatrix"));
        assert!(vm.check_subtype("Vector{Int64}", "AbstractArray"));
        assert!(vm.check_subtype("Matrix{Float64}", "AbstractArray"));
        assert!(vm.check_subtype("Array{Float64, 1}", "AbstractVector"));
        assert!(vm.check_subtype("Array{Float64, 2}", "AbstractMatrix"));
        assert!(vm.check_subtype("DenseArray{Int64, 1}", "AbstractVector"));
        assert!(!vm.check_subtype("Array{Float64}", "AbstractVector"));
        assert!(!vm.check_subtype("Matrix{Float64}", "AbstractVector"));
        assert!(vm.check_subtype("Vector{Int64}", "AbstractVector{Int64}"));
        assert!(!vm.check_subtype("Vector{Float64}", "AbstractVector{Int64}"));
        assert!(vm.check_subtype("Matrix{Float64}", "AbstractMatrix{Float64}"));
        assert!(!vm.check_subtype("Matrix{Int64}", "AbstractMatrix{Float64}"));
        assert!(!vm.check_subtype("Array{Float64, 2}", "AbstractVector{Float64}"));
        assert!(vm.check_subtype("Dict{String, Int64}", "AbstractDict{String, Int64}"));
        assert!(!vm.check_subtype("Dict{String, Int64}", "AbstractDict{String, Any}"));
        assert!(vm.check_subtype("Set{Int64}", "AbstractSet{Int64}"));
        assert!(!vm.check_subtype("Set{Int64}", "AbstractSet{Real}"));
        assert!(vm.check_subtype("UnitRange{Int64}", "AbstractUnitRange{Int64}"));
        assert!(!vm.check_subtype("UnitRange{Int64}", "AbstractUnitRange{Integer}"));
        assert!(vm.check_subtype("UnitRange{Int64}", "OrdinalRange{Int64}"));
        assert!(!vm.check_subtype("UnitRange{Int64}", "OrdinalRange{Integer}"));
        assert!(vm.check_subtype("StepRange{Int64, Int64}", "OrdinalRange{Int64}"));
        assert!(!vm.check_subtype("StepRange{Int64, Int64}", "OrdinalRange{Integer}"));
        assert!(vm.check_subtype("UnitRange{Int64}", "AbstractRange{Int64}"));
        assert!(!vm.check_subtype("UnitRange{Int64}", "AbstractRange{Integer}"));
        assert!(vm.check_subtype("StepRangeLen{Float64}", "AbstractRange{Float64}"));
        assert!(!vm.check_subtype("StepRangeLen{Float64}", "AbstractRange{Real}"));
        assert!(!vm.check_subtype("LogRange{Float64}", "AbstractRange"));
        assert!(!vm.check_subtype("LogRange{Float64}", "AbstractRange{Float64}"));
        assert!(vm.check_subtype("BitVector", "AbstractVector{Bool}"));
        assert!(vm.check_subtype("BitMatrix", "AbstractMatrix{Bool}"));
        assert!(vm.check_subtype("BitArray{3}", "AbstractArray{Bool,3}"));
        assert!(vm.check_subtype("BitVector", "BitArray"));
        assert!(vm.check_subtype("BitVector", "BitArray{1}"));
        assert!(!vm.check_subtype("BitVector", "AbstractVector{Any}"));
        assert!(!vm.check_subtype("BitArray{3}", "AbstractArray{Bool,2}"));
        assert!(!vm.check_subtype("BitVector", "DenseArray"));
        assert!(!vm.check_subtype("Vector{Bool}", "BitVector"));
        assert!(vm.check_subtype("RefValue{Int64}", "Ref{Int64}"));
        assert!(!vm.check_subtype("RefValue{Int64}", "Ref{Real}"));
        assert!(vm.check_subtype("RefValue{Int64}", "Ref"));
        assert!(!vm.check_subtype("Ref{Int64}", "Ref{Real}"));
        assert!(vm.check_subtype("Type{Int64}", "Type{Int64}"));
        assert!(!vm.check_subtype("Type{Int64}", "Type{Integer}"));
        assert!(vm.check_subtype("Type{Int64}", "Type{_<:Integer}"));
        assert!(!vm.check_subtype("Type{String}", "Type{_<:Integer}"));
        assert!(vm.check_subtype("Type{Vector{Int64}}", "Type{_<:AbstractVector}"));
        assert!(!vm.check_subtype("Type{Matrix{Int64}}", "Type{_<:AbstractVector}"));
        assert!(vm.check_subtype("Dog", "Animal"));
        assert!(vm.check_subtype("Tuple{Dog}", "Tuple{Animal}"));
        assert!(!vm.check_subtype("Tuple{Dog}", "Tuple{Vehicle}"));
    }

    #[test]
    fn test_parametric_instance_subtypes_bare_family_issue_5582() {
        let vm = make_vm();
        assert!(vm.check_subtype("Irrational{:π}", "Irrational"));
    }

    /// Issue #5614: a forall-left where-form over a user PARAMETRIC struct
    /// resolves its declared abstract parent through the reflection ancestry,
    /// since the rendered `where` operand is decided by the structured CoreType
    /// solver (which cannot see lazily-instantiated parametric user structs).
    #[test]
    fn test_check_subtype_forall_left_parametric_struct_5614() {
        let mut vm = make_vm();
        // Mirror what the VM start-up builds (Issue #5915 wave 3: the engine
        // reads `struct_hierarchy`, which `build_struct_hierarchy_from_program`
        // always populates alongside `type_ancestors` from the same program) for:
        //   abstract type Shape end; struct Circle{T<:Real} <: Shape ... end
        //   abstract type Animal end; abstract type Mammal <: Animal end
        //   struct Dog{T} <: Mammal ... end
        //   abstract type Wrapper{T} end; struct MyVec{T} <: Wrapper{T} ... end
        vm.struct_hierarchy
            .insert("Shape", Some("Any".to_string()), Vec::new());
        vm.struct_hierarchy
            .insert("Circle", Some("Shape".to_string()), vec!["T".to_string()]);
        vm.struct_hierarchy
            .insert("Animal", Some("Any".to_string()), Vec::new());
        vm.struct_hierarchy
            .insert("Mammal", Some("Animal".to_string()), Vec::new());
        vm.struct_hierarchy
            .insert("Dog", Some("Mammal".to_string()), vec!["T".to_string()]);
        vm.struct_hierarchy
            .insert("Wrapper", Some("Any".to_string()), vec!["T".to_string()]);
        vm.struct_hierarchy.insert(
            "MyVec",
            Some("Wrapper{T}".to_string()),
            vec!["T".to_string()],
        );
        vm.type_ancestors
            .insert("Circle".to_string(), vec!["Shape".to_string()]);
        vm.type_ancestors.insert(
            "Dog".to_string(),
            vec!["Mammal".to_string(), "Animal".to_string()],
        );
        vm.type_ancestors.insert(
            "MyVec".to_string(),
            vec!["Wrapper{T}".to_string(), "Wrapper".to_string()],
        );

        // The bug: explicit where-form over a parametric struct.
        assert!(vm.check_subtype("Circle{T} where T", "Shape"));
        assert!(vm.check_subtype("Circle{T} where T<:Real", "Shape"));
        // Multi-level chain through an intermediate user abstract type.
        assert!(vm.check_subtype("Dog{T} where T", "Mammal"));
        assert!(vm.check_subtype("Dog{T} where T", "Animal"));
        // Parametric abstract parent: bare matches, invariant param does not.
        assert!(vm.check_subtype("MyVec{T} where T", "Wrapper"));
        assert!(!vm.check_subtype("MyVec{T} where T", "Wrapper{Int64}"));
        // Unrelated abstract never matches; structural where-forms are untouched.
        assert!(!vm.check_subtype("Circle{T} where T", "Animal"));
        assert!(!vm.check_subtype("Dog{T} where T", "Wrapper"));
    }

    /// Bare `Tuple` is definitionally `Tuple{Vararg{Any}}` upstream, so it is a
    /// subtype of `Tuple{Vararg{Any}}` (and mutually with bare `Tuple`), but NOT
    /// of a narrower vararg pattern or any fixed-arity tuple (Issue #5061).
    #[test]
    fn test_check_subtype_bare_tuple_universal_vararg() {
        let vm = make_vm();
        // Tuple === Tuple{Vararg{Any}}: both directions hold.
        assert!(vm.check_subtype("Tuple", "Tuple{Vararg{Any}}"));
        assert!(vm.check_subtype("Tuple{Vararg{Any}}", "Tuple"));
        // Narrower or fixed-arity patterns must NOT have bare Tuple as a subtype.
        assert!(!vm.check_subtype("Tuple", "Tuple{Vararg{Int64}}"));
        assert!(!vm.check_subtype("Tuple", "Tuple{Vararg{Real}}"));
        assert!(!vm.check_subtype("Tuple", "Tuple{Any}"));
        assert!(!vm.check_subtype("Tuple", "Tuple{Any, Vararg{Any}}"));
    }

    #[test]
    fn test_check_subtype_shared_core_type_forms() {
        let vm = make_vm();

        assert!(vm.check_subtype("Tuple{Int64, String}", "Tuple{Real, Any}"));
        assert!(!vm.check_subtype("Tuple{String}", "Tuple{Real}"));
        assert!(vm.check_subtype("Tuple{Int64, String}", "Tuple"));
        assert!(vm.check_subtype("Tuple{Vararg{Int64}}", "Tuple"));
        assert!(vm.check_subtype("Type{Int64}", "Type"));
        assert!(vm.check_subtype("Type{Int64}", "Type{_<:Real}"));
        assert!(vm.check_subtype("Type{Int64}", "Type{T<:Real}"));
        assert!(!vm.check_subtype("Type{String}", "Type{_<:Real}"));
        assert!(vm.check_subtype("Union{Int64, Float64}", "Union{Real, String}"));
    }

    /// Verify String <: AbstractString.
    #[test]
    fn test_check_subtype_string() {
        let vm = make_vm();
        assert!(vm.check_subtype("String", "AbstractString"));
        assert!(!vm.check_subtype("String", "Number"));
    }

    /// Verify Union type handling.
    #[test]
    fn test_check_subtype_union() {
        let vm = make_vm();
        // Int64 <: Union{Int64, Float64}
        assert!(vm.check_subtype("Int64", "Union{Int64, Float64}"));
        // Float64 <: Union{Int64, Float64}
        assert!(vm.check_subtype("Float64", "Union{Int64, Float64}"));
        // String is NOT <: Union{Int64, Float64}
        assert!(!vm.check_subtype("String", "Union{Int64, Float64}"));
        // Issue #5257: Nothing is a concrete singleton, NOT bottom, so it is
        // NOT a subtype of a union that does not contain it.
        assert!(!vm.check_subtype("Nothing", "Union{Int64}"));
        assert!(!vm.check_subtype("Nothing", "Union{Int64, Float64}"));
        // ...but Nothing IS a subtype of a union that contains it.
        assert!(vm.check_subtype("Nothing", "Union{Nothing, Int64}"));
        // Union{} <: T is always true (bottom type)
        assert!(vm.check_subtype("Union{}", "Int64"));
    }

    /// Parity test: verify check_subtype() (runtime, string-based) agrees with
    /// JuliaType::is_subtype_of() (compile-time, enum-based) for ALL numeric
    /// and range type pairs. Both paths now delegate the built-in hierarchy to
    /// the shared CoreSubtypeEngine (Issue #5921), so this is the regression
    /// test that the delegation stays complete — a pair handled by only one
    /// path fails here. (Originally the Issue #2494 manual-sync check.)
    #[test]
    fn test_check_subtype_parity_with_julia_type() {
        use crate::types::JuliaType;
        let vm = make_vm();

        // All concrete and abstract numeric types that both implementations handle
        let type_pairs: Vec<(&str, JuliaType)> = vec![
            // Signed integers
            ("Int8", JuliaType::Int8),
            ("Int16", JuliaType::Int16),
            ("Int32", JuliaType::Int32),
            ("Int64", JuliaType::Int64),
            ("Int128", JuliaType::Int128),
            ("BigInt", JuliaType::BigInt),
            // Unsigned integers
            ("UInt8", JuliaType::UInt8),
            ("UInt16", JuliaType::UInt16),
            ("UInt32", JuliaType::UInt32),
            ("UInt64", JuliaType::UInt64),
            ("UInt128", JuliaType::UInt128),
            // Bool
            ("Bool", JuliaType::Bool),
            // Floats
            ("Float16", JuliaType::Float16),
            ("Float32", JuliaType::Float32),
            ("Float64", JuliaType::Float64),
            ("BigFloat", JuliaType::BigFloat),
            // Abstract types
            ("Signed", JuliaType::Signed),
            ("Unsigned", JuliaType::Unsigned),
            ("Integer", JuliaType::Integer),
            ("AbstractFloat", JuliaType::AbstractFloat),
            ("Real", JuliaType::Real),
            ("Number", JuliaType::Number),
            // Range types (Issue #5921): the compile-time range arms were
            // deleted in favor of CoreSubtypeEngine delegation; keep the
            // runtime string path and the enum path agreeing on them.
            ("AbstractRange", JuliaType::AbstractRange),
            ("UnitRange", JuliaType::UnitRange),
            ("StepRange", JuliaType::StepRange),
            (
                "UnitRange{Int64}",
                JuliaType::Struct("UnitRange{Int64}".to_string()),
            ),
            (
                "StepRange{Int64, Int64}",
                JuliaType::Struct("StepRange{Int64, Int64}".to_string()),
            ),
            (
                "OneTo{Int64}",
                JuliaType::Struct("OneTo{Int64}".to_string()),
            ),
            (
                "AbstractUnitRange{Int64}",
                JuliaType::Struct("AbstractUnitRange{Int64}".to_string()),
            ),
        ];

        // Check all pairs: for each (left, right), verify both implementations agree
        for (left_name, left_jtype) in &type_pairs {
            for (right_name, right_jtype) in &type_pairs {
                let runtime_result = vm.check_subtype(left_name, right_name);
                let compile_result = left_jtype.is_subtype_of(right_jtype);
                assert_eq!(
                    runtime_result, compile_result,
                    "Parity mismatch: check_subtype({left_name}, {right_name}) = {runtime_result}, \
                     is_subtype_of({left_name}, {right_name}) = {compile_result}"
                );
            }
        }
    }
}

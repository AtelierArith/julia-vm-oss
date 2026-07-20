use subset_julia_vm::types::{JuliaType, StructHierarchy, TypeParam};

mod subtype_negative_tests {
    //! Regression matrix for `JuliaType::is_subtype_of` (Issue #4710).
    //!
    //! Bug #4708: the `AbstractUser` arm had a parent-chain fallback that
    //! returned `self.is_subtype_of(parent)`; for `Any`-rooted abstracts
    //! (e.g. `AbstractDict{K,V} <: Any`) this made *every* type a spurious
    //! subtype (Array <: AbstractDict, Tuple <: AbstractString, ...) and
    //! triggered AmbiguousMethod errors during dispatch.
    //!
    //! These tests lock the *negative* cases: unrelated types must NOT be
    //! subtypes of `Any`-rooted user abstracts, while the real container
    //! relations (Dict <: AbstractDict, Set <: AbstractSet,
    //! String <: AbstractString) stay true.

    use super::*;

    fn abstract_user(name: &str) -> JuliaType {
        JuliaType::AbstractUser(name.to_string(), Some("Any".to_string()))
    }

    /// Types that must never be subtypes of the container/string abstracts.
    fn unrelated_types() -> Vec<JuliaType> {
        vec![
            JuliaType::Array,
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::MatrixOf(Box::new(JuliaType::Float64)),
            JuliaType::Tuple,
            JuliaType::NamedTuple,
            JuliaType::Int64,
            JuliaType::Float64,
            JuliaType::Bool,
            JuliaType::String,
            JuliaType::Symbol,
        ]
    }

    #[test]
    fn test_abstract_user_negative_subtypes() {
        // Issue #4708: Array, Tuple, primitive numbers etc. must NOT be
        // subtypes of AbstractDict / AbstractSet.
        let abstract_dict = abstract_user("AbstractDict");
        let abstract_set = abstract_user("AbstractSet");

        for not_dict_like in unrelated_types() {
            assert!(
                !not_dict_like.is_subtype_of(&abstract_dict),
                "{not_dict_like:?} should not be <: AbstractDict"
            );
            assert!(
                !not_dict_like.is_subtype_of(&abstract_set),
                "{not_dict_like:?} should not be <: AbstractSet"
            );
        }

        // Positive cases stay true via the CoreType lattice.
        assert!(JuliaType::Dict.is_subtype_of(&abstract_dict));
        assert!(JuliaType::Set.is_subtype_of(&abstract_set));
    }

    #[test]
    fn test_abstract_string_negative_subtypes() {
        // Issue #4708: containers / numbers must NOT be <: AbstractString,
        // but String itself must be.
        let abstract_string = abstract_user("AbstractString");

        for ty in unrelated_types() {
            if matches!(ty, JuliaType::String) {
                continue;
            }
            assert!(
                !ty.is_subtype_of(&abstract_string),
                "{ty:?} should not be <: AbstractString"
            );
        }

        assert!(
            JuliaType::String.is_subtype_of(&abstract_string),
            "String should be <: AbstractString"
        );
    }

    #[test]
    fn test_any_rooted_abstract_does_not_swallow_everything() {
        // Guard the exact #4708 false positives explicitly so a future
        // regression in the `AbstractUser` arm fails loudly.
        let abstract_dict = abstract_user("AbstractDict");
        assert!(!JuliaType::Array.is_subtype_of(&abstract_dict));

        let abstract_string = abstract_user("AbstractString");
        assert!(!JuliaType::Tuple.is_subtype_of(&abstract_string));
    }

    #[test]
    fn test_user_abstract_parent_does_not_swallow_siblings_issue_5582() {
        let abstract_irrational =
            JuliaType::AbstractUser("AbstractIrrational".to_string(), Some("Real".to_string()));

        assert!(
            !JuliaType::Float64.is_subtype_of(&abstract_irrational),
            "Float64 <: Real must not imply Float64 <: AbstractIrrational"
        );
        assert!(
            JuliaType::AbstractUser(
                "IrrationalLike".to_string(),
                Some("AbstractIrrational".to_string()),
            )
            .is_subtype_of(&abstract_irrational),
            "direct child abstracts should still match their declared parent"
        );
    }

    #[test]
    fn test_unbounded_typevar_is_any_like() {
        // An unbounded TypeVar's bound is `Any`, so any type is a subtype of
        // it (matches upstream `T where T`). This is intentional and the
        // *bounded* case must filter by the declared bound name.
        let unbounded = JuliaType::TypeVar("T".to_string(), None);
        assert!(JuliaType::Int64.is_subtype_of(&unbounded));
        assert!(JuliaType::Array.is_subtype_of(&unbounded));

        // A TypeVar bounded by a numeric type rejects unrelated types but
        // accepts the bound itself.
        let bounded_int = JuliaType::TypeVar("T".to_string(), Some("Int64".to_string()));
        assert!(JuliaType::Int64.is_subtype_of(&bounded_int));
        assert!(!JuliaType::String.is_subtype_of(&bounded_int));
    }

    #[test]
    fn test_typeof_subtype_invariance_issue_5068() {
        // `Type{T}` is invariant in its concrete parameter:
        // `Type{Int} <: Type{Integer}` is false, but `Type{Int} <: Type{Int}` holds.
        let type_int = JuliaType::TypeOf(Box::new(JuliaType::Int64));
        let type_integer = JuliaType::TypeOf(Box::new(JuliaType::Integer));
        assert!(type_int.is_subtype_of(&type_int.clone()));
        assert!(!type_int.is_subtype_of(&type_integer));

        // The covariant spelling `Type{<:Number}` reduces to `Int <: Number`.
        let type_le_number = JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "_".to_string(),
            Some("Number".to_string()),
        )));
        assert!(type_int.is_subtype_of(&type_le_number));
        let type_string = JuliaType::TypeOf(Box::new(JuliaType::String));
        assert!(!type_string.is_subtype_of(&type_le_number));

        // `Type{A} <: Type` (the bare `Type` alias) for any concrete `A`.
        assert!(type_int.is_subtype_of(&JuliaType::Type));
    }

    #[test]
    fn typeof_anonymous_pairs_bound_extracts_inner_params_issue_6251() {
        let actual = JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "Base.Pairs{Int64, Int8, UnitRange{Int64}, Vector{Int8}}".to_string(),
        )));
        let pattern = JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "_".to_string(),
            Some("Pairs{K,V,I,A}".to_string()),
        )));
        let params = ["K", "V", "I", "A"]
            .into_iter()
            .map(|name| TypeParam::new(name.to_string()))
            .collect::<Vec<_>>();

        let bindings = actual
            .extract_type_bindings(&pattern, &params)
            .expect("Type{<:Pairs{K,V,I,A}} should bind all inner parameters");

        assert_eq!(bindings.get("K"), Some(&JuliaType::Int64));
        assert_eq!(bindings.get("V"), Some(&JuliaType::Int8));
        assert_eq!(
            bindings.get("I"),
            Some(&JuliaType::Struct("UnitRange{Int64}".to_string()))
        );
        assert_eq!(
            bindings.get("A"),
            Some(&JuliaType::VectorOf(Box::new(JuliaType::Int8)))
        );
    }

    #[test]
    fn struct_binding_trims_spaced_params_with_nested_actuals() {
        let actual = JuliaType::Struct("Dict{Vector{Int64}, Vector{Any}}".to_string());
        let pattern = JuliaType::Struct("Dict{K, V}".to_string());
        let params = ["K", "V"]
            .into_iter()
            .map(|name| TypeParam::new(name.to_string()))
            .collect::<Vec<_>>();

        let bindings = actual
            .extract_type_bindings(&pattern, &params)
            .expect("Dict{K, V} should bind nested actual parameters");

        assert_eq!(
            bindings.get("K"),
            Some(&JuliaType::VectorOf(Box::new(JuliaType::Int64)))
        );
        assert_eq!(
            bindings.get("V"),
            Some(&JuliaType::VectorOf(Box::new(JuliaType::Any)))
        );
    }
}

mod engine_delegation_matrix_issue_5921 {
    //! Full regression matrix for the numeric / range subtype relations that
    //! used to be hand-maintained as parallel `match` tables inside
    //! `is_subtype_of` (the Issue #2494 manual-sync duty). The expected values
    //! are verified against upstream `julia` 1.12 (Issue #5921); after the
    //! tables were deleted, every relation below is decided by the
    //! `CoreSubtypeEngine` delegation at the top of `is_subtype_of`.

    use super::*;

    /// Strict abstract ancestors of `t` within the built-in numeric lattice,
    /// exactly as upstream Julia declares them. The numeric hierarchy is a
    /// tree, so `l <: r` over this set is `l == r || r ∈ ancestors(l)`.
    fn upstream_numeric_ancestors(t: &JuliaType) -> Vec<JuliaType> {
        use JuliaType as J;
        match t {
            J::Int8 | J::Int16 | J::Int32 | J::Int64 | J::Int128 | J::BigInt => {
                vec![J::Signed, J::Integer, J::Real, J::Number]
            }
            J::UInt8 | J::UInt16 | J::UInt32 | J::UInt64 | J::UInt128 => {
                vec![J::Unsigned, J::Integer, J::Real, J::Number]
            }
            // Upstream: `Bool <: Integer` but NOT `<: Signed` / `<: Unsigned`.
            J::Bool => vec![J::Integer, J::Real, J::Number],
            J::Float16 | J::Float32 | J::Float64 | J::BigFloat => {
                vec![J::AbstractFloat, J::Real, J::Number]
            }
            J::Signed | J::Unsigned => vec![J::Integer, J::Real, J::Number],
            J::Integer | J::AbstractFloat => vec![J::Real, J::Number],
            J::Real => vec![J::Number],
            J::Number => vec![],
            other => unreachable!("not part of the numeric matrix: {other:?}"),
        }
    }

    #[test]
    fn numeric_matrix_matches_upstream_julia() {
        use JuliaType as J;
        let all = vec![
            J::Int8,
            J::Int16,
            J::Int32,
            J::Int64,
            J::Int128,
            J::BigInt,
            J::UInt8,
            J::UInt16,
            J::UInt32,
            J::UInt64,
            J::UInt128,
            J::Bool,
            J::Float16,
            J::Float32,
            J::Float64,
            J::BigFloat,
            J::Signed,
            J::Unsigned,
            J::Integer,
            J::AbstractFloat,
            J::Real,
            J::Number,
        ];
        for left in &all {
            let ancestors = upstream_numeric_ancestors(left);
            for right in &all {
                let expected = left == right || ancestors.contains(right);
                assert_eq!(
                    left.is_subtype_of(right),
                    expected,
                    "{left:?} <: {right:?} should be {expected} (upstream julia)"
                );
            }
        }
    }

    #[test]
    fn range_subtypes_match_upstream_julia() {
        use JuliaType as J;
        let s = |n: &str| J::Struct(n.to_string());

        // Enum spellings.
        assert!(J::UnitRange.is_subtype_of(&J::AbstractRange));
        assert!(J::StepRange.is_subtype_of(&J::AbstractRange));
        assert!(!J::UnitRange.is_subtype_of(&J::StepRange));
        assert!(!J::StepRange.is_subtype_of(&J::UnitRange));
        assert!(!J::AbstractRange.is_subtype_of(&J::UnitRange));

        // Struct spellings (parametric range types, Issue #3550), including
        // the parametric *abstract* spellings: upstream
        // `AbstractUnitRange{Int64} <: AbstractRange` is true.
        for name in [
            "UnitRange{Int64}",
            "StepRange{Int64, Int64}",
            "StepRangeLen{Float64, Float64, Float64, Int64}",
            "LinRange{Float64, Int64}",
            "OneTo{Int64}",
            "Base.OneTo{Int64}",
            "AbstractUnitRange",
            "AbstractUnitRange{Int64}",
            "AbstractRange{Int64}",
        ] {
            assert!(
                s(name).is_subtype_of(&J::AbstractRange),
                "{name} <: AbstractRange should be true (upstream julia)"
            );
        }

        // Upstream: `LogRange <: AbstractArray{T,1}`, NOT `<: AbstractRange`.
        assert!(!s("LogRange{Float64, Float64}").is_subtype_of(&J::AbstractRange));
        assert!(!J::Int64.is_subtype_of(&J::AbstractRange));
        assert!(!s("Vector{Int64}").is_subtype_of(&J::AbstractRange));

        // Concrete range targets stay invariant in their base name:
        // `Base.OneTo` and `UnitRange` are sibling subtypes of
        // `AbstractUnitRange` upstream.
        assert!(s("UnitRange{Int64}").is_subtype_of(&J::UnitRange));
        assert!(!s("OneTo{Int64}").is_subtype_of(&J::UnitRange));
        assert!(s("StepRange{Int64, Int64}").is_subtype_of(&J::StepRange));
        assert!(!s("UnitRange{Int64}").is_subtype_of(&J::StepRange));
        assert!(s("UnitRange{Int64}").is_subtype_of(&s("AbstractUnitRange{Int64}")));
        assert!(!s("OneTo{Int64}").is_subtype_of(&s("StepRange{Int64, Int64}")));
    }
}

mod engine_delegated_union_typeof_arms_issue_5915 {
    //! Issue #5915 compile-side residual: the local Union decomposition
    //! early-returns and the local `Type{}` invariance arm of `is_subtype_of`
    //! were deleted in favour of the `CoreSubtypeEngine` delegation (the
    //! engine's `CoreType` solver has its own Union and `(TypeOf, TypeOf)`
    //! arms). Every expectation below is verified against upstream `julia`
    //! 1.12 unless explicitly marked as the documented permissive residue.

    use super::*;

    fn s(name: &str) -> JuliaType {
        JuliaType::Struct(name.to_string())
    }

    #[test]
    fn union_left_decomposition_is_forall_members() {
        // julia: Union{Int64, Float64} <: Real == true
        assert!(JuliaType::Union(vec![JuliaType::Int64, JuliaType::Float64])
            .is_subtype_of(&JuliaType::Real));
        // julia: Union{Int64, String} <: Real == false
        assert!(!JuliaType::Union(vec![JuliaType::Int64, JuliaType::String])
            .is_subtype_of(&JuliaType::Real));
        // julia: Union{Vector{Int64}, Matrix{Int64}} <: Array == true
        assert!(JuliaType::Union(vec![
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::MatrixOf(Box::new(JuliaType::Int64)),
        ])
        .is_subtype_of(&JuliaType::Array));
        // julia: Union{BitVector, BitMatrix} <: AbstractArray == true (the
        // bitarray family is engine-resolved, not local-projection-resolved)
        assert!(JuliaType::Union(vec![s("BitVector"), s("BitMatrix")])
            .is_subtype_of(&JuliaType::AbstractArray));
        // julia: Union{BitVector, Int64} <: AbstractArray == false
        assert!(!JuliaType::Union(vec![s("BitVector"), JuliaType::Int64])
            .is_subtype_of(&JuliaType::AbstractArray));
        // julia: Union{UnitRange{Int64}, StepRange{Int64, Int64}} <: AbstractRange
        assert!(
            JuliaType::Union(vec![s("UnitRange{Int64}"), s("StepRange{Int64, Int64}")])
                .is_subtype_of(&JuliaType::AbstractRange)
        );
    }

    #[test]
    fn union_right_decomposition_is_exists_member() {
        // julia: Int64 <: Union{Integer, String} == true
        assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Union(vec![
            JuliaType::Integer,
            JuliaType::String
        ])));
        // julia: Float64 <: Union{Integer, String} == false
        assert!(!JuliaType::Float64.is_subtype_of(&JuliaType::Union(vec![
            JuliaType::Integer,
            JuliaType::String
        ])));
        // julia: Vector{Int64} <: Union{AbstractVector, Dict} == true
        assert!(
            JuliaType::VectorOf(Box::new(JuliaType::Int64)).is_subtype_of(&JuliaType::Union(vec![
                s("AbstractVector"),
                JuliaType::Dict
            ]))
        );
        // julia: Union{} <: Union{Int64} == true (empty union is Bottom; the
        // non-canonical `Union(vec![])` spelling must agree)
        assert!(JuliaType::Union(vec![]).is_subtype_of(&JuliaType::Union(vec![JuliaType::Int64])));
        assert!(JuliaType::Bottom.is_subtype_of(&JuliaType::Union(vec![JuliaType::Int64])));
    }

    #[test]
    fn typeof_invariance_decided_by_engine() {
        let type_of = |t: JuliaType| JuliaType::TypeOf(Box::new(t));
        // julia: Type{Int64} <: Type{Int64} == true
        assert!(type_of(JuliaType::Int64).is_subtype_of(&type_of(JuliaType::Int64)));
        // julia: Type{Int64} <: Type{Integer} == false (invariant)
        assert!(!type_of(JuliaType::Int64).is_subtype_of(&type_of(JuliaType::Integer)));
        // julia: Type{Int64} <: Type{<:Number} == true (covariant spelling)
        let le_number = type_of(JuliaType::TypeVar(
            "_".to_string(),
            Some("Number".to_string()),
        ));
        assert!(type_of(JuliaType::Int64).is_subtype_of(&le_number));
        // julia: Type{String} <: Type{<:Number} == false
        assert!(!type_of(JuliaType::String).is_subtype_of(&le_number));
        // julia: Type{Int64} <: (Type{T} where T) == true (unbounded var)
        let unbounded = type_of(JuliaType::TypeVar("T".to_string(), None));
        assert!(type_of(JuliaType::Int64).is_subtype_of(&unbounded));
        // A non-Type left side never matches a `Type{...}` pattern.
        assert!(!JuliaType::Int64.is_subtype_of(&type_of(JuliaType::Int64)));
        // julia: Type{Box{Int64}} <: Type{<:Box} == true (parametric base)
        let le_box = type_of(JuliaType::TypeVar("_".to_string(), Some("Box".to_string())));
        // ("Box" is unresolvable at this enum level — documented permissive
        // residue below — so this stays true, agreeing with upstream here.)
        assert!(type_of(s("Box{Int64}")).is_subtype_of(&le_box));
    }

    /// The legacy local invariance check recursed through the full
    /// `is_subtype_of`, whose reverse-parametric quirk (`Foo <: Foo{Int64}`)
    /// made `Type{Vector} <: Type{Vector{Int64}}` spuriously true. The engine
    /// agrees with upstream: julia `Type{Vector} <: Type{Vector{Int64}}` ==
    /// false (and the parameterized-vs-bare direction is also false).
    #[test]
    fn typeof_reverse_parametric_quirk_removed() {
        let type_of = |t: JuliaType| JuliaType::TypeOf(Box::new(t));
        assert!(!type_of(s("Vector")).is_subtype_of(&type_of(s("Vector{Int64}"))));
        assert!(!type_of(s("Vector{Int64}")).is_subtype_of(&type_of(s("Vector"))));
    }

    /// Documented permissive residue (pre-existing behavior, deliberately
    /// preserved): a `Type{<:Bound}` whose bound name `JuliaType::from_name`
    /// cannot resolve (user structs / user abstracts / bounds spelled with
    /// method type-params like `Pairs{K,V,I,A}`) stays accepting, because
    /// this enum-level check has no struct hierarchy to consult. Upstream
    /// would resolve these through the actual type; the hierarchy-aware
    /// runtime path (`Vm::check_subtype` → engine `with_hierarchy`) does.
    #[test]
    fn typeof_unresolvable_bound_stays_permissive() {
        let type_of = |t: JuliaType| JuliaType::TypeOf(Box::new(t));
        let le_user = type_of(JuliaType::TypeVar(
            "_".to_string(),
            Some("MyUserAbstract".to_string()),
        ));
        assert!(type_of(s("MyStruct")).is_subtype_of(&le_user));
        let le_pairs = type_of(JuliaType::TypeVar(
            "_".to_string(),
            Some("Pairs{K,V,I,A}".to_string()),
        ));
        assert!(
            type_of(s("Base.Pairs{Int64, Int8, UnitRange{Int64}, Vector{Int8}}"))
                .is_subtype_of(&le_pairs)
        );
    }
}

mod typebound_hierarchy_strictening_issue_6596 {
    //! Issue #6596: `is_subtype_of_in` plumbs the program `StructHierarchy`
    //! into the enum-level subtype check so a `Type{<:Bound}` / bounded-typevar
    //! whose bound name `JuliaType::from_name` cannot resolve (user abstracts,
    //! parametric `Pairs{K,V,I,A}` spellings) is decided by
    //! `CoreSubtypeEngine::with_hierarchy` instead of being permissively
    //! accepted. Every expected value is verified against upstream `julia`
    //! 1.12.6 (`Type{Child} <: Type{<:Abstract}` etc.).

    use super::StructHierarchy;
    use super::*;

    fn s(name: &str) -> JuliaType {
        JuliaType::Struct(name.to_string())
    }
    fn type_of(t: JuliaType) -> JuliaType {
        JuliaType::TypeOf(Box::new(t))
    }

    /// `abstract type MyUserAbstract end; struct MyChild <: MyUserAbstract end`.
    fn user_abstract_hierarchy() -> StructHierarchy {
        let mut h = StructHierarchy::new();
        h.insert("MyUserAbstract", Some("Any".to_string()), Vec::new());
        h.insert("MyChild", Some("MyUserAbstract".to_string()), Vec::new());
        h.insert("MyStruct", Some("Any".to_string()), Vec::new());
        h
    }

    #[test]
    fn typeof_user_abstract_bound_is_strict_with_hierarchy() {
        let h = user_abstract_hierarchy();
        let le_user = type_of(JuliaType::TypeVar(
            "_".to_string(),
            Some("MyUserAbstract".to_string()),
        ));

        // julia: Type{MyChild} <: Type{<:MyUserAbstract} == true
        assert!(type_of(s("MyChild")).is_subtype_of_in(&le_user, &h));
        // julia: Type{MyStruct} <: Type{<:MyUserAbstract} == false
        // (the *strictening*: the no-hierarchy path accepts this permissively).
        assert!(!type_of(s("MyStruct")).is_subtype_of_in(&le_user, &h));
        assert!(
            type_of(s("MyStruct")).is_subtype_of(&le_user),
            "no-hierarchy path stays permissive (protects dispatch)"
        );
    }

    #[test]
    fn bare_user_abstract_bounded_typevar_is_strict_with_hierarchy() {
        let h = user_abstract_hierarchy();
        let bounded = JuliaType::TypeVar("T".to_string(), Some("MyUserAbstract".to_string()));

        // julia: MyChild <: MyUserAbstract == true; MyStruct <: MyUserAbstract == false
        assert!(s("MyChild").is_subtype_of_in(&bounded, &h));
        assert!(!s("MyStruct").is_subtype_of_in(&bounded, &h));
        // Old enum-only path is permissive for the unresolvable bound.
        assert!(s("MyStruct").is_subtype_of(&bounded));
    }

    #[test]
    fn typeof_pairs_family_bound_resolves_with_hierarchy() {
        // The Base.Pairs family is registered in the program hierarchy; a
        // matching concrete Pairs is a subtype, an unrelated type is not.
        let mut h = StructHierarchy::new();
        h.insert("Pairs", Some("AbstractDict".to_string()), Vec::new());
        let le_pairs = type_of(JuliaType::TypeVar(
            "_".to_string(),
            Some("Pairs".to_string()),
        ));

        // julia: Type{<concrete Pairs>} <: Type{<:Base.Pairs} == true
        assert!(
            type_of(s("Base.Pairs{Int64, Int8, UnitRange{Int64}, Vector{Int8}}"))
                .is_subtype_of_in(&le_pairs, &h)
        );
        // julia: Type{Int64} <: Type{<:Base.Pairs} == false
        assert!(!type_of(JuliaType::Int64).is_subtype_of_in(&le_pairs, &h));
    }

    #[test]
    fn builtin_bounds_unchanged_between_paths() {
        // A bound `from_name` *can* resolve is decided identically with or
        // without a hierarchy (the engine resolves it the same way).
        let h = StructHierarchy::new();
        let le_number = type_of(JuliaType::TypeVar(
            "_".to_string(),
            Some("Number".to_string()),
        ));
        assert!(type_of(JuliaType::Int64).is_subtype_of_in(&le_number, &h));
        assert_eq!(
            type_of(JuliaType::Int64).is_subtype_of(&le_number),
            type_of(JuliaType::Int64).is_subtype_of_in(&le_number, &h)
        );
        assert!(!type_of(JuliaType::String).is_subtype_of_in(&le_number, &h));
        // Invariance still holds under the hierarchy path.
        assert!(!type_of(JuliaType::Int64).is_subtype_of_in(&type_of(JuliaType::Integer), &h));
    }

    #[test]
    fn unknown_bound_without_registration_is_not_a_subtype() {
        // A bound that is neither built-in nor registered in the hierarchy is
        // authoritatively rejected (no permissive accept) when a hierarchy is
        // supplied — matching the engine's "unknown Named only matches exactly".
        let h = StructHierarchy::new();
        let le_unknown = type_of(JuliaType::TypeVar(
            "_".to_string(),
            Some("TotallyUnknownAbstract".to_string()),
        ));
        assert!(!type_of(s("Whatever")).is_subtype_of_in(&le_unknown, &h));
        // But an exact bound name still matches its own concrete spelling.
        let le_self = type_of(JuliaType::TypeVar(
            "_".to_string(),
            Some("Whatever".to_string()),
        ));
        assert!(type_of(s("Whatever")).is_subtype_of_in(&le_self, &h));
    }
}

mod array_type_ndims_tests {
    //! Type-level `ndims` rank projection (Issue #5118):
    //! `ndims(Vector{Int}) === 1`, `ndims(Matrix{Int}) === 2`,
    //! `ndims(Array{Int,3}) === 3`, and `None` for bare/underspecified
    //! `Array` schemas (where upstream `ndims` is a `MethodError`).

    use super::*;

    #[test]
    fn vector_and_matrix_projections_have_fixed_rank() {
        assert_eq!(
            JuliaType::VectorOf(Box::new(JuliaType::Int64)).array_type_ndims(),
            Some(1)
        );
        assert_eq!(
            JuliaType::MatrixOf(Box::new(JuliaType::Float64)).array_type_ndims(),
            Some(2)
        );
    }

    #[test]
    fn parametric_array_struct_reports_its_dimension() {
        assert_eq!(
            JuliaType::Struct("Array{Int64, 3}".to_string()).array_type_ndims(),
            Some(3)
        );
        assert_eq!(
            JuliaType::Struct("Array{Float64, 4}".to_string()).array_type_ndims(),
            Some(4)
        );
    }

    #[test]
    fn underspecified_or_non_array_types_have_no_rank() {
        // Bare `Array` and `Array{T}` leave the dimension unspecified, so the
        // rank is unknown (upstream `ndims` is a MethodError there).
        assert_eq!(JuliaType::Array.array_type_ndims(), None);
        assert_eq!(
            JuliaType::Struct("Array{Int64}".to_string()).array_type_ndims(),
            None
        );
        // Non-array types never project to an array rank.
        assert_eq!(JuliaType::Int64.array_type_ndims(), None);
        assert_eq!(JuliaType::Dict.array_type_ndims(), None);
    }
}

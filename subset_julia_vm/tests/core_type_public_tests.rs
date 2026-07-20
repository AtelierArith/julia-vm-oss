use subset_julia_vm::inference_core::{CorePrimitive, CoreType};
use subset_julia_vm::types::JuliaType;

mod named_tuple_marker_subtype_5890 {
    use super::*;
    fn concrete_xy() -> CoreType {
        CoreType::NamedTuple(vec![
            ("x".to_string(), CoreType::Primitive(CorePrimitive::Int64)),
            ("y".to_string(), CoreType::Primitive(CorePrimitive::Int64)),
        ])
    }
    fn marker(name: &str) -> CoreType {
        CoreType::from(&JuliaType::from_name(name).unwrap())
    }
    #[test]
    fn concrete_named_tuple_only_subtypes_matching_names_marker() {
        // unrelated names -> NOT a subtype (the #5890 bug)
        assert!(!concrete_xy().is_subtype_of(&marker("NamedTuple{(:a,:b)}")));
        // matching names, in order -> subtype
        assert!(concrete_xy().is_subtype_of(&marker("NamedTuple{(:x,:y)}")));
        // bare NamedTuple -> universal supertype
        assert!(concrete_xy().is_subtype_of(&CoreType::Struct {
            name: "NamedTuple".to_string(),
            params: vec![],
        }));
        // right names, wrong order -> NOT a subtype
        assert!(!concrete_xy().is_subtype_of(&marker("NamedTuple{(:y,:x)}")));
    }
}

// The `case_H` test name mirrors the upstream "case H" divergence label from the
// Issue #5925 analysis; keep it verbatim rather than snake-casing it.
#[allow(non_snake_case)]
mod strict_subtype_dominance_5925 {
    use super::*;

    /// Issue #5926: a built-in abstract alias that reaches the core wrapped as
    /// `AbstractUser` (the dispatch `core_signature` path) must subtype-resolve
    /// like the `from_julia_name` form, so the morespecific dominance check sees
    /// `Vector{T} <: AbstractVector`. A genuine user abstract is left unaffected.
    #[test]
    fn builtin_abstract_alias_as_abstractuser_resolves() {
        use subset_julia_vm::types::JuliaType as JT;

        let av = CoreType::from(&JT::AbstractUser(
            "AbstractVector".to_string(),
            Some("AbstractArray".to_string()),
        ));
        let vec = CoreType::from_julia_name("Vector{T} where T");
        assert!(
            vec.is_subtype_of(&av),
            "Vector{{T}} <: AbstractVector (wrapped as AbstractUser)"
        );
        assert!(
            !av.is_subtype_of(&vec),
            "AbstractVector is not <: Vector{{T}}"
        );
        assert!(
            CoreType::from_julia_name("Tuple{Vector{T}} where T").is_subtype_of(&CoreType::Tuple(
                vec![av.clone()]
            )),
            "Tuple{{Vector{{T}}}} where T <: Tuple{{AbstractVector}} when AbstractVector is wrapped as AbstractUser"
        );

        let ad = CoreType::from(&JT::AbstractUser(
            "AbstractDict".to_string(),
            Some("Any".to_string()),
        ));
        let dict = CoreType::from_julia_name("Dict{K, V} where {K, V}");
        assert!(dict.is_subtype_of(&ad), "Dict{{K,V}} <: AbstractDict");
        assert!(!ad.is_subtype_of(&dict));

        // A genuine *user* abstract resolves to `Named`, not `Abstract`, so the
        // new arm does not fire and the relation stays as before (no spurious
        // subtype against an unrelated leaf type).
        let user = CoreType::from(&JT::AbstractUser(
            "MyUserAbstract".to_string(),
            Some("Any".to_string()),
        ));
        assert!(!CoreType::from_julia_name("Int64").is_subtype_of(&user));
    }

    /// Issue #5925: pin `strict_subtype_dominates` across five representative
    /// families of method-signature comparison. For each, the strictly more
    /// specific signature dominates and the reverse does not; the fifth pair is
    /// mutually non-dominating (ambiguous), as upstream dispatch would also flag.
    /// These are the unambiguous, subtype-decidable cases — the cases where the
    /// `<:` partial order already agrees with `morespecific` (the disagreement is
    /// pinned separately in `..._case_H`).
    #[test]
    fn strict_subtype_dominance_five_families() {
        fn dominates(a: &str, b: &str) -> bool {
            CoreType::from_julia_name(a).strict_subtype_dominates(&CoreType::from_julia_name(b))
        }

        // (1) diagonal `Tuple{T,T}` is more specific than `Tuple{Any,Any}`.
        assert!(dominates("Tuple{T, T} where T", "Tuple{Any, Any}"));
        assert!(!dominates("Tuple{Any, Any}", "Tuple{T, T} where T"));

        // (2) a fixed single-element tuple is more specific than a vararg tuple.
        assert!(dominates("Tuple{Int64}", "Tuple{Vararg{Int64}}"));
        assert!(!dominates("Tuple{Vararg{Int64}}", "Tuple{Int64}"));

        // (3) a leading concrete element + vararg is more specific than all-vararg.
        assert!(dominates("Tuple{Int64, Vararg{Any}}", "Tuple{Vararg{Any}}"));
        assert!(!dominates(
            "Tuple{Vararg{Any}}",
            "Tuple{Int64, Vararg{Any}}"
        ));

        // (4) a narrower abstract is more specific than a wider one.
        assert!(dominates("Signed", "Integer"));
        assert!(!dominates("Integer", "Signed"));

        // (5) two tuples where neither is a subtype of the other: ambiguous, so
        //     dominance is false in BOTH directions.
        assert!(!dominates("Tuple{Int64, Any}", "Tuple{Any, Int64}"));
        assert!(!dominates("Tuple{Any, Int64}", "Tuple{Int64, Any}"));
    }

    /// Issue #8439: align the former case-H divergence with upstream
    /// `type_morespecific_`. Upstream reports
    /// `morespecific(Tuple{Int64,Number}, Tuple{T,T} where T<:Number)` as
    /// `true` even though neither side is a strict subtype of the other.
    #[test]
    fn dominance_matches_upstream_morespecific_case_H() {
        let specific = CoreType::from_julia_name("Tuple{Int64, Number}");
        let mixed_concrete = CoreType::from_julia_name("Tuple{Int64, Float64}");
        let bound_tuple = CoreType::from_julia_name("Tuple{Number, Number}");
        let any_tuple = CoreType::from_julia_name("Tuple{Any, Any}");
        let diagonal = CoreType::from_julia_name("Tuple{T, T} where T<:Number");

        // Neither is a `<:` of the other in this core engine.
        assert!(!specific.is_subtype_of(&diagonal));
        assert!(!diagonal.is_subtype_of(&specific));

        // But the concrete/bounded tuple is more specific under upstream's
        // diagonal-aware method order.
        assert!(specific.strict_subtype_dominates(&diagonal));
        assert!(!diagonal.strict_subtype_dominates(&specific));
        assert!(mixed_concrete.strict_subtype_dominates(&diagonal));
        assert!(!diagonal.strict_subtype_dominates(&mixed_concrete));

        // If all tuple slots are exactly at or above the diagonal bound, the
        // diagonal signature is the more-specific side, matching upstream.
        assert!(!bound_tuple.strict_subtype_dominates(&diagonal));
        assert!(diagonal.strict_subtype_dominates(&bound_tuple));
        assert!(!any_tuple.strict_subtype_dominates(&diagonal));
        assert!(diagonal.strict_subtype_dominates(&any_tuple));

        // Sanity: the fully-concrete equal-element tuple dominates the
        // diagonal, where the subtype order and `morespecific` agree.
        let equal = CoreType::from_julia_name("Tuple{Int64, Int64}");
        assert!(equal.strict_subtype_dominates(&diagonal));
        assert!(!diagonal.strict_subtype_dominates(&equal));
    }
}

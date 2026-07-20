# Issue #10354: an arrow (`->`) lambda's parameter list must lower its keyword
# parameters through the SAME authority the named-function signature path uses
# (`lowering/function/signature.rs::parse_kwparam_node`). The two arrow
# collectors used to open-code their own PARTIAL copy of that match, which
# produced two independent divergences from upstream — both invisible while
# `@test_throws` ignored the expected exception type:
#
#  1. An ANNOTATED keyword (`k::Integer = 3`) parses as an `Assignment` with a
#     `TypedExpression` LHS, matched no arm of the copy, and was SILENTLY
#     DROPPED from the lambda's signature: the body's `k` compiled to a global
#     load (`UndefVarError: k not defined`) and a supplied `k = 3` was rejected
#     as an "unsupported keyword argument". Even a perfectly VALID default was
#     broken. The copy also discarded the declared type outright
#     (`KwParam::new(name, default, None, span)`), so the Issue #11024/#11081
#     declared-type and required-keyword handling never applied to arrows.
#  2. The parser's `Assignment -> KwParameter` rewrap for arrow parameter lists
#     ran over the WHOLE list, so an optional POSITIONAL default before the `;`
#     (`(y, x = 2; k = 3) ->`, the same `Assignment[Identifier, =, value]`
#     shape) was ALSO rewrapped and lowered as a KEYWORD — `f(1, 5)` raised
#     `NoMethodFound` instead of upstream's `(1, 5, 3)`.
#
# Every assertion below was verified against upstream julia 1.12.6.

using Test

arrow_annot_10354 = (y; k::Integer = 3) -> (y, k)
arrow_bad_annot_default_10354 = (y, x = 2; k::Integer = "oops") -> (y, x, k)
arrow_posdefault_10354 = (y, x = 2; k = 3) -> (y, x, k)
arrow_required_annot_10354 = (y; k::Integer) -> (y, k)
arrow_kwsplat_10354 = (y; kw...) -> (y, length(kw))

@testset "arrow lambda: annotated keyword parameter (Issue #10354)" begin
    # The annotated keyword must EXIST: omitted -> its default; supplied -> the
    # supplied value. Both used to raise (UndefVarError / "unsupported keyword
    # argument") because the keyword was dropped from the signature entirely.
    @test arrow_annot_10354(1) == (1, 3)
    @test arrow_annot_10354(1, k = 9) == (1, 9)

    # The declared type must be CARRIED and enforced, like the named-function
    # form: a wrong-typed supplied value is upstream's TypeError...
    @test_throws TypeError arrow_annot_10354(1, k = 1.5)
    # ...and a wrong-typed DEFAULT is upstream's MethodError (Issue #11135's
    # rule, now reaching arrows too).
    @test_throws MethodError arrow_bad_annot_default_10354(1)
    # A correct supplied value still bypasses the bad default.
    @test arrow_bad_annot_default_10354(1, k = 7) == (1, 2, 7)

    # A REQUIRED annotated keyword (no default) stays required (Issue #11081).
    @test_throws UndefKeywordError arrow_required_annot_10354(1)
    @test arrow_required_annot_10354(1, k = 4) == (1, 4)
end

@testset "arrow lambda: positional default stays POSITIONAL (Issue #10354)" begin
    # `x = 2` before the `;` is an optional positional default, NOT a keyword:
    # it must be settable by POSITION. Supplying it used to raise NoMethodFound.
    @test arrow_posdefault_10354(1) == (1, 2, 3)
    @test arrow_posdefault_10354(1, 5) == (1, 5, 3)
    @test arrow_posdefault_10354(1, 5, k = 9) == (1, 5, 9)
end

@testset "arrow lambda: keyword splat still works (Issue #10354)" begin
    @test arrow_kwsplat_10354(1, a = 1, b = 2) == (1, 2)
    @test arrow_kwsplat_10354(1) == (1, 0)
end

true

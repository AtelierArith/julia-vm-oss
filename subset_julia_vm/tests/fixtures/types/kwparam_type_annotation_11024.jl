# Issue #11024: KEYWORD-parameter type annotations were dropped at lowering
# (`KwParam::new(name, default, None, span)` in every path), so a declared
# keyword type could neither be validated at definition time (#10582's probes
# read exactly `KwParam.type_annotation`) nor asserted at the call site.
#
# Upstream treats a keyword annotation as an ASSERTION, not a conversion:
#   k(; x::Int64 = 1) = x;  k(x = 2.0)  ->  TypeError
#   h(; x::Real  = 1) = x;  h(x = 2.5)  ->  2.5   (abstract annotation accepts)
# Verified against julia 1.12.6.

using Test

@testset "keyword parameter type annotations (Issue #11024)" begin
    # --- The declared type is honored for accepted values ------------------
    kw_concrete_11024(; x::Int64 = 1) = x
    @test kw_concrete_11024() == 1
    @test kw_concrete_11024(x = 7) == 7

    # --- An abstract annotation constrains nothing concrete ----------------
    kw_abstract_11024(; x::Real = 1) = x
    @test kw_abstract_11024() == 1
    @test kw_abstract_11024(x = 2.5) == 2.5
    @test kw_abstract_11024(x = 3) == 3

    # --- A mistyped supplied value raises TypeError (assertion, not convert)
    @test_throws TypeError kw_concrete_11024(x = 2.0)

    kw_float_11024(; x::Float64 = 1.0) = x
    @test kw_float_11024(x = 2.5) == 2.5
    @test_throws TypeError kw_float_11024(x = 1)

    kw_string_11024(; s::String = "a") = s
    @test kw_string_11024(s = "b") == "b"
    @test_throws TypeError kw_string_11024(s = 1)

    # --- The annotation also applies to a REQUIRED keyword -----------------
    # Issue #11081: an annotated keyword with NO default is REQUIRED. The
    # annotation used to be lowered as the keyword's default expression, so the
    # keyword became optional and `kw_required_11081()` produced `0` instead of
    # upstream's UndefKeywordError.
    kw_required_11024(; x::Int64) = x
    @test kw_required_11024(x = 5) == 5
    @test_throws TypeError kw_required_11024(x = 5.0)
    @test_throws UndefKeywordError kw_required_11024()

    kw_required_str_11081(; s::String) = s
    @test kw_required_str_11081(s = "v") == "v"
    @test_throws UndefKeywordError kw_required_str_11081()

    # An UNannotated required keyword keeps working (control).
    kw_required_bare_11081(; x) = x
    @test kw_required_bare_11081(x = 1) == 1
    @test_throws UndefKeywordError kw_required_bare_11081()

    # --- Mixed positional + annotated keyword ------------------------------
    kw_mixed_11024(a, b = 2; x::Int64 = 3) = (a, b, x)
    @test kw_mixed_11024(1) == (1, 2, 3)
    @test kw_mixed_11024(1, 9, x = 4) == (1, 9, 4)
    @test_throws TypeError kw_mixed_11024(1, x = 4.0)

    # --- An unannotated keyword still accepts any value --------------------
    kw_unannotated_11024(; x = 1) = x
    @test kw_unannotated_11024(x = "any") == "any"
    @test kw_unannotated_11024(x = 2.5) == 2.5

    # --- A kwargs... collector is unaffected -------------------------------
    kw_varargs_11024(; kws...) = length(kws)
    @test kw_varargs_11024(a = 1, b = 2) == 2

    # --- Long-form definitions carry the annotation too ---------------------
    function kw_longform_11024(; x::Int64 = 1)
        return x
    end
    @test kw_longform_11024(x = 6) == 6
    @test_throws TypeError kw_longform_11024(x = 6.0)
end

true

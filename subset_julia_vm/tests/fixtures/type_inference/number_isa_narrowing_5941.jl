using Test

# Issue #5941: a value passed to an abstract-numeric parameter (`x::Number`,
# `x::Real`) is statically represented as `ValueType::F64` in the compiler's
# locals (`type_helpers.rs`: `JuliaType::Number => ValueType::F64`), while an
# Int64-valued `x::Integer` becomes `ValueType::I64`. The compile-time `isa`
# folding (`compile_time_isa_result`) treated that static F64 as an exact
# runtime type and folded `x isa Int64` to a constant `false`, so an Int64
# argument bound to `x::Number` / `x::Real` never narrowed and the guarded
# branch was skipped (returning the fallthrough value).
#
# The runtime value is still an Int64 (`typeof` is correct), so for
# abstract-numeric params `isa` must defer to the runtime check instead of
# folding on the representational static type.

function number_isa_int64_5941(x::Number)
    if x isa Int64
        return x * x
    end
    return -1
end

function real_isa_int64_5941(x::Real)
    if x isa Int64
        return x + 100
    end
    return -1
end

# `x::Integer` already worked (static I64); keep it as a guard against
# regressing the working path.
function integer_isa_int64_5941(x::Integer)
    if x isa Int64
        return x * x
    end
    return -1
end

@testset "isa narrowing on abstract-numeric params (Issue #5941)" begin
    # An Int64 argument must narrow through Number / Real / Integer.
    @test number_isa_int64_5941(6) == 36
    @test real_isa_int64_5941(7) == 107
    @test integer_isa_int64_5941(6) == 36

    # A non-Int64 numeric must NOT match — the guarded branch is skipped.
    @test number_isa_int64_5941(3.5) == -1
    @test real_isa_int64_5941(2.5) == -1
end

true

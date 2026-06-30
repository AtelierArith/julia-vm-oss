# Verify include_string dispatches to Pure Julia methods (Issue #3738)
# - Direct calls used to be intercepted by `BuiltinOp::IncludeString` in
#   lowering/expr/helpers.rs, bypassing the methods in base/meta.jl.
# - After Issue #3738, the public name is no longer in `map_builtin_name()`,
#   so calls fall through to method dispatch and the Pure Julia overload set
#   in base/meta.jl is the authoritative public API.

using Test

@testset "include_string Pure Julia dispatch" begin
    # 2-arg (m, code) overload
    r1 = include_string(Main, "1 + 2")
    @test r1 == 3

    # 3-arg (m, code, filename) overload
    r2 = include_string(Main, "10 * 5", "<dispatch-test>")
    @test r2 == 50

    # Whitespace-only code returns nothing
    r4 = include_string(Main, "   ")
    @test r4 === nothing

    # mapexpr overload (4-arg form (mapexpr, m, code, filename))
    r5 = include_string(identity, Main, "7 + 8", "<mapexpr-test>")
    @test r5 == 15

    # mapexpr overload without filename (3-arg form (mapexpr, m, code))
    r6 = include_string(identity, Main, "9 + 1")
    @test r6 == 10
end

true

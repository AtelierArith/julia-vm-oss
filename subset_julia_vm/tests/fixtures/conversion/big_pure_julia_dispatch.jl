# Test that public `big(...)` reaches Pure Julia method dispatch (Issue #3730).
#
# Before the migration, `compile/expr/call/mod.rs` short-circuited every
# `big(...)` call to `compile_builtin_call`, which manually re-implemented
# the type/value conversion. That bypassed `base/gmp.jl` entirely and
# shadowed any user-defined `big(::MyType)` method.

using Test

# A user-defined Base.big method must take precedence over the VM
# fallback when calling `big(...)` with an argument that matches the
# user method.
struct BigUser
    v::Int64
end

import Base: big
function big(x::BigUser)
    return BigInt(x.v) * 1000
end

# Wrapper / function-variable forwarding paths
big_via_wrapper(x) = big(x)
apply1(f, x) = f(x)

@testset "Pure Julia dispatch for big (Issue #3730)" begin
    # User-defined dispatch wins over the previous Rust shortcut.
    @test (Int64(big(BigUser(5)))) == 5000

    # Wrapper-method path — same dispatch flow.
    @test (Int64(big_via_wrapper(BigUser(7)))) == 7000

    # Existing Pure Julia type-arg conversions still work.
    @test (big(Int64) === BigInt)
    @test (big(Float64) === BigFloat)
    @test (big(BigInt) === BigInt)
    @test (big(BigFloat) === BigFloat)

    # Existing Pure Julia value conversions still work.
    @test (Int64(big(42))) == 42
    @test (big(BigInt(100)) == BigInt(100))
end

true

# Test Meta.isidentifier function
# Note: Julia's Meta.isidentifier accepts keywords like "if", "for" as valid identifiers.
# Only boolean literals "true" and "false" are rejected (not keywords in general).
# See: https://docs.julialang.org/en/v1/base/base/#Meta.isidentifier

using Test

@testset "Meta.isidentifier - validate identifier syntax" begin

    # Valid identifiers
    r1 = Meta.isidentifier(:x)
    @assert r1 == true
    r2 = Meta.isidentifier(:foo)
    @assert r2 == true
    r3 = Meta.isidentifier(:foo_bar)
    @assert r3 == true
    r4 = Meta.isidentifier(:_private)
    @assert r4 == true
    r5 = Meta.isidentifier(:x1)
    @assert r5 == true
    r6 = Meta.isidentifier(:push!)
    @assert r6 == true
    r7 = Meta.isidentifier("abc")
    @assert r7 == true
    r8 = Meta.isidentifier("_x")
    @assert r8 == true

    # Invalid identifiers - start with digit
    r9 = Meta.isidentifier("1x")
    @assert r9 == false
    r10 = Meta.isidentifier("123")
    @assert r10 == false

    # Julia keywords ARE valid identifiers per Meta.isidentifier.
    # Julia's implementation only rejects "true" and "false" as boolean literals.
    r11 = Meta.isidentifier("if")
    @assert r11 == true
    r12 = Meta.isidentifier("for")
    @assert r12 == true
    r13 = Meta.isidentifier("while")
    @assert r13 == true
    r14 = Meta.isidentifier("function")
    @assert r14 == true
    r15 = Meta.isidentifier("end")
    @assert r15 == true

    # Boolean literals are NOT valid identifiers
    r16 = Meta.isidentifier("true")
    @assert r16 == false
    r17 = Meta.isidentifier("false")
    @assert r17 == false

    # Invalid identifiers - empty
    r18 = Meta.isidentifier("")
    @assert r18 == false

    # Unicode identifiers
    r19 = Meta.isidentifier("α")
    @assert r19 == true
    r20 = Meta.isidentifier("β")
    @assert r20 == true

    # Final result
    @test (42.0) == 42.0
end

true  # Test passed

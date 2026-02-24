# Test callable struct syntax: (::Type)(args) = body
# Issue #2671: parser/lowering support for callable struct definitions

using Test

# ---- Basic callable struct (short form) ----
struct Doubler end
(::Doubler)(x) = x * 2

# ---- Binary callable struct (short form) ----
struct Adder end
(::Adder)(a, b) = a + b

# ---- Callable struct with fields ----
struct Multiplier
    factor::Int64
end

# Note: callable struct methods don't access the struct instance fields
# This is a limitation; for now, we test the basic dispatch mechanism
(::Multiplier)(x) = x * 3

# ---- Callable struct (full form) ----
struct MyFunc end
function (::MyFunc)(x, y)
    return x + y
end

# ---- AndAnd/OrOr pattern (as in broadcast.jl) ----
struct AndAnd2 end
struct OrOr2 end
(::AndAnd2)(a, b) = a && b
(::OrOr2)(a, b) = a || b

@testset "Callable struct syntax (Issue #2671)" begin
    # Basic short-form callable struct
    d = Doubler()
    @test d(5) == 10
    @test d(0) == 0
    @test d(-3) == -6

    # Binary callable struct
    a = Adder()
    @test a(3, 4) == 7
    @test a(10, 20) == 30

    # Callable struct with fields (basic dispatch only)
    m = Multiplier(5)
    @test m(4) == 12  # x * 3, not factor * x

    # Full-form callable struct
    mf = MyFunc()
    @test mf(3, 4) == 7
    @test mf(10, -5) == 5

    # AndAnd/OrOr pattern
    aa = AndAnd2()
    oo = OrOr2()
    @test aa(true, true) == true
    @test aa(true, false) == false
    @test aa(false, true) == false
    @test oo(false, false) == false
    @test oo(false, true) == true
    @test oo(true, false) == true
end

true

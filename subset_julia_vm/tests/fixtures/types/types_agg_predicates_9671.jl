# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: types/isa_infix.jl =====
# Test: isa infix syntax (a isa T => isa(a, T))
# Tests that the infix form of isa works correctly


@testset "isa infix syntax: a isa T => isa(a, T)" begin

    x = 1
    @assert x isa Int64
    @assert x isa Integer
    @assert !(x isa Float64)

    y = 1.5
    @assert y isa Float64
    @assert y isa Real
    @assert !(y isa Int64)

    s = "hello"
    @assert s isa String
    @assert s isa AbstractString

    # Test with negation
    @assert !(1 isa Float64)
    @assert !(1.0 isa Int64)

    @test (42) == 42.0
end

# ===== source: types/isbits_basic.jl =====
# Test isbits and isbitstype functions


@testset "isbits/isbitstype - check if type is bits type" begin

    # isbits returns true for primitive values
    @assert isbits(42)
    @assert isbits(3.14)
    @assert isbits(true)
    @assert isbits('a')
    @assert isbits(nothing)
    @assert isbits(missing)
    @assert isbits(UInt8(1))
    @assert isbits(Float16(1))

    # isbits returns false for non-bits values
    @assert !isbits("hello")
    @assert !isbits([1, 2, 3])

    # isbitstype returns true for primitive types
    @assert isbitstype(Int64)
    @assert isbitstype(Float64)
    @assert isbitstype(Bool)
    @assert isbitstype(Char)
    @assert isbitstype(Nothing)
    @assert isbitstype(Missing)
    @assert isbitstype(UInt8)
    @assert isbitstype(Int128)
    @assert isbitstype(Float16)

    # isbitstype returns false for non-bits types
    @assert !isbitstype(String)
    @assert !isbitstype(Array)
    @assert !isbitstype(Symbol)
    @assert !isbitstype(BigInt)
    @assert !isbitstype(BigFloat)

    @test (true)
end

# ===== source: types/test_datatype_equality.jl =====
# Test equality comparison for DataType values


@testset "DataType equality" begin
    # Same types should be equal
    @test Int64 == Int64
    @test Float64 == Float64
    @test String == String
    @test Bool == Bool

    # Different types should not be equal
    @test Int64 != Float64
    @test Int64 != Int32
    @test String != Symbol
    @test Bool != Int64

    # Type stored in variable
    T = Int64
    @test T == Int64
    @test T != Float64

    # Array and abstract types
    @test Array == Array
    @test Number == Number
    @test Integer == Integer
end

# ===== source: types/test_isequal_pure_julia.jl =====

@testset "isequal Pure Julia (Issue #2718)" begin
    # === Float64 specialization: uses === (bit identity) ===
    # NaN === NaN is true
    @test isequal(NaN, NaN) == true
    # -0.0 === 0.0 is false (different bit patterns)
    @test isequal(-0.0, 0.0) == false
    @test isequal(0.0, -0.0) == false
    # Normal float equality
    @test isequal(1.0, 1.0) == true
    @test isequal(1.0, 2.0) == false
    # Inf
    @test isequal(Inf, Inf) == true
    @test isequal(-Inf, -Inf) == true
    @test isequal(Inf, -Inf) == false

    # === Cross-type Int64/Float64 ===
    @test isequal(1, 1.0) == true
    @test isequal(1.0, 1) == true
    @test isequal(0, -0.0) == false
    @test isequal(-0.0, 0) == false
    @test isequal(2, 3.0) == false

    # === Int64 equality ===
    @test isequal(42, 42) == true
    @test isequal(42, 43) == false

    # === String equality ===
    @test isequal("hello", "hello") == true
    @test isequal("hello", "world") == false

    # === Char equality ===
    @test isequal('a', 'a') == true
    @test isequal('a', 'b') == false

    # === Nothing equality ===
    @test isequal(nothing, nothing) == true

    # === Missing specializations ===
    @test isequal(missing, missing) == true
    @test isequal(missing, 1) == false
    @test isequal(1, missing) == false
    @test isequal(missing, NaN) == false
    @test isequal(NaN, missing) == false

    # === Array specialization: element-wise with shape check ===
    @test isequal([1, 2, 3], [1, 2, 3]) == true
    @test isequal([1, 2, 3], [1, 2, 4]) == false
    @test isequal([1, 2], [1, 2, 3]) == false
    # NaN in arrays
    @test isequal([NaN, 1.0], [NaN, 1.0]) == true
    @test isequal([NaN, 1.0], [NaN, 2.0]) == false
    # -0.0 in arrays
    @test isequal([0.0], [-0.0]) == false

    # === Tuple specialization: element-wise ===
    @test isequal((1, 2), (1, 2)) == true
    @test isequal((1, 2), (1, 3)) == false
    @test isequal((1, 2), (1, 2, 3)) == false
    # NaN in tuples
    @test isequal((NaN,), (NaN,)) == true
    @test isequal((1, NaN), (1, NaN)) == true
    @test isequal((1, NaN), (2, NaN)) == false

    # === Bool equality ===
    @test isequal(true, true) == true
    @test isequal(true, false) == false
end

# ===== source: types/test_isunordered_pure_julia.jl =====

@testset "isunordered Pure Julia (Issue #2715)" begin
    # === NaN is unordered ===
    @test isunordered(NaN) == true

    # === Missing is unordered ===
    @test isunordered(missing) == true

    # === Normal values are ordered ===
    @test isunordered(1) == false
    @test isunordered(3.14) == false
    @test isunordered(0.0) == false
    @test isunordered(-0.0) == false
    @test isunordered(Inf) == false
    @test isunordered(-Inf) == false
    @test isunordered("hello") == false
    @test isunordered('a') == false
    @test isunordered(true) == false
    @test isunordered(false) == false
    @test isunordered(nothing) == false
end

# ===== source: types/types_isabstracttype.jl =====
# Test isabstracttype function


@testset "isabstracttype - check if type is abstract" begin

    # Abstract types return true
    @assert isabstracttype(Number)
    @assert isabstracttype(Real)
    @assert isabstracttype(Integer)
    @assert isabstracttype(Signed)
    @assert isabstracttype(Unsigned)
    @assert isabstracttype(AbstractFloat)
    @assert isabstracttype(AbstractString)
    @assert isabstracttype(AbstractVector)
    @assert isabstracttype(Function)
    @assert isabstracttype(IO)
    @assert isabstracttype(Type)
    @assert isabstracttype(Type{Int64})
    @assert isabstracttype(AbstractVector{Int64})
    @assert isabstracttype(Any)

    # Concrete types return false
    @assert !isabstracttype(Int64)
    @assert !isabstracttype(Float64)
    @assert !isabstracttype(Bool)
    @assert !isabstracttype(String)
    @assert !isabstracttype(DataType)
    @assert !isabstracttype(Tuple)
    @assert !isabstracttype(Vector)
    @assert !isabstracttype(Union{Int64, Float64})

    @test (true)
end

# ===== source: types/types_isconcretetype.jl =====
# Test isconcretetype function


@testset "isconcretetype - check if type is concrete" begin

    # Concrete types (can have instances)
    @assert isconcretetype(Int64)
    @assert isconcretetype(Float64)
    @assert isconcretetype(Bool)
    @assert isconcretetype(Char)
    @assert isconcretetype(String)
    @assert isconcretetype(Nothing)
    @assert isconcretetype(Missing)
    @assert isconcretetype(BigInt)
    @assert isconcretetype(BigFloat)
    @assert isconcretetype(Symbol)
    @assert isconcretetype(DataType)
    @assert isconcretetype(Complex{Float64})
    @assert isconcretetype(Rational{Int64})
    @assert isconcretetype(Vector{Int64})
    @assert isconcretetype(Dict{String, Int64})
    @assert isconcretetype(Set{Int64})
    @assert isconcretetype(UnitRange{Int64})
    @assert isconcretetype(Expr)
    @assert isconcretetype(QuoteNode)
    @assert isconcretetype(LineNumberNode)
    @assert isconcretetype(GlobalRef)
    @assert isconcretetype(Module)

    # Abstract types (cannot have instances directly)
    @assert !isconcretetype(Integer)
    @assert !isconcretetype(Real)
    @assert !isconcretetype(Number)
    @assert !isconcretetype(Any)
    @assert !isconcretetype(AbstractFloat)
    @assert !isconcretetype(Type)
    @assert !isconcretetype(Type{Int64})
    @assert !isconcretetype(Tuple)
    @assert !isconcretetype(Vector)
    @assert !isconcretetype(Union{Int64, Float64})

    @test (true)
end

# ===== source: types/types_ismutable.jl =====
# Test ismutable function


@testset "ismutable - check if value is mutable" begin

    # Mutable values
    arr = [1, 2, 3]
    @assert ismutable(arr)

    # Primitives are immutable
    @assert !ismutable(42)
    @assert !ismutable(3.14)

    @test (true)
end

# ===== source: types/types_ismutabletype.jl =====
# Test ismutabletype function


@testset "ismutabletype - check if type is mutable" begin

    # Mutable built-in types
    @assert ismutabletype(Array)
    @assert ismutabletype(Vector)
    @assert ismutabletype(Vector{Int64})
    @assert ismutabletype(Matrix)
    @assert ismutabletype(Dict)
    @assert ismutabletype(Dict{String, Int64})
    @assert ismutabletype(String)
    @assert ismutabletype(Symbol)
    @assert ismutabletype(BigInt)
    @assert ismutabletype(DataType)
    @assert ismutabletype(IOBuffer)
    @assert ismutabletype(Expr)
    @assert ismutabletype(Module)

    # Immutable or non-mutable built-in types
    @assert !ismutabletype(Int64)
    @assert !ismutabletype(Float64)
    @assert !ismutabletype(Bool)
    @assert !ismutabletype(Char)
    @assert !ismutabletype(BigFloat)
    @assert !ismutabletype(Nothing)
    @assert !ismutabletype(Missing)
    @assert !ismutabletype(Set)
    @assert !ismutabletype(Set{Int64})
    @assert !ismutabletype(Tuple)
    @assert !ismutabletype(Tuple{Int64, String})
    @assert !ismutabletype(Complex{Float64})
    @assert !ismutabletype(Rational{Int64})
    @assert !ismutabletype(Union{Int64, Float64})
    @assert !ismutabletype(QuoteNode)
    @assert !ismutabletype(LineNumberNode)
    @assert !ismutabletype(GlobalRef)

    @test true
end

# ===== source: types/types_isunordered.jl =====
# Test isunordered function


@testset "isunordered - check if value is unordered (NaN, Missing)" begin

    # NaN is unordered
    @assert isunordered(NaN)

    # Missing is unordered
    @assert isunordered(missing)

    # Regular values are ordered
    @assert !isunordered(1)
    @assert !isunordered(3.14)
    @assert !isunordered(0.0)
    @assert !isunordered(Inf)
    @assert !isunordered(-Inf)

    @test (true)
end

# ===== source: types/types_objectid.jl =====
# Test objectid function


@testset "objectid - get unique object identifier" begin

    # objectid returns UInt
    @assert typeof(objectid(1)) == UInt64
    @assert typeof(objectid(3.14)) == UInt64
    @assert typeof(objectid("hello")) == UInt64
    @assert typeof(objectid(nothing)) == UInt64
    @assert typeof(objectid(missing)) == UInt64

    # objectid works on various types
    @assert objectid([1, 2, 3]) isa UInt64
    @assert objectid((1, 2)) isa UInt64

    @test (true)
end

true

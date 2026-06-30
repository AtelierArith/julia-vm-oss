using Test

# Issue #7940: Dict operations keyed by a generic `DataType` value (a `where`
# type parameter) previously failed to compile with
# "Cannot convert DataType to I64", because the key was coerced to an integer
# array index instead of routing through the Dict get/set path.

# The exact reported MWE: a module-level const Dict indexed by a generic type
# parameter must COMPILE.
module P7940
const D = Dict()
function f(::Type{T}) where T
    if !haskey(D, T)
        D[T] = Dict()
    end
    return D[T]
end
end

# Runtime exercise of the same shape with the Dict passed as an argument so the
# generic `where T` parameter is used as a Dict key for haskey / write / read.
function get_or_create!(store, ::Type{T}) where T
    if !haskey(store, T)
        store[T] = Dict()
    end
    return store[T]
end

@testset "Issue #7940: Dict ops with generic DataType keys" begin
    # the reported MWE compiled (defining the module did not error)
    @test isa(P7940, Module)
    @test isdefined(P7940, :f)

    # generic `where T` DataType key: haskey + write + read on a Dict argument
    store = Dict{Type, Any}()
    a = get_or_create!(store, Int)
    @test a isa Dict
    @test haskey(store, Int)
    @test get_or_create!(store, Int) === a      # existing entry returns same dict
    b = get_or_create!(store, Float64)
    @test b !== a
    @test length(store) == 2

    # top-level untyped Dict with concrete DataType keys; scalar values keep
    # their type (no Int -> Float64 coercion through the array store path)
    d = Dict()
    d[Int] = 1
    d[Float64] = 2
    @test d[Int] === 1
    @test d[Float64] === 2

    # typed Dict{Type, Any}
    e = Dict{Type, Any}()
    e[Int] = 10
    @test e[Int] === 10
    @test e isa Dict{Type, Any}
end

true

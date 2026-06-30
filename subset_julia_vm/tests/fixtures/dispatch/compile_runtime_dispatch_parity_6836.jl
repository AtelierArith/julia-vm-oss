using Test

# Issue #6836: the compile-time dispatcher (statically-typed call sites,
# resolved to `CallResolved` / typed dispatch when argument types are known at
# compile time) and the runtime dispatcher (`Vm::find_best_method_index`, used
# when an argument's type is only known at run time — e.g. when it flows through
# an `Any` container) must select the SAME method for identical inputs. Both
# sides route through the shared `inference_core` selection core; this fixture
# pins that contract end to end.
#
# Each scenario calls a method once with a statically-typed argument (compile
# path) and once with the same value pulled from an `Any` container (runtime
# path). A divergence in method selection between the two paths would make a
# pair of results disagree and fail a `@test`.

kind(::Int64) = :int
kind(::Float64) = :float
kind(::String) = :string
kind(::Bool) = :bool
kind(::Number) = :number        # abstract-supertype fallback (e.g. Rational)

combine(::Int64, ::Int64) = :ii
combine(::Int64, ::Float64) = :if_
combine(::Number, ::Number) = :nn

elt(::Vector{Int64}) = :vint
elt(::Vector{Float64}) = :vfloat
elt(::AbstractVector) = :vabstract

@testset "single-arg concrete + abstract dispatch parity" begin
    box = Any[7, 3.5, "hi", true, 1//2]
    # static (typed literal) vs dynamic (Any-container element) must agree.
    @test kind(7) === kind(box[1]) === :int
    @test kind(3.5) === kind(box[2]) === :float
    @test kind("hi") === kind(box[3]) === :string
    @test kind(true) === kind(box[4]) === :bool
    @test kind(1//2) === kind(box[5]) === :number
end

@testset "multi-arg dispatch parity" begin
    box = Any[3, 4, 2.0, 1//1]
    @test combine(3, 4) === combine(box[1], box[2]) === :ii
    @test combine(3, 2.0) === combine(box[1], box[3]) === :if_
    @test combine(1//1, 1//1) === combine(box[4], box[4]) === :nn
end

@testset "parametric container dispatch parity" begin
    vi = Int64[1, 2]
    vf = Float64[1.0, 2.0]
    box = Any[vi, vf, 1:3]
    @test elt(vi) === elt(box[1]) === :vint
    @test elt(vf) === elt(box[2]) === :vfloat
    @test elt(1:3) === elt(box[3]) === :vabstract
end

true

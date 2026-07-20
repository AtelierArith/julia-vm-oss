# Abstract numeric field annotations (Number/Real/Integer/AbstractFloat)
# preserve the original runtime value: `1 isa Number` means no conversion
# happens on construction, so the stored value stays Int64 — it must not be
# forced through a concrete F64/I64 VM representation on reads, prints, or
# function-boundary loads (Issue #11407, tech-debt #11447).
using Test

struct NumberField11407
    x::Number
end

struct RealField11407
    x::Real
end

struct IntegerField11407
    x::Integer
end

struct FloatField11407
    x::AbstractFloat
end

mutable struct MutNumberField11407
    x::Number
end

read_x(w) = w.x

@testset "abstract numeric fields preserve the runtime value (Issue #11407)" begin
    v = NumberField11407(1)
    @test v.x === 1
    @test v.x isa Int64
    @test typeof(v.x) == Int64
    @test string(v.x) == "1"
    @test repr(v) == "NumberField11407(1)"
    # Function-boundary reads must not reinterpret through a concrete tag.
    @test read_x(v) === 1
    @test typeof(read_x(v)) == Int64

    # Floats stay floats.
    vf = NumberField11407(2.5)
    @test vf.x === 2.5
    @test read_x(vf) === 2.5

    # Complex satisfies Number without conversion.
    vc = NumberField11407(3 + 4im)
    @test vc.x == 3 + 4im

    r = RealField11407(7)
    @test r.x === 7 && typeof(read_x(r)) == Int64

    i = IntegerField11407(Int8(5))
    @test i.x === Int8(5)
    @test typeof(read_x(i)) == Int8
    ib = IntegerField11407(big(6))
    @test ib.x == big(6)
    @test read_x(ib) isa BigInt

    f = FloatField11407(Float32(1.5))
    @test f.x === Float32(1.5)
    @test typeof(read_x(f)) == Float32

    m = MutNumberField11407(1)
    @test m.x === 1
    m.x = 2.5
    @test m.x === 2.5
    m.x = Int8(3)
    @test m.x === Int8(3)
end

true

# Issue #6744 (#6729-2): the array generation functions zeros / ones / similar /
# reshape dispatch-first to their pure-Julia methods (base/array.jl). zeros/ones
# became pure-Julia allocation dispatch in Issue #4036 (BuiltinOp::Zeros/Ones are
# "dead but kept"); similar/reshape are pure-Julia `where {T,N}` methods. The
# legacy Rust array-creation builtins (Zeros/ZerosF64/ZerosI64/Ones/OnesF64/
# OnesI64) only ever allocate Float64 or Int64, so producing arrays of *other*
# element types proves the generic pure-Julia `zeros(::Type{T}, ...)` path is what
# runs (not the Rust fallback). Verified vs julia 1.12.

using Test

@testset "zeros/ones element type beyond F64/I64 → pure-Julia dispatch (Issue #6744)" begin
    # The Rust builtins can only make Float64/Int64 arrays; these types prove the
    # generic pure-Julia method (via _array_undef_from_dims + fill!) is used.
    @test zeros(Float32, 3) == Float32[0, 0, 0]
    @test eltype(zeros(Float32, 3)) === Float32
    @test zeros(Int32, 2, 2) == Int32[0 0; 0 0]
    @test eltype(zeros(Int32, 2, 2)) === Int32
    # NB: compared against `[0.0+0.0im, 0.0+0.0im]`, not `ComplexF64[0, 0]` —
    # the latter literal mis-stores Int elements in sjulia (tracked by #6771);
    # the zeros() result itself is correct ComplexF64.
    @test zeros(Complex{Float64}, 2) == [0.0 + 0.0im, 0.0 + 0.0im]
    @test eltype(zeros(Complex{Float64}, 2)) === ComplexF64
    @test ones(Float32, 3) == Float32[1, 1, 1]
    @test eltype(ones(Float32, 3)) === Float32
    @test ones(Int32, 2) == Int32[1, 1]
    @test eltype(ones(Int32, 2)) === Int32
end

@testset "zeros/ones defaults and basic forms (Issue #6744)" begin
    @test zeros(3) == [0.0, 0.0, 0.0]
    @test eltype(zeros(3)) === Float64        # default element type
    @test zeros(2, 3) == [0.0 0.0 0.0; 0.0 0.0 0.0]
    @test zeros(Int64, 2) == [0, 0]
    @test eltype(zeros(Int64, 2)) === Int64
    @test ones(2, 2) == [1.0 1.0; 1.0 1.0]
    @test ones(Int64, 3) == [1, 1, 1]
    @test zeros((2, 2)) == [0.0 0.0; 0.0 0.0]  # tuple-dims form
end

@testset "similar dispatch-first (Issue #6744)" begin
    a = [1 2 3; 4 5 6]
    @test size(similar(a)) == (2, 3)
    @test eltype(similar(a)) === Int64
    @test eltype(similar(a, Float64)) === Float64
    @test size(similar(a, (3, 2))) == (3, 2)
    @test size(similar(a, Float32, 4)) == (4,)
    @test eltype(similar(a, Float32, 4)) === Float32
end

@testset "reshape dispatch-first (Issue #6744)" begin
    a = [1 2 3; 4 5 6]
    @test size(reshape(a, 3, 2)) == (3, 2)
    @test reshape(a, 3, 2) == [1 5; 4 3; 2 6]   # column-major
    @test size(reshape(a, (6,))) == (6,)
    @test collect(reshape(1:6, 2, 3)) == [1 3 5; 2 4 6]
end

true

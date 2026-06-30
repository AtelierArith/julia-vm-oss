# Issue #6743 (#6729-1): the array query functions length / size / ndims /
# eltype dispatch-first to their pure-Julia methods (base/array.jl). The Rust
# builtins remain only as the no-method fallback for internal carriers, so a
# user-defined length/size/eltype method is NOT shadowed. Verified vs julia 1.12.

using Test

@testset "built-in length/size/ndims/eltype (Issue #6743)" begin
    a = [1 2 3; 4 5 6]
    @test length(a) == 6
    @test size(a) == (2, 3)
    @test size(a, 1) == 2
    @test ndims(a) == 2
    @test eltype(a) === Int64
    @test ndims([1, 2, 3]) == 1
    @test eltype([1.0, 2.0]) === Float64
    @test eltype(Float32[1, 2]) === Float32
    @test length("héllo") == 5     # character count
end

struct MyColl
    data::Vector{Int}
end
import Base: length, size, eltype, ndims
length(c::MyColl) = length(c.data)
size(c::MyColl) = (length(c.data),)
eltype(::Type{MyColl}) = Int
ndims(::MyColl) = 1

@testset "user-defined query methods are dispatch-first (Issue #6743)" begin
    c = MyColl([10, 20, 30, 40])
    @test length(c) == 4
    @test size(c) == (4,)
    @test eltype(MyColl) === Int
    @test ndims(c) == 1
    # works through a higher-order function too
    @test map(length, [MyColl([1]), MyColl([1, 2, 3])]) == [1, 3]
end

true

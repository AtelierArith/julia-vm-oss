# Test: Ref construction and dereference (Issue #5130)

using Test

@testset "Ref construct / deref / typeof" begin
    r = Ref(5)
    @assert r[] == 5
    @assert typeof(r) === Base.RefValue{Int}
    @assert typeof(r) === Base.RefValue{Int64}

    # Element types are preserved
    rf = Ref(3.5)
    @assert rf[] == 3.5
    @assert typeof(rf) === Base.RefValue{Float64}

    rs = Ref("hello")
    @assert rs[] == "hello"
    @assert typeof(rs) === Base.RefValue{String}

    # getindex(r) is equivalent to r[]
    @assert getindex(r) == r[]

    @test (true)
end

true  # Test passed

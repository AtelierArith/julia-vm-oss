using Test

@testset "runtime Tuple subtype CoreType gate (Issue #5615)" begin
    @test Tuple{Int64,String} <: Tuple{Real,Any}
    @test !(Tuple{String} <: Tuple{Real})
    @test Tuple{Int64,Float64} <: Tuple{Integer,Real}
    @test !(Tuple{Int64,String} <: Tuple{Integer,Real})

    @test Tuple{} <: Tuple
    @test Tuple{Int64} <: Tuple
    @test !(Tuple{Int64} <: Tuple{String})
end

true

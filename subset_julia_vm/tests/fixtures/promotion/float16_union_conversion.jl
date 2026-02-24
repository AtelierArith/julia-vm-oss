# Test Union to Float16 type conversion (Issue #1851)

using Test

@testset "Union to Float16 via if/else" begin
    x = true
    y = if x
        Float16(1.0)
    else
        Float16(0.0)
    end
    @test y == Float16(1.0)
    @test typeof(y) == Float16
end

@testset "Union to Float16 via if/else false branch" begin
    x = false
    y = if x
        Float16(1.0)
    else
        Float16(0.0)
    end
    @test y == Float16(0.0)
    @test typeof(y) == Float16
end

true

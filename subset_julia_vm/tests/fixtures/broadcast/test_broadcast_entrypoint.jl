using Test

# Test broadcast / broadcast! Pure Julia entry points (Issue #2548)
# These tests exercise broadcast()/broadcast!() which call
# broadcasted() -> materialize() internally (the Pure Julia pipeline).

double(x) = x * 2

# --- broadcast() entry point tests ---

@testset "broadcast: unary function on array" begin
    result = broadcast(double, [1, 2, 3])
    @test result[1] == 2
    @test result[2] == 4
    @test result[3] == 6
end

@testset "broadcast: binary array-array" begin
    result = broadcast(+, [1, 2, 3], [4, 5, 6])
    @test result[1] == 5
    @test result[2] == 7
    @test result[3] == 9
end

@testset "broadcast: scalar + array" begin
    result = broadcast(+, 10, [1, 2, 3])
    @test result[1] == 11
    @test result[2] == 12
    @test result[3] == 13
end

@testset "broadcast: array + scalar" begin
    result = broadcast(*, [10, 20, 30], 2)
    @test result[1] == 20
    @test result[2] == 40
    @test result[3] == 60
end

@testset "broadcast: scalar-only optimization" begin
    # broadcast(f, a::Number, b::Number) should skip Broadcasted pipeline
    @test broadcast(+, 3, 4) == 7
    @test broadcast(*, 5, 6) == 30
end

@testset "broadcast: unary scalar optimization" begin
    # broadcast(f, a::Number) should skip Broadcasted pipeline
    @test broadcast(abs, -5) == 5
    @test broadcast(double, 7) == 14
end

# --- broadcast!() entry point tests ---

@testset "broadcast!: unary in-place" begin
    dest = zeros(3)
    broadcast!(double, dest, [1, 2, 3])
    @test dest[1] == 2.0
    @test dest[2] == 4.0
    @test dest[3] == 6.0
end

@testset "broadcast!: binary in-place" begin
    dest = zeros(3)
    broadcast!(+, dest, [1, 2, 3], [4, 5, 6])
    @test dest[1] == 5.0
    @test dest[2] == 7.0
    @test dest[3] == 9.0
end

@testset "broadcast!: scalar broadcast in-place" begin
    dest = zeros(Int64, 3)
    broadcast!(*, dest, 10, [1, 2, 3])
    @test dest[1] == 10
    @test dest[2] == 20
    @test dest[3] == 30
end

# --- broadcasted() + materialize() pipeline (kept from original) ---

@testset "broadcasted + materialize: array-array" begin
    bc = broadcasted(+, [1, 2, 3], [4, 5, 6])
    @test isa(bc, Broadcasted)
    result = materialize(bc)
    @test result[1] == 5
    @test result[2] == 7
    @test result[3] == 9
end

@testset "broadcasted + materialize!: in-place" begin
    dest = zeros(3)
    bc = broadcasted(+, [1, 2, 3], [4, 5, 6])
    materialize!(dest, bc)
    @test dest[1] == 5.0
    @test dest[2] == 7.0
    @test dest[3] == 9.0
end

true

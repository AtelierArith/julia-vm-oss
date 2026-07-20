using Test

f32_body_9789() = Float32(1)
string_body_9789() = "s"
convert_any_9789(x) = convert(Any, x)
cond_string_9789 = false
conditional_string_body_9789() = cond_string_9789 ? 1 : "s"
numeric_body_9789(i) = i == 1 ? 1 : 2.5
mixed_body_9789(i) = i == 1 ? 1 : "s"

@testset "Empty runtime-typejoined collection eltype defaults (Issues #9789/#9796)" begin
    @test typeof([f32_body_9789() for _ in 1:0]) === Vector{Float32}
    @test typeof(collect(f32_body_9789() for _ in 1:0)) === Vector{Float32}
    @test typeof([f32_body_9789() for _ in 1:2]) === Vector{Float32}
    @test typeof(collect(f32_body_9789() for _ in 1:2)) === Vector{Float32}

    @test typeof([string_body_9789() for _ in 1:0]) === Vector{String}
    @test typeof(collect(string_body_9789() for _ in 1:0)) === Vector{String}
    @test typeof([string_body_9789() for _ in 1:2]) === Vector{String}
    @test typeof(collect(string_body_9789() for _ in 1:2)) === Vector{String}

    @test typeof([convert_any_9789(x) for x in 1:0]) === Vector{Int64}
    @test typeof(collect(convert_any_9789(x) for x in 1:0)) === Vector{Int64}
    @test typeof([convert_any_9789(x) for x in 1:2]) === Vector{Int64}
    @test typeof(collect(convert_any_9789(x) for x in 1:2)) === Vector{Int64}

    @test typeof([conditional_string_body_9789() for _ in 1:0]) === Vector{Any}
    @test typeof(collect(conditional_string_body_9789() for _ in 1:0)) === Vector{Any}
    @test typeof([conditional_string_body_9789() for _ in 1:2]) === Vector{String}
    @test typeof(collect(conditional_string_body_9789() for _ in 1:2)) === Vector{String}

    @test typeof([numeric_body_9789(i) for i in 1:0]) === Vector{Real}
    @test typeof(collect(numeric_body_9789(i) for i in 1:0)) === Vector{Real}
    numeric_values = [numeric_body_9789(i) for i in 1:2]
    numeric_collected = collect(numeric_body_9789(i) for i in 1:2)
    @test typeof(numeric_values) === Vector{Real}
    @test typeof(numeric_collected) === Vector{Real}
    @test numeric_values == numeric_collected == Real[1, 2.5]

    @test typeof([mixed_body_9789(i) for i in 1:0]) === Vector{Any}
    @test typeof(collect(mixed_body_9789(i) for i in 1:0)) === Vector{Any}
    mixed_values = [mixed_body_9789(i) for i in 1:2]
    mixed_collected = collect(mixed_body_9789(i) for i in 1:2)
    @test typeof(mixed_values) === Vector{Any}
    @test typeof(mixed_collected) === Vector{Any}
    @test mixed_values == mixed_collected == Any[1, "s"]
end

true

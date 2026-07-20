using Test

f32_body_9385() = Float32(1)
string_body_9385() = "s"
convert_any_9385(x) = convert(Any, x)
cond_string_9385 = false
conditional_string_body_9385() = cond_string_9385 ? 1 : "s"
numeric_body_9385(i) = i == 1 ? 1 : 2.5
mixed_body_9385(i) = i == 1 ? 1 : "s"

@testset "Any-inferred comprehension body narrows from runtime values (Issue #9385)" begin
    f32_values = [f32_body_9385() for _ in 1:2]
    @test typeof(f32_values) === Vector{Float32}
    @test f32_values == Float32[1, 1]

    string_values = [string_body_9385() for _ in 1:2]
    @test typeof(string_values) === Vector{String}
    @test string_values == ["s", "s"]

    converted_values = [convert_any_9385(x) for x in 1:2]
    @test typeof(converted_values) === Vector{Int64}
    @test converted_values == [1, 2]

    conditional_strings = [conditional_string_body_9385() for _ in 1:2]
    @test typeof(conditional_strings) === Vector{String}
    @test conditional_strings == ["s", "s"]

    numeric_values = [numeric_body_9385(i) for i in 1:2]
    @test typeof(numeric_values) === Vector{Real}
    @test numeric_values[1] === 1
    @test numeric_values[2] === 2.5

    mixed_values = [mixed_body_9385(i) for i in 1:2]
    @test typeof(mixed_values) === Vector{Any}
    @test mixed_values[1] === 1
    @test mixed_values[2] == "s"
end

true

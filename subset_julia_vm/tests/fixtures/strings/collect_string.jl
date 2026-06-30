# collect(string) - collect string into character array (Issue #2027)

using Test

collect_runtime_any_string(x) = collect(x)
collect_trait_string(x) = Base._collect(1:1, x, Base.HasEltype(), Base.HasLength())

@testset "collect(string) into Char array (Issue #2027)" begin
    @test eltype("abc") === Char
    @test eltype(String) === Char

    # Basic string collection
    result = collect("abc")
    @test typeof(result) === Vector{Char}
    @test eltype(result) === Char
    @test length(result) == 3
    @test result[1] == 'a'
    @test result[2] == 'b'
    @test result[3] == 'c'

    runtime_result = collect_runtime_any_string("abc")
    @test typeof(runtime_result) === Vector{Char}
    @test eltype(runtime_result) === Char
    @test length(runtime_result) == 3
    @test runtime_result[1] == 'a'
    @test runtime_result[2] == 'b'
    @test runtime_result[3] == 'c'

    # Longer string
    result2 = collect("hello")
    @test typeof(result2) === Vector{Char}
    @test eltype(result2) === Char
    @test length(result2) == 5
    @test result2[1] == 'h'
    @test result2[5] == 'o'

    # Single character string
    result3 = collect("x")
    @test typeof(result3) === Vector{Char}
    @test eltype(result3) === Char
    @test length(result3) == 1
    @test result3[1] == 'x'

    # Empty string
    result4 = collect("")
    @test typeof(result4) === Vector{Char}
    @test eltype(result4) === Char
    @test length(result4) == 0

    runtime_empty = collect_runtime_any_string("")
    @test typeof(runtime_empty) === Vector{Char}
    @test eltype(runtime_empty) === Char
    @test length(runtime_empty) == 0

    # String with spaces
    result5 = collect("a b")
    @test typeof(result5) === Vector{Char}
    @test eltype(result5) === Char
    @test length(result5) == 3
    @test result5[1] == 'a'
    @test result5[2] == ' '
    @test result5[3] == 'b'

    # Multibyte Unicode iteration is character-based, not byte-based.
    result6 = collect("éβ")
    @test typeof(result6) === Vector{Char}
    @test eltype(result6) === Char
    @test length(result6) == 2
    @test result6[1] == 'é'
    @test result6[2] == 'β'
end

@testset "_collect HasEltype string trait path (Issue #4062)" begin
    result = Base._collect(1:1, "abc", Base.HasEltype(), Base.HasLength())
    @test typeof(result) === Vector{Char}
    @test eltype(result) === Char
    @test length(result) == 3
    @test String(result) == "abc"

    runtime_result = collect_trait_string("éβ")
    @test typeof(runtime_result) === Vector{Char}
    @test eltype(runtime_result) === Char
    @test length(runtime_result) == 2
    @test String(runtime_result) == "éβ"

    empty_result = Base._collect(1:1, "", Base.IteratorEltype(""), Base.IteratorSize(""))
    @test typeof(empty_result) === Vector{Char}
    @test eltype(empty_result) === Char
    @test length(empty_result) == 0
end

true

using Test

@testset "repr(Float32) keeps the f0 suffix (Issue #4747)" begin
    @test repr(Float32(1.5)) == "1.5f0"
    @test repr(Float32(2.0)) == "2.0f0"
    @test repr(Float32(-3.25)) == "-3.25f0"
end

@testset "repr(Float16) keeps the Float16(...) wrapper (Issue #4747)" begin
    @test repr(Float16(1.5)) == "Float16(1.5)"
    @test repr(Float16(2.0)) == "Float16(2.0)"
end

@testset "repr(Float64) is unchanged by #4747" begin
    @test repr(1.5) == "1.5"
    @test repr(2.0) == "2.0"
end

@testset "print/string still use the bare form for typed floats (Issue #4747)" begin
    # print and string are the print-form (no suffix/wrapper); only
    # show / repr use the typed-literal preserving form.
    @test string(Float32(1.5)) == "1.5"
    @test string(Float16(1.5)) == "1.5"
end

true

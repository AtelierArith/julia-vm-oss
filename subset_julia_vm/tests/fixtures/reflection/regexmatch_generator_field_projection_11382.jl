using Test

# Issue #11382: RegexMatch and Base.Generator report their real upstream
# physical fields through fieldcount/fieldnames/getfield/propertynames
# instead of masquerading as zero-field values.

@testset "RegexMatch field projection (Issue #11382)" begin
    m = match(r"(x)(y)?", "x")

    @test fieldcount(typeof(m)) == 5
    @test fieldnames(typeof(m)) == (:match, :captures, :offset, :offsets, :regex)
    @test propertynames(m) == (:match, :captures, :offset, :offsets, :regex)

    # getfield by name.
    @test getfield(m, :match) == "x"
    @test getfield(m, :captures) == Union{Nothing,SubString{String}}["x", nothing]
    @test getfield(m, :offset) == 1
    @test getfield(m, :offsets) == [1, 0]
    @test getfield(m, :regex) isa Regex
    @test getfield(m, :regex).pattern == "(x)(y)?"

    # getfield by 1-based index, in fieldnames order.
    @test getfield(m, 1) == "x"
    @test getfield(m, 2) == Union{Nothing,SubString{String}}["x", nothing]
    @test getfield(m, 3) == 1
    @test getfield(m, 4) == [1, 0]
    @test getfield(m, 5) isa Regex

    # Dot-property access agrees with getfield (Issue #10182 path).
    @test m.match == getfield(m, :match)
    @test m.captures == getfield(m, :captures)
    @test m.offset == getfield(m, :offset)
    @test m.offsets == getfield(m, :offsets)
    @test m.regex.pattern == getfield(m, :regex).pattern

    # Unknown field name/out-of-range index still raise, matching upstream
    # (not a silent zero-field pass-through).
    @test_throws FieldError getfield(m, :bogus)
    @test_throws BoundsError getfield(m, 6)

    # [getfield(m, i) for i in 1:fieldcount(typeof(m))] round-trips the full
    # projection, matching the shape of the Issue #11382 MWE.
    all_fields = [getfield(m, i) for i in 1:fieldcount(typeof(m))]
    @test length(all_fields) == 5
    @test all_fields[1] == "x"
    @test all_fields[5] isa Regex
end

@testset "Base.Generator field projection (Issue #11382)" begin
    g = (x + 1 for x in 1:3)

    @test fieldcount(typeof(g)) == 2
    @test fieldnames(typeof(g)) == (:f, :iter)
    @test propertynames(g) == (:f, :iter)

    @test getfield(g, :f) isa Function
    @test getfield(g, :iter) == 1:3
    @test getfield(g, 1) isa Function
    @test getfield(g, 2) == 1:3
    @test g.iter == getfield(g, :iter)

    @test_throws FieldError getfield(g, :bogus)
    @test_throws BoundsError getfield(g, 3)

    all_fields = [getfield(g, i) for i in 1:fieldcount(typeof(g))]
    @test length(all_fields) == 2
    @test all_fields[2] == 1:3
end

true

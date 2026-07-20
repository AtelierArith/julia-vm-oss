# VM-raised Meta.parse failures must carry a structured JuliaSyntax detail
# object rather than `nothing`. Upstream qualifies the type as
# Base.JuliaSyntax.ParseError; sjulia's missing Base-owned submodule binding is
# tracked separately by Issue #11614, so this fixture compares the real type
# family suffix and all observable payload fields without pretending that
# owner qualification already works (Issue #11572).
using Test

function parse_error_11572(source, start)
    try
        Meta.parse(source, start)
        return nothing
    catch e
        return e
    end
end

@testset "ParseError carries JuliaSyntax detail (Issue #11572)" begin
    e = parse_error_11572(")", 1)
    @test e isa Exception
    @test endswith(string(typeof(e)), "ParseError")

    detail = e.detail
    @test !isnothing(detail)
    @test endswith(string(typeof(detail)), "JuliaSyntax.ParseError")
    @test fieldnames(typeof(detail)) == (:source, :diagnostics, :incomplete_tag)

    @test detail.source.code == ")"
    @test detail.source.byte_offset == 0
    @test detail.source.filename == "none"
    @test detail.source.first_line == 1
    @test detail.source.line_starts == [1, 2]

    @test length(detail.diagnostics) >= 1
    first_diagnostic = detail.diagnostics[1]
    @test first_diagnostic.first_byte == 1
    @test first_diagnostic.last_byte == 1
    @test first_diagnostic.level === :error
    @test occursin(")", first_diagnostic.message)
    @test detail.incomplete_tag === :none

    shifted = parse_error_11572("1; )", 4).detail
    @test shifted.source.code == ")"
    @test shifted.source.byte_offset == 3
    @test shifted.diagnostics[1].first_byte == 4
    @test shifted.diagnostics[1].last_byte == 4

    multiline = parse_error_11572(")\nx", 1).detail
    @test multiline.source.code == ")\n"
    @test multiline.source.line_starts == [1, 3]
    @test multiline.diagnostics[1].first_byte == 1
    @test multiline.diagnostics[1].last_byte == 1

    multibyte = parse_error_11572("é )", 1).detail
    @test multibyte.source.code == "é )"
    @test multibyte.source.line_starts == [1, 5]
    @test multibyte.diagnostics[1].first_byte == 3
    @test multibyte.diagnostics[1].last_byte == 4

    token_ascii = parse_error_11572("1 + )", 1).detail.diagnostics[1]
    @test token_ascii.first_byte == 5
    @test token_ascii.last_byte == 5

    trailing_ascii = parse_error_11572("a)", 1).detail.diagnostics[1]
    @test trailing_ascii.first_byte == 2
    @test trailing_ascii.last_byte == 2

    trailing_multibyte = parse_error_11572("é)", 1).detail.diagnostics[1]
    @test trailing_multibyte.first_byte == 3
    @test trailing_multibyte.last_byte == 3

    # The position-based API owns exactly one expression: a later line's
    # invalid token is the next parse segment, not an error in the first one
    # (Issue #11637).
    parsed_first, next_position = Meta.parse("x\n)", 1)
    @test parsed_first === :x
    @test next_position == 3

    semicolon_group, semicolon_next = Meta.parse("x;y\n)", 1)
    @test semicolon_group == Expr(:toplevel, :x, :y)
    @test semicolon_next == 5
    single_group, single_next = Meta.parse("x;", 1)
    @test single_group == Expr(:toplevel, :x)
    @test single_next == 3
    blank_line_first, blank_line_next = Meta.parse("x\n\n)", 1)
    @test blank_line_first === :x
    @test blank_line_next == 3

    @test Meta.parse("x;y") == Expr(:toplevel, :x, :y)
    @test_throws Exception Meta.parse("x;\ny")
    @test_throws Exception Meta.parse("x\n\n")

    @test_throws BoundsError Meta.parse("x", 0)
    @test_throws BoundsError Meta.parse("x", -1)
    @test_throws BoundsError Meta.parse("x", typemin(Int))
    @test_throws BoundsError Meta.parse("x", 3)
    @test Meta.parse("x", 2) == (nothing, 2)
    @test_throws BoundsError Meta.parse("", 0)
    @test_throws BoundsError Meta.parse("", 2)
    @test Meta.parse("", 1) == (nothing, 1)

    invalid_boundary = parse_error_11572("é)", 2)
    @test invalid_boundary isa Exception
    @test endswith(string(typeof(invalid_boundary)), "ParseError")
    @test occursin("invalid UTF-8 sequence", string(invalid_boundary))
    @test isnothing(invalid_boundary.detail)

    # A nested ignored ParseError must not overwrite the detail retained for
    # rethrowing the outer exception (#11632).
    nested = try
        Meta.parse(")")
        nothing
    catch
        try
            Meta.parse(")\nx")
        catch
        end
        try
            rethrow()
            nothing
        catch rethrown
            rethrown
        end
    end
    @test endswith(string(typeof(nested)), "ParseError")
    @test nested.detail.source.code == ")"
end

@testset "Meta.parse returns incomplete expressions (Issue #11633)" begin
    for (source, expected_tag) in (("`abc", :cmd), ("function", :other), ("let", :block))
        parsed = Meta.parse(source)
        @test parsed isa Expr
        @test parsed.head === :incomplete
        @test length(parsed.args) == 1
        error = parsed.args[1]
        @test error isa Exception
        @test endswith(string(typeof(error)), "ParseError")
        @test error.detail.incomplete_tag === expected_tag

        parsed_at, next_position = Meta.parse(source, 1)
        @test parsed_at isa Expr
        @test parsed_at.head === :incomplete
        @test parsed_at.args[1].detail.incomplete_tag === expected_tag
        @test next_position == ncodeunits(source) + 1
    end

    # Invalid (rather than appendably incomplete) syntax still throws.
    @test_throws Exception Meta.parse(")")
    @test_throws Exception Meta.parse(")", 1)
end

true

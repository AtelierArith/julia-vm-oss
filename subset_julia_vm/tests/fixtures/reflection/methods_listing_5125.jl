# Issue #5125: methods(f) listing reflection — upstream-style Method display
# and source-location fields.
#
# Complements methods_iteration_5125.jl (count + iteration) by verifying the
# reflection surface a user actually reads off a `methods(f)` listing:
#   * `show(m)` / `string(m)` / `println(m)` render upstream-style
#     `foo(x::Int64) @ Module file:line` instead of a raw struct dump.
#   * `.module`, `.file`, `.line` fields are populated.
#
# Parity note: sjulia's source file path/line for a fixture do not match the
# upstream `julia` invocation's path/line, so this fixture asserts STRUCTURE
# (the show output starts with `foo(`, contains `::Int64`, contains ` @ `, and
# the `.line` is a positive integer) rather than an exact `@ Main path:line`
# string. This keeps the fixture green under BOTH sjulia and upstream julia
# (scripts/fixture_julia_parity.sh).

using Test

foolist5125(x::Int64) = x
foolist5125(x::Float64, y) = x + y

barlist5125(x) = x

@testset "methods(f): Method show is upstream-style" begin
    m = first(methods(foolist5125))
    s = string(m)
    # Upstream: "foolist5125(x::Int64) @ Main <path>:<line>"
    @test startswith(s, "foolist5125(")
    @test occursin("::Int64", s)
    @test occursin(" @ ", s)

    # show(io, m) and println(m) must agree with string(m): all go through the
    # same Method display (no raw `Method(:foolist5125, ...)` struct dump).
    buf = IOBuffer()
    show(buf, m)
    @test String(take!(buf)) == s
    @test !occursin("Method(", s)
end

@testset "methods(f): each method shows its own signature" begin
    sigs = [string(m) for m in methods(foolist5125)]
    @test length(sigs) == 2
    # Exactly one single-arg Int64 method and one two-arg Float64 method.
    @test count(s -> occursin("::Int64", s) && !occursin("::Float64", s), sigs) == 1
    @test count(s -> occursin("::Float64", s), sigs) == 1
end

@testset "methods(f): .module / .file / .line fields" begin
    m = first(methods(barlist5125))
    # `.module` prints as a module name (Main for top-level definitions).
    @test string(m.module) == "Main"
    # `.file` is a Symbol (upstream type); accessing it must not error.
    @test m.file isa Symbol
    # `.line` is an integer line number, positive for a real definition.
    @test m.line isa Integer
    @test m.line > 0
end

true

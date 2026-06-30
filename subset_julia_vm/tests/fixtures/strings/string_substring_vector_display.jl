using Test

# Regression tests for Issue #3574:
# `split`/`rsplit` previously returned `Vector{String}` and rendered as
# `["a", "b"]` (no element-type prefix) — Julia 1.12 returns
# `Vector{SubString{String}}` and renders as `SubString{String}["a", "b"]`.
# The VM doesn't have a separate substring runtime type; the result array is
# tagged via `_substring_retag` so `typeof`, `eltype`, and `show` match Julia.

# Helper: capture the `print` (== `string`) form of a value the same way
# the issue's MWE checks (`println(split(...))`).
_show(x) = sprint(print, x)

@testset "split show form (#3574)" begin
    @test _show(split("a,b", ",")) == "SubString{String}[\"a\", \"b\"]"
    @test _show(split("a,b,c", ",")) == "SubString{String}[\"a\", \"b\", \"c\"]"
    @test _show(split("a,,b", ",")) == "SubString{String}[\"a\", \"\", \"b\"]"
    @test _show(split("a,,b", ","; keepempty=false)) == "SubString{String}[\"a\", \"b\"]"
    @test _show(split("hello world")) == "SubString{String}[\"hello\", \"world\"]"

    # Char delimiter — kwarg variants delegate to the String form, which retags.
    @test _show(split("a-b-c", '-')) == "SubString{String}[\"a\", \"b\", \"c\"]"
end

@testset "rsplit show form (#3574)" begin
    @test _show(rsplit("a,b,c", ",")) == "SubString{String}[\"a\", \"b\", \"c\"]"
    @test _show(rsplit("a,b,c", ","; limit=2)) == "SubString{String}[\"a,b\", \"c\"]"
    @test _show(rsplit("a-b", '-')) == "SubString{String}[\"a\", \"b\"]"
end

@testset "split typeof / eltype (#3574)" begin
    @test string(typeof(split("a,b", ","))) == "Vector{SubString{String}}"
    @test string(eltype(split("a,b", ","))) == "SubString{String}"

    # rsplit too
    @test string(typeof(rsplit("a,b", ","))) == "Vector{SubString{String}}"
    @test string(eltype(rsplit("a,b", ","))) == "SubString{String}"
end

@testset "non-ASCII split show (#3574)" begin
    # Multi-byte UTF-8 chars in the splitting result still render correctly.
    @test _show(split("aé,bê,cé", ",")) == "SubString{String}[\"aé\", \"bê\", \"cé\"]"
end

@testset "Vector{String} literal stays bare (#3574)" begin
    # Array literals do NOT get the SubString tag — only split/rsplit results do.
    # In Julia and the VM, `["a", "b"]` shows as `["a", "b"]`.
    @test _show(["a", "b"]) == "[\"a\", \"b\"]"
end

true

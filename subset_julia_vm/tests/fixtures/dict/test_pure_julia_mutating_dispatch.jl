# Pure Julia dispatch verification for mutating Dict{K,V} methods (Issue #3739)
#
# After removing `push!`, `pop!`, `delete!` from `map_builtin_name()`, public
# calls now go through method dispatch first. `Dict{K,V}` (Pure Julia mutable
# struct) defines `delete!`/`empty!`/`pop!` in `base/dict.jl` and these run
# end-to-end via Pure Julia. The legacy `Value::Dict` (Rust HashMap) still has
# explicit routing in `compile_call` to preserve in-place semantics.

using Test

@testset "Dict{K,V} delete! routes through Pure Julia" begin
    d = Dict{String, Int64}("a" => 1, "b" => 2, "c" => 3)
    delete!(d, "b")
    @test length(d) == 2
    @test !haskey(d, "b")
    @test haskey(d, "a")
    @test haskey(d, "c")
end

@testset "Dict{K,V} empty! routes through Pure Julia" begin
    d = Dict{String, Int64}("a" => 1, "b" => 2)
    empty!(d)
    @test length(d) == 0
end

@testset "legacy Dict (Value::Dict) delete! still works" begin
    # Untyped Dict literal becomes the Rust HashMap-backed Value::Dict; the
    # explicit `compile_call` route keeps it on the BuiltinOp::DictDelete path
    # so in-place mutation semantics are preserved.
    d = Dict("a" => 1, "b" => 2, "c" => 3)
    delete!(d, "b")
    @test length(d) == 2
end

true

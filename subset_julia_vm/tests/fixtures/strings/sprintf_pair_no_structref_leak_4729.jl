using Test

# sjulia exposes `sprintf` as a Base-level builtin (upstream Julia
# requires `Printf.@sprintf`). This fixture verifies the sjulia
# sprintf path also resolves StructRef Pair values against the heap
# (Issue #4729 sweep, follows Issues #4725 / #4727).

@testset "sjulia sprintf %s on Pair does not leak StructRef (Issue #4729)" begin
    p = Pair(1, 2)
    @test sprintf("%s", p) == "1 => 2"
    @test sprintf("Wrapped: %s", p) == "Wrapped: 1 => 2"
    @test sprintf("%s and %s", p, p) == "1 => 2 and 1 => 2"
end

true

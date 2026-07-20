# Issue #10164: `Lowering::lower_source_file_inner` (the plain, non-`include()`
# lowering path used by `pipeline::parse_source` -- the Base/prelude lowering
# entry point) never populated `pending_doc`, so a top-level docstring
# preceding a Base definition was silently dropped: Base's own docstrings
# (`Val`, `Exception`, `BoundsError`, `VERSION`, etc.) were never registered,
# even though a user program's own docstrings (lowered via
# `LoweringWithInclude`/`parse_source_with_include`) worked correctly.
#
# Fixing that capture uncovered a second, shared bug: neither lowering path's
# `ConstStatement` arm ever called `push_doc_registration`, so a docstring
# preceding a top-level `const` (`ConstStatement` is a valid docstring target,
# same as upstream Julia) leaked past the const into whatever later
# definition happened to consume `pending_doc` next instead of documenting
# the const itself -- misattributing Base's own `VERSION` docstring to an
# unrelated function in the next Base source file. This fixture exercises the
# ordinary user-program lowering path (`LoweringWithInclude`), which shares
# the fixed `ConstStatement` arm with the Base/prelude path; the plain
# `Lowering::lower_source_file_inner` path itself is covered directly by
# `lowering::tests::lower_source_file_captures_top_level_docstring_10164` in
# `subset_julia_vm_lowering/src/lowering/mod.rs` (that path is not reachable from an
# ordinary `.jl` fixture file).
using Test

"""
docstring for MyStruct10164
"""
struct MyStruct10164
    x::Int
end

"""
docstring for MyAbstract10164
"""
abstract type MyAbstract10164 end

"""
docstring for MY_CONST_10164
"""
const MY_CONST_10164 = 42

@testset "docstring targets are registered (Issue #10164)" begin
    @test occursin("docstring for MyStruct10164", string(@doc(MyStruct10164)))
    @test occursin("docstring for MyAbstract10164", string(@doc(MyAbstract10164)))
    @test occursin("docstring for MY_CONST_10164", string(@doc(MY_CONST_10164)))
end

true

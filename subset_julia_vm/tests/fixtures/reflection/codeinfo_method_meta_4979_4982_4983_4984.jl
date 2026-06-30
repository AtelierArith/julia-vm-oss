# Method.nospecialize and representative Core.CodeInfo structural / flag /
# purity fields (Issues #4979, #4982, #4983, #4984).
#
# Verified against upstream Julia 1.12.
using Test

plain_meta_4982(x) = x + 1
# A no-global identity function: upstream reports has_image_globalref == false
# because the body references no image global (Issue #4983).
identity_meta_4982(x) = x
@noinline noinline_meta_4982(x) = x + 2
Base.@propagate_inbounds prop_meta_4982(x) = x + 3
Base.@nospecializeinfer nsi_meta_4982(x) = x + 4
Base.@assume_effects :effect_free :nothrow pure_meta_4982(x) = x + 5
Base.@assume_effects :foldable foldable_meta_4982(x) = x + 6

function ns_stmt_meta_4982(x, y)
    @nospecialize x y
    x + y
end

function specialize_clear_meta_4982(x, y)
    @nospecialize x y
    @specialize
    x * y
end

@testset "Method.nospecialize bitmask (Issue #4984)" begin
    @test first(methods(plain_meta_4982)).nospecialize == 0
    @test first(methods(plain_meta_4982)).nospecialize isa Int32
    @test first(methods(ns_stmt_meta_4982)).nospecialize == 3
    # Trailing @specialize clears the accumulated bitmask.
    @test first(methods(specialize_clear_meta_4982)).nospecialize == 0
end

@testset "CodeInfo.propagate_inbounds / nospecializeinfer (Issue #4979)" begin
    pl = Base.code_lowered(plain_meta_4982, Tuple{Int64})[1]
    @test pl.propagate_inbounds == false
    @test pl.nospecializeinfer == false
    @test pl.propagate_inbounds isa Bool

    pci = Base.code_lowered(prop_meta_4982, Tuple{Int64})[1]
    @test pci.propagate_inbounds == true
    @test Base.code_typed(prop_meta_4982, Tuple{Int64})[1][1].propagate_inbounds == true

    ns = Base.code_lowered(nsi_meta_4982, Tuple{Int64})[1]
    @test ns.nospecializeinfer == true
    @test Base.code_typed(nsi_meta_4982, Tuple{Int64})[1][1].nospecializeinfer == true
end

@testset "CodeInfo structural fields (Issue #4982)" begin
    pl = Base.code_lowered(plain_meta_4982, Tuple{Int64})[1]
    @test pl.nargs == 2
    @test pl.nargs isa UInt64
    @test pl.isva == false
    @test pl.has_fcall == false
    @test pl.inlining_cost == UInt16(65535)

    tp = Base.code_typed(plain_meta_4982, Tuple{Int64})[1][1]
    @test tp.inlining_cost == UInt16(10)
    # @noinline retains the sentinel inlining cost when typed.
    @test Base.code_typed(noinline_meta_4982, Tuple{Int64})[1][1].inlining_cost == UInt16(65535)
end

@testset "CodeInfo purity / cost / basic flags (Issue #4983)" begin
    pl = Base.code_lowered(plain_meta_4982, Tuple{Int64})[1]
    @test pl.purity == UInt16(0)
    @test pl.purity isa UInt16
    # No-global identity function: has_image_globalref == false (the modeled
    # representative case).
    @test Base.code_lowered(identity_meta_4982, Tuple{Int64})[1].has_image_globalref == false
    @test Base.code_typed(identity_meta_4982, Tuple{Int64})[1][1].has_image_globalref == false

    # Base.@assume_effects :effect_free :nothrow -> 2 | 4 == 6
    @test Base.code_lowered(pure_meta_4982, Tuple{Int64})[1].purity == UInt16(6)
    @test Base.code_typed(pure_meta_4982, Tuple{Int64})[1][1].purity == UInt16(6)
    # Base.@assume_effects :foldable -> 1163
    @test Base.code_lowered(foldable_meta_4982, Tuple{Int64})[1].purity == UInt16(1163)
end

true

using Test

mutable struct CyclicDisplayNode10893
    next
end

@testset "cyclic mutable struct display terminates (Issue #10893)" begin
    self_cycle = CyclicDisplayNode10893(nothing)
    self_cycle.next = self_cycle
    self_text = string(self_cycle)
    @test occursin("CyclicDisplayNode10893", self_text)
    @test occursin("circular reference", self_text)

    left = CyclicDisplayNode10893(nothing)
    right = CyclicDisplayNode10893(left)
    left.next = right
    pair_text = string(left)
    @test occursin("CyclicDisplayNode10893", pair_text)
    @test occursin("circular reference", pair_text)
end

true

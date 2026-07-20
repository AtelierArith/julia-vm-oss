# A conflicting import is ignored when the name already has a source-earlier
# module binding, matching upstream Julia's warning-and-ignore behavior:
# the pre-existing value binding stays authoritative for value reads AND for
# parametric application, so `A{Int}(1)` reaches the ordinary runtime
# TypeError path instead of constructing the imported type (Issue #11426,
# tech-debt #11448). Non-conflicting imports keep their static authority.
using Test

module ImportConflictSource11426
struct Box{T}
    value::T
end
end

module ImportConflictRename11426
A = 42
import ..ImportConflictSource11426: Box as A
result = try
    A{Int}(1)
    :constructed
catch err
    typeof(err)
end
value_read = A
end

module ImportConflictSelective11426
Box = 7
import ..ImportConflictSource11426: Box
result = try
    Box{Int}(1)
    :constructed
catch err
    typeof(err)
end
value_read = Box
end

module ImportNoConflict11426
import ..ImportConflictSource11426: Box as B
import ..ImportConflictSource11426: Box
renamed = B{Int}(1).value
selected = Box{Float64}(2.5).value
end

@testset "conflicting imports are ignored (Issue #11426)" begin
    @test ImportConflictRename11426.result == TypeError
    @test ImportConflictRename11426.value_read == 42
    @test ImportConflictSelective11426.result == TypeError
    @test ImportConflictSelective11426.value_read == 7
end

@testset "non-conflicting imports keep static authority (Issue #11426)" begin
    @test ImportNoConflict11426.renamed == 1
    @test ImportNoConflict11426.selected == 2.5
end

true

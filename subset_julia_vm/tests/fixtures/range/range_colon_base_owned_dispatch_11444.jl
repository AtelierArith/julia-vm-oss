# Colon syntax is Base-owned: upstream lowers `a:b` through `Base.:(:)` into
# the UnitRange{T}/StepRange{T,S} inner constructors, so a user outer
# constructor on the bare range name never hijacks range literals — while
# direct bare-name calls still reach the user extension (Issue #11444).
using Test

UnitRange(a::Int64, b::Int64) = "hijacked"

r = 1:2
@test r isa UnitRange{Int64}
@test collect(r) == [1, 2]
@test first(r) == 1 && last(r) == 2
@test length(r) == 2

# The direct bare-name call reaches the user method (upstream 1.12 assumes
# the unqualified definition extends Base.UnitRange).
@test UnitRange(3, 4) == "hijacked"

# StepRange literals stay Base-owned too.
s = 1:2:9
@test collect(s) == [1, 3, 5, 7, 9]

# Ranges built after the contamination still behave as ranges downstream.
@test sum(1:10) == 55
@test (1:2) == UnitRange{Int64}(1, 2)

true

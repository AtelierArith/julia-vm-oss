using Test

struct LayoutPredicateDispatchBox3911
    n::Int64
end

Base.isbits(::LayoutPredicateDispatchBox3911) = false
Base.isbitstype(::Type{LayoutPredicateDispatchBox3911}) = false
Base.ismutable(::LayoutPredicateDispatchBox3911) = true

box = LayoutPredicateDispatchBox3911(7)

@test !Base.isbits(box)
@test !Base.isbitstype(LayoutPredicateDispatchBox3911)
@test Base.ismutable(box)

@test Base.isbits(1)
@test Base.isbitstype(Int64)
@test !Base.ismutable(1)

true

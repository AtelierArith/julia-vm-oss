using Test

mutable struct GuardChainBox5860
    val::Union{Int64,String,Nothing}
end

function chained_field_guard_refinement_5860(b::GuardChainBox5860)
    if b.val !== nothing
        if b.val isa Int64
            return b.val + 1
        else
            return length(b.val)
        end
    end
    return 0
end

@test Base.infer_return_type(
    chained_field_guard_refinement_5860,
    Tuple{GuardChainBox5860},
) == Int64
@test Core.Compiler.return_type(
    chained_field_guard_refinement_5860,
    Tuple{GuardChainBox5860},
) == Int64

@test chained_field_guard_refinement_5860(GuardChainBox5860(2)) == 3
@test chained_field_guard_refinement_5860(GuardChainBox5860("abc")) == 3
@test chained_field_guard_refinement_5860(GuardChainBox5860(nothing)) == 0

true

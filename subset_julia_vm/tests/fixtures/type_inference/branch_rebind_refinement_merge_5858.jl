using Test

mutable struct BranchRebindBox5858
    val::Union{Int64,Nothing}
end

function branch_rebind_refinement_merge_5858(flag::Bool, b::BranchRebindBox5858)
    if b.val !== nothing
        if flag
            b = BranchRebindBox5858(nothing)
        end
        return b.val
    end
    return 0
end

@test Base.infer_return_type(
    branch_rebind_refinement_merge_5858,
    Tuple{Bool,BranchRebindBox5858},
) == Union{Nothing,Int64}
@test Core.Compiler.return_type(
    branch_rebind_refinement_merge_5858,
    Tuple{Bool,BranchRebindBox5858},
) == Union{Nothing,Int64}

@test branch_rebind_refinement_merge_5858(false, BranchRebindBox5858(7)) == 7
@test branch_rebind_refinement_merge_5858(true, BranchRebindBox5858(7)) === nothing

true

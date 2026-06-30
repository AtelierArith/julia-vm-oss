module EvalMacroIssue8362
x = 1
end

function eval_module_qualified_contract_8362()
    result = @eval EvalMacroIssue8362 begin
        y = x + 1
        y
    end

    repeated = @eval EvalMacroIssue8362 y + x

    result == 2 && repeated == 3
end

top_result_8362 = @eval EvalMacroIssue8362 begin
    z = x + 2
end

eval_module_qualified_contract_8362() &&
    top_result_8362 == 3 &&
    EvalMacroIssue8362.z == 3 &&
    isdefined(EvalMacroIssue8362, :z)

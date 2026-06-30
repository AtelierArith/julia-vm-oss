function eval_println_stack_contract_8373()
    result = eval(:(println(2)))
    result === nothing
end

eval_println_stack_contract_8373()

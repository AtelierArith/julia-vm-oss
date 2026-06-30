module Parent8269
    module Child8269
        const x = 41
    end

    const y = Child8269.x + 1
end

using Base.Order: Forward, Reverse

function nested_module_order_binding_contract_8269()
    ok_nested = Parent8269.y == 42
    ok_order_values = (Base.Order.Forward isa Base.Order.Ordering) &&
        (Base.Order.Reverse isa Base.Order.Ordering)
    ok_order_call = Base.Order.lt(Forward, 1, 2) &&
        !Base.Order.lt(Reverse, 1, 2)
    return ok_nested && ok_order_values && ok_order_call
end

nested_module_order_binding_contract_8269()

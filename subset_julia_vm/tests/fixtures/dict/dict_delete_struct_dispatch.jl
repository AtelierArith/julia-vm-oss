using Test

# Test DictDelete dispatch on non-Dict StructRef (Issue #3169)
# The VM's DictDelete compilation uses compile_expr (not LoadDict) for Any-typed local
# variables, allowing runtime dispatch to user-defined delete! methods on non-Dict StructRef.

mutable struct DeleteDispatchTracker
    last_key::Symbol
    call_count::Int64
end

# DictDelete handler finds this via find_best_method_index(["delete!", "Base.delete!"], args).
function delete!(store::DeleteDispatchTracker, key::Symbol)
    store.last_key = key
    store.call_count = store.call_count + 1
    return store
end

@testset "DictDelete dispatch on non-Dict StructRef (Issue #3169)" begin
    s = DeleteDispatchTracker(:none, 0)
    @test s.last_key == :none
    @test s.call_count == 0

    # delete! dispatches to user method for non-Dict StructRef local variable
    delete!(s, :foo)
    @test s.last_key == :foo
    @test s.call_count == 1

    # Subsequent calls update last_key
    delete!(s, :bar)
    @test s.last_key == :bar
    @test s.call_count == 2
end

true

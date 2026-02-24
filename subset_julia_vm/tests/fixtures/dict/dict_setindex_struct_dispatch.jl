using Test

# Test DictSet dispatch on non-Dict StructRef (Issue #3169)
# The VM's DictSet handler swaps args[1] and args[2] before dispatching to
# user-defined setindex! methods on non-Dict StructRef values.
# Compilation order: push collection, push key (args[2]), push value (args[1])
# so after pop+reverse args = [collection, key, value]; swap restores Julia convention
# setindex!(collection, value, key).

mutable struct SetDispatchTracker
    last_key::Symbol
    call_count::Int64
end

# DictSet handler finds this via find_best_method_index(["setindex!", "Base.setindex!"], args)
# with corrected arg order: (collection::SetDispatchTracker, value, key::Symbol).
function setindex!(store::SetDispatchTracker, value, key::Symbol)
    store.last_key = key
    store.call_count = store.call_count + 1
    return store
end

@testset "DictSet dispatch on non-Dict StructRef (Issue #3169)" begin
    s = SetDispatchTracker(:none, 0)
    @test s.last_key == :none
    @test s.call_count == 0

    # setindex! dispatches to user method with correct arg order (value and key not swapped)
    setindex!(s, 10, :x)
    @test s.last_key == :x
    @test s.call_count == 1

    # Subsequent calls update last_key
    setindex!(s, 20, :y)
    @test s.last_key == :y
    @test s.call_count == 2
end

true

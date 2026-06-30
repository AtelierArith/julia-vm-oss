# Issue #7180: a module-level helper referenced from a closure (or function value)
# passed to a Base higher-order function must resolve inside the module's scope.
# Previously the closure lifted from the module function body was registered with
# no module association, so the module-private helper failed to resolve with
# "function 'help' is not imported".

using Test

module M7180
    help(a, b) = a == b

    # Closure passed to findfirst refers to the module-level `help`.
    find2(v) = findfirst(x -> help(x, 2), v)

    # Function value passed to reduce by name.
    add(a, b) = a + b
    sumv(v) = reduce(add, v)

    # Function value passed to sort(; by = ...) by name.
    negkey(x) = -x
    sortdesc(v) = sort(v; by = negkey)
end

@testset "module closure HOF helper (Issue #7180)" begin
    @test M7180.find2([1, 2, 3]) == 2
    @test M7180.sumv([1, 2, 3, 4]) == 10
    @test M7180.sortdesc([3, 1, 2]) == [3, 2, 1]
end

true

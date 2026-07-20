# vm_aot equivalence corpus widening (Issue #10815): user-defined recursive
# function calls (not a prelude builtin) combined with a `for` loop, under
# the AoT minimal-prelude codegen path.
function fib(n::Int64)::Int64
    if n <= 1
        n
    else
        fib(n - 1) + fib(n - 2)
    end
end

for i in 0:10
    println(fib(i))
end

fib(10) == 55

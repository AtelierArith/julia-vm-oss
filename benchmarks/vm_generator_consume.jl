# VM benchmark driver: consuming SIMPLE generators (Issue #9200 S2).
#
# Exercises the two hot generator-consumer forms the S2 desugar touches —
# `sum(x*x for x in 1:N)` and `collect(2x for x in 1:N)` — inside a typed
# function so the whole loop stays on the FunctionIndex generator + eager
# `collect_generator` fast path. The S2 desugar (`(x*x for x in 1:N)` =>
# `let __gen_body_N(x)=x*x; Generator(__gen_body_N, 1:N) end`) compiles to the
# byte-identical `MakeGenerator(FunctionIndex) + CallDynamic(collect/sum)` the
# pre-desugar `Expr::Generator` path produced, so this is a regression guard.

function run_generator_consume()
    total = 0
    for _ in 1:200
        total += sum(x * x for x in 1:1000)
        c = collect(2 * x for x in 1:1000)
        total += length(c)
    end
    return total
end

println(run_generator_consume())

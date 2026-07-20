# VM benchmark driver: consuming FILTERED generators (Issue #9200 S3).
#
# Exercises the two hot filtered-generator-consumer forms the S3 desugar touches —
# `sum(x*x for x in 1:N if p(x))` and `collect(2x for x in 1:N if p(x))` — inside
# a typed function so the whole loop stays on the native filtered generator
# (FilteredFunctionIndex + eager `collect_generator` FilterMap) fast path. The S3
# desugar `(x*x for x in 1:N if p(x))` =>
# `Base.Generator(__gen_body_N, Base.Iterators.Filter(__gen_pred_N, 1:N))`
# compiles (via `try_compile_generator_over_filter`) to the byte-identical
# `MakeGenerator(FilteredFunctionIndex) + CallDynamic(collect/sum)` the pre-desugar
# `Expr::Generator` filter path produced, so this is a regression guard.

function run_filtered_generator_consume()
    total = 0
    for _ in 1:200
        total += sum(x * x for x in 1:1000 if x % 2 == 0)
        c = collect(2 * x for x in 1:1000 if x > 500)
        total += length(c)
    end
    return total
end

println(run_filtered_generator_consume())

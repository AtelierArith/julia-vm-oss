# Generic-vs-optimized SSA parity smoke case.
#
# Chosen to exercise loop-carried values and branches without depending on
# unsupported library surface. The metamorphic harness compares this program
# under SJULIA_SSA_PIPELINE=0 and the default optimized pipeline.

function scalar_loop_branch(n)
    acc = 0
    for i in 1:n
        if i % 2 == 0
            acc += i * 3
        else
            acc -= i
        end
    end
    acc
end

result = scalar_loop_branch(12)
println(result)
println(typeof(result))

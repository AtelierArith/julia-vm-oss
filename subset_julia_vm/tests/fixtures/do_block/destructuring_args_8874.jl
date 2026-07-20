using Test

hits = Int[]

foreach([(Dict(:a => 1), :a, 1, 0)]) do (d, k, v, p)
    push!(hits, d[k] + v + p)
end

@test hits == [2]

true

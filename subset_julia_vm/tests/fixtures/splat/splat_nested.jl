using Test

# Nested splat: splatting into a function that itself uses splat
function inner_sum(args...)
    sum(args)
end

function wrapper_sum(args...)
    inner_sum(args...)
end

@testset "splat in nested function calls" begin
    nums = [1, 2, 3, 4, 5]
    @test wrapper_sum(nums...) == 15

    # Splat multiple arrays
    a = [1, 2]
    b = [3, 4]
    @test wrapper_sum(a..., b...) == 10

    # Tuple splat nested
    t = (10, 20, 30)
    @test inner_sum(t...) == 60
end

true

using Test

generator_trait_testset_map_10137(x) = x
generator_trait_testset_label_10137(iter) = Base.IteratorSize(iter)

@testset "generator trait query keeps lifted body callable in testset scope (Issue #10137)" begin
    let result = generator_trait_testset_label_10137(
        (generator_trait_testset_map_10137(x) for x in [1, 2, 3])
    )
        @test result isa Base.HasShape{1}
    end
end

let result = generator_trait_testset_label_10137(
    (generator_trait_testset_map_10137(x) for x in [1, 2, 3])
)
    @test result isa Base.HasShape{1}
end

true

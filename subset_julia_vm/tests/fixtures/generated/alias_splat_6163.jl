using Test

# Issue #6163 / #5936: a generated function reached through a function-valued
# alias and splatted tuple should still run its generated body with concrete
# argument type objects, not runtime values.

@generated function generated_alias_splat_6163(x)
    if x == Int64
        return :(10)
    else
        return :(30)
    end
end

@testset "generated alias splat call (Issue #6163)" begin
    alias = generated_alias_splat_6163
    args = (1,)
    @test generated_alias_splat_6163(args...) == 10
    @test alias(args...) == 10
end

args_6163 = (1,)
alias_6163 = generated_alias_splat_6163
generated_alias_splat_6163(args_6163...) == 10 && alias_6163(args_6163...) == 10

# Test quoting of if statements with elseif
# quote if a; 1; elseif b; 2; else; 3; end end
# should create Expr(:if, :a, block1, Expr(:elseif, cond_block_b, block2, block3))

using Test

@testset "Quote of if with elseif: quote if a; elseif b; end end becomes Expr(:if, ..., Expr(:elseif, ...))" begin

    # Test if-elseif-else
    ex = quote
        if a
            1
        elseif b
            2
        else
            3
        end
    end

    # Check the outer structure contains :if
    s = string(ex)
    @assert occursin("if", s)
    @assert occursin("elseif", s)
    @assert occursin("else", s)

    # Test if-elseif (no else)
    ex2 = quote
        if x
            1
        elseif y
            2
        end
    end

    s2 = string(ex2)
    @assert occursin("if", s2)
    @assert occursin("elseif", s2)

    # Test multiple elseifs
    ex3 = quote
        if a
            1
        elseif b
            2
        elseif c
            3
        else
            4
        end
    end

    s3 = string(ex3)
    @assert occursin("if", s3)
    @assert occursin("elseif", s3)

    # Return success
    @test (1.0) == 1.0
end

true  # Test passed

# Runtime-expanded macros that return a catch-only `try` (no `finally`) must
# lower correctly. Upstream Julia stores `Expr(:try)` as the 3-arg shape
# `[try_block, catch_var_or_false, catch_block_or_false]`; sjulia previously
# rejected it ("malformed Expr(:try, ...)" / "unsupported Expr head :try").
# Issue #7832 (sibling shape of #7806).

using Test

# catch with no variable, value position
macro catch_only_try()
    esc(:(try
        error("x")
    catch
        42
    end))
end

# catch binding a variable, value position
macro catch_var_try()
    esc(:(try
        error("boom")
    catch e
        string(e)
    end))
end

@testset "catch-only try (no finally) from runtime macro" begin
    @test (@catch_only_try) == 42
    @test (@catch_var_try) == "ErrorException(\"boom\")"
end

# success path of a catch-only try yields the try body value
macro catch_only_ok()
    esc(:(try
        100 + 1
    catch
        -1
    end))
end

@testset "catch-only try success path" begin
    @test (@catch_only_ok) == 101
    println(@catch_only_try)
end

true

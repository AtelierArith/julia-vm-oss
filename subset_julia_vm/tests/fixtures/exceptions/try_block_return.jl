# Test return from inside try block
# Issue #1447: Return from try block returns nothing instead of actual value

using Test

@testset "Return from try block" begin
    # Test basic return from try block
    function test_try_return()
        try
            return 42
        catch
            return -1
        end
    end
    @test test_try_return() == 42

    # Test return from catch block (when exception occurs)
    function test_catch_return()
        try
            error("test error")
            return 1
        catch
            return 99
        end
    end
    @test test_catch_return() == 99

    # Test return from try block in a loop
    function test_try_in_loop()
        for i in 1:5
            try
                return i * 10
            catch
                continue
            end
        end
        return 0
    end
    @test test_try_in_loop() == 10

    # Test nested try blocks with return
    function test_nested_try()
        try
            try
                return 100
            catch
                return 50
            end
        catch
            return 25
        end
    end
    @test test_nested_try() == 100

    # Test try with multiple statements before return
    function test_try_multi_stmt()
        try
            x = 5
            y = x * 2
            return y + 3
        catch
            return 0
        end
    end
    @test test_try_multi_stmt() == 13
end

true

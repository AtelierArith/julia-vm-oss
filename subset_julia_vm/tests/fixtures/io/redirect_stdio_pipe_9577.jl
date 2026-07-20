using Test

@testset "redirect stdout/stderr devnull and Pipe surface (Issue #9577)" begin
    stdout_result = redirect_stdout(devnull) do
        println("hidden stdout")
        42
    end
    @test stdout_result == 42

    stderr_result = redirect_stderr(devnull) do
        println(stderr, "hidden stderr")
        "ok"
    end
    @test stderr_result == "ok"

    stdio_result = redirect_stdio(stdout=devnull, stderr=devnull) do
        println("hidden stdio stdout")
        println(stderr, "hidden stdio stderr")
        :done
    end
    @test stdio_result === :done

    function io_do_kwargs_9577(f; token=nothing)
        f()
        token
    end
    @test io_do_kwargs_9577(token=:kept) do
        nothing
    end === :kept

    function io_kw_stderr_shadow_10034(; stderr=nothing)
        stderr
    end
    @test io_kw_stderr_shadow_10034(stderr=123) == 123
    function io_kw_devnull_shadow_10044(; devnull=nothing)
        devnull
    end
    @test io_kw_devnull_shadow_10044(devnull=123) == 123
    @test stdin === stdin
    @test stdout === stdout
    @test stderr === stderr
    @test devnull === devnull

    p = Pipe()
    @test string(typeof(p)) == "Pipe"
    @test p isa IO
end

true

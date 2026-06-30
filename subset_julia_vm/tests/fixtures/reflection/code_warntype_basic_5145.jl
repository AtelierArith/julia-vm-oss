using InteractiveUtils
using Test

cwt_stable_5145(x::Int64) = x + 1
cwt_unstable_5145(x) = x > 0 ? 1 : "neg"
cwt_add_5145(x::Int64, y::Float64) = x + y

@testset "code_warntype / return-type diagnostics (Issue #5145)" begin
    # Core.Compiler.return_type routes to the shared inference surface and must
    # report a concrete type for a type-stable method.
    @test Core.Compiler.return_type(cwt_stable_5145, Tuple{Int64}) === Int64
    # A type-unstable method infers to a Union.
    @test Core.Compiler.return_type(cwt_unstable_5145, Tuple{Int64}) === Union{Int64,String}
    @test Base.infer_return_type(cwt_add_5145, Tuple{Int64,Float64}) === Float64

    # `code_warntype([io], f, types)` prints a type-stability diagnostic and
    # returns `nothing`. Output is directed at `devnull` to keep the fixture quiet.
    @test code_warntype(devnull, cwt_stable_5145, Tuple{Int64}) === nothing
    @test code_warntype(devnull, cwt_unstable_5145, Tuple{Int64}) === nothing
    @test code_warntype(devnull, cwt_add_5145, Tuple{Int64,Float64}) === nothing

    # `@code_warntype f(args...)` extracts the argument types from the call and
    # forwards to `code_warntype`, returning `nothing`.
    @test (@code_warntype cwt_stable_5145(1)) === nothing
    @test (@code_warntype cwt_add_5145(1, 2.0)) === nothing
end

true

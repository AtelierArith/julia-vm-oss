using Test

# A method whose body directly returns a bare `where`-bound type parameter `T`
# matched against a *concrete* argument infers the precise `Type{Concrete}`
# (e.g. `Type{Int64}`), not the widened `DataType` (Issue #5933).
f(x::T) where T = T

@testset "bare type-parameter return inference (Issue #5933)" begin
    # return_types channel
    @test Base.return_types(f, Tuple{Int64})[1] == Type{Int64}
    # infer_return_type channel
    @test Base.infer_return_type(f, Tuple{Int64}) == Type{Int64}
    # Core.Compiler.return_type channel
    @test Core.Compiler.return_type(f, Tuple{Int64}) == Type{Int64}
    # A different concrete carrier type resolves analogously.
    @test Base.return_types(f, Tuple{String})[1] == Type{String}
end

# Final value is a conjunction of the checks (the fixture harness only verifies
# the file's FINAL expression == expected; bare `@test` failures do not abort).
(Base.return_types(f, Tuple{Int64})[1] == Type{Int64}) &&
    (Base.infer_return_type(f, Tuple{Int64}) == Type{Int64}) &&
    (Core.Compiler.return_type(f, Tuple{Int64}) == Type{Int64}) &&
    (Base.return_types(f, Tuple{String})[1] == Type{String})

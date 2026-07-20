# Struct-like AST values keep reflection metadata and getfield paths in sync.

using Test

@testset "struct-like AST reflection matrix" begin
    ex = :(x + 1)
    @test fieldnames(typeof(ex)) == (:head, :args)
    @test nfields(ex) == 2
    @test isdefined(ex, :head)
    @test isdefined(ex, :args)
    @test isdefined(ex, 1)
    @test isdefined(ex, 2)
    @test !isdefined(ex, :missing_9546)
    @test !isdefined(ex, 3)
    @test getfield(ex, :head) === :call
    @test getfield(ex, 1) === :call
    @test getfield(ex, :args)[1] === :+
    @test getfield(ex, 2)[1] === :+
    @test_throws BoundsError getfield(ex, 3)

    qn = QuoteNode(:x)
    @test fieldnames(typeof(qn)) == (:value,)
    @test nfields(qn) == 1
    @test isdefined(qn, :value)
    @test isdefined(qn, 1)
    @test !isdefined(qn, :missing_9546)
    @test !isdefined(qn, 2)
    @test getfield(qn, :value) === :x
    @test getfield(qn, 1) === :x
    @test_throws BoundsError getfield(qn, 2)

    ln = LineNumberNode(42, :file_9546)
    @test fieldnames(typeof(ln)) == (:line, :file)
    @test nfields(ln) == 2
    @test isdefined(ln, :line)
    @test isdefined(ln, :file)
    @test isdefined(ln, 1)
    @test isdefined(ln, 2)
    @test !isdefined(ln, :missing_9546)
    @test !isdefined(ln, 3)
    @test getfield(ln, :line) == 42
    @test getfield(ln, 1) == 42
    @test getfield(ln, :file) === :file_9546
    @test getfield(ln, 2) === :file_9546
    @test_throws BoundsError getfield(ln, 3)

    ln_without_file = LineNumberNode(100)
    @test isdefined(ln_without_file, :file)
    @test isdefined(ln_without_file, 2)
    @test getfield(ln_without_file, :file) === nothing
    @test getfield(ln_without_file, 2) === nothing

    gr = GlobalRef(Main, :sin)
    @test fieldnames(typeof(gr)) == (:mod, :name, :binding)
    @test nfields(gr) == 3
    @test isdefined(gr, :mod)
    @test isdefined(gr, :name)
    @test isdefined(gr, :binding)
    @test isdefined(gr, 1)
    @test isdefined(gr, 2)
    @test isdefined(gr, 3)
    @test !isdefined(gr, :missing_9546)
    @test !isdefined(gr, 4)
    @test getfield(gr, :mod) === Main
    @test getfield(gr, 1) === Main
    @test getfield(gr, :name) === :sin
    @test getfield(gr, 2) === :sin
    binding_by_name = getfield(gr, :binding)
    binding_by_index = getfield(gr, 3)
    @test typeof(binding_by_name) === Core.Binding
    @test typeof(binding_by_index) === Core.Binding
    @test fieldnames(typeof(binding_by_name)) == (:globalref, :value, :partitions, :backedges, :flags)
    @test nfields(binding_by_name) == 5
    @test getfield(binding_by_name, :globalref) == gr
    @test getfield(binding_by_name, 1) == gr
    @test getfield(binding_by_name, :flags) == UInt8(0)
    @test getfield(binding_by_name, 5) == UInt8(0)
    @test_throws BoundsError getfield(gr, 4)

    # Issue #10067: :value/:partitions/:backedges exist in upstream
    # Core.Binding's fieldnames but are unset in sjulia, so they must raise a
    # catchable UndefRefError ("access to undefined reference"), not a
    # missing-field error. :globalref/:flags remain modeled and defined.
    @test isdefined(binding_by_name, :globalref)
    @test isdefined(binding_by_name, :flags)
    @test !isdefined(binding_by_name, :value)
    @test !isdefined(binding_by_name, :partitions)
    @test !isdefined(binding_by_name, :backedges)
    @test isdefined(binding_by_name, 1)
    @test !isdefined(binding_by_name, 2)
    @test !isdefined(binding_by_name, 3)
    @test !isdefined(binding_by_name, 4)
    @test isdefined(binding_by_name, 5)
    @test !isdefined(binding_by_name, 0)
    @test !isdefined(binding_by_name, 6)
    @test_throws UndefRefError getfield(binding_by_name, :value)
    @test_throws UndefRefError getfield(binding_by_name, :partitions)
    @test_throws UndefRefError getfield(binding_by_name, :backedges)
    @test_throws UndefRefError getfield(binding_by_name, 2)
    @test_throws UndefRefError getfield(binding_by_name, 3)
    @test_throws UndefRefError getfield(binding_by_name, 4)
    # A field name that is not part of Core.Binding's layout at all is a
    # distinct FieldError, not UndefRefError.
    @test_throws FieldError getfield(binding_by_name, :nonexistent_field_10067)

    # UndefRefError must be catchable like any other Julia exception.
    caught_undef_ref = false
    try
        getfield(binding_by_name, :value)
    catch e
        caught_undef_ref = e isa UndefRefError
    end
    @test caught_undef_ref
end

true

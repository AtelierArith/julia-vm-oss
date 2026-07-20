# Static-vs-dynamic type-application parity matrix (Issue #10556).
#
# Prevention fixture for the drift class behind #10554 / #10587 / #10586 / #10422:
# static `ConstructParametricType*` and dynamic `ApplyTypeDynamic*` construction
# converged only on name-based success and never shared a NEGATIVE-case contract
# (bound violations, over-arity after all binders are consumed, non-type base,
# concrete-base re-application). This matrix exercises `T{args...}` and
# `Core.apply_type` across:
#   * static syntax vs. runtime-held base (local / function parameter);
#   * positional vs. one-or-more-splat argument forms;
#   * valid full / partial applications;
#   * invalid upper-bound, lower-bound, over-arity, non-type-base, concrete-base.
# Every asserted case's success value or thrown exception CLASS was verified to
# match upstream `julia` 1.12 first (see scripts/check_type_application_matrix.sh).
#
# The critical invariant it pins: the SAME logical application reaches the same
# verdict whether it lowers to a static opcode (`Core.apply_type(KnownName, ...)`
# -> ConstructParametricType) or a dynamic one (runtime-held base -> ApplyTypeDynamic).
# #10554 (static path skipped bound/arity enforcement) and #10642 (lower-bound
# `>:` handling) are now fixed and asserted here on BOTH paths.
#
# This file asserts ONLY cases where sjulia already matches upstream, so it stays
# green in BOTH interpreters and remains upstream-parity-comparable (no @test_skip
# / Broken column). Cases where sjulia still diverges are tracked in
# docs/vm/TYPE_APPLICATION_MATRIX_SKIPLIST.tsv with their upstream verdict and
# tracking issue (#10643 chained-literal `T{a}{b}`; #10654 literal bare
# non-parametric base; #10586 under-applied builtin family). When a fix lands,
# move its row out of the skiplist and add the assertion here.
#
# TYPE-APPLICATION-OPCODE-COVERAGE (Issue #10556): the audit
# scripts/check_type_application_matrix.sh discovers every type-application opcode
# in subset_julia_vm_bytecode/src/instr.rs and requires each to be listed here AND
# (when a sjulia binary is available) actually emitted by this fixture's compiled
# user code. Adding a new type-application opcode/route without a matrix case that
# emits it fails the audit.
#   opcode-covered: PushDataType                 # push_dt10556()      — constant-folded fully-applied builtin
#   opcode-covered: ConstructParametricType      # cpt10556(T)         — builtin family, runtime positional arg
#   opcode-covered: ConstructParametricTypeSplat # cpt_splat10556(a)   — builtin family, splat arg
#   opcode-covered: ApplyTypeDynamic             # apply_dyn10556(b,x)  — runtime-held base, positional
#   opcode-covered: ApplyTypeDynamicSplat        # apply_dyn_splat10556(b,a) — runtime-held base, splat

using Test

# User parametric types spanning the binder/bound space:
struct Trio10556{A,B,C} end                    # 3 unbounded binders
struct UBPair10556{T<:Real,S<:Integer} end     # 2 upper-bounded binders
struct LBHolder10556{S>:Int32} end             # 1 lower-bounded binder
struct BoundedPair10556{T<:Real,S>:Int32} end  # upper + lower bounds, 2 binders (acceptance)

# Canonical opcode-bearing helpers (named so --dump-bytecode always emits them;
# the audit greps the compiled bytecode of this fixture for each opcode).
push_dt10556() = Vector{Int64}                        # PushDataType
cpt10556(T) = Vector{T}                               # ConstructParametricType
cpt_splat10556(a) = Vector{a...}                      # ConstructParametricTypeSplat
apply_dyn10556(b, x) = Core.apply_type(b, x)          # ApplyTypeDynamic
apply_dyn_splat10556(b, a) = Core.apply_type(b, a...) # ApplyTypeDynamicSplat

@testset "type-application matrix: valid full application (Issue #10556)" begin
    # user type, static syntax, positional + splat
    @test Trio10556{Int64,Float64,String} === Trio10556{Int64,Float64,String}
    let a = (Int64, Float64, String)
        @test Trio10556{a...} === Trio10556{Int64,Float64,String}
    end
    # user type, Core.apply_type, statically-known name (positional)
    @test Core.apply_type(Trio10556, Int64, Float64, String) ===
          Trio10556{Int64,Float64,String}
    # user type, runtime-held base, positional + splat
    let b = Trio10556
        @test Core.apply_type(b, Int64, Float64, String) === Trio10556{Int64,Float64,String}
        @test apply_dyn_splat10556(b, Any[Int64, Float64, String]) ===
              Trio10556{Int64,Float64,String}
    end
    # upper-bounded user type, valid arguments (literal + runtime base)
    @test UBPair10556{Int64,Int32} === UBPair10556{Int64,Int32}
    @test Core.apply_type(UBPair10556, Int64, Int32) === UBPair10556{Int64,Int32}
    # lower-bounded user type: exact bound and valid supertypes, all paths (#10642)
    @test LBHolder10556{Int32} === LBHolder10556{Int32}
    @test LBHolder10556{Integer} === LBHolder10556{Integer}        # Int32 <: Integer
    let b = LBHolder10556
        @test b{Signed} === LBHolder10556{Signed}                  # dynamic path
        @test Core.apply_type(b, Number) === LBHolder10556{Number}
    end
    # upper+lower-bounded user type (>=2 binders), lower arg at bound and above
    @test BoundedPair10556{Int64,Int32} === BoundedPair10556{Int64,Int32}
    @test BoundedPair10556{Int64,Integer} === BoundedPair10556{Int64,Integer}
    let a = (Int64, Integer)
        @test BoundedPair10556{a...} === BoundedPair10556{Int64,Integer}
    end
    @test Core.apply_type(BoundedPair10556, Int64, Integer) === BoundedPair10556{Int64,Integer}

    # builtin families, each canonical opcode helper
    @test push_dt10556() === Vector{Int64}            # PushDataType
    @test cpt10556(Int64) === Vector{Int64}           # ConstructParametricType
    @test cpt_splat10556(Any[Int64]) === Vector{Int64} # ConstructParametricTypeSplat
    @test apply_dyn10556(Vector, Int64) === Vector{Int64}          # ApplyTypeDynamic
    @test apply_dyn_splat10556(Vector, Any[Int64]) === Vector{Int64} # ApplyTypeDynamicSplat
    @test Array{Float64,2} === Matrix{Float64}
    @test Dict{String,Int64} === Dict{String,Int64}
    @test apply_dyn_splat10556(Tuple, Any[Int64, String]) === Tuple{Int64,String}
end

@testset "type-application matrix: valid partial application (Issue #10556)" begin
    @test Trio10556{Int64} isa UnionAll
    @test Core.apply_type(Trio10556, Int64) isa UnionAll
    # partial then complete, via Core.apply_type on the partial
    let t = Core.apply_type(Trio10556, Int64)
        @test Core.apply_type(t, Float64, String) === Trio10556{Int64,Float64,String}
    end
    let t = Core.apply_type(Trio10556, Int64, Float64)
        @test Core.apply_type(t, String) === Trio10556{Int64,Float64,String}
    end
    # partial then complete, via a variable-held partial and brace syntax
    let w = Trio10556{Int64}
        @test w{Float64,String} === Trio10556{Int64,Float64,String}
    end
    # under-applied builtin family that IS a trailing UnionAll (contrast #10586)
    @test Dict{String} isa UnionAll
    let d = Dict{String}
        @test d{Int64} === Dict{String,Int64}
    end
end

@testset "type-application matrix: invalid upper-bound violation (Issue #10556)" begin
    # literal path enforces the bound
    @test_throws TypeError UBPair10556{String,Int32}   # T=String ⊄ Real
    @test_throws TypeError UBPair10556{Int64,Float64}  # S=Float64 ⊄ Integer
    @test_throws TypeError BoundedPair10556{String,Int32}  # T=String ⊄ Real
    # runtime-held base (ApplyTypeDynamic) enforces the bound, positional + splat
    let b = UBPair10556
        @test_throws TypeError b{String,Int32}
        @test_throws TypeError b{Any[String, Int32]...}
        @test_throws TypeError Core.apply_type(b, String, Int32)
        @test_throws TypeError Core.apply_type(b, Any[String, Int32]...)
    end
    let b = BoundedPair10556
        @test_throws TypeError b{String,Int32}
    end
    # static Core.apply_type(KnownName, ...) enforces the bound too (#10554),
    # positional + splat — the static/dynamic parity this matrix guards.
    @test_throws TypeError Core.apply_type(UBPair10556, String, Int32)
    @test_throws TypeError Core.apply_type(UBPair10556, Any[String, Int32]...)
end

@testset "type-application matrix: invalid lower-bound violation (Issue #10556)" begin
    # An argument that is neither the bound nor a supertype is rejected on all paths.
    @test_throws TypeError LBHolder10556{Int64}
    let b = LBHolder10556
        @test_throws TypeError b{Int64}
        @test_throws TypeError Core.apply_type(b, Int64)
    end
    # static Core.apply_type(KnownName, ...) rejects it too (#10554 parity).
    @test_throws TypeError Core.apply_type(LBHolder10556, Int64)
end

@testset "type-application matrix: invalid over-arity (Issue #10556)" begin
    # literal path rejects leftover params after all binders are consumed
    @test_throws ErrorException Trio10556{Int64,Float64,String,Bool}
    @test_throws ErrorException UBPair10556{Int64,Int32,Bool}
    # runtime-held base rejects over-arity, positional + splat
    let b = Trio10556
        @test_throws ErrorException b{Int64,Float64,String,Bool}
        @test_throws ErrorException Core.apply_type(b, Int64, Float64, String, Bool)
        @test_throws ErrorException Core.apply_type(b, Any[Int64, Float64, String, Bool]...)
    end
    # static Core.apply_type(KnownName, ...) rejects over-arity too (#10554 parity),
    # positional + splat.
    @test_throws ErrorException Core.apply_type(Trio10556, Int64, Float64, String, Bool)
    @test_throws ErrorException Core.apply_type(Trio10556, Any[Int64, Float64, String, Bool]...)
end

@testset "type-application matrix: invalid non-type base (Issue #10556)" begin
    @test_throws TypeError Core.apply_type(5, Int64)
    @test_throws TypeError Core.apply_type(5, Any[Int64]...)
    let x = 5
        @test_throws TypeError Core.apply_type(x, Int64)
    end
end

@testset "type-application matrix: invalid concrete-base re-application (Issue #10556)" begin
    # applying parameters to an already-concrete DataType raises TypeError (#10422)
    @test_throws TypeError Core.apply_type(Vector{Int64}, Float64)
    @test_throws TypeError Core.apply_type(Vector{Int64}, Any[Float64]...)
    @test_throws TypeError Core.apply_type(Trio10556{Int64,Float64,String}, Bool)
    @test_throws TypeError (Trio10556{Int64,Float64,String}){Bool}
    let c = Vector{Int64}
        @test_throws TypeError Core.apply_type(c, Float64)
        @test_throws TypeError c{Float64}
    end
end

@testset "type-application matrix: invalid bare non-parametric base (Issue #10556)" begin
    # #10587 (PR #10638) routed the Core.apply_type bare-base path through the
    # UnionAll validator — positional + runtime var. (The literal `Int64{Float64}`
    # form still diverges; tracked as #10654 in the skiplist.)
    @test_throws TypeError Core.apply_type(Int64, Float64)
    @test_throws TypeError Core.apply_type(Real, Int64)
    let b = Int64
        @test_throws TypeError Core.apply_type(b, Float64)
    end
end

true

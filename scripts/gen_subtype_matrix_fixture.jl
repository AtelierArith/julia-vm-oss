#!/usr/bin/env julia

# Generate the upstream subtype (`<:`) oracle matrix for Issue #10049 (slice E
# of the "subtyping lattice / typejoin" tech-debt epic).
#
# This script is intentionally upstream-Julia-only, mirroring
# scripts/gen_numeric_matrix_fixture.jl (Issue #8698): it records upstream's
# `A <: B` verdict for a fixed, deterministic list of type-pair expressions,
# then emits
#   1. an oracle TSV (full candidate matrix, for provenance/diffing), and
#   2. an executable fixture (`subset_julia_vm/tests/fixtures/types/
#      subtype_matrix_oracle_10049.jl`) containing one `@test` per pair whose
#      id is NOT listed in `docs/vm/SUBTYPE_MATRIX_SKIPLIST.tsv`.
#
# Regeneration story: when a subtype bug fixed elsewhere makes sjulia agree
# with upstream on a previously skiplisted pair, remove that pair's row from
# the skiplist TSV and re-run this generator — the pair reappears in the
# fixture and starts being asserted again ("future subtype fixes shrink the
# skiplist").
#
# Usage:
#   julia --startup-file=no scripts/gen_subtype_matrix_fixture.jl
#   julia --startup-file=no scripts/gen_subtype_matrix_fixture.jl --out-tsv PATH --out-fixture PATH --skiplist PATH

const DEFAULT_TSV = joinpath(@__DIR__, "..", "subset_julia_vm", "tests", "fixtures", "types", "subtype_matrix_oracle_10049.tsv")
const DEFAULT_FIXTURE = joinpath(@__DIR__, "..", "subset_julia_vm", "tests", "fixtures", "types", "subtype_matrix_oracle_10049.jl")
const DEFAULT_SKIPLIST = joinpath(@__DIR__, "..", "docs", "vm", "SUBTYPE_MATRIX_SKIPLIST.tsv")

# ---------------------------------------------------------------------------
# Shared type declarations (embedded verbatim into the emitted fixture so it
# is self-contained; also `eval`'d here so the oracle pass can construct the
# same types).
# ---------------------------------------------------------------------------

const DECLARATIONS = """
abstract type Animal10049 end
struct Dog10049 <: Animal10049 end
struct Cat10049 <: Animal10049 end

struct Wrap10049{T} end

abstract type Shape10049{T} end
struct Circle10049{T} <: Shape10049{T}
    r::T
end

struct PairT10049{A,B}
    a::A
    b::B
end
"""

# ---------------------------------------------------------------------------
# Deterministic pair matrix. Each entry is (id, category, left_expr, right_expr).
# `left_expr <: right_expr` is evaluated under upstream `julia` to produce the
# oracle boolean (or an error status, for pairs that do not evaluate).
# ---------------------------------------------------------------------------

const PAIRS = [
    # -- concrete numerics --------------------------------------------------
    ("S001", "concrete_numeric", "Int64", "Real"),
    ("S002", "concrete_numeric", "Int64", "Integer"),
    ("S003", "concrete_numeric", "Int64", "Float64"),
    ("S004", "concrete_numeric", "Float64", "AbstractFloat"),
    ("S005", "concrete_numeric", "Bool", "Integer"),
    ("S006", "concrete_numeric", "Bool", "Int64"),
    ("S007", "concrete_numeric", "Int64", "Int64"),
    ("S008", "concrete_numeric", "Int8", "Signed"),
    ("S009", "concrete_numeric", "UInt8", "Unsigned"),
    ("S010", "concrete_numeric", "UInt8", "Signed"),
    ("S011", "concrete_numeric", "Complex{Float64}", "Complex"),
    ("S012", "concrete_numeric", "Complex", "Complex{Float64}"),
    ("S013", "concrete_numeric", "Rational{Int64}", "Rational"),
    ("S014", "concrete_numeric", "Rational{Int64}", "Real"),
    ("S015", "concrete_numeric", "BigInt", "Integer"),
    ("S016", "concrete_numeric", "BigFloat", "AbstractFloat"),

    # -- abstract supertype hierarchy ---------------------------------------
    ("S020", "abstract_hierarchy", "Integer", "Real"),
    ("S021", "abstract_hierarchy", "Real", "Number"),
    ("S022", "abstract_hierarchy", "AbstractFloat", "Real"),
    ("S023", "abstract_hierarchy", "Signed", "Integer"),
    ("S024", "abstract_hierarchy", "Unsigned", "Integer"),
    ("S025", "abstract_hierarchy", "Number", "Real"),
    ("S026", "abstract_hierarchy", "Real", "Integer"),
    ("S027", "abstract_hierarchy", "AbstractString", "Any"),
    ("S028", "abstract_hierarchy", "Any", "AbstractString"),
    ("S029", "abstract_hierarchy", "Animal10049", "Any"),
    ("S030", "abstract_hierarchy", "Dog10049", "Animal10049"),
    ("S031", "abstract_hierarchy", "Cat10049", "Animal10049"),
    ("S032", "abstract_hierarchy", "Dog10049", "Cat10049"),

    # -- parametric structs: invariant params --------------------------------
    ("S040", "invariant_params", "Wrap10049{Int64}", "Wrap10049{Int64}"),
    ("S041", "invariant_params", "Wrap10049{Int64}", "Wrap10049{Real}"),
    ("S042", "invariant_params", "Wrap10049{Int64}", "Wrap10049"),
    ("S043", "invariant_params", "Wrap10049", "Wrap10049{Int64}"),
    ("S044", "invariant_params", "Wrap10049{Int64}", "Wrap10049{Number}"),

    # -- Vector / Matrix / Tuple combinations --------------------------------
    ("S050", "array_family", "Vector{Int64}", "Vector{Number}"),
    ("S051", "array_family", "Vector{Int64}", "AbstractVector{Int64}"),
    ("S052", "array_family", "Vector{Int64}", "AbstractVector"),
    ("S053", "array_family", "Vector{Int64}", "Matrix"),
    ("S054", "array_family", "Matrix{Int64}", "Vector"),
    ("S055", "array_family", "Vector{Int64}", "Array"),
    ("S056", "array_family", "Array{Int64,1}", "Vector{Int64}"),
    ("S057", "array_family", "Tuple{Vector{Int64}}", "Tuple{Matrix}"),
    ("S058", "array_family", "Tuple{Matrix{Int64}}", "Tuple{Vector}"),
    ("S059", "array_family", "Tuple{Int64,Int64}", "Tuple{Real,Real}"),
    ("S060", "array_family", "Tuple{Int64}", "Tuple{Int64,Int64}"),
    ("S061", "array_family", "Tuple{Int64,Float64}", "Tuple{Real,Real}"),
    ("S062", "array_family", "Tuple{}", "Tuple{}"),
    ("S063", "array_family", "Tuple{Int64,Vararg{Int64}}", "Tuple{Vararg{Integer}}"),
    ("S064", "array_family", "Tuple{Int64,String}", "Tuple{Vararg{Any}}"),

    # -- Union types ----------------------------------------------------------
    ("S070", "union", "Union{Int64,Float64}", "Real"),
    ("S071", "union", "Real", "Union{Int64,Float64}"),
    ("S072", "union", "Int64", "Union{Int64,String}"),
    ("S073", "union", "String", "Union{Int64,Float64}"),
    ("S074", "union", "Union{Int64,String}", "Any"),
    ("S075", "union", "Union{}", "Int64"),
    ("S076", "union", "Int64", "Union{}"),

    # -- UnionAll with upper/lower TypeVar bounds ------------------------------
    ("S080", "unionall_bounds", "Vector{Int64}", "(Vector{T} where T<:Integer)"),
    ("S081", "unionall_bounds", "Vector{Float64}", "(Vector{T} where T<:Integer)"),
    ("S082", "unionall_bounds", "Vector{Int64}", "(Vector{T} where T<:Real)"),
    ("S083", "unionall_bounds", "Vector{Int64}", "(Vector{T} where Int64<:T<:Real)"),
    ("S084", "unionall_bounds", "Vector{Float32}", "(Vector{T} where Int64<:T<:Real)"),
    # Contravariant `{>:T}` (Issue #9468 anchor).
    ("S085", "unionall_bounds", "Circle10049{Real}", "Shape10049{>:Int64}"),
    ("S086", "unionall_bounds", "Circle10049{Number}", "Shape10049{>:Int64}"),
    ("S087", "unionall_bounds", "Circle10049{Any}", "Shape10049{>:Int64}"),
    ("S088", "unionall_bounds", "Circle10049{Int64}", "Shape10049{>:Int64}"),
    ("S089", "unionall_bounds", "Circle10049{Int32}", "Shape10049{>:Int64}"),
    ("S090", "unionall_bounds", "Vector{Real}", "Vector{>:Int64}"),
    ("S091", "unionall_bounds", "Vector{Any}", "Vector{>:Int64}"),
    ("S092", "unionall_bounds", "Vector{Int32}", "Vector{>:Int64}"),

    # -- Type{T} ---------------------------------------------------------------
    ("S100", "type_of", "Type{Int64}", "Type{Int64}"),
    ("S101", "type_of", "Type{Int64}", "Type{Real}"),
    ("S102", "type_of", "Type{Int64}", "Type{<:Real}"),
    ("S103", "type_of", "Type{String}", "Type{<:Real}"),
    ("S104", "type_of", "DataType", "Type"),
    ("S105", "type_of", "Type{Int64}", "DataType"),

    # -- diagonal rule (a TypeVar occurring twice, only covariantly, is
    #    constrained to concrete types; Jeff Bezanson PhD thesis section 4.2.2,
    #    julia/src/subtype.c `subtype_unionall`'s `diagonal` handling) ---------
    ("S110", "diagonal_rule", "Tuple{Int64,Int64}", "(Tuple{T,T} where T)"),
    ("S111", "diagonal_rule", "Tuple{Int64,String}", "(Tuple{T,T} where T)"),

    # -- UnionAll <: UnionAll (both sides bind a TypeVar; exercises the
    #    env-based bound-narrowing upstream's `subtype_unionall`/`var_lt`/
    #    `var_gt` perform, which sjulia splits into two different,
    #    non-symmetric code paths depending on which side the UnionAll is on
    #    — see docs/vm/SUBTYPING.md) --------------------------------------------
    ("S120", "unionall_vs_unionall", "(Vector{T} where T<:Integer)", "(Vector{S} where S<:Real)"),
    ("S121", "unionall_vs_unionall", "(Vector{T} where T<:Real)", "(Vector{S} where S<:Integer)"),

    # -- nested / chained `where` over a 2-typevar user struct -----------------
    ("S130", "nested_where", "(PairT10049{A,B} where {A<:Integer,B<:Real})", "(PairT10049{A,B} where {A<:Real,B<:Real})"),
    ("S131", "nested_where", "PairT10049{Int64,Float64}", "(PairT10049{A,B} where {A<:Integer,B<:Real})"),
    ("S132", "nested_where", "PairT10049{Float64,Float64}", "(PairT10049{A,B} where {A<:Integer,B<:Real})"),
]

# ---------------------------------------------------------------------------

function tsv_escape(x)
    s = string(x)
    s = replace(s, "\\" => "\\\\")
    s = replace(s, "\t" => "\\t")
    s = replace(s, "\n" => "\\n")
    return s
end

function evaluate_pair(left_expr, right_expr)
    try
        left = Base.eval(Main, Meta.parse(left_expr))
        right = Base.eval(Main, Meta.parse(right_expr))
        result = left <: right
        return ("ok", string(result), "none")
    catch err
        return ("error", "none", string(typeof(err)))
    end
end

function read_skiplist(path)
    ids = Set{String}()
    isfile(path) || return ids
    for line in eachline(path)
        startswith(line, "id\t") && continue
        isempty(strip(line)) && continue
        f = split(line, '\t')
        isempty(f) && continue
        push!(ids, f[1])
    end
    return ids
end

function usage()
    println("""
    Usage: julia --startup-file=no scripts/gen_subtype_matrix_fixture.jl [options]

    Options:
      --out-tsv PATH       oracle TSV path (default: $DEFAULT_TSV)
      --out-fixture PATH   generated fixture path (default: $DEFAULT_FIXTURE)
      --skiplist PATH      skiplist TSV path (default: $DEFAULT_SKIPLIST)
      -h, --help           show this help
    """)
end

function parse_args(args)
    out_tsv = abspath(DEFAULT_TSV)
    out_fixture = abspath(DEFAULT_FIXTURE)
    skiplist = abspath(DEFAULT_SKIPLIST)
    i = 1
    while i <= length(args)
        arg = args[i]
        if arg == "--out-tsv"
            i += 1
            i <= length(args) || error("--out-tsv requires a path")
            out_tsv = abspath(args[i])
        elseif arg == "--out-fixture"
            i += 1
            i <= length(args) || error("--out-fixture requires a path")
            out_fixture = abspath(args[i])
        elseif arg == "--skiplist"
            i += 1
            i <= length(args) || error("--skiplist requires a path")
            skiplist = abspath(args[i])
        elseif arg == "-h" || arg == "--help"
            usage()
            exit(0)
        else
            error("unknown argument: $arg")
        end
        i += 1
    end
    return out_tsv, out_fixture, skiplist
end

function write_tsv(path, rows)
    mkpath(dirname(path))
    open(path, "w") do io
        println(io, join(("id", "category", "left_expr", "right_expr", "status", "result", "exception_type"), '\t'))
        for row in rows
            println(io, join(map(tsv_escape, row), '\t'))
        end
    end
end

function write_fixture(path, rows, skipped_ids)
    mkpath(dirname(path))
    kept = [r for r in rows if r[5] == "ok" && !(r[1] in skipped_ids)]
    skipped_kept = [r[1] for r in rows if r[1] in skipped_ids]
    open(path, "w") do io
        println(io, "# Generated by scripts/gen_subtype_matrix_fixture.jl for Issue #10049.")
        println(io, "# Differential subtype (`<:`) property test: each @test asserts sjulia agrees")
        println(io, "# with upstream julia's verdict for one type-pair expression.")
        println(io, "#")
        println(io, "# This measures the `<:` engine surface (CoreType::is_subtype_of /")
        println(io, "# JuliaType::is_subtype_of) only. Dispatch-scoring divergences whose `<:`")
        println(io, "# verdict already agrees with upstream (e.g. Issue #8806, where the engine")
        println(io, "# was correct but the dispatch scorer was not) are out of this instrument's")
        println(io, "# reach; see docs/vm/SUBTYPING.md.")
        println(io, "#")
        println(io, "# Pairs where sjulia disagrees with upstream are excluded here and tracked in")
        println(io, "# docs/vm/SUBTYPE_MATRIX_SKIPLIST.tsv. Re-run this generator after a fix lands")
        println(io, "# and remove the pair's row from the skiplist to re-include it.")
        if !isempty(skipped_kept)
            println(io, "#")
            println(io, "# Currently skiplisted ids: ", join(sort(skipped_kept), ", "))
        end
        println(io)
        println(io, "using Test")
        println(io)
        print(io, DECLARATIONS)
        println(io)

        by_category = Dict{String,Vector{NTuple{7,String}}}()
        for row in kept
            push!(get!(by_category, row[2], NTuple{7,String}[]), row)
        end

        for category in sort(collect(keys(by_category)))
            println(io, "@testset \"subtype matrix - $category (Issue #10049)\" begin")
            for (id, _cat, left_expr, right_expr, _status, result, _exc) in by_category[category]
                expr = "($left_expr) <: ($right_expr)"
                if result == "true"
                    println(io, "    @test $expr  # $id")
                else
                    println(io, "    @test !($expr)  # $id")
                end
            end
            println(io, "end")
            println(io)
        end

        println(io, "true")
    end
end

function main(args)
    out_tsv, out_fixture, skiplist_path = parse_args(args)

    Base.eval(Main, Meta.parse("begin\n" * DECLARATIONS * "\nend"))

    rows = NTuple{7,String}[]
    for (id, category, left_expr, right_expr) in PAIRS
        status, result, exception_type = evaluate_pair(left_expr, right_expr)
        push!(rows, (id, category, left_expr, right_expr, status, result, exception_type))
    end

    write_tsv(out_tsv, rows)
    skipped_ids = read_skiplist(skiplist_path)
    write_fixture(out_fixture, rows, skipped_ids)

    n_ok = count(r -> r[5] == "ok", rows)
    n_error = count(r -> r[5] == "error", rows)
    n_skipped = count(r -> r[1] in skipped_ids, rows)
    n_asserted = n_ok - n_skipped
    println("subtype matrix pairs: ", length(rows), " (ok: ", n_ok, ", error: ", n_error, ")")
    println("skiplisted: ", n_skipped, " (from ", skiplist_path, ")")
    println("asserted in fixture: ", n_asserted)
    println("tsv: ", out_tsv)
    println("fixture: ", out_fixture)
end

main(ARGS)

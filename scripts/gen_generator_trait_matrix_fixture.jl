#!/usr/bin/env julia

# Generate the upstream generator/iterator trait matrix for Issue #9566.
#
# This script is intentionally upstream-Julia-only. It records a deterministic
# matrix over generator transformations, iterator traits, consumers, and base
# element carriers, then emits:
#   1. an oracle TSV, for provenance and skiplist validation; and
#   2. an executable fixture with one assertion per non-skiplisted cell.
#
# Re-generate with:
#   julia --startup-file=no scripts/gen_generator_trait_matrix_fixture.jl

const DEFAULT_TSV = joinpath(@__DIR__, "..", "subset_julia_vm", "tests", "fixtures", "generator", "generator_trait_matrix_9566.tsv")
const DEFAULT_FIXTURE = joinpath(@__DIR__, "..", "subset_julia_vm", "tests", "fixtures", "generator", "generator_trait_matrix_9566.jl")
const DEFAULT_SKIPLIST = joinpath(@__DIR__, "..", "docs", "vm", "GENERATOR_TRAIT_MATRIX_SKIPLIST.tsv")

const DECLARATIONS = raw"""
matrix_dispatch_9566(x::Int64) = x
matrix_dispatch_9566(x::Float64) = 2
matrix_dispatch_9566(x::String) = 3
matrix_dispatch_9566(x) = 4

matrix_typed_base_9566() = [1, 2, 3]
matrix_any_base_9566() = Any[1, 2.0, "s"]

function matrix_iteratorsize_label_9566(iter)
    trait = Base.IteratorSize(iter)
    if trait isa Base.HasShape{1}
        return "HasShape{1}"
    elseif trait isa Base.HasShape{2}
        return "HasShape{2}"
    elseif trait isa Base.HasShape{3}
        return "HasShape{3}"
    elseif trait isa Base.HasLength
        return "HasLength"
    elseif trait isa Base.SizeUnknown
        return "SizeUnknown"
    elseif trait isa Base.IsInfinite
        return "IsInfinite"
    end
    return string(typeof(trait))
end

function matrix_iteratoreltype_label_9566(iter)
    trait = Base.IteratorEltype(iter)
    if trait isa Base.HasEltype
        return "HasEltype"
    elseif trait isa Base.EltypeUnknown
        return "EltypeUnknown"
    end
    return string(typeof(trait))
end

matrix_collect_summary_9566(value) = (string(typeof(value)), repr(value))

function matrix_for_sum_9566(iter)
    acc = 0
    for value in iter
        acc += value
    end
    return acc
end

struct MatrixIterOnly9566
    n::Int64
end

Base.iterate(iter::MatrixIterOnly9566, state=1) =
    state > iter.n ? nothing : (state, state + 1)

matrix_iteronly_9566() = MatrixIterOnly9566(5)

function matrix_dynamic_loopsum_9566(xs)
    acc = 0
    for x in xs
        acc += x
    end
    return acc
end
"""

struct MatrixCell
    id::String
    category::String
    transform::String
    base::String
    consumer::String
    expr::String
    issue_refs::String
end

const BASES = [
    ("typed_array", "matrix_typed_base_9566()"),
    ("range", "1:3"),
    ("any_mixed", "matrix_any_base_9566()"),
]

const TRANSFORMS = [
    "map",
    "filter",
    "flatten",
    "product",
    "zip",
    "enumerate",
]

const CONSUMERS = [
    "iterator_size",
    "iterator_eltype",
    "collect",
    "sum",
    "foldl",
    "first",
    "length",
    "forloop",
]

function usage()
    println("""
    Usage: julia --startup-file=no scripts/gen_generator_trait_matrix_fixture.jl [options]

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

function iter_expr(transform, base_expr)
    if transform == "map"
        return "(matrix_dispatch_9566(x) for x in $base_expr)"
    elseif transform == "filter"
        return "(matrix_dispatch_9566(x) for x in $base_expr if matrix_dispatch_9566(x) >= 2)"
    elseif transform == "flatten"
        return "(matrix_dispatch_9566(x) + y for x in $base_expr for y in 1:2)"
    elseif transform == "product"
        return "(matrix_dispatch_9566(x) + y for x in $base_expr, y in 1:2)"
    elseif transform == "zip"
        return "(matrix_dispatch_9566(x) + y for (x, y) in zip($base_expr, 10:12))"
    elseif transform == "enumerate"
        return "(i + matrix_dispatch_9566(x) for (i, x) in enumerate($base_expr))"
    end
    error("unknown transform: $transform")
end

function consumer_expr(consumer, iter)
    if consumer == "iterator_size"
        return "matrix_iteratorsize_label_9566($iter)"
    elseif consumer == "iterator_eltype"
        return "matrix_iteratoreltype_label_9566($iter)"
    elseif consumer == "collect"
        return "matrix_collect_summary_9566(collect($iter))"
    elseif consumer == "sum"
        return "sum($iter)"
    elseif consumer == "foldl"
        return "foldl(+, $iter; init=0)"
    elseif consumer == "first"
        return "first($iter)"
    elseif consumer == "length"
        return "length($iter)"
    elseif consumer == "forloop"
        return "matrix_for_sum_9566($iter)"
    end
    error("unknown consumer: $consumer")
end

function cell_issue_refs(transform, base, consumer)
    refs = String["#9566"]
    if transform == "map" && base == "any_mixed" && consumer in ("sum", "foldl")
        push!(refs, "#9399")
    end
    if transform == "map" && base == "typed_array" && consumer == "iterator_size"
        push!(refs, "#9393")
    end
    if transform == "flatten" && consumer == "collect"
        push!(refs, "#9438")
    end
    return join(refs, ",")
end

function build_cells()
    cells = MatrixCell[]
    for (base, base_expr) in BASES
        for transform in TRANSFORMS
            iter = iter_expr(transform, base_expr)
            for consumer in CONSUMERS
                id = "M_" * uppercase(transform) * "_" * uppercase(base) * "_" * uppercase(consumer)
                push!(cells, MatrixCell(
                    id,
                    "matrix",
                    transform,
                    base,
                    consumer,
                    consumer_expr(consumer, iter),
                    cell_issue_refs(transform, base, consumer),
                ))
            end
        end
    end

    push!(cells, MatrixCell(
        "BUG_9403_DYNAMIC_FORLOOP",
        "open_bug",
        "iterate_only",
        "custom_iterate_only",
        "forloop",
        "matrix_dynamic_loopsum_9566(matrix_iteronly_9566())",
        "#9403,#9566",
    ))
    push!(cells, MatrixCell(
        "BUG_9405_NESTED_FILTERED_COLLECT",
        "open_bug",
        "nested_filtered",
        "range",
        "collect",
        "matrix_collect_summary_9566(collect(v for v in (x for x in 1:5 if x > 2)))",
        "#9405,#9566",
    ))
    push!(cells, MatrixCell(
        "BUG_9405_NESTED_FILTERED_MAP",
        "open_bug",
        "nested_filtered",
        "range",
        "map",
        "matrix_collect_summary_9566(map(x -> x + 1, (v for v in (x for x in 1:5 if x > 2))))",
        "#9405,#9566",
    ))
    push!(cells, MatrixCell(
        "BUG_9405_NESTED_FILTERED_SUM",
        "open_bug",
        "nested_filtered",
        "range",
        "sum",
        "sum(v for v in (x^2 for x in 1:5 if isodd(x)))",
        "#9405,#9566",
    ))
    push!(cells, MatrixCell(
        "BUG_9385_COMPREHENSION_ANY_NARROW",
        "closed_bug",
        "comprehension",
        "any_mixed",
        "collect",
        "matrix_collect_summary_9566([matrix_dispatch_9566(x) for x in matrix_any_base_9566()])",
        "#9385,#9566",
    ))
    push!(cells, MatrixCell(
        "BUG_9457_GENERATOR_GETINDEX_METHODERROR",
        "closed_bug",
        "indexing",
        "range",
        "getindex",
        "((x for x in 1:3)[1])",
        "#9457,#9566",
    ))
    push!(cells, MatrixCell(
        "BUG_9533_TUPLE_BROADCAST_ABS",
        "closed_bug",
        "tuple_broadcast",
        "tuple",
        "broadcast",
        "matrix_collect_summary_9566(abs.((-1, -2, -3)))",
        "#9533,#9566",
    ))
    push!(cells, MatrixCell(
        "BUG_9547_TUPLE_BROADCAST_COMPARE",
        "closed_bug",
        "tuple_broadcast",
        "tuple",
        "broadcast",
        "matrix_collect_summary_9566((1, 2, 3) .< (2, 2, 2))",
        "#9547,#9566",
    ))

    return cells
end

function tsv_escape(x)
    s = string(x)
    s = replace(s, "\\" => "\\\\")
    s = replace(s, "\t" => "\\t")
    s = replace(s, "\n" => "\\n")
    return s
end

function evaluate_expr(expr)
    try
        value = Base.eval(Main, Meta.parse(expr))
        return ("ok", string(typeof(value)), repr(value), "none")
    catch err
        return ("error", "none", "none", string(typeof(err)))
    end
end

function read_skiplist(path)
    ids = Set{String}()
    isfile(path) || return ids
    for line in eachline(path)
        startswith(line, "id\t") && continue
        isempty(strip(line)) && continue
        startswith(line, "#") && continue
        fields = split(line, '\t')
        isempty(fields) && continue
        push!(ids, fields[1])
    end
    return ids
end

function write_tsv(path, rows)
    mkpath(dirname(path))
    open(path, "w") do io
        println(io, join(("id", "category", "transform", "base", "consumer", "expr", "status", "result_type", "result_repr", "exception_type", "issue_refs"), '\t'))
        for row in rows
            println(io, join(map(tsv_escape, row), '\t'))
        end
    end
end

function julia_string_literal(s)
    return repr(s)
end

function write_fixture(path, rows, skipped_ids)
    mkpath(dirname(path))
    kept = [row for row in rows if !(row[1] in skipped_ids)]
    skipped_kept = [row[1] for row in rows if row[1] in skipped_ids]

    open(path, "w") do io
        println(io, "# Generated by scripts/gen_generator_trait_matrix_fixture.jl for Issue #9566.")
        println(io, "# Differential generator/iterator trait matrix. Each assertion records")
        println(io, "# upstream julia's result for one {transform, base, consumer} cell.")
        println(io, "#")
        println(io, "# Divergent cells are excluded here and tracked in")
        println(io, "# docs/vm/GENERATOR_TRAIT_MATRIX_SKIPLIST.tsv. Re-run the generator after")
        println(io, "# removing a fixed row from that skiplist to promote it into this fixture.")
        if !isempty(skipped_kept)
            println(io, "#")
            println(io, "# Currently skiplisted ids: ", join(sort(skipped_kept), ", "))
        end
        println(io)
        println(io, "using Test")
        println(io)
        print(io, DECLARATIONS)
        println(io)

        println(io, "# Workaround: evaluate generated generator cells through function-scope")
        println(io, "# helpers because @testset block scope loses lifted generator body")
        println(io, "# bindings for trait queries (Issue #10137).")
        for row in kept
            id = row[1]
            expr = row[6]
            println(io, "function matrix_cell_", id, "_9566()")
            println(io, "    return $expr")
            println(io, "end")
            println(io)
        end

        by_category = Dict{String,Vector{NTuple{11,String}}}()
        for row in kept
            push!(get!(by_category, row[2], NTuple{11,String}[]), row)
        end

        for category in sort(collect(keys(by_category)))
            println(io, "@testset \"generator trait matrix - $category (Issue #9566)\" begin")
            for row in by_category[category]
                id, _category, transform, base, consumer, expr, status, result_type, result_repr, exception_type, issue_refs = row
                label = "$id $transform/$base/$consumer $issue_refs"
                cell_call = "matrix_cell_" * id * "_9566()"
                if status == "ok"
                    println(io, "    let result = $cell_call")
                    println(io, "        @test string(typeof(result)) == ", julia_string_literal(result_type), "  # $label")
                    println(io, "        @test repr(result) == ", julia_string_literal(result_repr), "  # $label")
                    println(io, "    end")
                else
                    println(io, "    @test_throws $exception_type $cell_call  # $label")
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

    rows = NTuple{11,String}[]
    for cell in build_cells()
        status, result_type, result_repr, exception_type = evaluate_expr(cell.expr)
        push!(rows, (
            cell.id,
            cell.category,
            cell.transform,
            cell.base,
            cell.consumer,
            cell.expr,
            status,
            result_type,
            result_repr,
            exception_type,
            cell.issue_refs,
        ))
    end

    write_tsv(out_tsv, rows)
    skipped_ids = read_skiplist(skiplist_path)
    write_fixture(out_fixture, rows, skipped_ids)

    n_error = count(row -> row[7] == "error", rows)
    n_skipped = count(row -> row[1] in skipped_ids, rows)
    println("generator trait matrix cells: ", length(rows), " (upstream errors: ", n_error, ")")
    println("skiplisted: ", n_skipped, " (from ", skiplist_path, ")")
    println("asserted in fixture: ", length(rows) - n_skipped)
    println("tsv: ", out_tsv)
    println("fixture: ", out_fixture)
end

main(ARGS)

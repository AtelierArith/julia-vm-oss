#!/usr/bin/env julia

# Generate upstream numeric operator matrices for Issues #8696/#8698.
#
# This script is intentionally upstream-Julia-only. It records the oracle data
# that sjulia integration (#8697) compares against.

using SHA

const DEFAULT_TSV = joinpath(@__DIR__, "..", "subset_julia_vm", "tests", "fixtures", "numeric", "numeric_matrix_reduced_8696.tsv")
const DEFAULT_FIXTURE = joinpath(@__DIR__, "..", "subset_julia_vm", "tests", "fixtures", "numeric", "numeric_matrix_reduced_8696.jl")

struct ValueSpec
    type_name::String
    label::String
    expr::String
    factory::Function
end

# The signed/unsigned/float/big numeric tower. This is the deterministic base
# shared by both profiles; every pair resolves to a method or throws a catchable
# runtime error, so the single-file probe never hard-aborts.
const TOWER_VALUE_SPECS = [
    ValueSpec("Bool", "true", "true", () -> true),
    ValueSpec("Int8", "normal", "Int8(3)", () -> Int8(3)),
    ValueSpec("Int16", "normal", "Int16(3)", () -> Int16(3)),
    ValueSpec("Int32", "normal", "Int32(3)", () -> Int32(3)),
    ValueSpec("Int64", "normal", "Int64(3)", () -> Int64(3)),
    ValueSpec("Int128", "normal", "Int128(3)", () -> Int128(3)),
    ValueSpec("UInt8", "normal", "UInt8(5)", () -> UInt8(5)),
    ValueSpec("UInt16", "normal", "UInt16(5)", () -> UInt16(5)),
    ValueSpec("UInt32", "normal", "UInt32(5)", () -> UInt32(5)),
    ValueSpec("UInt64", "normal", "UInt64(5)", () -> UInt64(5)),
    ValueSpec("UInt128", "normal", "UInt128(5)", () -> UInt128(5)),
    ValueSpec("Float16", "normal", "Float16(2.5)", () -> Float16(2.5)),
    ValueSpec("Float32", "normal", "Float32(2.5)", () -> Float32(2.5)),
    ValueSpec("Float64", "normal", "Float64(2.5)", () -> Float64(2.5)),
    ValueSpec("BigInt", "normal", "big(7)", () -> big(7)),
    ValueSpec("BigFloat", "normal", "BigFloat(\"2.5\")", () -> BigFloat("2.5")),
]

# Non-tower Real/Number families surfaced by Issue #9347. Rational participates
# in the ordered Real tower and probes cleanly against the reduced (normal-value)
# tower. It is added to the REDUCED profile only; it is intentionally kept OUT of
# the FULL profile because Rational × typemax(UInt128) `rem` aborts the whole
# multi-cell probe with an uncatchable OverflowError (Issue #9422).
#
# Other families remain deferred entirely:
#   * Complex{Int64}/ComplexF64/Char -> Issue #9409: sjulia raises an uncatchable
#     compile-time MethodError on statically no-method calls, aborting the whole
#     probe instead of recording an error row.
#   * Irrational{:pi} -> Issue #9412: mixed Irrational×Rational ordering hits the
#     promote-fallback recursion trap (Issue #5966), throwing StackOverflowError
#     and making the probe pathologically slow (risking the nightly 900s timeout).
const REDUCED_VALUE_SPECS = vcat(
    TOWER_VALUE_SPECS,
    [ValueSpec("Rational{Int64}", "normal", "3//4", () -> 3 // 4)],
)

const FULL_VALUE_SPECS = vcat(
    [ValueSpec("Bool", "false", "false", () -> false)],
    TOWER_VALUE_SPECS,
    [
        ValueSpec("Int8", "minus_one", "Int8(-1)", () -> Int8(-1)),
        ValueSpec("Int8", "zero", "Int8(0)", () -> Int8(0)),
        ValueSpec("Int8", "typemin", "typemin(Int8)", () -> typemin(Int8)),
        ValueSpec("Int8", "typemax", "typemax(Int8)", () -> typemax(Int8)),
        ValueSpec("Int16", "minus_one", "Int16(-1)", () -> Int16(-1)),
        ValueSpec("Int16", "zero", "Int16(0)", () -> Int16(0)),
        ValueSpec("Int16", "typemin", "typemin(Int16)", () -> typemin(Int16)),
        ValueSpec("Int16", "typemax", "typemax(Int16)", () -> typemax(Int16)),
        ValueSpec("Int32", "minus_one", "Int32(-1)", () -> Int32(-1)),
        ValueSpec("Int32", "zero", "Int32(0)", () -> Int32(0)),
        ValueSpec("Int32", "typemin", "typemin(Int32)", () -> typemin(Int32)),
        ValueSpec("Int32", "typemax", "typemax(Int32)", () -> typemax(Int32)),
        ValueSpec("Int64", "minus_one", "Int64(-1)", () -> Int64(-1)),
        ValueSpec("Int64", "zero", "Int64(0)", () -> Int64(0)),
        ValueSpec("Int64", "typemin", "typemin(Int64)", () -> typemin(Int64)),
        ValueSpec("Int64", "typemax", "typemax(Int64)", () -> typemax(Int64)),
        ValueSpec("Int128", "minus_one", "Int128(-1)", () -> Int128(-1)),
        ValueSpec("Int128", "zero", "Int128(0)", () -> Int128(0)),
        ValueSpec("Int128", "typemin", "typemin(Int128)", () -> typemin(Int128)),
        ValueSpec("Int128", "typemax", "typemax(Int128)", () -> typemax(Int128)),
        ValueSpec("UInt8", "zero", "UInt8(0)", () -> UInt8(0)),
        ValueSpec("UInt8", "typemax", "typemax(UInt8)", () -> typemax(UInt8)),
        ValueSpec("UInt16", "zero", "UInt16(0)", () -> UInt16(0)),
        ValueSpec("UInt16", "typemax", "typemax(UInt16)", () -> typemax(UInt16)),
        ValueSpec("UInt32", "zero", "UInt32(0)", () -> UInt32(0)),
        ValueSpec("UInt32", "typemax", "typemax(UInt32)", () -> typemax(UInt32)),
        ValueSpec("UInt64", "zero", "UInt64(0)", () -> UInt64(0)),
        ValueSpec("UInt64", "typemax", "typemax(UInt64)", () -> typemax(UInt64)),
        ValueSpec("UInt128", "zero", "UInt128(0)", () -> UInt128(0)),
        ValueSpec("UInt128", "typemax", "typemax(UInt128)", () -> typemax(UInt128)),
        ValueSpec("Float16", "minus_one", "Float16(-1.0)", () -> Float16(-1.0)),
        ValueSpec("Float16", "zero", "Float16(0.0)", () -> Float16(0.0)),
        ValueSpec("Float16", "neg_zero", "-zero(Float16)", () -> -zero(Float16)),
        ValueSpec("Float16", "inf", "Float16(Inf)", () -> Float16(Inf)),
        ValueSpec("Float16", "neg_inf", "Float16(-Inf)", () -> Float16(-Inf)),
        ValueSpec("Float16", "nan", "Float16(NaN)", () -> Float16(NaN)),
        ValueSpec("Float32", "minus_one", "Float32(-1.0)", () -> Float32(-1.0)),
        ValueSpec("Float32", "zero", "Float32(0.0)", () -> Float32(0.0)),
        ValueSpec("Float32", "neg_zero", "-zero(Float32)", () -> -zero(Float32)),
        ValueSpec("Float32", "inf", "Float32(Inf)", () -> Float32(Inf)),
        ValueSpec("Float32", "neg_inf", "Float32(-Inf)", () -> Float32(-Inf)),
        ValueSpec("Float32", "nan", "Float32(NaN)", () -> Float32(NaN)),
        ValueSpec("Float64", "minus_one", "Float64(-1.0)", () -> Float64(-1.0)),
        ValueSpec("Float64", "zero", "Float64(0.0)", () -> Float64(0.0)),
        ValueSpec("Float64", "neg_zero", "-zero(Float64)", () -> -zero(Float64)),
        ValueSpec("Float64", "inf", "Inf", () -> Inf),
        ValueSpec("Float64", "neg_inf", "-Inf", () -> -Inf),
        ValueSpec("Float64", "nan", "NaN", () -> NaN),
        ValueSpec("BigInt", "minus_one", "big(-1)", () -> big(-1)),
        ValueSpec("BigInt", "zero", "big(0)", () -> big(0)),
        ValueSpec("BigInt", "large", "big(typemax(Int128))", () -> big(typemax(Int128))),
        ValueSpec("BigFloat", "minus_one", "BigFloat(\"-1.0\")", () -> BigFloat("-1.0")),
        ValueSpec("BigFloat", "zero", "BigFloat(\"0.0\")", () -> BigFloat("0.0")),
        ValueSpec("BigFloat", "neg_zero", "-zero(BigFloat)", () -> -zero(BigFloat)),
        ValueSpec("BigFloat", "inf", "BigFloat(Inf)", () -> BigFloat(Inf)),
        ValueSpec("BigFloat", "neg_inf", "BigFloat(-Inf)", () -> BigFloat(-Inf)),
        ValueSpec("BigFloat", "nan", "BigFloat(NaN)", () -> BigFloat(NaN)),
    ],
)

const BASE_OP_SPECS = [
    ("+", (a, b) -> a + b),
    ("-", (a, b) -> a - b),
    ("*", (a, b) -> a * b),
    ("/", (a, b) -> a / b),
    ("div", (a, b) -> div(a, b)),
    ("fld", (a, b) -> fld(a, b)),
    ("cld", (a, b) -> cld(a, b)),
    ("rem", (a, b) -> rem(a, b)),
    ("mod", (a, b) -> mod(a, b)),
    ("==", (a, b) -> a == b),
    ("!=", (a, b) -> a != b),
    ("<", (a, b) -> a < b),
    ("<=", (a, b) -> a <= b),
    (">", (a, b) -> a > b),
    (">=", (a, b) -> a >= b),
]

# Ordering probes added by Issue #9347. (promote_type is a type-domain operator,
# not value-domain, so it does not fit the value substitution matrix and is
# intentionally excluded.) These are added to the REDUCED profile only: at the
# FULL profile's typemax(UInt64)/typemax(UInt128) boundaries, `isless`/`min`/`max`
# hit the same uncatchable OverflowError abort as the boundary skiplist cells
# (Issue #9422 class), which would abort the whole deterministic full probe.
const EXTRA_OP_SPECS = [
    ("isless", (a, b) -> isless(a, b)),
    ("min", (a, b) -> min(a, b)),
    ("max", (a, b) -> max(a, b)),
]

op_specs_for(profile) = profile == "full" ? BASE_OP_SPECS : vcat(BASE_OP_SPECS, EXTRA_OP_SPECS)

function usage()
    println("""
    Usage: julia --startup-file=no scripts/gen_numeric_matrix_fixture.jl [options]

    Options:
      --out-tsv PATH       expected matrix TSV path
                           default: $DEFAULT_TSV
      --out-fixture PATH   generated fixture summary path
                           default: $DEFAULT_FIXTURE
      --profile NAME       reduced (default) or full
      -h, --help           show this help
    """)
end

function parse_args(args)
    out_tsv = abspath(DEFAULT_TSV)
    out_fixture = abspath(DEFAULT_FIXTURE)
    profile = "reduced"
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
        elseif arg == "--profile"
            i += 1
            i <= length(args) || error("--profile requires a value")
            profile = args[i]
            profile in ("reduced", "full") || error("--profile must be reduced or full")
        elseif arg == "-h" || arg == "--help"
            usage()
            exit(0)
        else
            error("unknown argument: $arg")
        end
        i += 1
    end
    return out_tsv, out_fixture, profile
end

function tsv_escape(x)
    s = string(x)
    s = replace(s, "\\" => "\\\\")
    s = replace(s, "\t" => "\\t")
    s = replace(s, "\n" => "\\n")
    return s
end

function capture_result(op, left, right)
    try
        result = op(left, right)
        return ("ok", string(typeof(result)), repr(result), "none")
    catch err
        return ("error", "none", "none", string(typeof(err)))
    end
end

function collect_rows(profile)
    specs = profile == "full" ? FULL_VALUE_SPECS : REDUCED_VALUE_SPECS
    op_specs = op_specs_for(profile)
    rows = Vector{NTuple{13,String}}()
    for left_spec in specs
        left = left_spec.factory()
        left_repr = repr(left)
        for right_spec in specs
            right = right_spec.factory()
            right_repr = repr(right)
            for (op_name, op) in op_specs
                status, result_type, result_repr, exception_type = capture_result(op, left, right)
                push!(rows, (
                    left_spec.type_name,
                    right_spec.type_name,
                    op_name,
                    left_repr,
                    right_repr,
                    status,
                    result_type,
                    result_repr,
                    exception_type,
                    left_spec.label,
                    right_spec.label,
                    left_spec.expr,
                    right_spec.expr,
                ))
            end
        end
    end
    return rows
end

function write_tsv(path, rows)
    mkpath(dirname(path))
    open(path, "w") do io
        println(io, join(("left_type", "right_type", "operator", "left_value", "right_value", "status", "result_type", "result_value", "exception_type", "left_label", "right_label", "left_expr", "right_expr"), '\t'))
        for row in rows
            println(io, join(map(tsv_escape, row), '\t'))
        end
    end
end

function write_fixture(path, row_count, digest)
    mkpath(dirname(path))
    open(path, "w") do io
        println(io, "# Generated by scripts/gen_numeric_matrix_fixture.jl for Issue #8696.")
        println(io, "# The full upstream oracle matrix lives in numeric_matrix_reduced_8696.tsv.")
        println(io)
        println(io, "using Test")
        println(io)
        println(io, "const NUMERIC_MATRIX_REDUCED_8696_ROWS = $row_count")
        println(io, "const NUMERIC_MATRIX_REDUCED_8696_SHA256 = \"$digest\"")
        println(io)
        println(io, "@testset \"numeric matrix reduced oracle metadata (Issue #8696)\" begin")
        println(io, "    @test NUMERIC_MATRIX_REDUCED_8696_ROWS == $row_count")
        println(io, "    @test NUMERIC_MATRIX_REDUCED_8696_SHA256 == \"$digest\"")
        println(io, "end")
        println(io)
        println(io, "true")
    end
end

function main(args)
    out_tsv, out_fixture, profile = parse_args(args)
    rows = collect_rows(profile)
    write_tsv(out_tsv, rows)
    digest = bytes2hex(open(sha256, out_tsv))
    if profile == "reduced"
        write_fixture(out_fixture, length(rows), digest)
    end
    println("numeric matrix ", profile, " rows: ", length(rows))
    println("tsv: ", out_tsv)
    profile == "reduced" && println("fixture: ", out_fixture)
    println("sha256: ", digest)
end

main(ARGS)

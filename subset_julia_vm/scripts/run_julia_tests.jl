#!/usr/bin/env julia
#
# Run fixture tests with official Julia interpreter
#
# Usage:
#   julia scripts/run_julia_tests.jl              # Run all tests
#   julia scripts/run_julia_tests.jl --json       # Output JSON for comparison
#   julia scripts/run_julia_tests.jl test_name    # Run specific test
#
# Supports both:
# - Single root manifest.toml (legacy mode)
# - Distributed manifest.toml files in each category directory

using TOML

const SCRIPT_DIR = @__DIR__
const FIXTURES_DIR = joinpath(dirname(SCRIPT_DIR), "tests", "fixtures")
const ROOT_MANIFEST_PATH = joinpath(FIXTURES_DIR, "manifest.toml")

struct TestCase
    name::String
    file::String
    expected::Float64
    description::String
    skip::Bool
end

struct TestResult
    name::String
    passed::Bool
    expected::Float64
    actual::Float64
    error::Union{String, Nothing}
end

"""
Load all test cases from root manifest and distributed category manifests.
"""
function load_manifest()
    # 1. Load root manifest (required for config)
    root_manifest = TOML.parsefile(ROOT_MANIFEST_PATH)
    epsilon = get(get(root_manifest, "config", Dict()), "epsilon", 1e-10)

    tests = TestCase[]

    # Add tests from root manifest (legacy support)
    for t in get(root_manifest, "tests", [])
        push!(tests, TestCase(
            t["name"],
            t["file"],
            Float64(t["expected"]),
            get(t, "description", ""),
            get(t, "skip", false)
        ))
    end

    # 2. Scan for category manifest.toml files
    for entry in readdir(FIXTURES_DIR; join=true)
        if isdir(entry)
            category_manifest_path = joinpath(entry, "manifest.toml")
            if isfile(category_manifest_path)
                category_name = basename(entry)
                try
                    category_manifest = TOML.parsefile(category_manifest_path)
                    for t in get(category_manifest, "tests", [])
                        file = t["file"]
                        # Prefix file path with category name if needed
                        if !contains(file, "/")
                            file = "$category_name/$file"
                        end
                        push!(tests, TestCase(
                            t["name"],
                            file,
                            Float64(t["expected"]),
                            get(t, "description", ""),
                            get(t, "skip", false)
                        ))
                    end
                catch e
                    @warn "Failed to parse $category_manifest_path" exception=e
                end
            end
        end
    end

    return tests, epsilon
end

function run_test(test::TestCase, epsilon::Float64)::TestResult
    file_path = joinpath(FIXTURES_DIR, test.file)

    try
        # Read and evaluate the Julia file
        source = read(file_path, String)

        # Evaluate in a fresh module to avoid namespace pollution
        m = Module()
        result = Base.eval(m, Meta.parse("begin\n$source\nend"))

        # Convert result to Float64 for comparison
        actual = Float64(result)
        passed = abs(actual - test.expected) < epsilon

        return TestResult(test.name, passed, test.expected, actual, nothing)
    catch e
        return TestResult(test.name, false, test.expected, NaN, sprint(showerror, e))
    end
end

function run_all_tests(; json_output::Bool=false, name_filter::Union{String, Nothing}=nothing)
    tests, epsilon = load_manifest()

    if name_filter !== nothing
        tests = Base.filter(t -> occursin(name_filter, t.name), tests)
    end

    results = TestResult[]
    skipped = 0

    for test in tests
        if test.skip
            skipped += 1
            continue
        end
        result = run_test(test, epsilon)
        push!(results, result)
    end

    if json_output
        print_json(results)
    else
        print_summary(results, skipped)
    end

    return all(r -> r.passed, results)
end

function print_summary(results::Vector{TestResult}, skipped::Int=0)
    passed = count(r -> r.passed, results)
    failed = count(r -> !r.passed, results)
    total = length(results)

    println("=" ^ 60)
    println("Julia Fixture Test Results")
    println("=" ^ 60)

    for result in results
        status = result.passed ? "✓" : "✗"
        if result.passed
            println("  $status $(result.name)")
        else
            println("  $status $(result.name)")
            println("      Expected: $(result.expected)")
            println("      Actual:   $(result.actual)")
            if result.error !== nothing
                println("      Error:    $(result.error)")
            end
        end
    end

    println("-" ^ 60)
    skip_str = skipped > 0 ? " | Skipped: $skipped" : ""
    println("Total: $total | Passed: $passed | Failed: $failed$skip_str")
    println("=" ^ 60)

    if failed > 0
        println("\nFailed tests:")
        for result in results
            if !result.passed
                println("  - $(result.name)")
            end
        end
    end
end

function print_json(results::Vector{TestResult})
    # Simple JSON output for comparison with Rust results
    println("{")
    println("  \"results\": [")
    for (i, result) in enumerate(results)
        comma = i < length(results) ? "," : ""
        error_str = result.error === nothing ? "null" : "\"$(escape_string(result.error))\""
        println("""    {"name": "$(result.name)", "passed": $(result.passed), "expected": $(result.expected), "actual": $(result.actual), "error": $error_str}$comma""")
    end
    println("  ]")
    println("}")
end

# Main entry point
function main()
    args = ARGS
    json_output = "--json" in args

    # Remove flags from args
    filter_args = Base.filter(a -> !startswith(a, "-"), args)
    filter_name = isempty(filter_args) ? nothing : first(filter_args)

    success = run_all_tests(; json_output=json_output, name_filter=filter_name)

    exit(success ? 0 : 1)
end

# Run if executed directly
if abspath(PROGRAM_FILE) == @__FILE__
    main()
end

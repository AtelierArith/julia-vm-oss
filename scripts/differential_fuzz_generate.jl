#!/usr/bin/env julia

# Deterministic program-level differential fuzz generator for Issues #8716/#9006.
# Output is TSV: case_seed, case_index, depth, source_b64.

using Base64

mutable struct Lcg
    state::UInt64
end

function next_u32!(rng::Lcg)::UInt32
    rng.state = rng.state * UInt64(6364136223846793005) + UInt64(1442695040888963407)
    return UInt32(rng.state >> 32)
end

rand_range!(rng::Lcg, n::Int)::Int = Int(next_u32!(rng) % UInt32(n)) + 1

abstract type Node end
struct LitNode <: Node
    text::String
end
struct VarNode <: Node
    name::String
end
struct BinNode <: Node
    op::String
    left::Node
    right::Node
end
struct CallNode <: Node
    fn::String
    arg::Node
end

render(node::LitNode)::String = node.text
render(node::VarNode)::String = node.name
render(node::BinNode)::String = "(" * render(node.left) * " " * node.op * " " * render(node.right) * ")"
render(node::CallNode)::String = node.fn * "(" * render(node.arg) * ")"

depth(node::LitNode)::Int = 1
depth(node::VarNode)::Int = 1
depth(node::BinNode)::Int = 1 + max(depth(node.left), depth(node.right))
depth(node::CallNode)::Int = 1 + depth(node.arg)

function literal!(rng::Lcg)::LitNode
    small = rand_range!(rng, 9) - 5
    if rand_range!(rng, 2) == 1
        return LitNode("Int64($small)")
    end
    return LitNode("Float64(" * string(small) * ".0)")
end

function leaf!(rng::Lcg)::Node
    choice = rand_range!(rng, 4)
    if choice == 1
        return VarNode("x")
    elseif choice == 2
        return VarNode("y")
    end
    return literal!(rng)
end

function expr!(rng::Lcg, max_depth::Int)::Node
    if max_depth <= 1
        return leaf!(rng)
    end
    choice = rand_range!(rng, 5)
    if choice <= 2
        ops = ["+", "-", "*"]
        return BinNode(ops[rand_range!(rng, length(ops))], expr!(rng, max_depth - 1), expr!(rng, max_depth - 1))
    elseif choice == 3
        return CallNode("g8716", expr!(rng, max_depth - 1))
    elseif choice == 4
        return CallNode("f8716", expr!(rng, max_depth - 1))
    end
    return leaf!(rng)
end

function shrink_nodes(node::Node)::Vector{Node}
    out = Node[]
    if node isa BinNode
        n = node::BinNode
        push!(out, n.left)
        push!(out, n.right)
        append!(out, [BinNode(n.op, child, n.right) for child in shrink_nodes(n.left)])
        append!(out, [BinNode(n.op, n.left, child) for child in shrink_nodes(n.right)])
    elseif node isa CallNode
        n = node::CallNode
        push!(out, n.arg)
        append!(out, [CallNode(n.fn, child) for child in shrink_nodes(n.arg)])
    end
    return out
end

function case_rng(seed::UInt64, index::Int)::Lcg
    mixed = seed ⊻ (UInt64(index) * UInt64(0x9e3779b97f4a7c15))
    return Lcg(mixed)
end

function case_expr(seed::UInt64, index::Int, max_depth::Int)::Node
    rng = case_rng(seed, index)
    return expr!(rng, max_depth)
end

function program_template(index::Int)::Symbol
    templates = [:numeric_let, :if_function, :for_function, :while_function]
    return templates[((index - 1) % length(templates)) + 1]
end

function helper_defs()::String
    return """
f8716(v) = v + Int64(1)
g8716(v) = f8716(v) * Int64(2)
"""
end

function render_numeric_let(body::String)::String
    return """
$(helper_defs())

let x = Int64(3), y = Float64(2.0)
    println($body)
end
"""
end

function render_if_function(body::String)::String
    return """
$(helper_defs())

function p9006_if(x, y)
    value = $body
    if x > Int64(0)
        value = f8716(value)
    else
        value = g8716(value)
    end
    return value
end

let x = Int64(3), y = Float64(2.0)
    println(p9006_if(x, y))
end
"""
end

function render_for_function(body::String)::String
    return """
$(helper_defs())

function p9006_for(n, x, y)
    acc = $body
    for i = 1:n
        acc = acc + i
    end
    return acc
end

let x = Int64(3), y = Float64(2.0)
    println(p9006_for(Int64(3), x, y))
end
"""
end

function render_while_function(body::String)::String
    return """
$(helper_defs())

function p9006_while(n, x, y)
    acc = $body
    i = Int64(1)
    while i <= n
        acc = acc + i
        i = i + Int64(1)
    end
    return acc
end

let x = Int64(3), y = Float64(2.0)
    println(p9006_while(Int64(3), x, y))
end
"""
end

function render_program(node::Node, template::Symbol)::String
    body = render(node)
    if template == :numeric_let
        return render_numeric_let(body)
    elseif template == :if_function
        return render_if_function(body)
    elseif template == :for_function
        return render_for_function(body)
    elseif template == :while_function
        return render_while_function(body)
    end
    error("unknown program template: " * string(template))
end

function parse_args(argv)
    opts = Dict{String,String}(
        "seed" => "1",
        "count" => "1",
        "max-depth" => "4",
        "mode" => "programs",
        "case-index" => "1",
    )
    i = 1
    while i <= length(argv)
        key = argv[i]
        if !startswith(key, "--") || i == length(argv)
            error("expected --key value, got: " * key)
        end
        opts[key[3:end]] = argv[i + 1]
        i += 2
    end
    return opts
end

function print_row(seed::UInt64, index::Int, node::Node)
    source = render_program(node, program_template(index))
    encoded = base64encode(source)
    println(string(seed), '\t', string(index), '\t', string(depth(node)), '\t', encoded)
end

function main(argv)
    opts = parse_args(argv)
    seed = parse(UInt64, opts["seed"])
    count = parse(Int, opts["count"])
    max_depth = parse(Int, opts["max-depth"])
    mode = opts["mode"]
    println("case_seed\tcase_index\tdepth\tsource_b64")
    if mode == "programs"
        for index in 1:count
            print_row(seed, index, case_expr(seed, index, max_depth))
        end
    elseif mode == "shrinks"
        index = parse(Int, opts["case-index"])
        node = case_expr(seed, index, max_depth)
        for child in shrink_nodes(node)
            print_row(seed, index, child)
        end
    else
        error("unknown mode: " * mode)
    end
end

main(ARGS)

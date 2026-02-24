# This file is a part of SubsetJuliaVM.
# Based on Julia's base/meta.jl and base/expr.jl

# ============================================================================
# gensym - implemented as VM builtin (placeholder stub in prelude)
# ============================================================================
# gensym() generates unique symbols for macro hygiene.
# In SubsetJuliaVM, gensym is implemented as a VM builtin for use in fixture tests
# and user code. These prelude stubs exist only for documentation purposes.
#
# Note: The Pure Julia implementation using global counters cannot be used in
# prelude because global mutable state (const arrays) cannot be accessed from
# function bodies in prelude code (Issue #1443).
#
# For full gensym functionality, use it in user code (not prelude) where
# the VM builtin handles symbol generation with its internal counter.

# Stub documentation for gensym (actual implementation is VM builtin)
# These stubs exist to avoid "Undefined variable" errors during prelude compilation.

# ============================================================================
# Pure Julia implementation of copy(::Expr) - AST deep copy
# Based on Julia's base/expr.jl implementation
# ============================================================================

"""
    copy(e::Expr) -> Expr

Create a deep copy of an expression. The copy is independent of the original,
so modifications to the copy do not affect the original expression.

# Examples
```julia
julia> ex = :(x + 1)
:(x + 1)

julia> ex2 = copy(ex)
:(x + 1)

julia> ex2.args[2] = :y
:y

julia> ex  # Original is unchanged
:(x + 1)

julia> ex2
:(y + 1)
```
"""
function copy(e::Expr)
    # Delegate to deepcopy which is already implemented as a Rust builtin
    # and properly handles recursive Expr copying
    return deepcopy(e)
end

# ============================================================================
# Pure Julia implementation of Meta functions (defined at top level for
# Base function lookup, then re-exported through Meta module)
# ============================================================================

"""
    _meta_quot(ex)

Internal implementation of Meta.quot - creates a quoted expression.
"""
_meta_quot(ex) = Expr(Symbol("quote"), ex)

"""
    _meta_isexpr(ex, head)
    _meta_isexpr(ex, head, n)

Internal implementation of Meta.isexpr - checks if ex is an Expr with given head.
"""
function _meta_isexpr(ex, head::Symbol)
    ex isa Expr && ex.head === head
end

function _meta_isexpr(ex, head::Symbol, n::Int64)
    ex isa Expr && ex.head === head && length(ex.args) == n
end

# Support for collections of heads (Array or Tuple)
function _meta_isexpr(ex, heads::Array)
    if !(ex isa Expr)
        return false
    end
    for h in heads
        if ex.head === h
            return true
        end
    end
    return false
end

function _meta_isexpr(ex, heads::Tuple)
    if !(ex isa Expr)
        return false
    end
    for h in heads
        if ex.head === h
            return true
        end
    end
    return false
end

function _meta_isexpr(ex, heads::Array, n::Int64)
    if !(ex isa Expr) || length(ex.args) != n
        return false
    end
    for h in heads
        if ex.head === h
            return true
        end
    end
    return false
end

function _meta_isexpr(ex, heads::Tuple, n::Int64)
    if !(ex isa Expr) || length(ex.args) != n
        return false
    end
    for h in heads
        if ex.head === h
            return true
        end
    end
    return false
end

"""
    _meta_unblock(ex)

Internal implementation of Meta.unblock - peel away redundant block expressions.
Removes LineNumberNode and :line expressions from blocks.
"""
function _meta_unblock(ex)
    _meta_isexpr(ex, :block) || return ex
    # Filter out LineNumberNode and :line expressions
    # Note: Use index-based loop to work around VM iteration issue with arrays
    args = ex.args
    n = length(args)
    exs = []
    for i in 1:n
        arg = args[i]
        is_linenumber = isa(arg, LineNumberNode)
        is_line = _meta_isexpr(arg, :line)
        if !is_linenumber && !is_line
            push!(exs, arg)
        end
    end
    length(exs) == 1 || return ex
    return _meta_unblock(exs[1])
end

"""
    _meta_unescape(ex)

Internal implementation of Meta.unescape - peel away escape expressions.
"""
function _meta_unescape(ex)
    ex = _meta_unblock(ex)
    while _meta_isexpr(ex, :escape)
        ex = _meta_unblock(ex.args[1])
    end
    return ex
end

# Helper function to generate spaces for indentation
# Note: Uses string() instead of * operator due to type inference issues in loops
function _meta_spaces(n::Int64)
    s = ""
    for _ in 1:n
        s = string(s, " ")
    end
    return s
end

"""
    _meta_show_sexpr_impl(ex, indent)

Internal implementation helper for show_sexpr with indentation tracking.
Uses indent width of 2.
Note: Uses explicit type checks instead of multiple dispatch to work around
      indirect call dispatch bug in SubsetJuliaVM.
"""
function _meta_show_sexpr_impl(ex, indent::Int64)
    # Explicit type dispatch to work around indirect call dispatch bug
    if ex isa Expr
        inner = indent + 2
        head = ex.head
        # Use index-based loop to work around VM iteration issue with arrays
        args = ex.args
        n = length(args)
        print("(")
        _meta_show_sexpr_impl(head, inner)
        for i in 1:n
            arg = args[i]
            if head === :block
                print(",\n")
                print(_meta_spaces(inner))
            else
                print(", ")
            end
            _meta_show_sexpr_impl(arg, inner)
        end
        if n == 0
            print(",)")
        else
            if head === :block
                print("\n")
                print(_meta_spaces(indent))
            end
            print(")")
        end
    elseif ex isa QuoteNode
        inner = indent + 2
        print("(:quote, #QuoteNode\n")
        print(_meta_spaces(inner))
        _meta_show_sexpr_impl(ex.value, inner)
        print("\n")
        print(_meta_spaces(indent))
        print(")")
    else
        # Default: just print the value (Symbol, Int, String, etc.)
        print(ex)
    end
end

"""
    _meta_show_sexpr(ex)

Internal implementation of Meta.show_sexpr - show expression as S-expression.
"""
function _meta_show_sexpr(ex)
    _meta_show_sexpr_impl(ex, 0)
    println()
end

# ============================================================================
# Meta module - re-exports the internal implementations
# ============================================================================

"""
Convenience functions for metaprogramming.
"""
module Meta

export quot,
       isexpr,
       isidentifier,
       isoperator,
       isunaryoperator,
       isbinaryoperator,
       ispostfixoperator,
       unblock,
       unescape,
       show_sexpr,
       lower

# parse is public but not exported (use Meta.parse)

"""
    Meta.quot(ex)::Expr

Quote expression `ex` to produce an expression with head `quote`. This can for
instance be used to represent objects of type `Expr` in the AST.

# Examples
```julia
julia> eval(Meta.quot(:x))
:x

julia> Meta.quot(:(1+2))
:(quote
    1 + 2
end)
```
"""
# Delegate to top-level Pure Julia implementation
quot(ex) = _meta_quot(ex)

"""
    Meta.isexpr(ex, head)::Bool
    Meta.isexpr(ex, head, n)::Bool

Return `true` if `ex` is an `Expr` with the given type `head` and optionally that
the argument list is of length `n`.

# Examples
```julia
julia> ex = :(f(x))
:(f(x))

julia> Meta.isexpr(ex, :call)
true

julia> Meta.isexpr(ex, :block)
false

julia> Meta.isexpr(ex, :call, 2)
true

julia> Meta.isexpr(ex, [:block, :call])  # multiple possible heads
true
```
"""
# Delegate to top-level Pure Julia implementation
isexpr(ex, head) = _meta_isexpr(ex, head)
isexpr(ex, head, n) = _meta_isexpr(ex, head, n)

"""
    Meta.isidentifier(s) -> Bool

Return whether the symbol or string `s` contains characters that are parsed as
a valid ordinary identifier (not a binary/unary operator) in Julia code.

# Examples
```julia
julia> Meta.isidentifier(:x), Meta.isidentifier("1x")
(true, false)
```
"""
# isidentifier implemented as Rust builtin

"""
    Meta.isoperator(s::Symbol) -> Bool

Return `true` if the symbol can be used as an operator, `false` otherwise.
"""
# isoperator implemented as Rust builtin

"""
    Meta.isunaryoperator(s::Symbol) -> Bool

Return `true` if the symbol can be used as a unary (prefix) operator.
"""
# isunaryoperator implemented as Rust builtin

"""
    Meta.isbinaryoperator(s::Symbol) -> Bool

Return `true` if the symbol can be used as a binary (infix) operator.
"""
# isbinaryoperator implemented as Rust builtin

"""
    Meta.ispostfixoperator(s) -> Bool

Return `true` if the symbol can be used as a postfix operator.
"""
# ispostfixoperator implemented as Rust builtin

"""
    Meta.parse(str::AbstractString)

Parse the expression string greedily, returning a single expression.

# Examples
```julia
julia> Meta.parse("x = 3")
:(x = 3)

julia> Meta.parse("1 + 2")
:(1 + 2)
```
"""
function parse(str::AbstractString)
    # Calls the Rust builtin _meta_parse
    return _meta_parse(str)
end

"""
    Meta.parse(str::AbstractString, start::Integer)

Parse the expression string starting at the given position.
Returns a tuple `(expr, next_pos)`.

# Examples
```julia
julia> Meta.parse("x = 3; y = 4", 1)
(:(x = 3), 7)
```
"""
function parse(str::AbstractString, start::Integer)
    # Calls the Rust builtin _meta_parse_at
    return _meta_parse_at(str, start)
end

"""
    Meta.unblock(ex)

Peel away redundant block expressions.

Specifically, checks if `ex` is a block expression with a single non-line-number
argument, and if so, returns that argument. If the argument is also a block,
this is recursively applied.

# Examples
```julia
julia> ex = quote
           1 + 2
       end
quote
    #= ... =#
    1 + 2
end

julia> Meta.unblock(ex)
:(1 + 2)
```
"""
unblock(ex) = _meta_unblock(ex)

"""
    Meta.unescape(ex)

Peel away escape expressions.

Combines `unblock` with removal of `:escape` wrappers.

# Examples
```julia
julia> Meta.unescape(Expr(:escape, :(x + 1)))
:(x + 1)
```
"""
unescape(ex) = _meta_unescape(ex)

"""
    Meta.show_sexpr(ex)

Show expression `ex` as a lisp style S-expression.

# Examples
```julia
julia> Meta.show_sexpr(:(f(x, g(y))))
(:call, :f, :x, (:call, :g, :y))
```
"""
show_sexpr(ex) = _meta_show_sexpr(ex)

"""
    Meta.lower(m::Module, x)

Lower the expression `x` in the context of module `m`.
Returns the lowered IR representation as an Expr.

For simple values (literals, symbols), returns them unchanged.
For expressions, performs macro expansion and desugaring to produce
a lowered form suitable for compilation.

# Examples
```julia
julia> Meta.lower(Main, :(1 + 2))
:(call(+, 1, 2))

julia> Meta.lower(Main, 42)
42

julia> Meta.lower(Main, :x)
:x
```

!!! note
    In SubsetJuliaVM, the second argument (module) is not used because
    there is only a single global namespace. The module argument is
    accepted for compatibility with Julia's standard library.
"""
function lower(m::Module, x)
    # Calls the Rust builtin _meta_lower
    # The module argument is currently ignored in SubsetJuliaVM
    return _meta_lower(x)
end

# Single-argument version for convenience (defaults to Main module)
lower(x) = lower(Main, x)

end # module Meta

# ============================================================================
# include_string and evalfile - Dynamic code evaluation
# Based on Julia's base/loading.jl
# ============================================================================

"""
    include_string([mapexpr::Function,] m::Module, code::AbstractString, filename::AbstractString="string")

Parse and evaluate all expressions in `code` in the global scope of module `m`.
Return the value of the last expression.

The optional first argument `mapexpr` can be used to transform the parsed expressions
before they are evaluated. In SubsetJuliaVM, this argument is accepted for API
compatibility but currently ignored.

# Examples
```julia
julia> include_string(Main, "x = 1 + 1")
2

julia> x
2

julia> include_string(Main, "y = 2\\nz = y + 3")
5
```

!!! note
    In SubsetJuliaVM, the module argument is accepted for API compatibility but
    there is only a single global namespace (Main).
"""
function include_string(m::Module, code::AbstractString, filename::AbstractString="string")
    result = nothing
    pos = 1
    code_length = length(code)

    while pos <= code_length
        # Parse one expression starting at pos
        parsed = Meta.parse(code, pos)
        expr = parsed[1]
        next_pos = parsed[2]

        # Check if we got nothing (end of string or whitespace-only)
        if expr === nothing
            break
        end

        # Evaluate the expression
        result = eval(expr)

        # Check for progress to avoid infinite loop
        if next_pos <= pos
            break
        end

        pos = next_pos
    end

    return result
end

# Version with mapexpr function (accepted for API compatibility, mapexpr is ignored)
function include_string(mapexpr::Function, m::Module, code::AbstractString, filename::AbstractString="string")
    # Note: In SubsetJuliaVM, we don't apply mapexpr transformation.
    # This overload exists for API compatibility with official Julia.
    return include_string(m, code, filename)
end

# Convenience version without module (defaults to Main)
function include_string(code::AbstractString, filename::AbstractString="string")
    return include_string(Main, code, filename)
end

"""
    evalfile(path::AbstractString, args::Vector{String}=String[])

Evaluate all expressions in the given file and return the value of the last one.
This is equivalent to `include_string(Main, read(path, String), path)`.

The optional `args` parameter can provide command-line arguments to the script
(accessible via `ARGS`). In SubsetJuliaVM, this is currently not supported.

# Examples
```julia
julia> # If test.jl contains: x = 1 + 2
julia> evalfile("test.jl")
3
```

!!! warning
    This function reads and evaluates arbitrary code from a file. Only use with
    trusted files.
"""
function evalfile(path::AbstractString)
    code = read(path, String)
    return include_string(Main, code, path)
end

# Version with args (accepted for API compatibility)
function evalfile(path::AbstractString, args::AbstractVector)
    # Note: ARGS is not modifiable in SubsetJuliaVM
    code = read(path, String)
    return include_string(Main, code, path)
end

import MacroTools

using Base: ismutabletype

"""
    @attributes typedef

Ensure that a mutable struct has storage for attributes.
"""
macro attributes(expr)
   # The following two lines are borrowed from Base.@kwdef
   expr = macroexpand(__module__, expr) # to expand @static
   if expr isa Expr && expr.head === :struct && expr.args[1]
      # Handle the following usage:
      #    @attributes mutable struct Type ... end

      # add member for storing the attributes
      push!(expr.args[3].args, :(__attrs::Dict{Symbol,Any}))
      return quote
        Base.@__doc__($(esc(expr)))
      end
   elseif expr isa Expr && expr.head === :struct && !expr.args[1] && all(x -> x isa LineNumberNode, expr.args[3].args)
      # Ignore application to singleton types:
      #    @attributes struct Singleton end
      return esc(expr)
   elseif expr isa Symbol || (expr isa Expr && expr.head === :. &&
                              length(expr.args) == 2 && expr.args[2] isa QuoteNode) ||
                              (expr isa Expr && expr.head === :curly &&
                                expr.args[1] isa Symbol || (expr.args[1] isa Expr && expr.args[1].head === :. &&
                                length(expr.args[1].args) == 2 && expr.args[1].args[2] isa QuoteNode))
      # Handle the following usage:
      #    @attributes Type
      #    @attributes Module.Type
      #    @attributes [Module[.Submodule].]Type{T}
      # Workaround: this upstream branch generates quoted typed parameters with
      # interpolated type annotations, which sjulia cannot lower yet. (Issue #7933)
      error("@attributes Type is not supported yet")
   end
   error("attributes can only be attached to mutable structs")
end

_is_attribute_storing_type(::Type{T}) where T = Base.issingletontype(T) || (isstructtype(T) && ismutabletype(T) && hasfield(T, :__attrs))

# Workaround: upstream uses typed `Dict{...}()` constructors here, but typed
# Dict constructors with DataType parameters are not supported yet. (Issue #7934)
const _singleton_attr_storage = Dict()

function _get_attributes(G::T) where T
   if Base.issingletontype(T)
      # Workaround: upstream indexes singleton attribute storage by the
      # DataType-valued generic parameter `T`, but generic DataType Dict keys are
      # not supported yet. (Issue #7940)
      return nothing
   end
   is_attribute_storing_type(T) || error("attributes storage not supported")
   return isdefined(G, :__attrs) ? G.__attrs : nothing
end

function _get_attributes!(G::T) where T
   if Base.issingletontype(T)
      # Workaround: upstream indexes singleton attribute storage by the
      # DataType-valued generic parameter `T`, but generic DataType Dict keys are
      # not supported yet. (Issue #7940)
      error("singleton attribute storage is not supported yet")
   end
   is_attribute_storing_type(T) || error("attributes storage not supported")
   # Workaround: upstream lazily initializes `G.__attrs` here, but generic field
   # assignment to a macro-injected field is not supported yet. (Issue #7941)
   error("attribute mutation is not supported yet")
end

is_attribute_storing(G::T) where T = is_attribute_storing_type(T)

is_attribute_storing_type(::Type{T}) where T = _is_attribute_storing_type(T)

function has_attribute(G::Any, attr::Symbol)
   D = _get_attributes(G)
   return D isa Dict && haskey(D, attr)
end

function get_attribute(f, G::Any, attr::Symbol)
   D = _get_attributes(G)
   D isa Dict && return get(f, D, attr)
   return f()
end

function get_attribute(G::Any, attr::Symbol, default::Any = nothing)
   D = _get_attributes(G)
   D isa Dict && return get(D, attr, default)
   return default
end

function get_attribute(G::Any, attr::Symbol, default::Symbol)
   D = _get_attributes(G)
   D isa Dict && return get(D, attr, default)
   return default
end

function get_attribute!(f, G::Any, attr::Symbol)
   D = _get_attributes!(G)
   if !haskey(D, attr)
      D[attr] = f()
   end
   return D[attr]
end

function set_attribute!(G::Any, attr::Symbol, val)
   D = _get_attributes!(G)
   D[attr] = val
   return val
end

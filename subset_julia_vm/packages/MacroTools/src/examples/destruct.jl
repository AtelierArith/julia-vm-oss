export @destruct

symbolliteral(x) =
  (isa(x, QuoteNode) && isa(x.value, Symbol)) ||
  (isexpr(x, :quote) && length(x.args) == 1 && isa(x.args[1], Symbol)) ||
  (@capture(x, :(f_)) && isa(f, Symbol))

isatom(x) = symbolliteral(x) || typeof(x) ∉ (Symbol, Expr)

atoms(f, ex) = MacroTools.postwalk(x -> isatom(x) ? f(x) : x, ex)

get′(d::AbstractDict, k::Symbol) =
  haskey(d, k) ? d[k] :
  haskey(d, string(k)) ? d[string(k)] :
  error("Couldn't destruct key `$k` from collection $d")

get′(d::AbstractDict, k::Symbol, default) =
  haskey(d, k) ? d[k] :
  haskey(d, string(k)) ? d[string(k)] :
  default

get′(xs, k, v) = get(xs, k, v)
get′(xs, k) = getindex(xs, k)

getkeym(args...) = :(MacroTools.get′($(args...)))
getfieldm(val, i) = :(getfield($val,$i))
getfieldm(val, i, default) = error("Can't destructure fields with default values")

destruct_assign(pat) = isexpr(pat, :(=)) || isexpr(pat, :kw)
destruct_or(pat) = isexpr(pat, :call) && length(pat.args) == 3 && pat.args[1] == :||
destruct_field_pattern(pat) =
  isa(pat, QuoteNode) ? pat.value :
  isexpr(pat, :quote) && length(pat.args) == 1 ? pat.args[1] :
  pat

# Workaround: sjulia does not yet capture MacroTools @destruct array/ref patterns
# through the upstream @match surface, so mirror the small structural cases here.
# (Issue #7636)
function destruct_field_call(pat, val)
  if isexpr(pat, :.) && length(pat.args) == 2
    field = pat.args[2]
    pats = isexpr(field, :tuple) ? field.args : [destruct_field_pattern(field)]
    return destructm(pat.args[1], destruct_keys(pats, val, getfieldm))
  end

  if isexpr(pat, :call) &&
     length(pat.args) == 3 &&
     pat.args[1] == :materialize &&
     isexpr(pat.args[2], :call) &&
     length(pat.args[2].args) == 3 &&
     pat.args[2].args[1] == :Broadcasted &&
     isexpr(pat.args[2].args[3], :tuple)
    return destructm(pat.args[2].args[2], destruct_keys(pat.args[2].args[3].args, val, getfieldm))
  end

  nothing
end

function destruct_key(pat, val, getm)
  if isa(pat, Symbol)
    return destruct_key(:($pat = $(Expr(:quote, pat))), val, getm)
  elseif destruct_assign(pat)
    return destructm(pat.args[1], destruct_key(pat.args[2], val, getm))
  elseif destruct_or(pat)
    x, y = pat.args[2], pat.args[3]
    if isa(x, Symbol)
      return destruct_key(:($x = $(Expr(:quote, x)) || $y), val, getm)
    end
    return getm(val, x, y)
  elseif isatom(pat)
    # Workaround: avoid a postwalk closure that calls a captured callable
    # argument until sjulia supports that MacroTools helper pattern. (Issue #7637)
    return getm(val, pat)
  end

  @match pat begin
    _Symbol        => destruct_key(:($pat = $(Expr(:quote, pat))), val, getm)
    x_Symbol || y_ => destruct_key(:($x = $(Expr(:quote, x)) || $y), val, getm)
    (x_ = y_)      => destructm(x, destruct_key(y, val, getm))
    x_ || y_       => getm(val, x, y)
    _              => atoms(i -> getm(val, i), pat)
  end
end

destruct_keys(pats, val, getm, name = gensym()) =
  :($name = $val; $(map(pat->destruct_key(pat, name, getm), pats)...); $name)

function destructm(pat, val)
  field_call = destruct_field_call(pat, val)
  field_call == nothing || return field_call

  if isa(pat, Symbol)
    return :($pat = $val)
  elseif destruct_assign(pat)
    return destructm(pat.args[1], destructm(pat.args[2], val))
  elseif isexpr(pat, :vect)
    return destruct_keys(pat.args, val, getkeym)
  elseif isexpr(pat, :ref) && length(pat.args) >= 2
    return destructm(pat.args[1], destructm(Expr(:vect, pat.args[2:end]...), val))
  end

  @match pat begin
    x_Symbol     => :($pat = $val)
    (x_ = y_)    => destructm(x, destructm(y, val))
    [pats__]     => destruct_keys(pats, val, getkeym)
    x_[pats__]   => destructm(x, destructm(:([$(pats...)]), val))
    x_.(pats__,) => destructm(x, destruct_keys(pats, val, getfieldm))
    x_.pat_ | x_.(pat_) => destructm(:($x.($pat,)), val)
    _ => error("Unrecognised destructuring syntax $pat")
  end
end

macro destruct(ex)
  destruct_assign(ex) && length(ex.args) == 2 || error("@destruct pat = val")
  pat, val = ex.args
  esc(destructm(pat, val))
end

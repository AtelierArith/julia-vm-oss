# Test exception types: ErrorException, DimensionMismatch, KeyError, StringIndexError, AssertionError, DivideError, DomainError, InexactError, TypeError, ArgumentError, EOFError, UndefKeywordError

# Test ErrorException
ee = ErrorException("something went wrong")
@assert ee.msg == "something went wrong"

# Test DimensionMismatch
e1 = DimensionMismatch("arrays have different dimensions")
@assert e1.msg == "arrays have different dimensions"

e2 = DimensionMismatch()
@assert e2.msg == ""

# Test KeyError
k = KeyError("missing_key")
@assert k.key == "missing_key"

# Test StringIndexError
s = StringIndexError("hello", 3)
@assert s.string == "hello"
@assert s.index == 3

# Test AssertionError
a1 = AssertionError("assertion failed")
@assert a1.msg == "assertion failed"

a2 = AssertionError()
@assert a2.msg == ""

# Test DivideError
d = DivideError()
@assert typeof(d) == DivideError

# Test DomainError
dom1 = DomainError(-1, "sqrt requires non-negative argument")
@assert dom1.val == -1
@assert dom1.msg == "sqrt requires non-negative argument"

dom2 = DomainError(-1.0)
@assert dom2.val == -1.0
@assert dom2.msg == ""

# Test InexactError
ie = InexactError(:Int64, Int64, 3.14)
@assert ie.func == :Int64
@assert ie.T == Int64
@assert ie.val == 3.14

# Test TypeError
te = TypeError(:typeassert, "in typeassert", Int64, "foo")
@assert te.func == :typeassert
@assert te.context == "in typeassert"
@assert te.expected == Int64
@assert te.got == "foo"

# Test ArgumentError
ae = ArgumentError("invalid argument")
@assert ae.msg == "invalid argument"

# Test EOFError
eof = EOFError()
@assert typeof(eof) == EOFError

# Test UndefKeywordError
uke = UndefKeywordError(:x)
@assert uke.var == :x

# Test Exception abstract type exists
# (Exception is the supertype of all exception types)
@assert ErrorException <: Exception
@assert DimensionMismatch <: Exception
@assert KeyError <: Exception
@assert StringIndexError <: Exception
@assert AssertionError <: Exception
@assert DivideError <: Exception
@assert DomainError <: Exception
@assert InexactError <: Exception
@assert TypeError <: Exception
@assert ArgumentError <: Exception
@assert EOFError <: Exception
@assert UndefKeywordError <: Exception

println("All exception type tests passed!")

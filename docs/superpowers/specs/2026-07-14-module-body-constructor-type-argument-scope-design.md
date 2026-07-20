# Module-body constructor type-argument scope design

## Problem

An explicit parametric constructor call compiled in a module body parses an
unknown bare type name such as `V` as `TypeExpr::TypeVar("V")`. The constructor
pipeline resolves the constructor base through `current_module_path`, but it
does not apply the same lexical resolution to the type arguments. A single
module can pass accidentally through the sole-candidate runtime-validation
fallback. Two sibling modules with the same bare constructor and type names
expose the bug: the qualified constructor table and the shared bare table
produce distinct fallback candidates, so the call fails closed with a
`MethodError`.

## Design

At the entry to `try_compile_parametric_constructor_call`, preserve the
runtime-dependence decision from the parsed spelling, then canonicalize the
constructor base and its statically named type arguments through the compiler's
existing lexical type-object resolvers:

- A bare constructor base defined in the current module becomes its qualified
  owner identity, such as `A.C`.
- A bare type object visible in the current module becomes its qualified
  nominal identity, such as `A.V`.
- A parameterized type recursively qualifies both its base and its parameters,
  such as `Wrap{V}` becoming `A.Wrap{A.V}`.
- Active `where` parameters, local runtime values, numeric/value parameters,
  and runtime expressions remain unchanged.
- Already-qualified visible type names remain stable.
- Constructor method candidates use the qualified owner table when it exists.
  The legacy short-name table is only a fallback when no qualified table is
  registered, preventing a sibling module's later bare alias from leaking into
  the call.

Runtime-dependence classification happens before qualification because a bare
user type such as `V` still needs the shared-context-aware runtime validator;
the generic static method-table subtype checker cannot prove module-local
nominal ancestry. Constructor-bound matching, static-parameter binding
emission, and concrete instantiation consume the qualified identity.

## Alternatives rejected

Resolving names only during constructor-bound matching would leave emitted
static bindings and instantiated identities unqualified. Rewriting the source
call string in `compile_call` would duplicate structured type-expression
parsing and make nested parameters brittle.

## Tests

Add an upstream-parity `struct` fixture with two sibling modules that reuse the
same bare `Bound`, `V`, `Wrap`, and constructor names. Assert that top-level
module-body calls select their owning inner constructor for both `C{V}()` and
`C{Wrap{V}}()`. Retain a module-function control to ensure ordinary function
compilation continues to resolve the same names.

The externally qualified constructor regression mentioned in #11034 is tracked
separately by #8516 and is not required for this fix unless the shared
canonicalization naturally reaches that path.

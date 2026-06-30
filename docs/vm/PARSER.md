# Parser: Dispatch Decision Table

This document describes the parser's top-level dispatch logic, which determines how different Julia syntax patterns are routed to their respective parsing functions.

## Overview

The `parse_top_level_item()` function in `subset_julia_vm_parser/src/parser/mod.rs` is the central dispatch point for parsing Julia source code. It examines the current token and determines which parsing function to call.

## Key Files

- `subset_julia_vm_parser/src/parser/mod.rs` - Main parser and `parse_top_level_item()` dispatch
- `subset_julia_vm_parser/src/token/mod.rs` - Token definitions and helper methods

## Dispatch Decision Table

The following table describes how the parser routes different syntax patterns:

| Token | Next Token | Dispatch Target | Example | NodeKind |
|-------|------------|-----------------|---------|----------|
| `function` | any | `parse_function_definition()` | `function f() end` | `FunctionDefinition` |
| `macro` | any | `parse_macro_definition()` | `macro m() end` | `MacroDefinition` |
| `struct` | any | `parse_struct_definition()` | `struct Point x end` | `StructDefinition` |
| `mutable` | any | `parse_struct_definition()` | `mutable struct S x end` | `MutableStructDefinition` |
| `abstract` | any | `parse_abstract_definition()` | `abstract type T end` | `AbstractDefinition` |
| `primitive` | any | `parse_primitive_definition()` | `primitive type T 32 end` | `PrimitiveDefinition` |
| `module` | any | `parse_module_definition()` | `module M end` | `ModuleDefinition` |
| `baremodule` | any | `parse_module_definition()` | `baremodule M end` | `BaremoduleDefinition` |
| `if` | any | `parse_if_statement()` | `if x y end` | `IfStatement` |
| `for` | any | `parse_for_statement()` | `for i in 1:10 end` | `ForStatement` |
| `while` | any | `parse_while_statement()` | `while cond end` | `WhileStatement` |
| `try` | any | `parse_try_statement()` | `try ... catch end` | `TryStatement` |
| `begin` | any | `parse_begin_block()` | `begin x end` | `BeginBlock` |
| `let` | any | `parse_let_expression()` | `let x = 1 end` | `LetExpression` |
| `quote` | any | `parse_quote_expression()` | `quote x end` | `QuoteExpression` |
| `return` | any | `parse_return_statement()` | `return x` | `ReturnStatement` |
| `break` | any | `parse_break_statement()` | `break` | `BreakStatement` |
| `continue` | any | `parse_continue_statement()` | `continue` | `ContinueStatement` |
| `using` | any | `parse_using_statement()` | `using Base` | `UsingStatement` |
| `import` | any | `parse_import_statement()` | `import Base` | `ImportStatement` |
| `export` | any | `parse_export_statement()` | `export foo` | `ExportStatement` |
| `public` | any | `parse_public_statement()` | `public foo` | `PublicStatement` |
| `const` | any | `parse_const_declaration()` | `const PI = 3.14` | `ConstDeclaration` |
| `global` | any | `parse_global_declaration()` | `global x` | `GlobalDeclaration` |
| `local` | any | `parse_local_declaration()` | `local y` | `LocalDeclaration` |
| `Identifier` | `,` | `parse_bare_tuple_assignment()` | `a, b = 1, 2` | `TupleExpression` + `Assignment` |
| `Identifier` | other | `parse_expression()` | `x + 1` | varies |
| Regular operator¹ | `(` | `parse_operator_method_definition()` | `+(x, y) = x` | `ShortFunctionDefinition` |
| Dotted operator² | `(` | `parse_expression()` | `.+([1,2])` | `BroadcastCallExpression` |
| other | any | `parse_expression()` | `[1,2,3]` | varies |

### Notes

1. **Regular operator**: Any operator from `is_operator()` that is NOT a dotted operator. Examples: `+`, `-`, `*`, `/`, `<`, `>`, `==`, etc.

2. **Dotted operator**: Broadcast operators starting with `.` such as `.+`, `.-`, `.*`, `./`, etc. These are identified by `is_dotted_operator()`.

## Critical Dispatch Logic

The most complex dispatch logic is in the default case (lines 269-280 of `parser/mod.rs`):

```rust
_ => {
    // Operator method definitions: *(x, y) = expr, <(x, y) = expr, etc.
    // But NOT dotted operators like .+, .- which are broadcast function calls
    if token.is_operator()
        && !token.is_dotted_operator()
        && self.peek_next() == Some(Token::LParen)
    {
        self.parse_operator_method_definition()
    } else {
        self.parse_expression()
    }
}
```

### Why This Matters

The distinction between regular operators and dotted operators is crucial:

- `+(x, y) = x + y` → Operator method definition (defines how `+` works)
- `.+([1, 2, 3])` → Broadcast function call (applies `.+` element-wise)

If this logic were incorrect:
- Dotted operators followed by `(` would be incorrectly parsed as operator method definitions
- This would cause semantic errors downstream (Issue #1574)

## Token Helper Methods

### `is_operator()`

Returns `true` for all operator tokens including:
- Arithmetic: `+`, `-`, `*`, `/`, `%`, `^`, `\`
- Comparison: `<`, `>`, `<=`, `>=`, `==`, `===`, `!=`, `!==`
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `~`, `<<`, `>>`, `>>>`
- Type: `<:`, `>:`, `::`
- Dotted (broadcast): `.+`, `.-`, `.*`, `./`, etc.

### `is_dotted_operator()`

Returns `true` only for broadcast operators:
- `.+`, `.-`, `.*`, `./`, `.^`, `.%`, `.\\`
- `.<`, `.>`, `.<=`, `.>=`, `.==`, `.!=`
- `.&`, `.|`

## Bare Tuple Assignment

When an identifier is followed by a comma, the parser dispatches to `parse_bare_tuple_assignment()`:

```julia
a, b = 1, 2        # Parsed as bare tuple assignment
a, b, c = f()      # Multiple assignment from function return
```

This special case is needed because without it, `a, b = 1, 2` would be parsed as an expression starting with `a`, and the comma would cause issues.

## Testing

### Dispatch Decision Table Tests

Tests verifying the dispatch table are in `subset_julia_vm_parser/tests/parser_dispatch_tests.rs`:

```bash
# Run dispatch decision table tests
timeout 1800 cargo nextest run --release --package subset_julia_vm_parser --test parser_dispatch_tests
```

### Key Test Cases

Each row in the decision table should have at least one corresponding test:

| Dispatch Case | Test Function |
|---------------|---------------|
| `function` keyword | `test_dispatch_function_keyword` |
| `Identifier` + `,` | `test_dispatch_bare_tuple_assignment` |
| Regular operator + `(` | `test_dispatch_operator_method_definition` |
| Dotted operator + `(` | `test_dispatch_dotted_operator_broadcast` |
| Default expression | `test_dispatch_expression_default` |

### Exhaustive Operator Dispatch Tests (Issue #1756)

In addition to the per-row tests above, exhaustive tests verify dispatch correctness for **every** operator:

| Test Function | What It Verifies | Operators Covered |
|---------------|------------------|-------------------|
| `test_exhaustive_regular_operators_dispatch_to_method_definition` | All regular operators + `(` → `ShortFunctionDefinition` | `+`, `-`, `*`, `/`, `%`, `^`, `\`, `<`, `>`, `<=`, `>=`, `==`, `===`, `!=`, `!==`, `<:`, `>:`, `&`, `\|`, `~`, `<<`, `>>`, `>>>`, `//`, `\|>`, `<\|` |
| `test_exhaustive_dotted_operators_dispatch_to_broadcast_call` | All dotted operators + `(` → `BroadcastCallExpression` | `.+`, `.-`, `.*`, `./`, `.\\`, `.^`, `.%`, `.<`, `.>`, `.<=`, `.>=`, `.==`, `.!=`, `.&`, `.\|` |
| `test_exhaustive_dotted_operators_as_binary_expressions` | All dotted operators in binary position → `BinaryExpression` with correct operator text | Same 15 dotted operators |

### Cross-Validation Tests (Issue #1756)

Cross-validation tests verify structural invariants across dispatch paths:

| Test Function | Invariant Verified |
|---------------|-------------------|
| `test_cross_validation_method_def_has_assignment` | `ShortFunctionDefinition` nodes have correct child structure |
| `test_cross_validation_broadcast_call_has_arguments` | `BroadcastCallExpression` nodes contain argument children |
| `test_cross_validation_operator_method_def_vs_expression_context` | Same operator dispatches differently in method-def vs expression context |
| `test_cross_validation_dotted_vs_regular_operator_dispatch` | Each dotted/regular operator pair dispatches to different paths (core #1574 invariant) |

## Related Issues

- **Issue #1573**: Test expectation mismatch due to misunderstanding dispatch rules
- **Issue #1574**: Operator dispatch error for dotted operators
- **Issue #1756**: Prevention measures for parser dispatch errors (this document)

## Common Pitfalls

1. **Forgetting dotted operator exclusion**: When adding new operator handling, always check if dotted operators should be excluded.

2. **Bare tuple vs expression**: The `Identifier` + `,` check must come before general expression parsing.

3. **Keyword token ordering**: More specific keyword matches (like `mutable struct`) are handled first.

## Code Review Checklist

When modifying parser dispatch logic, verify:

- [ ] Update this document's dispatch decision table
- [ ] Add/update tests in `parser_dispatch_tests.rs`
- [ ] If adding a new operator: add it to the exhaustive tests
- [ ] If adding a new dotted operator: add it to both broadcast and binary exhaustive tests
- [ ] Verify dotted operators are excluded from method definition dispatch
- [ ] Test both operator method definitions AND broadcast calls
- [ ] Run `timeout 1800 cargo nextest run --release --package subset_julia_vm_parser --test parser_dispatch_tests`

## Keyword Disambiguation in Primary Expressions

When parsing primary expressions, certain keywords have dual meanings depending on context. The parser in `subset_julia_vm_parser/src/parser/expressions/primary.rs` must disambiguate between these uses.

### Keyword Classification Table

| Keyword | Expression-Starter? | Contextual Identifier? | Disambiguation Method | Example as Expression | Example as Identifier |
|---------|---------------------|------------------------|----------------------|----------------------|----------------------|
| `begin` | Yes | Yes | Lookahead | `z = begin x end` | `a[begin:end]` |
| `end` | No | Yes | Always identifier | N/A | `a[end]`, `a[1:end]` |
| `if` | Yes | No | Direct dispatch | `y = if c a else b end` | N/A |
| `let` | Yes | No | Direct dispatch | `y = let x=1; x end` | N/A |
| `quote` | Yes | No | Direct dispatch | `esc(quote x end)` | N/A |
| `return` | Yes | No | Direct dispatch | `x && return nothing` | N/A |
| `break` | Yes | No | Direct dispatch | `x && break` | N/A |
| `continue` | Yes | No | Direct dispatch | `x && continue` | N/A |
| `isa` | No | Yes | Always identifier | N/A | `isa(x, T)` |
| `outer` | No | Yes | Always identifier | N/A | `for outer in 1:3` |

### Disambiguation Categories

1. **Pure Expression-Starters** (`if`, `let`, `quote`, `return`, `break`, `continue`)
   - Always dispatch to their respective block/statement parsers
   - Cannot be used as identifiers in primary expression context

2. **Pure Contextual Identifiers** (`end`, `isa`, `outer`)
   - Always treated as identifiers in primary expression context
   - `end` is special in indexing: `a[end]` means last index
   - `isa` can be called as function: `isa(x, Int)`
   - `outer` is only special in `for outer x in ...` loop syntax; that modifier
     form is currently rejected during lowering instead of being mis-executed

3. **Dual-Meaning Keywords** (`begin`)
   - Requires lookahead-based disambiguation
   - `begin` as block: `z = begin x; y end`
   - `begin` as identifier: `a[begin:end]`, `a[begin+1]`

### `begin` Keyword Disambiguation Logic

The `begin` keyword is disambiguated by examining the following token:

```rust
// From primary.rs
Token::KwBegin => {
    let next = self.peek_next();
    match next {
        // Indexing context indicators → treat as identifier
        Some(Token::Colon) | Some(Token::Comma)
        | Some(Token::RBracket) | Some(Token::RParen)
        | Some(Token::Plus) | Some(Token::Minus)
        | Some(Token::Star) | Some(Token::Slash) | Some(Token::SlashSlash)
        | Some(Token::Percent) | Some(Token::Caret)
        | Some(Token::EqEq) | Some(Token::NotEq)
        | Some(Token::Lt) | Some(Token::Gt)
        | Some(Token::LtEq) | Some(Token::GtEq)
        | None => {
            // Parse as identifier (for indexing)
            let token = self.advance().unwrap();
            Ok(CstNode::leaf(NodeKind::Identifier, token.span, token.text))
        }
        // Otherwise → parse as begin...end block
        _ => self.parse_begin_block(),
    }
}
```

**Rationale**: In indexing context, `begin` is followed by operators (`:`, `+`, `-`, etc.), delimiters (`,`, `]`, `)`), or end-of-input. A `begin...end` block would be followed by an expression start (identifier, literal, etc.).

### Adding New Dual-Meaning Keywords

When a keyword needs to serve as both an expression-starter AND a contextual identifier:

1. **Identify the contexts**: Determine when it starts an expression vs. when it's an identifier
2. **Define disambiguation tokens**: List all tokens that indicate the identifier context
3. **Implement lookahead**: Use `peek_next()` to examine the following token
4. **Add tests**: Test both uses in `parser_dispatch_tests.rs` or corpus tests
5. **Update this table**: Document the keyword's dual meaning here

### Code Review Checklist for Keyword Changes

When modifying keyword handling in `parse_primary()`:

- [ ] Verify the keyword's role: expression-starter, contextual identifier, or both?
- [ ] If dual-meaning, implement proper lookahead disambiguation
- [ ] Test both uses: as block/statement AND as identifier (if applicable)
- [ ] Update the Keyword Classification Table above
- [ ] For `begin`-like keywords, test with all applicable operator combinations
- [ ] Verify `a[keyword:end]` and `a[keyword+1]` patterns still work

### Related Issues

- **Issue #1794**: `begin` in expression context (`z = begin...end`) wasn't supported
- **Issue #2310**: `a[begin+1]` arithmetic with `begin` in indexing context
- **Issue #2308**: This documentation (prevention measure)

## Related Documentation

- `docs/vm/LOWERING.md` - How parsed CST is lowered to Core IR
- `docs/vm/TYPE_SYSTEM.md` - Type system documentation
- `docs/vm/STATUS.md` - Current implementation status
- `docs/vm/DONE.md` - Completed features and fixes

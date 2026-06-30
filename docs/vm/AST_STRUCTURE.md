# AST Node Structure Documentation

This document describes the structure of each CST (Concrete Syntax Tree) node type produced by the parser. Each node's child structure is documented with examples.

## Overview

The parser produces `CstNode` instances with the following structure:

```rust
pub struct CstNode {
    pub kind: NodeKind,       // The type of node
    pub span: Span,           // Source location
    pub text: Option<String>, // Leaf node text (identifiers, literals)
    pub children: Vec<CstNode>, // Child nodes
}
```

## Important Notes

1. **ArgumentList Always Present**: `CallExpression` always has an `ArgumentList` child, even for calls with no arguments like `foo()`.

2. **RangeExpression vs BinaryExpression**: Ranges like `1:10` produce `RangeExpression`, not `BinaryExpression`.

3. **Operator in BinaryExpression**: Binary expressions have 3 children: `[left, operator, right]`, where operator is a node.

## Node Structure Table

### Expressions

#### CallExpression

**Children:** `[callee: Expression, arguments: ArgumentList]`

**Child Count:** Always 2

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Identifier or Expression | The function being called |
| 1 | ArgumentList | The arguments (may be empty) |

**Examples:**
```julia
foo(1, 2)   # → CallExpression[Identifier("foo"), ArgumentList[IntegerLiteral(1), IntegerLiteral(2)]]
bar()       # → CallExpression[Identifier("bar"), ArgumentList[]]
obj.method(x)  # → CallExpression[FieldExpression[...], ArgumentList[Identifier("x")]]
```

**Common Mistake:** Assuming empty calls have 1 child. They always have 2.

---

#### BinaryExpression

**Children:** `[left: Expression, operator: Operator, right: Expression]`

**Child Count:** Always 3

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression | Left operand |
| 1 | Operator | The operator node (contains text like "+", "-") |
| 2 | Expression | Right operand |

**Examples:**
```julia
1 + 2       # → BinaryExpression[IntegerLiteral(1), Operator("+"), IntegerLiteral(2)]
a * b + c   # → BinaryExpression[BinaryExpression[...], Operator("+"), Identifier("c")]
x && y      # → BinaryExpression[Identifier("x"), Operator("&&"), Identifier("y")]
```

**Note:** The operator is a node, not just text. Access via `children[1].text`.

---

#### UnaryExpression

**Children:** `[operator: Operator, operand: Expression]`

**Child Count:** Always 2

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Operator | The operator node ("-", "!", "~") |
| 1 | Expression | The operand |

**Examples:**
```julia
-x          # → UnaryExpression[Operator("-"), Identifier("x")]
!flag       # → UnaryExpression[Operator("!"), Identifier("flag")]
```

---

#### TernaryExpression

**Children:** `[condition: Expression, then_branch: Expression, else_branch: Expression]`

**Child Count:** Always 3

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression | Condition |
| 1 | Expression | Value if true |
| 2 | Expression | Value if false |

**Examples:**
```julia
x > 0 ? 1 : 0  # → TernaryExpression[BinaryExpression[...], IntegerLiteral(1), IntegerLiteral(0)]
```

---

#### IndexExpression

**Children:** `[object: Expression, index1, index2, ...]`

**Child Count:** 2 or more (1 object + N indices)

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression | The indexed object |
| 1+ | Expression | Index expressions |

**Examples:**
```julia
arr[1]      # → IndexExpression[Identifier("arr"), IntegerLiteral(1)]
matrix[i, j]  # → IndexExpression[Identifier("matrix"), Identifier("i"), Identifier("j")]
a[1][2]     # → IndexExpression[IndexExpression[...], IntegerLiteral(2)]
```

---

#### FieldExpression

**Children:** `[object: Expression, field: Identifier]`

**Child Count:** Always 2

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression | The object being accessed |
| 1 | Identifier | The field name |

**Examples:**
```julia
obj.field   # → FieldExpression[Identifier("obj"), Identifier("field")]
a.b.c       # → FieldExpression[FieldExpression[Identifier("a"), Identifier("b")], Identifier("c")]
```

---

#### RangeExpression

**Children:** `[start, end]` or `[RangeExpression(start, step), end]` (nested for 3-part)

**Child Count:** Always 2 (3-part ranges are nested)

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression or RangeExpression | Start value (or nested range for 3-part) |
| 1 | Expression | End value |

**Examples:**
```julia
1:10        # → RangeExpression[IntegerLiteral(1), IntegerLiteral(10)]
1:2:10      # → RangeExpression[RangeExpression[1, 2], IntegerLiteral(10)]  (NESTED!)
```

**Common Mistakes:**
1. Assuming ranges are BinaryExpression. They have their own NodeKind.
2. Assuming 3-part ranges have 3 children. They are nested (2 children each level).

---

#### TypedExpression

**Children:** `[expression: Expression, type: Expression]`

**Child Count:** Always 2

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression | The value being typed |
| 1 | Expression | The type annotation |

**Examples:**
```julia
x::Int      # → TypedExpression[Identifier("x"), Identifier("Int")]
y::Vector{T}  # → TypedExpression[Identifier("y"), ParametrizedTypeExpression[...]]
```

---

#### BroadcastCallExpression

**Children:** `[callee: Expression, arg1, arg2, ...]`

**Child Count:** 1 or more

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression or Operator | The function/operator being broadcast |
| 1+ | Expression | Arguments |

**Examples:**
```julia
f.(x, y)    # → BroadcastCallExpression[Identifier("f"), Identifier("x"), Identifier("y")]
.+([1,2])   # → BroadcastCallExpression[Operator(".+"), VectorExpression[...]]
```

**Note:** Unlike CallExpression, BroadcastCallExpression does NOT use ArgumentList.

---

### Collections

#### VectorExpression

**Children:** `[element1, element2, ...]`

**Child Count:** 0 or more

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0+ | Expression | Each element |

**Examples:**
```julia
[]          # → VectorExpression[]
[1, 2, 3]   # → VectorExpression[IntegerLiteral(1), IntegerLiteral(2), IntegerLiteral(3)]
```

---

#### TupleExpression

**Children:** `[element1, element2, ...]`

**Child Count:** 0 or more

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0+ | Expression or NamedField | Each element |

**Examples:**
```julia
()          # → TupleExpression[]
(1, 2, 3)   # → TupleExpression[IntegerLiteral(1), IntegerLiteral(2), IntegerLiteral(3)]
(a=1, b=2)  # → TupleExpression[NamedField[...], NamedField[...]]
```

---

#### MatrixExpression

**Children:** `[row1: MatrixRow, row2: MatrixRow, ...]`

**Child Count:** 1 or more

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0+ | MatrixRow | Each row of the matrix |

**Examples:**
```julia
[1 2; 3 4]  # → MatrixExpression[MatrixRow[1, 2], MatrixRow[3, 4]]
[1 2 3]     # → MatrixExpression[MatrixRow[1, 2, 3]]  (row vector)
```

---

### Statements

#### IfStatement

**Children:** `[condition, body, (elseif_clauses...), (else_clause)]`

**Child Count:** 2 or more

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression | Condition |
| 1 | Block | Then body |
| 2+ | ElseifClause or ElseClause | Optional branches |

**Examples:**
```julia
if x end                    # → IfStatement[Identifier("x"), Block[]]
if x y else z end           # → IfStatement[..., Block[y], ElseClause[Block[z]]]
if a b elseif c d else e end  # → IfStatement[..., Block[b], ElseifClause[...], ElseClause[...]]
```

---

#### ForStatement

**Children:** `[binding: ForBinding, body: Block]`

**Child Count:** Always 2

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | ForBinding | Iterator binding (e.g., `i in 1:10`) |
| 1 | Block | Loop body |

**Examples:**
```julia
for i in 1:10 x end   # → ForStatement[ForBinding[Identifier("i"), RangeExpression[...]], Block[x]]
```

---

#### WhileStatement

**Children:** `[condition: Expression, body: Block]`

**Child Count:** Always 2

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression | Loop condition |
| 1 | Block | Loop body |

**Examples:**
```julia
while x > 0 x -= 1 end  # → WhileStatement[BinaryExpression[...], Block[...]]
```

---

#### FunctionDefinition

**Children:** `[name: Identifier, (params: ParameterList), body: Block]`

**Child Count:** 2 or 3

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Identifier | Function name |
| 1 | ParameterList or Block | Parameters (if present) or body (if no params) |
| 2 | Block | Body (if parameters present) |

**Examples:**
```julia
function foo() end          # → FunctionDefinition[Identifier("foo"), Block[]]
function add(x, y) x+y end  # → FunctionDefinition[Identifier("add"), ParameterList[...], Block[...]]
```

---

#### ReturnStatement

**Children:** `[]` or `[value: Expression]`

**Child Count:** 0 or 1

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Expression | Return value (optional) |

**Examples:**
```julia
return        # → ReturnStatement[]
return x + 1  # → ReturnStatement[BinaryExpression[...]]
```

---

### Definitions

#### StructDefinition

**Children:** `[name: Identifier or TypeHead, field1, field2, ...]`

**Child Count:** 1 or more

| Child Index | Kind | Description |
|-------------|------|-------------|
| 0 | Identifier or TypeHead | Struct name (with optional type params) |
| 1+ | Identifier or TypedExpression | Field definitions |

**Examples:**
```julia
struct Point x y end           # → StructDefinition[Identifier("Point"), Identifier("x"), Identifier("y")]
struct Point{T} x::T end       # → StructDefinition[TypeHead[...], TypedExpression[...]]
```

---

## Debug Helper

When tests fail due to incorrect structure assumptions, use this helper to print the actual structure:

```rust
fn debug_node_structure(node: &CstNode, indent: usize) {
    let prefix = "  ".repeat(indent);
    println!("{}[{}] {:?} = {:?}", prefix, indent, node.kind, node.text);
    for (i, child) in node.children.iter().enumerate() {
        println!("{}  Child {}:", prefix, i);
        debug_node_structure(child, indent + 2);
    }
}
```

## Testing Guidelines

When writing tests for parser output:

1. **Always check child count first:**
   ```rust
   assert_eq!(node.children.len(), EXPECTED_COUNT);
   ```

2. **Check each child's kind:**
   ```rust
   assert_eq!(node.children[0].kind, NodeKind::Expected);
   ```

3. **Use meaningful assertions:**
   ```rust
   assert_eq!(
       node.children[0].kind,
       NodeKind::Identifier,
       "First child of CallExpression should be the callee"
   );
   ```

4. **Document expected structure in test:**
   ```rust
   #[test]
   fn test_call_expression_structure() {
       // CallExpression: [callee: Identifier, arguments: ArgumentList]
       let node = parse_expr("foo(1, 2)");
       assert_eq!(node.kind, NodeKind::CallExpression);
       assert_eq!(node.children.len(), 2);
       // ... detailed assertions
   }
   ```

## Related Documentation

- `docs/vm/PARSER.md` - Parser dispatch logic
- `subset_julia_vm_parser/tests/parser_ast_structure_tests.rs` - Structure validation tests

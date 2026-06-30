# Builtin Handler Ownership

This document records active Rust `execute_builtin` ownership for `BuiltinId`
variants handled by `builtins_*.rs` files and their submodules.

**Rule**: Every active Rust `execute_builtin` handler must be owned by exactly
**one** file. Some `BuiltinId` variants are deliberately Pure Julia routes,
runtime dynamic-call fallbacks, cache-compatibility names, or currently
unimplemented; those variants should not gain a second Rust handler by accident.
The `dispatch_builtin!` macro in `builtins_exec.rs` calls handlers in a fixed order and stops
at the first `Ok(Some(()))`. If two files handle the same `BuiltinId`, the one called earlier
in the chain silently shadows the other — this is the root cause of Issue #3026 (scalar `size`)
and Issue #3031 (dead `Eltype`).

## Dispatch Chain Order

Handlers are called in this order in `builtins_exec.rs`:

| # | Method | File |
|---|--------|------|
| 1 | `execute_builtin_math` | `builtins_math.rs` |
| 2 | `execute_builtin_io` | `builtins_io.rs` |
| 3 | `execute_builtin_collections` | `builtins_collections.rs` |
| 4 | `execute_builtin_dicts` | `builtins_dicts.rs` |
| 5 | `execute_builtin_sets` | `builtins_sets/` |
| 6 | `execute_builtin_numeric` | `builtins_numeric.rs` |
| 7 | `execute_builtin_strings` | `builtins_strings.rs` |
| 8 | `execute_builtin_arrays` | `builtins_arrays.rs` |
| 9 | `execute_builtin_types` | `builtins_types.rs` |
| 10 | `execute_builtin_reflection` | `builtins_reflection/` |
| 11 | `execute_builtin_equality` | `builtins_equality.rs` |
| 12 | `execute_builtin_macro` | `builtins_macro/` |
| 13 | `execute_builtin_linalg` | `builtins_linalg.rs` |

`execute_builtin_types` delegates first to
`execute_builtin_types_conversion` in `builtins_types_conversion.rs`; conversion
builtins are therefore owned by that subhandler while still occupying position 9
in the dispatch chain.

### Fallback Handlers (`builtins_exec.rs`)

After the dispatch chain, `builtins_exec.rs` also contains **legacy fallback
handlers**:

`TupleFirst`, `TupleLast`, `TupleLen`, `Iterate`, `RangeCollect`, `Compose`,
`IsExported`, `IsPublic`, `Convert`, `Promote`

> **Note**: Some of these overlap with dispatch-chain handlers. The dispatch
> chain takes precedence: `Convert` and `Promote` are handled by
> `builtins_types_conversion.rs` before the fallback match can reach them.

### Empty Placeholder: `builtins_stats.rs`

`builtins_stats.rs` exists but currently contains **no handlers**. It is reserved for
future statistics-related builtins (e.g., `mean`, `std`, `var`).

## BuiltinId Ownership Table

### `builtins_math.rs`
`Sqrt`, `Round`, `RoundDigits`, `RoundSigDigits`, `FloorDigits`, `FloorSigDigits`, `CeilDigits`,
`CeilSigDigits`, `Trunc`, `TruncDigits`, `TruncSigDigits`, `NextFloat`, `PrevFloat`,
`NextFloatN`, `PrevFloatN`,
`CountOnes`, `CountZeros`, `LeadingZeros`, `TrailingOnes`, `Bitreverse`, `LeadingOnes`,
`TrailingZeros`, `Bitrotate`, `Bswap`, `Exponent`, `Significand`, `Frexp`, `Issubnormal`,
`Fma`

### `builtins_io.rs`
`Print`, `Println`, `IOBufferNew`, `IOBufferFromString`, `TakeString`, `IOWrite`, `IOPrint`, `Displaysize`,
`IncludeDependency`, `Precompile`, `Normpath`, `Abspath`, `Homedir`, `Sleep`, `TimeNs`,
`ReadFile`, `ReadLines`, `Readline`, `Countlines`, `Isfile`, `Isdir`, `Ispath`, `Filesize`,
`Pwd`, `Readdir`, `Mkdir`, `Mkpath`, `Rm`, `Tempdir`, `Tempname`, `Touch`, `Cd`, `Islink`,
`Cp`, `Mv`, `Mtime`, `Open`, `Close`, `Eof`, `Isopen`, `ReadlineIo`

### `builtins_collections.rs`
`Length`, `Eltype`, `_Eltype`, `MemoryRefNew`, `MemoryRefGet`, `MemoryRefSet`,
`MemoryRefOffset`, `MemoryRefParent`

> **Note**: `Eltype` and `_Eltype` belong here (position 3 in dispatch chain).
> Do NOT add `Eltype` to `builtins_arrays.rs` — it would be dead code (Issue #3031).

### `builtins_dicts.rs`
`DictGet`, `DictGetkey`, `DictSet`, `DictDelete`, `DictHasKey`, `DictLen`, `DictKeys`,
`DictValues`, `DictPairs`, `DictMerge`, `DictNew`, `DictGetBang`, `DictMergeBang`,
`DictEmpty`, `DictPop`, `_DictGet`, `_DictSet`, `_DictDelete`, `_DictHaskey`,
`_DictLength`, `_DictEmpty`, `_DictKeys`, `_DictValues`, `_DictPairs`

### `builtins_sets/` (directory module: `mod.rs`, `set_ops.rs`, `intrinsics.rs`, `shared.rs`)
`SetNew`, `SetPush`, `SetDelete`, `SetIn`, `SetEmpty`, `_SetPush`,
`_SetDelete`, `_SetIn`, `_SetEmpty`, `_SetLength`

### `builtins_numeric.rs`
`BigInt`, `BigFloat`, `BigFloatPrecision`, `BigFloatDefaultPrecision`,
`SetBigFloatDefaultPrecision`, `BigFloatRounding`, `SetBigFloatRounding`,
`GetZeroSubnormals`, `SetZeroSubnormals`,
`Int8`, `Int16`, `Int32`, `Int64`, `Int128`,
`UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128`,
`Float16`, `Float32`, `Float64`

### `builtins_strings.rs`
`StringNew`, `StringFromChars`, `Repr`, `Sprintf`, `Ncodeunits`, `Codeunit`, `CodeUnits`,
`Occursin`, `StringToFloat`, `StringIntToBase`, `CharToInt`, `Codepoint`,
`IntToChar`, `Bitstring`, `UnescapeString`, `IsvalidIndex`, `TryparseFloat64`,
`StringFindAll`, `StringCount`, `SubStringRetag`

### `builtins_arrays.rs`
`Zeros`, `ZerosF64`, `ZerosI64`, `Ones`, `OnesF64`, `OnesI64`,
`AllocUndefF64`, `AllocUndefI64`, `AllocUndefBool`, `AllocUndefAny`,
`MarkBitVector`, `MarkBitArray`, `Similar`, `Reshape`, `GetIndex`, `Push`, `Pop`,
`PushFirst`, `PopFirst`, `Insert`, `DeleteAt`, `Size`, `Ndims`, `Keytype`,
`Valtype`

### `builtins_types.rs`
`TypeOf`, `TypeVar`, `UnionAll`, `Isa`, `Subtype`, `SupertypeOp`, `Sizeof`,
`Isbits`, `Isbitstype`, `Subtypes`, `Hasfield`, `_Isabstracttype`,
`_Isconcretetype`, `_Isprimitivetype`, `_Isstructtype`, `_Ismutabletype`,
`Ismutable`, `_Supertype`, `_Typename`, `_FunctionName`, `_Typeintersect`,
`Objectid`, `Isunordered`, `In`, `NonMissingType`

### `builtins_types_conversion.rs` (subhandler called by `builtins_types.rs`)
`Convert`, `Promote`, `Signed`, `Unsigned`, `FloatConv`, `Widemul`,
`Reinterpret`

### `builtins_reflection/` (directory module: `mod.rs`, `primitives.rs`)
`_Fieldnames`, `_Fieldtypes`, `_Getfield`, `Getfield`, `Setfield`,
`Deepcopy`, `HasMethod`, `Which`, `_TypeUnion`, `_TypeVarName`,
`_TypeVarLowerBound`, `_TypeVarUpperBound`, `_UnionAllVar`, `_UnionAllBody`,
`_TypeParameters`, `_Allocatedinline`, `_DatatypeAlignment`, `_Fieldoffset`,
`_MakeTupleType`, `_MethodsByFtype`, `_ReturnTypesByFtype`,
`IsdefinedModuleBinding`, `ComposeExceptionType`

### `builtins_equality.rs`
`Egal`, `Isequal`, `TupleEquals`, `Hash`, `_Hash`, `Isless`

### `builtins_macro/` (directory module: `mod.rs`, `eval.rs`, `parse.rs`, `helpers.rs`, `ir_conversion.rs`)
`SymbolNew`, `ExprNew`, `ExprNewWithSplat`, `Gensym`, `QuoteNodeNew`, `LineNumberNodeNew`,
`GlobalRefNew`, `Esc`, `Eval`, `GeneratedEval`, `MacroExpand`, `MacroExpandBang`, `IncludeString`, `EvalFile`,
`MetaParse`, `MetaParseAt`, `MetaIsExpr`, `MetaQuot`, `MetaIsIdentifier`, `MetaIsOperator`,
`MetaIsUnaryOperator`, `MetaIsBinaryOperator`, `MetaIsPostfixOperator`, `MetaLower`,
`TestRecord`, `TestRecordBroken`, `TestSetBegin`, `TestSetEnd`,
`RegexNew`, `RegexMatch`, `RegexOccursin`, `EndsWithRegex`, `RegexReplace`,
`RegexSplit`, `RegexEachmatch`

### `builtins_linalg.rs`
`Lu`, `Det`, `Inv`, `Ldiv`, `Svd`, `Qr`, `Eigen`, `Eigvals`, `Cholesky`, `Rank`, `Cond`

`Rank` is retained as a cache/bootstrap compatibility fallback; public
`LinearAlgebra.rank` calls route through the stdlib Pure Julia wrapper first
(Issue #4020).

## Detecting Duplicate Handlers

Run the canonical script from the repository root:

```bash
bash scripts/check_builtin_duplicates.sh
```

This script is also run in CI via the `builtin-ownership` job and verified with
`shellcheck` to catch bash script bugs (Issue #3035).

### Script Limitations

The detection script uses grep-based heuristics. Be aware of these limitations:

- **`builtins_exec.rs` is excluded** — it contains legacy fallback handlers
  intentionally shadowed by specialized handlers. Duplicates between specialized
  files and `builtins_exec.rs` are not flagged.
- **Directory submodules are not expanded** — the script scans top-level
  `builtins_*.rs` files. Directory modules such as `builtins_sets/`,
  `builtins_macro/`, and `builtins_reflection/` are still maintained manually in
  this document.
- **Comment lines are excluded** — lines matching `^\s*//` are filtered before
  pattern extraction. `// BuiltinId::Foo` in a comment will not be detected.
- **Inline comments after code are NOT excluded** — a line like
  `do_something(); // BuiltinId::Bar` would be counted. Avoid this pattern.
- **This file is not auto-verified** — the ownership table above is maintained
  manually. When adding or moving handlers, update this file alongside the code.

## Adding a New Builtin

1. Decide which file is the appropriate owner (or create `builtins_<category>.rs`)
2. Add the `BuiltinId::X =>` arm to the owning file
3. If creating a new file, add `execute_builtin_<category>` to the `dispatch_builtin!` list in `builtins_exec.rs`
4. Update this table

## Known Issues

- Issue #2880: 555 potential integer overflow sites in `as` casts across builtins
- Issue #2931: `Value` enum has 52 variants — consider sub-enum grouping for dispatch

---

*Last updated: 2026-06-11.*

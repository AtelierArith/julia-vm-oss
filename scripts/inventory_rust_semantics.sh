#!/usr/bin/env bash
# inventory_rust_semantics.sh — Rust-side semantics inventory (Issue #8671, parent #8648)
#
# "Pure Julia First" (CLAUDE.md principle #2) says Julia-expressible semantics
# live in subset_julia_vm/src/julia/ and Rust keeps only what Julia cannot
# express (ARCHITECTURE_OVERVIEW.md three-layer rule, RUST_BOUNDARY_JUSTIFICATION.md
# conditions 1-4). This script makes the actual Rust-side semantic surface
# visible so that gap can be measured and worked down:
#
#   1. Every `BuiltinId` variant (from the `define_builtin_table!` single-source
#      table in subset_julia_vm/src/builtins.rs): surface name / from_name
#      aliases / owning vm/builtins_*.rs handler file / matching upstream
#      julia/base (or stdlib) definition path / whether a Pure Julia definition
#      of the same surface name exists under subset_julia_vm/src/julia/.
#   2. Every *semantic* `Instr` variant (VM instructions that implement a Julia
#      surface function, as opposed to stack/slot/jump/call machinery), from the
#      curated SEMANTIC_INSTR table below (validated against the live enum).
#   3. The large Rust semantic modules (matmul/, hof_exec/, type_ops/, ...) by
#      LOC, for the #8672 classification of "big blob" surfaces.
#   4. Summary metrics, including the ARCHITECTURE_OVERVIEW.md three-layer-rule
#      conformance rate (see "Conformance metric" below).
#
# Conformance metric (mechanical proxy, refined by the #8672 classification):
#   An inventoried Rust builtin is COUNTED AS CONFORMING when either
#     (a) its canonical name is an underscored intrinsic (`_foo`) — an explicit
#         Layer-1/2 primitive that Pure Julia wrappers call, or
#     (b) a Pure Julia definition with the same public surface name exists, so
#         method dispatch owns the public API and Rust is a fallback boundary
#         (the Public Base Routing Rule, BUILTIN_REMOVAL.md Issue #3831).
#   A PUBLIC surface name implemented ONLY in Rust is a NON-CONFORMANCE
#   CANDIDATE: it must either be justified by RUST_BOUNDARY_JUSTIFICATION.md
#   conditions 1-4 (the #8672 classification records that) or be migrated.
#
# Usage (from repository root):
#   ./scripts/inventory_rust_semantics.sh              # full markdown report
#   ./scripts/inventory_rust_semantics.sh --summary    # key=value metrics only
#
# The upstream Julia checkout defaults to ./julia (submodule); override with
# SJULIA_UPSTREAM_JULIA=/path/to/julia. Without it, upstream-path columns
# degrade to "?" and a warning is printed (metrics are unaffected).
#
# Optional classification join (Issue #8672): if
# docs/vm/rust_semantics_classification.tsv exists, its category column is
# joined into the tables and per-category counts are added to the summary.
# TSV columns: kind(builtin|instr) <TAB> key(variant name) <TAB>
# category(intrinsic|perf-measured|perf-pending|migratable|pure-julia-first)
# <TAB> evidence.
#
# Exit code: 0 on success, 1 on parse/validation failure (table out of sync
# with the enum, stale curated Instr entries), 2 on usage error.
#
# bash 3.2 compatible (macOS stock): no associative arrays, no mapfile.

set -euo pipefail

MODE=markdown
case "${1:-}" in
    --summary) MODE=summary ;;
    --markdown | "") ;;
    *)
        echo "usage: $0 [--markdown|--summary]" >&2
        exit 2
        ;;
esac

# BuiltinId moved to subset_julia_vm_bytecode (Issue #8656); fall back to
# subset_julia_vm/src/builtins.rs for repositories that pre-date the move.
if [[ -f "subset_julia_vm_bytecode/src/builtins.rs" ]] && \
   grep -q 'pub enum BuiltinId' "subset_julia_vm_bytecode/src/builtins.rs" 2>/dev/null; then
    BUILTINS_RS="subset_julia_vm_bytecode/src/builtins.rs"
else
    BUILTINS_RS="subset_julia_vm/src/builtins.rs"
fi
# Instr moved to subset_julia_vm_bytecode (Issue #8656); fall back to the old path.
if [[ -f "subset_julia_vm_bytecode/src/instr.rs" ]] && \
   grep -q 'pub enum Instr' "subset_julia_vm_bytecode/src/instr.rs" 2>/dev/null; then
    INSTR_RS="subset_julia_vm_bytecode/src/instr.rs"
else
    INSTR_RS="subset_julia_vm_vm/src/vm/instr.rs"
fi
VM_DIR="subset_julia_vm_vm/src/vm"
JULIA_SRC="subset_julia_vm/src/julia"
UPSTREAM="${SJULIA_UPSTREAM_JULIA:-julia}"
CLASSIFICATION_TSV="docs/vm/rust_semantics_classification.tsv"

if [[ ! -f "$BUILTINS_RS" || ! -f "$INSTR_RS" ]]; then
    echo "ERROR: $BUILTINS_RS / $INSTR_RS not found. Run from the repository root." >&2
    exit 1
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# =============================================================================
# 1. BuiltinId enum variants (declaration order) and the single-source table
# =============================================================================
awk '/^pub enum BuiltinId \{/{f=1;next} f&&/^\}/{f=0} f' "$BUILTINS_RS" \
    | grep -vE '^[[:space:]]*//' \
    | grep -oE '^[[:space:]]*[A-Z_][A-Za-z0-9_]*,' \
    | tr -d ' ,' >"$WORK/enum_variants.txt"

# Rows: `Variant: "canonical_name" => ["alias", ...],`
awk '/^define_builtin_table! \{/{f=1;next} f&&/^\}$/{f=0} f' "$BUILTINS_RS" \
    | grep -vE '^[[:space:]]*//' \
    | sed -nE 's/^[[:space:]]*([A-Za-z0-9_]+):[[:space:]]*"([^"]+)"[[:space:]]*=>[[:space:]]*\[([^]]*)\],?[[:space:]]*$/\1\t\2\t\3/p' \
    | sed -E 's/"//g; s/,[[:space:]]*/,/g; s/[[:space:]]*$//' >"$WORK/builtin_table.tsv"

# Validation: table and enum must agree 1:1 (the macro guarantees compile-time
# sync; this guards the *parser* against formatting drift).
sort "$WORK/enum_variants.txt" >"$WORK/enum_sorted.txt"
cut -f1 "$WORK/builtin_table.tsv" | sort >"$WORK/table_sorted.txt"
if ! diff -q "$WORK/enum_sorted.txt" "$WORK/table_sorted.txt" >/dev/null; then
    echo "ERROR: BuiltinId enum vs define_builtin_table! parse mismatch:" >&2
    diff "$WORK/enum_sorted.txt" "$WORK/table_sorted.txt" >&2 || true
    echo "Fix the parser in $0 (formatting drift in $BUILTINS_RS)." >&2
    exit 1
fi

# =============================================================================
# 2. Owner index: which vm/builtins_* file handles each BuiltinId
# =============================================================================
# One pass over the builtins handler files; prefer specialized owners over the
# builtins_exec.rs dispatch hub / legacy fallbacks.
find "$VM_DIR" \( -name 'builtins_*.rs' -o -path '*/builtins_*/*.rs' \) -type f | sort \
    | while IFS= read -r f; do
        grep -nE 'BuiltinId::[A-Z_][A-Za-z0-9_]*' "$f" /dev/null 2>/dev/null || true
    done \
    | grep -vE ':[0-9]+:[[:space:]]*//' \
    | awk -F: '{
        file=$1
        line=$0
        while (match(line, /BuiltinId::[A-Z_][A-Za-z0-9_]*/)) {
            v=substr(line, RSTART+11, RLENGTH-11)
            print v "\t" file
            line=substr(line, RSTART+RLENGTH)
        }
    }' | sort -u >"$WORK/builtin_owner_raw.tsv"

# Reduce to one owner per variant: first non-exec file, else builtins_exec.rs.
awk -F'\t' '{
    v=$1; f=$2
    sub(/^subset_julia_vm\/src\//, "", f)
    if (f ~ /builtins_exec\.rs$/) { if (!(v in fallback)) fallback[v]=f }
    else if (!(v in owner)) owner[v]=f
    seen[v]=1
} END {
    for (v in seen) print v "\t" ((v in owner) ? owner[v] : ((v in fallback) ? fallback[v] : "-"))
}' "$WORK/builtin_owner_raw.tsv" | sort >"$WORK/builtin_owner.tsv"

# =============================================================================
# 3. Definition indexes: Pure Julia layer and upstream julia/base + stdlib
# =============================================================================
# Recognized definition shapes (line-anchored):
#   function name(            function Base.name(       function Base.:(+)(
#   name(args) = ...          Base.name(args) = ...     (short form, incl. `::T =`)
#   ==(args) = ...            (:)(args) = ...           (operator definitions)
OP_ALT='(==|!=|<=|>=|\+|-|\*|//|/|\\\\|\^|%|÷|<|>|!|&|\||⊻|∘|:)'
index_julia_defs() {
    # $1 = output tsv (name<TAB>path), remaining args = roots (scanned in order;
    # first definition wins, so pass preferred roots first).
    local out=$1
    shift
    : >"$out.raw"
    local root
    local name_alt="((Base\\.)?[A-Za-z_!][A-Za-z0-9_!]*|(Base\\.:?)?\\(?${OP_ALT}\\)?)"
    for root in "$@"; do
        [[ -d "$root" ]] || continue
        find "$root" -name '*.jl' -type f | sort \
            | while IFS= read -r f; do
                grep -nE "^[[:space:]]*(function[[:space:]]+${name_alt}[[:space:]]*[({]|${name_alt}\\([^)]*\\)([[:space:]]*::[^=]*)?[[:space:]]*=([^=]|\$))" "$f" /dev/null 2>/dev/null || true
            done >>"$out.raw"
    done
    awk -F: '{
        file=$1
        # strip path:lineno: prefix to get the code line
        code=$0
        sub(/^[^:]*:[0-9]*:/, "", code)
        sub(/^[[:space:]]*/, "", code)
        if (code ~ /^function[[:space:]]/) {
            sub(/^function[[:space:]]+/, "", code)
        }
        sub(/^Base\.:?/, "", code)
        name=""
        if (match(code, /^[A-Za-z_!][A-Za-z0-9_!]*\(/)) {
            # `!` is also an operator: an identifier match ending before `(`
            name=substr(code, RSTART, RLENGTH-1)
        } else if (match(code, /^\(/)) {
            # parenthesized operator: (:)(...), (+)(...)
            rest=substr(code, 2)
            if (match(rest, /^[^)]+/)) name=substr(rest, RSTART, RLENGTH)
        } else if (match(code, /^[^[:space:](]+\(/)) {
            # bare operator short form: ==(T, S) = ...
            name=substr(code, RSTART, RLENGTH-1)
        }
        if (name != "" && !(name in seen)) { seen[name]=1; print name "\t" file }
    }' "$out.raw" | sort >"$out"
    rm -f "$out.raw"
}

index_julia_defs "$WORK/pj_defs.tsv" "$JULIA_SRC/base" "$JULIA_SRC/stdlib" "$JULIA_SRC/packages" "$JULIA_SRC"

UPSTREAM_OK=1
if [[ -d "$UPSTREAM/base" ]]; then
    index_julia_defs "$WORK/up_defs.tsv" "$UPSTREAM/base" "$UPSTREAM/stdlib"
else
    UPSTREAM_OK=0
    : >"$WORK/up_defs.tsv"
    echo "WARNING: upstream Julia checkout not found at '$UPSTREAM' (set SJULIA_UPSTREAM_JULIA); upstream-path columns degraded to '?'." >&2
fi

# =============================================================================
# 4. Curated semantic Instr variants (Julia-surface-function instructions)
# =============================================================================
# Everything NOT listed here is counted as VM machinery: stack/slot/jump/call
# plumbing, typed Load*/Store*/Push*/Return* forms, fused optimization forms
# (LoadAddI64Slot, JumpIfEqI64, ...) that duplicate an already-listed base op,
# exception-handler plumbing, and cache-compatibility decode stubs.
# Columns: Variant <TAB> surface function <TAB> note.
cat >"$WORK/semantic_instr.tsv" <<'EOF'
AddI64	+	typed fast path
AddF64	+	typed fast path
SubI64	-	typed fast path
SubF64	-	typed fast path
MulI64	*	typed fast path
MulF64	*	typed fast path
DivF64	/	typed fast path
ModI64	rem	typed fast path
PowF64	^	typed fast path
NegI64	-	unary
NegF64	-	unary
NotBool	!	unary
DynamicAdd	+	dynamic dispatch fallback
DynamicSub	-	dynamic dispatch fallback
DynamicMul	*	dynamic dispatch fallback
DynamicDiv	/	dynamic dispatch fallback
DynamicIntDiv	div	dynamic dispatch fallback
DynamicMod	rem	dynamic dispatch fallback
DynamicPow	^	dynamic dispatch fallback
DynamicNeg	-	dynamic dispatch fallback
EqI64	==	typed fast path
EqF64	==	typed fast path
EqStr	==	typed fast path
EqStruct	==	struct equality
NeI64	!=	typed fast path
NeF64	!=	typed fast path
LtI64	<	typed fast path
LtF64	<	typed fast path
LtStr	<	typed fast path
LeI64	<=	typed fast path
LeF64	<=	typed fast path
LeStr	<=	typed fast path
GtI64	>	typed fast path
GtF64	>	typed fast path
GtStr	>	typed fast path
GeI64	>=	typed fast path
GeF64	>=	typed fast path
GeStr	>=	typed fast path
SqrtF64	sqrt	typed fast path
AbsF64	abs	typed fast path
Abs2F64	abs2	typed fast path
FloorF64	floor	typed fast path
CeilF64	ceil	typed fast path
Zero	zero	generic zero
SelectI64	ifelse	branchless select
SelectF64	ifelse	branchless select
ToF64	Float64	conversion
ToI64	Int64	conversion
BoolToI64	Int64	conversion
I64ToBool	Bool	conversion
DynamicToBool	Bool	conversion
DynamicToI8	Int8	conversion
DynamicToI16	Int16	conversion
DynamicToI32	Int32	conversion
DynamicToI64	Int64	conversion
DynamicToU8	UInt8	conversion
DynamicToU16	UInt16	conversion
DynamicToU32	UInt32	conversion
DynamicToU64	UInt64	conversion
DynamicToF16	Float16	conversion
DynamicToF32	Float32	conversion
DynamicToF64	Float64	conversion
ToStr	string	conversion
ToString	string	conversion
StringConcat	string	concatenation (*)
ConcatStrings	string	concatenation (*)
ArrayPush	push!	array mutation
ArrayPushTypejoin	push!	array mutation (widening)
ArrayPushFirst	pushfirst!	array mutation
ArrayPop	pop!	array mutation
ArrayPopFirst	popfirst!	array mutation
ArrayInsert	insert!	array mutation
ArrayDeleteAt	deleteat!	array mutation
ArrayDeleteAtIndices	deleteat!	array mutation
IndexLoad	getindex	indexing
IndexLoadInbounds	getindex	indexing (@inbounds)
IndexLoadTyped	getindex	indexing (typed)
IndexLoadTypedInbounds	getindex	indexing (typed @inbounds)
IndexStore	setindex!	indexing
IndexStoreInbounds	setindex!	indexing (@inbounds)
IndexStoreTyped	setindex!	indexing (typed)
IndexSlice	getindex	slicing
SliceAll	getindex	slicing (:)
NewArray	Base.vect	array literal
NewArrayTyped	Base.vect	array literal (typed)
AllocUndefTyped	Array{T}(undef, ...)	allocation
AllocUndefTypedFromTuple	Array{T}(undef, ...)	allocation
AllocUndefDynamicTyped	Array{T}(undef, ...)	allocation
AllocUndefDynamicTypedFromTuple	Array{T}(undef, ...)	allocation
NewMemory	Memory{T}(undef, n)	allocation
NewMemoryDynamic	Memory{T}(undef, n)	allocation
NewMemoryDynamicTyped	Memory{T}(undef, n)	allocation
MemoryGet	memoryrefget	Memory primitive
MemorySet	memoryrefset!	Memory primitive
MemoryLength	length	Memory primitive
MatMul	*	matrix multiplication
MakeRange	:	range construction
MakeRangeF64	:	range construction
MakeRangeLazy	:	range construction
MakeStepRangeLazy	:	range construction
RangeFirst	first	range query
RangeLast	last	range query
RangeGetIndex	getindex	range indexing
RangeCollect	collect	range materialization
IterateDynamic	iterate	iteration protocol
IterateFirst	iterate	iteration protocol
IterateFirstSplit	iterate	iteration protocol
IterateNext	iterate	iteration protocol
IterateNextSplit	iterate	iteration protocol
NewTuple	tuple	construction
TupleFirst	first	tuple query
TupleSecond	getindex	tuple query
TupleGet	getindex	tuple indexing
NewNamedTuple	NamedTuple	construction
NamedTupleGetField	getproperty	field access
NamedTupleGetBySymbol	getproperty	field access
NamedTupleGetIndex	getindex	indexing
NewPairs	pairs	construction
PairsKeys	keys	pairs query
PairsValues	values	pairs query
PairsLength	length	pairs query
PairsGetBySymbol	getindex	pairs indexing
MakeRef	Ref	construction
UnwrapRef	getindex	Ref dereference
MakeSimpleVector	Core.svec	construction
DictSet	setindex!	legacy dict carrier
DictLen	length	legacy dict carrier
NewSet	Set	legacy set carrier
NewSetTyped	Set	legacy set carrier
SetAdd	push!	legacy set carrier
NtupleFunc	ntuple	HOF
NtupleRuntime	ntuple	HOF
SprintFunc	sprint	HOF
MakeGenerator	Base.Generator	construction
MakeGeneratorRuntime	Base.Generator	construction
WrapInGenerator	Base.Generator	construction
GetField	getfield	field access
GetFieldByName	getfield	field access
SetField	setfield!	field mutation
SetFieldByName	setfield!	field mutation
GetExprField	getproperty	Expr field access
GetQuoteNodeValue	getproperty	QuoteNode field access
GetGlobalRefField	getproperty	GlobalRef field access
GetLineNumberNodeField	getproperty	LineNumberNode field access
CreateExpr	Expr	construction
CreateQuoteNode	QuoteNode	construction
NewStruct	T(...)	default constructor
NewStructSplat	T(...)	default constructor (splat)
NewParametricStruct	T{...}(...)	default constructor
NewDynamicParametricStruct	T{...}(...)	default constructor
ConstructParametricType	T{...}(...)	default constructor
ConstructParametricTypeSplat	T{...}(...)	default constructor (splat)
ConstructEnum	@enum	enum construction
ApplyTypeDynamic	Core.apply_type	parametric type application
RandF64	rand	RNG
RandArg	rand	RNG
RandArray	rand	RNG
RandIntArray	rand	RNG
RandnF64	randn	RNG
RandnArg	randn	RNG
RandnArray	randn	RNG
RngRandF64	rand	RNG (explicit rng)
RngRandArrayF64	rand	RNG (explicit rng)
RngRandArrayI64	rand	RNG (explicit rng)
RngRandnF64	randn	RNG (explicit rng)
RngRandnArrayF64	randn	RNG (explicit rng)
NewMersenne	MersenneTwister	RNG construction
NewXoshiro	Xoshiro	RNG construction
NewStableRng	StableRNG	RNG construction
SeedGlobalRng	Random.seed!	RNG seeding
SleepF64	sleep	OS boundary
SleepI64	sleep	OS boundary
TimeNs	time_ns	OS boundary
PrintAny	print	IO sink
PrintAnyNoNewline	print	IO sink
PrintI64	println	IO sink (typed)
PrintI64NoNewline	print	IO sink (typed)
PrintF64	println	IO sink (typed)
PrintF64NoNewline	print	IO sink (typed)
PrintStr	println	IO sink (typed)
PrintStrNoNewline	print	IO sink (typed)
PrintNewline	println	IO sink
IOPrintlnNewline	println	IO sink
IsNothing	isnothing	predicate
IsDefined	isdefined	reflection
ThrowError	throw	exception
ThrowValue	throw	exception
Rethrow	rethrow	exception
RethrowCurrent	rethrow	exception
RethrowOther	rethrow	exception
Test	@test	Test stdlib
TestSetBegin	@testset	Test stdlib
TestSetEnd	@testset	Test stdlib
TestThrowsBegin	@test_throws	Test stdlib
TestThrowsEnd	@test_throws	Test stdlib
EOF

# Live Instr enum variant list.
awk '/^pub enum Instr \{/{f=1;next} f&&/^\}/{f=0} f' "$INSTR_RS" \
    | grep -vE '^[[:space:]]*(//|#)' \
    | grep -oE '^[[:space:]]*[A-Z][A-Za-z0-9_]*' \
    | sed 's/^[[:space:]]*//' | sort -u >"$WORK/instr_variants.txt"

# Validate curated entries against the live enum (stale rows = hard error so
# the inventory cannot silently drift as instructions are removed).
stale=$(cut -f1 "$WORK/semantic_instr.tsv" | sort | comm -23 - "$WORK/instr_variants.txt")
if [[ -n "$stale" ]]; then
    echo "ERROR: SEMANTIC_INSTR entries no longer present in Instr enum:" >&2
    echo "$stale" >&2
    echo "Remove them from the curated table in $0." >&2
    exit 1
fi

# Instr owner index (first vm/ file with a handler arm `Instr::X` + `=>`).
grep -rEn 'Instr::[A-Z][A-Za-z0-9_]*' "$VM_DIR" --include='*.rs' 2>/dev/null \
    | grep -vE ':[0-9]+:[[:space:]]*//' \
    | grep -E '=>' \
    | awk -F: '{
        file=$1
        line=$0
        while (match(line, /Instr::[A-Z][A-Za-z0-9_]*/)) {
            v=substr(line, RSTART+7, RLENGTH-7)
            print v "\t" file
            line=substr(line, RSTART+RLENGTH)
        }
    }' | sort -u \
    | awk -F'\t' '{
        v=$1; f=$2
        sub(/^subset_julia_vm\/src\//, "", f)
        if (f ~ /vm\/instr\.rs$/ || f ~ /peephole\.rs$/) { if (!(v in fb)) fb[v]=f }
        else if (!(v in owner)) owner[v]=f
        seen[v]=1
    } END {
        for (v in seen) print v "\t" ((v in owner) ? owner[v] : fb[v])
    }' | sort >"$WORK/instr_owner.tsv"

# =============================================================================
# 5. Classification join (Issue #8672; optional until that lands)
# =============================================================================
if [[ -f "$CLASSIFICATION_TSV" ]]; then
    grep -vE '^(#|kind\t)' "$CLASSIFICATION_TSV" >"$WORK/classification.tsv" || true
else
    : >"$WORK/classification.tsv"
fi

# =============================================================================
# 6. Assemble rows + metrics in one awk pass
# =============================================================================
awk -F'\t' \
    -v mode="$MODE" \
    -v upstream_ok="$UPSTREAM_OK" \
    -v owner_f="$WORK/builtin_owner.tsv" \
    -v pj_f="$WORK/pj_defs.tsv" \
    -v up_f="$WORK/up_defs.tsv" \
    -v instr_owner_f="$WORK/instr_owner.tsv" \
    -v instr_total_f="$WORK/instr_variants.txt" \
    -v semantic_f="$WORK/semantic_instr.tsv" \
    -v class_f="$WORK/classification.tsv" \
    '
function surface_base(name,    b) {
    # Normalize internal builtin canon names to their public surface function:
    # kwarg-splitting variants (floor_digits) and typed allocation variants
    # (zeros_f64, alloc_undef_i64) map back to the public name.
    b = name
    sub(/_sigdigits$/, "", b)
    sub(/_digits$/, "", b)
    sub(/_(f64|i64|bool|any)$/, "", b)
    if (b ~ /^alloc_undef/) b = "Array{T}(undef, ...)"
    return b
}
function lookup_key(name,    k) {
    # Module-qualified surfaces (Base.vect, Random.seed!, Core.svec) are
    # indexed by their unqualified name.
    k = name
    sub(/^[A-Za-z]+\./, "", k)
    return k
}
function pj_has(name) { return (lookup_key(name) in pj) }
function pj_mark(name,    k) {
    k = lookup_key(name)
    return (k in pj) ? "yes (" pj[k] ")" : "no"
}
function up_mark(name,    k) {
    k = lookup_key(name)
    if (k in up) return up[k]
    return (upstream_ok == 1) ? "-" : "?"
}
BEGIN {
    while ((getline line < owner_f) > 0) { split(line, a, "\t"); owner[a[1]] = a[2] }
    while ((getline line < pj_f) > 0) {
        split(line, a, "\t"); p = a[2]
        sub(/^subset_julia_vm\/src\/julia\//, "", p)
        pj[a[1]] = p
    }
    while ((getline line < up_f) > 0) {
        split(line, a, "\t"); p = a[2]
        sub(/^.*\/julia\//, "", p)
        up[a[1]] = p
    }
    while ((getline line < instr_owner_f) > 0) { split(line, a, "\t"); iowner[a[1]] = a[2] }
    while ((getline line < instr_total_f) > 0) instr_total++
    while ((getline line < class_f) > 0) {
        split(line, a, "\t")
        cls[a[1] ":" a[2]] = a[3]
        clsev[a[1] ":" a[2]] = a[4]
    }
    has_cls = 0
    for (k in cls) { has_cls = 1; break }

    b_total = 0; b_intrinsic = 0; b_public = 0; b_public_pj = 0; b_public_rust_only = 0
}
# --- main input: builtin_table.tsv rows (variant, canon, aliases) ------------
{
    variant = $1; canon = $2; aliases = $3
    b_total++
    base = surface_base(canon)
    is_intrinsic = (canon ~ /^_/) ? 1 : 0
    if (is_intrinsic) b_intrinsic++
    else {
        b_public++
        pub_surface[base] = 1
        if (pj_has(base)) { b_public_pj++; pub_surface_pj[base] = 1 }
        else b_public_rust_only++
    }
    row_variant[b_total] = variant
    row_canon[b_total] = canon
    row_aliases[b_total] = (aliases == "") ? "(routing only)" : aliases
    row_base[b_total] = base
    row_intrinsic[b_total] = is_intrinsic
}
END {
    # ---- semantic instr rows ----
    s_total = 0
    while ((getline line < semantic_f) > 0) {
        n = split(line, a, "\t")
        if (n < 2) continue
        s_total++
        si_variant[s_total] = a[1]
        si_surface[s_total] = a[2]
        si_note[s_total] = (n >= 3) ? a[3] : ""
        surf = a[2]
        instr_surface[surf] = 1
        if (pj_has(surf)) instr_surface_pj[surf] = 1
    }
    # distinct public rust surfaces (builtins + instrs), and rust-only among them
    for (s in pub_surface) all_surface[s] = 1
    for (s in instr_surface) all_surface[s] = 1
    n_surface = 0; n_surface_pj = 0
    for (s in all_surface) {
        n_surface++
        if (pj_has(s) || (s in pub_surface_pj) || (s in instr_surface_pj)) n_surface_pj++
    }
    n_surface_rust_only = n_surface - n_surface_pj

    # Pure Julia public function count (names not starting with _)
    pj_public = 0
    for (nme in pj) if (nme !~ /^_/) pj_public++

    conforming = b_intrinsic + b_public_pj
    conf_pct = (b_total > 0) ? sprintf("%.1f", 100 * conforming / b_total) : "0"
    ratio_pct = (n_surface + pj_public - n_surface_pj > 0) \
        ? sprintf("%.1f", 100 * n_surface_rust_only / (pj_public + n_surface_rust_only)) : "0"

    # classification counts
    delete ccount
    if (has_cls) {
        for (i = 1; i <= b_total; i++) {
            c = cls["builtin:" row_variant[i]]
            if (c == "") c = "unclassified"
            ccount[c]++
        }
        for (i = 1; i <= s_total; i++) {
            c = cls["instr:" si_variant[i]]
            if (c == "") c = "unclassified"
            ccount[c]++
        }
    }

    if (mode == "summary") {
        printf "builtin_total=%d\n", b_total
        printf "builtin_intrinsic_named=%d\n", b_intrinsic
        printf "builtin_public=%d\n", b_public
        printf "builtin_public_with_pure_julia=%d\n", b_public_pj
        printf "builtin_public_rust_only=%d\n", b_public_rust_only
        printf "semantic_instr_total=%d\n", s_total
        printf "instr_total=%d\n", instr_total
        printf "rust_semantic_surface_functions=%d\n", n_surface
        printf "rust_semantic_surface_with_pure_julia=%d\n", n_surface_pj
        printf "rust_semantic_surface_rust_only=%d\n", n_surface_rust_only
        printf "pure_julia_public_functions=%d\n", pj_public
        printf "three_layer_conformance_pct=%s\n", conf_pct
        printf "rust_only_surface_ratio_pct=%s\n", ratio_pct
        if (has_cls) for (c in ccount) printf "classified_%s=%d\n", c, ccount[c]
        exit
    }

    # ---- markdown report ----
    print "# Rust-Side Semantics Inventory (Issue #8671, parent #8648)"
    print ""
    print "Generated by `scripts/inventory_rust_semantics.sh`. Do not edit by hand."
    print ""
    print "## Summary"
    print ""
    printf "| Metric | Value |\n| --- | ---: |\n"
    printf "| `BuiltinId` variants (total) | %d |\n", b_total
    printf "| — underscored intrinsics (`_foo`, Layer-1/2 primitives) | %d |\n", b_intrinsic
    printf "| — public surface builtins | %d |\n", b_public
    printf "| — public builtins with a Pure Julia same-name definition (dispatch-first, Rust = fallback) | %d |\n", b_public_pj
    printf "| — public builtins implemented ONLY in Rust (non-conformance candidates) | %d |\n", b_public_rust_only
    printf "| Semantic `Instr` variants (of %d total `Instr` variants) | %d |\n", instr_total, s_total
    printf "| Distinct public Julia surface functions implemented in Rust | %d |\n", n_surface
    printf "| — with a Pure Julia definition of the same name | %d |\n", n_surface_pj
    printf "| — Rust-only (no Pure Julia definition) | %d |\n", n_surface_rust_only
    printf "| Pure Julia public function names (`subset_julia_vm/src/julia/`) | %d |\n", pj_public
    printf "| **Three-layer-rule conformance (builtins: intrinsic-named or dispatch-first)** | **%s%%** |\n", conf_pct
    printf "| Rust-only share of the public surface (`rust_only / (pure_julia + rust_only)`) | %s%% |\n", ratio_pct
    if (has_cls) {
        print ""
        print "Classification counts (docs/vm/rust_semantics_classification.tsv):"
        print ""
        for (c in ccount) printf "- `%s`: %d\n", c, ccount[c]
    }
    print ""
    print "## 1. BuiltinId inventory"
    print ""
    if (has_cls)
        print "| BuiltinId | Surface name | from_name aliases | Rust handler | Upstream julia path | Pure Julia def | Classification |"
    else
        print "| BuiltinId | Surface name | from_name aliases | Rust handler | Upstream julia path | Pure Julia def |"
    if (has_cls) print "| --- | --- | --- | --- | --- | --- | --- |"
    else print "| --- | --- | --- | --- | --- | --- |"
    for (i = 1; i <= b_total; i++) {
        v = row_variant[i]; canon = row_canon[i]; base = row_base[i]
        o = (v in owner) ? owner[v] : "(no vm/builtins_* handler)"
        upstr = row_intrinsic[i] ? "(VM intrinsic)" : up_mark(base)
        pjstr = row_intrinsic[i] ? "(intrinsic)" : pj_mark(base)
        line = sprintf("| `%s` | `%s` | %s | `%s` | %s | %s |", v, canon, row_aliases[i], o, upstr, pjstr)
        if (has_cls) {
            c = cls["builtin:" v]; if (c == "") c = "unclassified"
            line = line " " c " |"
        }
        print line
    }
    print ""
    print "## 2. Semantic Instr inventory"
    print ""
    printf "%d of %d `Instr` variants implement a Julia surface function; the rest are VM machinery\n", s_total, instr_total
    print "(stack/slot/jump/call plumbing, typed load/store forms, fused optimization forms,"
    print "exception-handler plumbing, cache-compatibility decode stubs)."
    print ""
    if (has_cls)
        print "| Instr | Surface function | Note | Rust handler | Pure Julia def | Classification |"
    else
        print "| Instr | Surface function | Note | Rust handler | Pure Julia def |"
    if (has_cls) print "| --- | --- | --- | --- | --- | --- |"
    else print "| --- | --- | --- | --- | --- |"
    for (i = 1; i <= s_total; i++) {
        v = si_variant[i]; surf = si_surface[i]
        o = (v in iowner) ? iowner[v] : "-"
        line = sprintf("| `%s` | `%s` | %s | `%s` | %s |", v, surf, si_note[i], o, pj_mark(surf))
        if (has_cls) {
            c = cls["instr:" v]; if (c == "") c = "unclassified"
            line = line " " c " |"
        }
        print line
    }
}' "$WORK/builtin_table.tsv"

# =============================================================================
# 7. Large Rust semantic modules by LOC (markdown mode only)
# =============================================================================
if [[ "$MODE" == "markdown" ]]; then
    echo ""
    echo "## 3. Large Rust semantic modules (LOC)"
    echo ""
    echo "Blob surfaces for the #8672 classification (matmul/hof/type_ops/... are"
    echo "semantics implemented as modules, not per-BuiltinId handlers)."
    echo ""
    echo "| Module | LOC | Semantics |"
    echo "| --- | ---: | --- |"
    print_loc() {
        # $1 = path (dir or file), $2 = description
        local n=0
        if [[ -d "$1" ]]; then
            n=$(find "$1" -name '*.rs' -type f -exec cat {} + | wc -l | tr -d ' ')
        elif [[ -f "$1" ]]; then
            n=$(wc -l <"$1" | tr -d ' ')
        else
            return 0
        fi
        echo "| \`${1#subset_julia_vm/src/}\` | $n | $2 |"
    }
    print_loc "$VM_DIR/matmul" "matrix multiplication (incl. interleaved Complex fast path)"
    print_loc "$VM_DIR/hof_exec" "higher-order function execution (map/filter/reduce runtime)"
    print_loc "$VM_DIR/type_ops" "runtime type operations (comparison/conversion/introspection/iteration/deep_copy)"
    print_loc "$VM_DIR/broadcast.rs" "broadcast fast paths"
    print_loc "$VM_DIR/formatting" "value display / show / repr formatting"
    print_loc "$VM_DIR/dynamic_ops" "dynamic dispatch helpers"
    print_loc "$VM_DIR/exec" "instruction execution (incl. semantic Instr handlers above)"
    total_builtins_loc=0
    while IFS= read -r f; do
        n=$(wc -l <"$f" | tr -d ' ')
        total_builtins_loc=$((total_builtins_loc + n))
    done < <(find "$VM_DIR" \( -name 'builtins_*.rs' -o -path '*/builtins_*/*.rs' \) -type f)
    echo "| \`vm/builtins_*\` (all handler files) | $total_builtins_loc | BuiltinId handlers (section 1) |"
    print_loc "subset_julia_vm/src/intrinsics.rs" "Layer-1 CPU intrinsics"
fi

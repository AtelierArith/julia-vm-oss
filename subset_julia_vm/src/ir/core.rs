use crate::span::Span;
use crate::types::{JuliaType, TypeExpr, TypeParam};
use half::f16;
use serde::{Deserialize, Serialize};

/// Using/import statement representation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsingImport {
    pub module: String,
    /// If None, import all exported functions (`using Module`).
    /// If Some, import only these specific functions (`using Module: func1, func2`).
    pub symbols: Option<Vec<String>>,
    /// If true, this is a relative import (`using .Module`).
    /// Relative imports refer to user-defined modules in the current program,
    /// not stdlib or external packages.
    #[serde(default)]
    pub is_relative: bool,
    pub span: Span,
}

/// Core IR - minimal representation of Julia subset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    #[serde(default)]
    pub abstract_types: Vec<AbstractTypeDef>,
    /// Type alias definitions (const TypeName = TypeExpr)
    #[serde(default)]
    pub type_aliases: Vec<TypeAliasDef>,
    pub structs: Vec<StructDef>,
    pub functions: Vec<Function>,
    /// Number of base functions (from prelude). Functions at index >= base_function_count are user functions.
    #[serde(default)]
    pub base_function_count: usize,
    pub modules: Vec<Module>,
    /// Using/import statements
    pub usings: Vec<UsingImport>,
    /// Macro definitions
    #[serde(default)]
    pub macros: Vec<MacroDef>,
    /// Enum definitions
    #[serde(default)]
    pub enums: Vec<EnumDef>,
    pub main: Block,
}

/// Module definition: `module Name ... end` or `baremodule Name ... end`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    /// Whether this is a baremodule (no automatic Base import)
    #[serde(default)]
    pub is_bare: bool,
    pub functions: Vec<Function>,
    /// Struct definitions within this module
    #[serde(default)]
    pub structs: Vec<StructDef>,
    /// Abstract type definitions within this module
    #[serde(default)]
    pub abstract_types: Vec<AbstractTypeDef>,
    /// Type alias definitions within this module
    #[serde(default)]
    pub type_aliases: Vec<TypeAliasDef>,
    /// Nested submodules
    pub submodules: Vec<Module>,
    /// Using/import statements within this module
    #[serde(default)]
    pub usings: Vec<UsingImport>,
    /// Macro definitions within this module
    #[serde(default)]
    pub macros: Vec<MacroDef>,
    /// Exported names (functions, structs, abstract types)
    pub exports: Vec<String>,
    /// Public names (Julia 1.11+): part of public API but not automatically exported
    #[serde(default)]
    pub publics: Vec<String>,
    pub body: Block,
    pub span: Span,
}

/// Struct definition: `struct Point x::Float64; y::Float64 end`
///
/// Also supports parametric types: `struct Point{T} x::T; y::T end`
/// Also supports subtyping: `struct Dog <: Animal ... end`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDef {
    pub name: String,
    pub is_mutable: bool,
    /// Type parameters for parametric structs (e.g., [T] for Point{T})
    pub type_params: Vec<TypeParam>,
    /// Parent abstract type for subtyping (e.g., "Animal" in `struct Dog <: Animal`)
    #[serde(default)]
    pub parent_type: Option<String>,
    pub fields: Vec<StructField>,
    /// Inner constructors defined within the struct body.
    /// If non-empty, no default constructor is generated.
    #[serde(default)]
    pub inner_constructors: Vec<InnerConstructor>,
    pub span: Span,
}

/// Abstract type definition: `abstract type Animal end`
///
/// Also supports subtyping: `abstract type Mammal <: Animal end`
/// Also supports parametric types: `abstract type Container{T} end`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbstractTypeDef {
    pub name: String,
    /// Parent abstract type (e.g., "Animal" in `abstract type Mammal <: Animal`).
    /// If None, defaults to Any.
    pub parent: Option<String>,
    /// Type parameters for parametric abstract types (e.g., [T] for Container{T})
    pub type_params: Vec<TypeParam>,
    pub span: Span,
}

/// Type alias definition: `const IntOrFloat = Union{Int64, Float64}`
///
/// Type aliases allow creating shorthand names for complex type expressions.
/// Examples:
/// - `const IntOrFloat = Union{Int64, Float64}`
/// - `const ComplexF64 = Complex{Float64}`
/// - `const RealArray = Array{<:Real}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeAliasDef {
    /// The alias name (e.g., "IntOrFloat", "ComplexF64")
    pub name: String,
    /// The target type expression as a string (e.g., "Union{Int64, Float64}", "Complex{Float64}")
    /// Stored as string to preserve the original syntax for resolution at compile time.
    pub target_type: String,
    pub span: Span,
}

/// Macro definition: `macro name(args) body end`
///
/// Macros are compile-time AST transformations. They receive their arguments
/// as Expr objects (unevaluated syntax) and return an Expr to be compiled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroDef {
    pub name: String,
    /// Parameter names (the macro receives AST nodes, not values)
    pub params: Vec<String>,
    /// Whether the last parameter is a varargs parameter (p...)
    pub has_varargs: bool,
    /// The macro body - an expression that should return an Expr
    pub body: Block,
    pub span: Span,
}

/// Enum definition: `@enum Color red green blue`
///
/// Enums are integer-backed symbolic types created at compile time.
/// Each member has a unique integer value, auto-incremented if not specified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumDef {
    /// The enum type name (e.g., "Color")
    pub name: String,
    /// The underlying integer type (default: "Int32")
    #[serde(default = "default_enum_base_type")]
    pub base_type: String,
    /// The enum members
    pub members: Vec<EnumMember>,
    pub span: Span,
}

fn default_enum_base_type() -> String {
    "Int32".to_string()
}

/// A single member of an enum definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumMember {
    /// Member name (e.g., "red")
    pub name: String,
    /// Integer value
    pub value: i64,
    pub span: Span,
}

impl StructDef {
    /// Check if this struct has type parameters.
    pub fn is_parametric(&self) -> bool {
        !self.type_params.is_empty()
    }
}

/// Field in a struct definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    pub name: String,
    /// Type expression (can be concrete, type variable, or parameterized)
    pub type_expr: Option<TypeExpr>,
    pub span: Span,
}

impl StructField {
    /// Get the type expression as a JuliaType if it's concrete.
    /// Returns None if the type is a type variable or parameterized.
    pub fn as_julia_type(&self) -> Option<JuliaType> {
        match &self.type_expr {
            Some(TypeExpr::Concrete(jt)) => Some(jt.clone()),
            _ => None,
        }
    }
}

/// Inner constructor definition within a struct.
///
/// Represents constructors defined inside a struct body that use `new` to create instances.
/// When a struct has inner constructors, no default constructor is generated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InnerConstructor {
    pub params: Vec<TypedParam>,
    pub kwparams: Vec<KwParam>,
    pub type_params: Vec<TypeParam>,
    pub body: Block,
    pub span: Span,
}

/// Typed parameter in function signature.
///
/// Represents a parameter with optional type annotation.
/// If `type_annotation` is `None`, the parameter is treated as `Any`.
/// If `is_varargs` is `true`, this parameter collects all remaining arguments as a Tuple.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypedParam {
    pub name: String,
    pub type_annotation: Option<JuliaType>,
    /// True if this is a varargs parameter (e.g., `args...`)
    #[serde(default)]
    pub is_varargs: bool,
    /// For Vararg{T, N}: fixed argument count N. None = any count. (Issue #2525)
    #[serde(default)]
    pub vararg_count: Option<usize>,
    pub span: Span,
}

impl TypedParam {
    /// Create a new typed parameter with a type annotation.
    pub fn new(name: String, type_annotation: Option<JuliaType>, span: Span) -> Self {
        Self {
            name,
            type_annotation,
            is_varargs: false,
            vararg_count: None,
            span,
        }
    }

    /// Create an untyped parameter (treated as Any).
    pub fn untyped(name: String, span: Span) -> Self {
        Self {
            name,
            type_annotation: None,
            is_varargs: false,
            vararg_count: None,
            span,
        }
    }

    /// Create a varargs parameter (e.g., `args...`).
    /// Collects all remaining arguments as a Tuple.
    pub fn varargs(name: String, type_annotation: Option<JuliaType>, span: Span) -> Self {
        Self {
            name,
            type_annotation,
            is_varargs: true,
            vararg_count: None,
            span,
        }
    }

    /// Create a fixed-count varargs parameter (e.g., `x::Vararg{Int64, 2}`). (Issue #2525)
    pub fn varargs_fixed(
        name: String,
        type_annotation: Option<JuliaType>,
        count: usize,
        span: Span,
    ) -> Self {
        Self {
            name,
            type_annotation,
            is_varargs: true,
            vararg_count: Some(count),
            span,
        }
    }

    /// Get the effective type (returns Any if no annotation).
    pub fn effective_type(&self) -> JuliaType {
        self.type_annotation.clone().unwrap_or(JuliaType::Any)
    }
}

/// Keyword parameter in function signature.
///
/// Represents a keyword parameter with a default value.
/// Example: `function f(; x=1, y=2.0)` has kwparams [KwParam{name="x", default=1}, ...]
/// For varargs kwargs like `kwargs...`, set `is_varargs=true` to collect all remaining kwargs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KwParam {
    pub name: String,
    pub default: Expr,
    pub type_annotation: Option<JuliaType>,
    /// True if this is a varargs kwparam (e.g., `kwargs...`).
    /// Collects all remaining keyword arguments as a NamedTuple.
    #[serde(default)]
    pub is_varargs: bool,
    pub span: Span,
}

impl KwParam {
    /// Create a new keyword parameter.
    pub fn new(
        name: String,
        default: Expr,
        type_annotation: Option<JuliaType>,
        span: Span,
    ) -> Self {
        Self {
            name,
            default,
            type_annotation,
            is_varargs: false,
            span,
        }
    }

    /// Create a varargs keyword parameter (e.g., `kwargs...`).
    pub fn varargs(name: String, span: Span) -> Self {
        Self {
            name,
            default: Expr::Literal(Literal::Nothing, span),
            type_annotation: None,
            is_varargs: true,
            span,
        }
    }

    /// Get the effective type (returns Any if no annotation).
    pub fn effective_type(&self) -> JuliaType {
        self.type_annotation.clone().unwrap_or(JuliaType::Any)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub params: Vec<TypedParam>,
    /// Keyword parameters (after `;` in function signature)
    pub kwparams: Vec<KwParam>,
    /// Type parameters from `where` clause (e.g., `where T<:Number`)
    #[serde(default)]
    pub type_params: Vec<TypeParam>,
    /// Return type annotation (e.g., `::Int` in `f(x)::Int = x`)
    /// If present, the return value will be converted to this type using `convert`.
    #[serde(default)]
    pub return_type: Option<JuliaType>,
    pub body: Block,
    /// True if this function extends a Base operator (e.g., `function Base.:+(...)`)
    /// Base extension functions do NOT shadow builtin operators for primitive types.
    #[serde(default)]
    pub is_base_extension: bool,
    pub span: Span,
}

/// Block of statements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

/// Statement in Core IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    /// Inline block (for lowering else blocks as statements)
    Block(Block),
    Assign {
        var: String,
        value: Expr,
        span: Span,
    },
    AddAssign {
        var: String,
        value: Expr,
        span: Span,
    },
    For {
        var: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Block,
        span: Span,
    },
    /// For-each loop over an iterable (string, array, tuple, etc.)
    /// `for var in iterable ... end`
    ForEach {
        var: String,
        iterable: Expr,
        body: Block,
        span: Span,
    },
    /// For-each loop with tuple destructuring
    /// `for (a, b) in iterable ... end`
    ForEachTuple {
        vars: Vec<String>,
        iterable: Expr,
        body: Block,
        span: Span,
    },
    While {
        condition: Expr,
        body: Block,
        span: Span,
    },
    If {
        condition: Expr,
        then_branch: Block,
        else_branch: Option<Block>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Try {
        try_block: Block,
        catch_var: Option<String>,
        catch_block: Option<Block>,
        else_block: Option<Block>,
        finally_block: Option<Block>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Expr {
        expr: Expr,
        span: Span,
    },
    /// @time macro: execute body, measure and print elapsed time
    /// Note: @time is now Pure Julia but Timed IR is kept for backwards compatibility
    Timed {
        body: Block,
        span: Span,
    },
    /// @test macro: test that condition is true, record result
    Test {
        condition: Expr,
        message: Option<String>,
        span: Span,
    },
    /// @testset macro: group tests and report results
    TestSet {
        name: String,
        body: Block,
        span: Span,
    },
    /// @test_throws macro: test that expression throws expected exception
    TestThrows {
        exception_type: String,
        expr: Box<Expr>,
        span: Span,
    },
    /// Array element assignment: arr[i] = x or arr[i, j] = x
    IndexAssign {
        array: String,
        indices: Vec<Expr>,
        value: Expr,
        span: Span,
    },
    /// Field assignment: obj.field = value (for mutable structs)
    FieldAssign {
        object: String,
        field: String,
        value: Expr,
        span: Span,
    },
    /// Destructuring assignment: (a, b, c) = tuple
    DestructuringAssign {
        targets: Vec<String>,
        value: Expr,
        span: Span,
    },
    /// Dict key-value assignment: dict[key] = value
    DictAssign {
        dict: String,
        key: Expr,
        value: Expr,
        span: Span,
    },
    /// Using statement: `using Module` - imports module's exported functions
    Using {
        module: String,
        span: Span,
    },
    /// Export statement: `export func1, func2` - exports functions from module
    Export {
        names: Vec<String>,
        span: Span,
    },
    /// Function definition statement (for functions defined inside blocks)
    /// This allows function definitions inside @testset and other macro bodies.
    FunctionDef {
        func: Box<Function>,
        span: Span,
    },
    /// Label statement: @label name
    /// Defines a jump target for @goto. Part of Julia's low-level control flow.
    Label {
        name: String,
        span: Span,
    },
    /// Goto statement: @goto name
    /// Unconditionally jumps to the corresponding @label. Part of Julia's low-level control flow.
    Goto {
        name: String,
        span: Span,
    },
    /// Enum definition statement: @enum TypeName member1 member2 ...
    /// Creates an integer-backed enum type with named constants.
    EnumDef {
        enum_def: EnumDef,
        span: Span,
    },
}

/// Expression in Core IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal(Literal, Span),
    Var(String, Span),
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Call {
        function: String,
        args: Vec<Expr>,
        /// Keyword arguments: [(name, value), ...]
        kwargs: Vec<(String, Expr)>,
        /// Splat mask: true at index i means args[i] should be splatted
        splat_mask: Vec<bool>,
        /// Kwargs splat mask: true at index i means kwargs[i] should be splatted.
        /// When true, the key is "" (empty) and value is the expression to expand.
        #[serde(default)]
        kwargs_splat_mask: Vec<bool>,
        span: Span,
    },
    Builtin {
        name: BuiltinOp,
        args: Vec<Expr>,
        span: Span,
    },
    /// Array literal: [1, 2, 3] or [1 2; 3 4]
    ArrayLiteral {
        elements: Vec<Expr>,
        shape: Vec<usize>,
        span: Span,
    },
    /// Typed empty array: Int[], Float64[], Complex{Float64}[], etc.
    TypedEmptyArray {
        element_type: String,
        span: Span,
    },
    /// Array indexing: arr[i] or arr[i, j]
    Index {
        array: Box<Expr>,
        indices: Vec<Expr>,
        span: Span,
    },
    /// Range expression: start:stop or start:step:stop
    Range {
        start: Box<Expr>,
        step: Option<Box<Expr>>,
        stop: Box<Expr>,
        span: Span,
    },
    /// Comprehension: [expr for var in iter] or [expr for var in iter if cond]
    Comprehension {
        body: Box<Expr>,
        var: String,
        iter: Box<Expr>,
        filter: Option<Box<Expr>>,
        span: Span,
    },
    /// Multi-variable comprehension: [expr for i in R1, j in R2] (Issue #2143)
    /// Produces a flat array via cartesian product of iterators.
    MultiComprehension {
        body: Box<Expr>,
        /// Each iteration clause: (variable_name, iterator_expression)
        iterations: Vec<(String, Expr)>,
        filter: Option<Box<Expr>>,
        span: Span,
    },
    /// Generator expression: (expr for var in iter) or (expr for var in iter if cond)
    /// Unlike Comprehension, Generator is lazy - it doesn't evaluate until iterated.
    Generator {
        body: Box<Expr>,
        var: String,
        iter: Box<Expr>,
        filter: Option<Box<Expr>>,
        span: Span,
    },
    /// Slice all elements in a dimension (:) within indexing
    SliceAll {
        span: Span,
    },
    /// Field access: obj.field
    FieldAccess {
        object: Box<Expr>,
        field: String,
        span: Span,
    },
    /// Function reference (for passing to higher-order functions)
    /// Resolved to a function index at compile time
    FunctionRef {
        name: String,
        span: Span,
    },
    /// Tuple literal: (1, 2, 3) or (x, y, z)
    TupleLiteral {
        elements: Vec<Expr>,
        span: Span,
    },
    /// Named tuple literal: (a=1, b=2, c=3)
    NamedTupleLiteral {
        fields: Vec<(String, Expr)>,
        span: Span,
    },
    /// Pair expression: key => value
    Pair {
        key: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    /// Dict literal: Dict("a" => 1, "b" => 2) or Dict(pairs...)
    DictLiteral {
        pairs: Vec<(Expr, Expr)>,
        span: Span,
    },
    /// Let block: let a = 1, b = 2; body end
    /// Evaluates to the value of the last expression in body
    LetBlock {
        /// Variable bindings: (name, value)
        bindings: Vec<(String, Expr)>,
        /// Body block containing statements, last one is the return value
        body: Block,
        span: Span,
    },
    /// String concatenation for interpolation: "x = $(x)" becomes StringConcat(["x = ", ToString(x)])
    StringConcat {
        parts: Vec<Expr>,
        span: Span,
    },
    /// Module-qualified call: Module.func(args)
    ModuleCall {
        module: String,
        function: String,
        args: Vec<Expr>,
        kwargs: Vec<(String, Expr)>,
        span: Span,
    },
    /// Ternary conditional expression: cond ? then_expr : else_expr
    /// Short-circuit evaluation: only one branch is evaluated
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        span: Span,
    },
    /// `new(args...)` or `new{T}(args...)` in inner constructor.
    /// Creates a new instance of the enclosing struct.
    New {
        type_args: Vec<TypeExpr>,
        args: Vec<Expr>,
        is_splat: bool,
        span: Span,
    },
    /// Construct a parametric type at runtime with dynamically evaluated type arguments.
    /// Example: `Complex{promote_type(T, S)}` where T, S are runtime type values.
    /// The type_args are expressions that evaluate to DataType values at runtime.
    DynamicTypeConstruct {
        /// Base type name (e.g., "Complex", "Vector")
        base: String,
        /// Expressions that evaluate to DataType values
        type_args: Vec<Expr>,
        span: Span,
    },
    /// Quoted expression: :symbol or :(expr)
    /// The inner expression constructs the quoted value at runtime.
    /// For :x -> creates Symbol("x")
    /// For :(1+2) -> creates Expr(:call, :+, 1, 2)
    QuoteLiteral {
        /// The expression that constructs the quoted value
        constructor: Box<Expr>,
        span: Span,
    },
    /// Assignment as an expression: x = value
    /// In Julia, assignments are expressions that return the assigned value.
    /// This is used for chained assignments like `local result = x = 42`
    /// or when an assignment is used in expression context.
    AssignExpr {
        /// Variable name to assign to
        var: String,
        /// Value expression
        value: Box<Expr>,
        span: Span,
    },
    /// Return expression: return expr (used in short-circuit context like `cond && return x`)
    ReturnExpr {
        value: Option<Box<Expr>>,
        span: Span,
    },
    /// Break expression: break (used in short-circuit context like `cond && break`)
    BreakExpr {
        span: Span,
    },
    /// Continue expression: continue (used in short-circuit context like `cond && continue`)
    ContinueExpr {
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Int(i64),
    Int128(i128),
    BigInt(String),
    BigFloat(String), // Julia's BigFloat type (from big"1.0" literals)
    Float(f64),
    Float32(f32), // Julia's Float32 type (from 1.0f0 literals)
    Float16(f16), // Julia's Float16 type (for REPL persistence)
    Bool(bool),
    Str(String),
    Char(char), // Julia's Char type (32-bit Unicode codepoint)
    Nothing,    // Julia's `nothing` literal
    Missing,    // Julia's `missing` literal
    /// Internal marker for required keyword arguments (no default value)
    /// Used to distinguish required kwargs from optional ones during compilation
    Undef,
    /// Module literal (e.g., Base, Core, Main)
    Module(String),
    /// Array literal with data and shape (for REPL persistence)
    Array(Vec<f64>, Vec<usize>),
    /// Int64 array literal with data and shape (for REPL persistence)
    ArrayI64(Vec<i64>, Vec<usize>),
    /// Bool array literal with data and shape (for REPL persistence)
    ArrayBool(Vec<bool>, Vec<usize>),
    /// Struct literal with type name and field values (for REPL persistence)
    /// The type name is used to look up the type_id at compile time
    Struct(String, Vec<Literal>),
    /// Symbol literal for REPL persistence (e.g., :foo)
    Symbol(String),
    /// Expr literal for REPL persistence (e.g., Meta.parse("1+1"))
    /// head: the expression head (e.g., "call", "block")
    /// args: child arguments (can contain nested Expr, Symbol, or other Literals)
    Expr {
        head: String,
        args: Vec<Literal>,
    },
    /// QuoteNode literal for REPL persistence
    QuoteNode(Box<Literal>),
    /// LineNumberNode literal for REPL persistence
    LineNumberNode {
        line: i64,
        file: Option<String>,
    },
    /// Regex literal (r"pattern" or r"pattern"imsx)
    /// pattern: the regex pattern string
    /// flags: optional flags (i=case insensitive, m=multiline, s=dotall, x=extended)
    Regex {
        pattern: String,
        flags: String,
    },
    /// Enum literal for REPL persistence (@enum type values)
    /// type_name: the enum type (e.g., "Color")
    /// value: the integer backing value of the enum member
    Enum {
        type_name: String,
        value: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv, // Integer division (÷)
    Mod,
    Pow,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    Egal,    // === (object identity)
    NotEgal, // !== (not object identity)
    Subtype, // <: (subtype check)
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
    Pos, // Unary plus (identity)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinOp {
    Rand,
    Sqrt,
    IfElse,
    TimeNs, // Get current time in nanoseconds
    // Array operations
    Zeros, // zeros(dims...) - create array filled with zeros
    Ones,  // ones(dims...) - create array filled with ones
    // Note: Fill, Trues, Falses are now Pure Julia (base/array.jl) — Issue #2640
    Reshape, // reshape(arr, dims...) - change array dimensions
    Length,  // length(arr) - total number of elements
    // Note: Sum is now Pure Julia (base/array.jl)
    Size,      // size(arr) or size(arr, dim) - dimensions
    Ndims,     // ndims(arr) - number of dimensions
    Push,      // push!(arr, val) - append element
    Pop,       // pop!(arr) - remove and return last element
    PushFirst, // pushfirst!(arr, val) - prepend element
    PopFirst,  // popfirst!(arr) - remove and return first element
    Insert,    // insert!(arr, i, val) - insert at position
    DeleteAt,  // deleteat!(arr, i) - delete at position
    Zero,      // zero(x) - return zero of the same type as x
    // Note: Complex operations (complex, real, imag, conj, abs, abs2) are now Pure Julia
    // Note: Adjoint and Transpose are now Pure Julia (base/array.jl, base/number.jl, base/complex.jl)
    // Linear algebra operations (via faer library)
    Lu,  // lu(A) - LU decomposition with partial pivoting
    Det, // det(A) - matrix determinant
    // Note: Inv removed — BuiltinOp::Inv was dead code (Issue #2643)
    // inv() is handled via BuiltinId::Inv in call.rs, not through BuiltinOp
    // RNG constructors
    StableRNG,  // StableRNG(seed) - create StableRNG instance
    XoshiroRNG, // Xoshiro(seed) - create Xoshiro256++ RNG instance
    // Normal distribution
    Randn, // randn() or randn(rng) - standard normal distribution
    // Tuple operations
    TupleFirst, // first(tuple) - get first element
    TupleLast,  // last(tuple) - get last element
    // Note: TupleLength removed — dead code, generic Length handles tuples (Issue #2643)
    // Dict operations
    HasKey,        // haskey(dict, key) - check if key exists
    DictGet,       // get(dict, key, default) - get value with default
    DictDelete,    // delete!(dict, key) - remove key-value pair
    DictKeys,      // keys(dict) - get all keys
    DictValues,    // values(dict) - get all values
    DictPairs,     // pairs(dict) - iterate over key-value pairs
    DictMerge,     // merge(dict1, dict2) - merge dictionaries
    DictGetBang,   // get!(dict, key, default) - get or insert default
    DictMergeBang, // merge!(dict1, dict2) - merge in-place
    DictEmpty,     // empty!(dict) - clear all entries
    DictGetkey,    // getkey(dict, key, default) - get the key if it exists, else default
    // Broadcasting control
    Ref, // Ref(x) - wrap value to protect from broadcasting (treated as scalar)
    // Type operations
    TypeOf,        // typeof(x) - get type name as string
    Isa,           // isa(x, T) - check if x is of type T
    Eltype,        // eltype(x) - get element type of collection
    Keytype,       // keytype(x) - get key type of collection
    Valtype,       // valtype(x) - get value type of collection
    Sizeof,        // sizeof(x) - size of value in bytes
    Isbits,        // isbits(x) - check if x is a bits type instance
    Isbitstype,    // isbitstype(T) - check if T is a bits type
    Supertype,     // supertype(T) - get parent type
    Supertypes,    // supertypes(T) - tuple of all supertypes
    Subtypes,      // subtypes(T) - vector of direct subtypes
    Typeintersect, // typeintersect(A, B) - type intersection
    // Typejoin removed - now Pure Julia (base/reflection.jl)
    // Fieldcount removed - now Pure Julia (base/reflection.jl)
    Hasfield, // hasfield(T, name) - check if field exists
    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
    // removed - now Pure Julia (base/reflection.jl) with internal intrinsics
    Ismutable, // ismutable(x) - is x mutable
    // NameOf removed - now Pure Julia (base/reflection.jl)
    Objectid,    // objectid(x) - unique object identifier
    Isunordered, // isunordered(x) - check if x is unordered (NaN, Missing)
    // Reflection (method introspection)
    Methods,   // methods(f) - get all methods for function
    HasMethod, // hasmethod(f, types) - check if method exists
    Which,     // which(f, types) - get specific method
    // Set operations
    In, // in(x, collection) - check if element is in collection
    // RNG seeding
    Seed, // seed!(n) - reseed global RNG
    // Iterator Protocol
    Iterate,   // iterate(collection) or iterate(collection, state)
    Collect,   // collect(iterable) -> Array
    Generator, // Generator(f, iter) - create lazy generator
    // Metaprogramming
    SymbolNew,          // Symbol("name") - create a symbol
    ExprNew,            // Expr(head, args...) - create an expression
    LineNumberNodeNew,  // LineNumberNode(line) or LineNumberNode(line, file)
    QuoteNodeNew,       // QuoteNode(value) - wrap value in QuoteNode
    GlobalRefNew,       // GlobalRef(mod, name) - create a global reference
    Gensym,             // gensym() or gensym("base") - generate unique symbol
    Esc,                // esc(expr) - escape expression for macro hygiene
    Eval,               // eval(expr) - evaluate an Expr at runtime
    MacroExpand,        // macroexpand(m, x) - return expanded form of macro call
    MacroExpandBang,    // macroexpand!(m, x) - destructively expand macro call
    IncludeString,      // include_string(m, code) - parse and evaluate code string
    EvalFile,           // evalfile(path) - evaluate all expressions in a file
    SplatInterpolation, // Marker for $(expr...) splat interpolation in quotes (compile-time)
    // Note: RuntimeSplatInterpolation, ExprNewWithSplat removed — dead code (Issue #2643)
    // Test operations (for Pure Julia @test/@testset/@test_throws macros)
    TestRecord,       // _test_record!(passed, msg) - record test result
    TestRecordBroken, // _test_record_broken!(passed, msg) - record broken test result
    TestSetBegin,     // _testset_begin!(name) - begin test set
    TestSetEnd,       // _testset_end!() - end test set and print summary
    // Variable reflection
    IsDefined, // @isdefined(x) - check if variable is defined
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Literal(_, span) => *span,
            Self::Var(_, span) => *span,
            Self::BinaryOp { span, .. } => *span,
            Self::UnaryOp { span, .. } => *span,
            Self::Call { span, .. } => *span,
            Self::Builtin { span, .. } => *span,
            Self::ArrayLiteral { span, .. } => *span,
            Self::TypedEmptyArray { span, .. } => *span,
            Self::Index { span, .. } => *span,
            Self::Range { span, .. } => *span,
            Self::Comprehension { span, .. } => *span,
            Self::MultiComprehension { span, .. } => *span,
            Self::Generator { span, .. } => *span,
            Self::SliceAll { span, .. } => *span,
            Self::FieldAccess { span, .. } => *span,
            Self::FunctionRef { span, .. } => *span,
            Self::TupleLiteral { span, .. } => *span,
            Self::NamedTupleLiteral { span, .. } => *span,
            Self::Pair { span, .. } => *span,
            Self::DictLiteral { span, .. } => *span,
            Self::LetBlock { span, .. } => *span,
            Self::StringConcat { span, .. } => *span,
            Self::ModuleCall { span, .. } => *span,
            Self::Ternary { span, .. } => *span,
            Self::New { span, .. } => *span,
            Self::DynamicTypeConstruct { span, .. } => *span,
            Self::QuoteLiteral { span, .. } => *span,
            Self::AssignExpr { span, .. } => *span,
            Self::ReturnExpr { span, .. } => *span,
            Self::BreakExpr { span } => *span,
            Self::ContinueExpr { span } => *span,
        }
    }
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Self::Block(block) => block.span,
            Self::Assign { span, .. } => *span,
            Self::AddAssign { span, .. } => *span,
            Self::For { span, .. } => *span,
            Self::ForEach { span, .. } => *span,
            Self::ForEachTuple { span, .. } => *span,
            Self::While { span, .. } => *span,
            Self::If { span, .. } => *span,
            Self::Break { span } => *span,
            Self::Continue { span } => *span,
            Self::Try { span, .. } => *span,
            Self::Return { span, .. } => *span,
            Self::Expr { span, .. } => *span,
            Self::Timed { span, .. } => *span,
            Self::Test { span, .. } => *span,
            Self::TestSet { span, .. } => *span,
            Self::TestThrows { span, .. } => *span,
            Self::IndexAssign { span, .. } => *span,
            Self::FieldAssign { span, .. } => *span,
            Self::DestructuringAssign { span, .. } => *span,
            Self::DictAssign { span, .. } => *span,
            Self::Using { span, .. } => *span,
            Self::Export { span, .. } => *span,
            Self::FunctionDef { span, .. } => *span,
            Self::Label { span, .. } => *span,
            Self::Goto { span, .. } => *span,
            Self::EnumDef { span, .. } => *span,
        }
    }
}

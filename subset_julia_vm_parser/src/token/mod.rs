//! Token definitions for Julia lexer
//!
//! Based on tree-sitter-julia grammar.js (lines 11-130)

mod precedence;

#[cfg(test)]
mod tests;

use logos::Logos;

pub use precedence::{Associativity, Precedence};

/// Julia tokens
///
/// Defined to match tree-sitter-julia's grammar.js
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\f]+")] // Skip whitespace (but not newlines)
pub enum Token {
    // ==================== Keywords ====================
    // grammar.js:105-130
    #[token("baremodule")]
    KwBaremodule,
    #[token("module")]
    KwModule,
    #[token("abstract")]
    KwAbstract,
    #[token("primitive")]
    KwPrimitive,
    #[token("mutable")]
    KwMutable,
    #[token("struct")]
    KwStruct,
    #[token("quote")]
    KwQuote,
    #[token("let")]
    KwLet,
    #[token("if")]
    KwIf,
    #[token("else")]
    KwElse,
    #[token("elseif")]
    KwElseif,
    #[token("try")]
    KwTry,
    #[token("catch")]
    KwCatch,
    #[token("finally")]
    KwFinally,
    #[token("for")]
    KwFor,
    #[token("while")]
    KwWhile,
    #[token("break")]
    KwBreak,
    #[token("continue")]
    KwContinue,
    #[token("using")]
    KwUsing,
    #[token("import")]
    KwImport,
    #[token("export")]
    KwExport,
    #[token("public")]
    KwPublic,
    #[token("const")]
    KwConst,
    #[token("global")]
    KwGlobal,
    #[token("local")]
    KwLocal,
    #[token("end")]
    KwEnd,
    #[token("function")]
    KwFunction,
    #[token("macro")]
    KwMacro,
    #[token("return")]
    KwReturn,
    #[token("begin")]
    KwBegin,
    #[token("do")]
    KwDo,
    #[token("in")]
    KwIn,
    #[token("isa")]
    KwIsa,
    #[token("where")]
    KwWhere,
    #[token("outer")]
    KwOuter,
    #[token("type")]
    KwType,
    #[token("as")]
    KwAs,

    // ==================== Boolean Literals ====================
    #[token("true")]
    True,
    #[token("false")]
    False,

    // ==================== Delimiters ====================
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,

    // ==================== Punctuation ====================
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token("::")]
    DoubleColon,
    #[token(":")]
    Colon,
    #[token(".")]
    Dot,
    #[token("...")]
    Ellipsis,
    #[token("@")]
    At,
    #[token("$")]
    Dollar,
    #[token("?")]
    Question,

    // ==================== Assignment Operators ====================
    // grammar.js:43-46
    #[token("=")]
    Eq,
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("//=")]
    SlashSlashEq,
    #[token("\\=")]
    BackslashEq,
    #[token("^=")]
    CaretEq,
    #[token("%=")]
    PercentEq,
    #[token("<<=")]
    LtLtEq,
    #[token(">>=")]
    GtGtEq,
    #[token(">>>=")]
    GtGtGtEq,
    #[token("|=")]
    PipeEq,
    #[token("&=")]
    AmpEq,
    #[token(":=")]
    ColonEq,
    #[token("$=")]
    DollarEq,
    #[token(".=")]
    DotEq,
    #[token(".+=")]
    DotPlusEq,
    #[token(".-=")]
    DotMinusEq,
    #[token(".*=")]
    DotStarEq,
    #[token("./=")]
    DotSlashEq,
    #[token(".\\=")]
    DotBackslashEq,
    #[token(".^=")]
    DotCaretEq,
    #[token(".%=")]
    DotPercentEq,
    #[token(".//=")]
    DotSlashSlashEq,
    #[token(".&=")]
    DotAmpEq,
    #[token(".|=")]
    DotPipeEq,
    #[token("~")]
    Tilde,

    // Unicode assignment operators
    #[token("\u{2212}=")] // −=
    MinusSignEq,
    #[token("\u{00F7}=")] // ÷=
    DivisionSignEq,
    #[token("\u{22BB}=")] // ⊻=
    XorEq,
    #[token("\u{2254}")] // ≔
    ColonEquals,
    #[token("\u{2A74}")] // ⩴
    DoubleColonEquals,
    #[token("\u{2255}")] // ≕
    EqualsColon,

    // ==================== Arrow Operators ====================
    // grammar.js:48-54
    #[token("->")]
    Arrow,
    #[token("<--")]
    LeftArrow2,
    #[token("-->")]
    RightArrow2,
    #[token("<-->")]
    LeftRightArrow2,
    #[token("\u{2190}")] // ←
    LeftArrow,
    #[token("\u{2192}")] // →
    RightArrow,
    #[token("\u{2194}")] // ↔
    LeftRightArrow,

    // ==================== Comparison Operators ====================
    // grammar.js:56-66
    #[token(">")]
    Gt,
    #[token("<")]
    Lt,
    #[token(">=")]
    GtEq,
    #[token("<=")]
    LtEq,
    #[token("==")]
    EqEq,
    #[token("===")]
    EqEqEq,
    #[token("!=")]
    NotEq,
    #[token("!==")]
    NotEqEq,
    #[token("<:")]
    Subtype,
    #[token(">:")]
    Supertype,

    // Unicode comparison operators
    #[token("\u{2265}")] // ≥
    GreaterEqual,
    #[token("\u{2264}")] // ≤
    LessEqual,
    #[token("\u{2261}")] // ≡
    Identical,
    #[token("\u{2260}")] // ≠
    NotEqual,
    #[token("\u{2248}")] // ≈
    Approx,
    #[token("\u{2249}")] // ≉
    NotApprox,
    #[token("\u{2262}")] // ≢
    NotIdentical,
    #[token("\u{2208}")] // ∈
    ElementOf,
    #[token("\u{2209}")] // ∉
    NotElementOf,
    #[token("\u{220B}")] // ∋
    Contains,
    #[token("\u{220C}")] // ∌
    NotContains,
    #[token("\u{2286}")] // ⊆
    SubsetEq,
    #[token("\u{2288}")] // ⊈
    NotSubsetEq,
    #[token("\u{2282}")] // ⊂
    Subset,
    #[token("\u{2284}")] // ⊄
    NotSubset,
    #[token("\u{228A}")] // ⊊
    StrictSubset,
    #[token("\u{2287}")] // ⊇
    SupersetEq,
    #[token("\u{2289}")] // ⊉
    NotSupersetEq,
    #[token("\u{2283}")] // ⊃
    Superset,
    #[token("\u{2285}")] // ⊅
    NotSuperset,
    #[token("\u{228B}")] // ⊋
    StrictSuperset,

    // ==================== Lazy Boolean Operators ====================
    #[token("||")]
    OrOr,
    #[token("&&")]
    AndAnd,

    // ==================== Pipe Operators ====================
    #[token("|>")]
    PipeRight,
    #[token("<|")]
    PipeLeft,

    // ==================== Range/Ellipsis Operators ====================
    #[token("..")]
    DotDot,
    #[token("\u{2026}")] // …
    HorizontalEllipsis,

    // ==================== Plus Operators ====================
    // grammar.js:70-74
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("++")]
    PlusPlus,
    #[token("|")]
    Pipe,

    // Unicode plus operators
    #[token("\u{2212}")] // −
    MinusSign,
    #[token("\u{2295}")] // ⊕
    CirclePlus,
    #[token("\u{2296}")] // ⊖
    CircleMinus,
    #[token("\u{222A}")] // ∪
    Union,
    #[token("\u{2228}")] // ∨
    LogicalOr,
    #[token("\u{2294}")] // ⊔
    SquareUnion,

    // ==================== Times Operators ====================
    // grammar.js:76-80
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("&")]
    Amp,
    #[token("\\")]
    Backslash,

    // Unicode times operators
    #[token("\u{00D7}")] // ×
    Times,
    #[token("\u{00F7}")] // ÷
    Divide,
    #[token("\u{22C5}")] // ⋅
    DotOperator,
    #[token("\u{2218}")] // ∘
    RingOperator,
    #[token("\u{2229}")] // ∩
    Intersection,
    #[token("\u{2227}")] // ∧
    LogicalAnd,
    #[token("\u{2297}")] // ⊗
    CircleTimes,
    #[token("\u{2298}")] // ⊘
    CircleDivide,
    #[token("\u{2299}")] // ⊙
    CircleDot,
    #[token("\u{2293}")] // ⊓
    SquareIntersection,
    #[token("\u{22BB}")] // ⊻
    Xor,

    // ==================== Rational Operator ====================
    #[token("//")]
    SlashSlash,

    // ==================== Bitshift Operators ====================
    // grammar.js:82
    #[token("<<")]
    LtLt,
    #[token(">>")]
    GtGt,
    #[token(">>>")]
    GtGtGt,

    // ==================== Power Operator ====================
    // grammar.js:84-87
    #[token("^")]
    Caret,

    // Unicode power operators
    #[token("\u{2191}")] // ↑
    UpArrow,
    #[token("\u{2193}")] // ↓
    DownArrow,

    // ==================== Unary Operators ====================
    // grammar.js:89
    #[token("!")]
    Not,
    #[token("\u{00AC}")] // ¬
    LogicalNot,
    #[token("\u{221A}")] // √
    SquareRoot,
    #[token("\u{221B}")] // ∛
    CubeRoot,
    #[token("\u{221C}")] // ∜
    FourthRoot,

    // ==================== Broadcast Operators ====================
    #[token(".+")]
    DotPlus,
    #[token(".-")]
    DotMinus,
    #[token(".*")]
    DotStar,
    #[token("./")]
    DotSlash,
    #[token(".\\")]
    DotBackslash,
    #[token(".^")]
    DotCaret,
    #[token(".%")]
    DotPercent,
    #[token(".<")]
    DotLt,
    #[token(".>")]
    DotGt,
    #[token(".<=")]
    DotLtEq,
    #[token(".>=")]
    DotGtEq,
    #[token(".==")]
    DotEqEq,
    #[token(".!=")]
    DotNotEq,
    #[token(".&")]
    DotAmp,
    #[token(".|")]
    DotPipe,
    #[token(".&&")]
    DotAndAnd,
    #[token(".||")]
    DotOrOr,

    // ==================== Special ====================
    #[token("'")]
    Prime, // Adjoint/transpose
    #[token("=>")]
    FatArrow, // Pair operator

    // ==================== Newline ====================
    #[regex(r"\r?\n")]
    Newline,

    // ==================== Comments ====================
    // Line comment must not start with #= (block comment)
    #[regex(r"#([^=\n][^\n]*)?")]
    LineComment,

    // Block comments handled specially (need nesting support)
    // Higher priority ensures #= is matched as BlockCommentStart, not LineComment
    #[token("#=", priority = 3)]
    BlockCommentStart,

    // ==================== Literals ====================

    // Integer literals (0b, 0o, 0x with underscores)
    #[regex(r"0[bB][01]([01]|_[01])*")]
    BinaryLiteral,
    #[regex(r"0[oO][0-7]([0-7]|_[0-7])*")]
    OctalLiteral,
    #[regex(r"0[xX][0-9a-fA-F]([0-9a-fA-F]|_[0-9a-fA-F])*")]
    HexLiteral,
    #[regex(r"[0-9]([0-9]|_[0-9])*")]
    DecimalLiteral,

    // Float literals
    #[regex(r"\.[0-9]([0-9]|_[0-9])*([eEf][+-]?[0-9]+)?")]
    FloatLeadingDot,
    #[regex(r"[0-9]([0-9]|_[0-9])*\.[0-9]*([eEf][+-]?[0-9]+)?")]
    FloatLiteral,
    #[regex(r"[0-9]([0-9]|_[0-9])*[eEf][+-]?[0-9]+")]
    FloatExponent,
    // Hex float: 0x... with p exponent
    #[regex(r"0[xX]([0-9a-fA-F]([0-9a-fA-F]|_[0-9a-fA-F])*)?\.?[0-9a-fA-F]*[pP][+-]?[0-9]+")]
    HexFloat,

    // String literals
    #[token("\"")]
    DoubleQuote,
    #[token("\"\"\"")]
    TripleDoubleQuote,

    // Character literals
    // Supports:
    // - Single character: 'a', 'α'
    // - Standard escapes: '\n', '\t', '\\', '\'', '\"', '\0'
    // - Hex escapes: '\x41' (2 hex digits)
    // - Unicode escapes: '\u0041' (4 hex digits)
    // - Unicode escapes (long): '\U00000041' (8 hex digits)
    // - Named escapes: '\N{GREEK SMALL LETTER ALPHA}'
    #[regex(
        r"'([^'\\]|\\x[0-9a-fA-F]{2}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}|\\N\{[^}]+\}|\\[^\n])'"
    )]
    CharLiteral,

    // Command literals
    #[token("`")]
    Backtick,
    #[token("```")]
    TripleBacktick,

    // ==================== Identifiers ====================
    // Julia identifiers can include many Unicode characters
    // This includes:
    // - XID_Start/XID_Continue for standard Unicode identifiers
    // - Mathematical symbols like ∑ (U+2211), ∫ (U+222B)
    // - Subscript digits (U+2080-U+2089) and letters (U+2090-U+209C)
    // - Superscript digits (U+2070-U+2079) and letters
    // - Trailing ! for mutating functions (sort!, push!, etc.)
    // Note: Excludes √∛∜ (U+221A-U+221C) which are unary operators
    #[regex(r"[_\p{XID_Start}\u{2200}-\u{2219}\u{221D}-\u{22FF}\u{2A00}-\u{2AFF}][_\p{XID_Continue}\u{2080}-\u{209C}\u{2070}-\u{207F}]*!?")]
    Identifier,

    // Macro identifier
    // @name - handled separately (@ is its own token)

    // ==================== Error ====================
    Error,
}

impl Token {
    /// Check if this token is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::KwBaremodule
                | Token::KwModule
                | Token::KwAbstract
                | Token::KwPrimitive
                | Token::KwMutable
                | Token::KwStruct
                | Token::KwQuote
                | Token::KwLet
                | Token::KwIf
                | Token::KwElse
                | Token::KwElseif
                | Token::KwTry
                | Token::KwCatch
                | Token::KwFinally
                | Token::KwFor
                | Token::KwWhile
                | Token::KwBreak
                | Token::KwContinue
                | Token::KwUsing
                | Token::KwImport
                | Token::KwExport
                | Token::KwPublic
                | Token::KwConst
                | Token::KwGlobal
                | Token::KwLocal
                | Token::KwEnd
                | Token::KwFunction
                | Token::KwMacro
                | Token::KwReturn
                | Token::KwBegin
                | Token::KwDo
                | Token::KwIn
                | Token::KwIsa
                | Token::KwWhere
                | Token::KwOuter
                | Token::KwType
                | Token::KwAs
        )
    }

    /// Check if this token is an operator
    pub fn is_operator(&self) -> bool {
        matches!(
            self,
            Token::Plus
                | Token::Minus
                | Token::Star
                | Token::Slash
                | Token::Percent
                | Token::Caret
                | Token::Amp
                | Token::Pipe
                | Token::Backslash
                | Token::Lt
                | Token::Gt
                | Token::LtEq
                | Token::GtEq
                | Token::EqEq
                | Token::EqEqEq
                | Token::NotEq
                | Token::NotEqEq
                | Token::Subtype
                | Token::Supertype
                | Token::AndAnd
                | Token::OrOr
                | Token::Not
                | Token::Tilde
                | Token::LtLt
                | Token::GtGt
                | Token::GtGtGt
                | Token::SlashSlash
                | Token::PipeRight
                | Token::PipeLeft
                | Token::Arrow
                | Token::FatArrow
                | Token::DotPlus
                | Token::DotMinus
                | Token::DotStar
                | Token::DotSlash
                | Token::DotBackslash
                | Token::DotCaret
                | Token::DotPercent
                | Token::DotLt
                | Token::DotGt
                | Token::DotLtEq
                | Token::DotGtEq
                | Token::DotEqEq
                | Token::DotNotEq
                | Token::DotAmp
                | Token::DotPipe
                | Token::DotAndAnd
                | Token::DotOrOr
                | Token::Xor
                | Token::DoubleColon
        )
    }

    /// Check if this token is an assignment operator (including simple =)
    pub fn is_assignment(&self) -> bool {
        matches!(
            self,
            Token::Eq
                | Token::PlusEq
                | Token::MinusEq
                | Token::StarEq
                | Token::SlashEq
                | Token::SlashSlashEq
                | Token::BackslashEq
                | Token::CaretEq
                | Token::PercentEq
                | Token::LtLtEq
                | Token::GtGtEq
                | Token::GtGtGtEq
                | Token::PipeEq
                | Token::AmpEq
                | Token::ColonEq
                | Token::DollarEq
                | Token::DotEq
                | Token::DotPlusEq
                | Token::DotMinusEq
                | Token::DotStarEq
                | Token::DotSlashEq
                | Token::DotBackslashEq
                | Token::DotCaretEq
                | Token::DotPercentEq
                | Token::DotSlashSlashEq
                | Token::DotAmpEq
                | Token::DotPipeEq
        )
    }

    /// Check if this token is a compound assignment operator (e.g., +=, -=, but NOT =)
    pub fn is_compound_assignment(&self) -> bool {
        matches!(
            self,
            Token::PlusEq
                | Token::MinusEq
                | Token::StarEq
                | Token::SlashEq
                | Token::SlashSlashEq
                | Token::BackslashEq
                | Token::CaretEq
                | Token::PercentEq
                | Token::LtLtEq
                | Token::GtGtEq
                | Token::GtGtGtEq
                | Token::PipeEq
                | Token::AmpEq
                | Token::ColonEq
                | Token::DollarEq
                | Token::DotEq
                | Token::DotPlusEq
                | Token::DotMinusEq
                | Token::DotStarEq
                | Token::DotSlashEq
                | Token::DotBackslashEq
                | Token::DotCaretEq
                | Token::DotPercentEq
                | Token::DotSlashSlashEq
                | Token::DotAmpEq
                | Token::DotPipeEq
                | Token::MinusSignEq
                | Token::DivisionSignEq
                | Token::XorEq
        )
    }

    /// Check if this token is a literal
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            Token::True
                | Token::False
                | Token::BinaryLiteral
                | Token::OctalLiteral
                | Token::HexLiteral
                | Token::DecimalLiteral
                | Token::FloatLeadingDot
                | Token::FloatLiteral
                | Token::FloatExponent
                | Token::HexFloat
                | Token::CharLiteral
        )
    }

    /// Check if this token is a dotted (broadcast) operator like .+, .-, .*, etc.
    pub fn is_dotted_operator(&self) -> bool {
        matches!(
            self,
            Token::DotPlus
                | Token::DotMinus
                | Token::DotStar
                | Token::DotSlash
                | Token::DotBackslash
                | Token::DotCaret
                | Token::DotPercent
                | Token::DotLt
                | Token::DotGt
                | Token::DotLtEq
                | Token::DotGtEq
                | Token::DotEqEq
                | Token::DotNotEq
                | Token::DotAmp
                | Token::DotPipe
                | Token::DotAndAnd
                | Token::DotOrOr
        )
    }

    /// Get the base operator name for a dotted operator (e.g., .+ -> "+")
    pub fn dotted_operator_base(&self) -> Option<&'static str> {
        match self {
            Token::DotPlus => Some("+"),
            Token::DotMinus => Some("-"),
            Token::DotStar => Some("*"),
            Token::DotSlash => Some("/"),
            Token::DotBackslash => Some("\\"),
            Token::DotCaret => Some("^"),
            Token::DotPercent => Some("%"),
            Token::DotLt => Some("<"),
            Token::DotGt => Some(">"),
            Token::DotLtEq => Some("<="),
            Token::DotGtEq => Some(">="),
            Token::DotEqEq => Some("=="),
            Token::DotNotEq => Some("!="),
            Token::DotAmp => Some("&"),
            Token::DotPipe => Some("|"),
            Token::DotAndAnd => Some("&&"),
            Token::DotOrOr => Some("||"),
            _ => None,
        }
    }

    /// Get the symbol text for a keyword token (for keyword symbols like :if, :for, :quote)
    /// Returns None if the token is not a keyword
    pub fn keyword_as_symbol_text(&self) -> Option<&'static str> {
        match self {
            Token::KwBaremodule => Some("baremodule"),
            Token::KwModule => Some("module"),
            Token::KwAbstract => Some("abstract"),
            Token::KwPrimitive => Some("primitive"),
            Token::KwMutable => Some("mutable"),
            Token::KwStruct => Some("struct"),
            Token::KwQuote => Some("quote"),
            Token::KwLet => Some("let"),
            Token::KwIf => Some("if"),
            Token::KwElse => Some("else"),
            Token::KwElseif => Some("elseif"),
            Token::KwTry => Some("try"),
            Token::KwCatch => Some("catch"),
            Token::KwFinally => Some("finally"),
            Token::KwFor => Some("for"),
            Token::KwWhile => Some("while"),
            Token::KwBreak => Some("break"),
            Token::KwContinue => Some("continue"),
            Token::KwUsing => Some("using"),
            Token::KwImport => Some("import"),
            Token::KwExport => Some("export"),
            Token::KwPublic => Some("public"),
            Token::KwConst => Some("const"),
            Token::KwGlobal => Some("global"),
            Token::KwLocal => Some("local"),
            Token::KwEnd => Some("end"),
            Token::KwFunction => Some("function"),
            Token::KwMacro => Some("macro"),
            Token::KwReturn => Some("return"),
            Token::KwBegin => Some("begin"),
            Token::KwDo => Some("do"),
            Token::KwIn => Some("in"),
            Token::KwIsa => Some("isa"),
            Token::KwWhere => Some("where"),
            Token::KwOuter => Some("outer"),
            Token::KwType => Some("type"),
            Token::KwAs => Some("as"),
            Token::True => Some("true"),
            Token::False => Some("false"),
            _ => None,
        }
    }
}

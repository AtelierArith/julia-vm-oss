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
    // NOTE: `outer`, `type`, `as`, `where`, and `public` are NOT lexed as
    // keywords. Upstream Julia treats each as a *contextual* keyword,
    // significant only in one position:
    //   - `outer` only inside `for outer x in ...` (outer-local-variable
    //     modifier) — Issue #8099.
    //   - `type` only after `abstract`/`primitive` (`abstract type … end`,
    //     `primitive type … N end`) — Issue #8108.
    //   - `as` only in import/using aliasing (`import X as Y`,
    //     `using M: f as g`) — Issue #8108.
    //   - `where` only after a type/function-head expression where a `where`
    //     clause is syntactically valid — Issue #8755.
    //   - `public` only at statement start introducing a public-name list
    //     (`public foo, bar`) — Issue #9637. Everywhere else it is a plain
    //     identifier, including as a macro/function name (`macro public(ex)`,
    //     `public(x) = ...`).
    // We therefore lex them as normal `Identifier`s and detect the contextual
    // positions by text in `parse_top_level_item` / `parse_for_binding` /
    // `parse_abstract_definition` / `parse_primitive_definition` / the import
    // parser.

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
    #[token(".<<=")]
    DotLtLtEq,
    #[token(".>>=")]
    DotGtGtEq,
    #[token(".>>>=")]
    DotGtGtGtEq,
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
    #[token(".\u{00F7}=")] // .÷=
    DotDivisionSignEq,
    #[token(".\u{22BB}=")] // .⊻=
    DotXorEq,
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
    #[token(".-->")]
    DotRightArrow2,
    #[token(".<-->")]
    DotLeftRightArrow2,
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
    #[token("\u{2272}")] // ≲
    LessSimilar,

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

    // ==================== Generic Unicode Operators (Issue #11083) ====================
    // Upstream Julia lets ANY character in its operator tables be used as a
    // user-definable operator, and lets an operator name carry identifier-like
    // suffixes (primes, sub/superscripts, combining marks). sjulia used to
    // recognize only an ad-hoc allowlist (`OtherUnicodeOperator`), so `⊛`, `⊝`,
    // `⊞`, `⊠`, `⋆` and suffixed names such as `⊗ᵢ` failed to parse.
    //
    // The character classes below are derived MECHANICALLY from upstream's
    // precedence tables in `julia/src/julia-parser.scm` (`prec-arrow`,
    // `prec-comparison`, `prec-plus`, `prec-times`, `prec-power`, `prec-colon`)
    // and the operator-suffix table in `julia/src/flisp/julia_opsuffs.h`
    // (`jl_op_suffix_char`: primes, sub/superscripts and combining marks).
    // Each class therefore carries UPSTREAM's precedence, not a single
    // catch-all precedence.
    //
    // Shape of each pattern: `(<chars that already have a dedicated token, plus
    // the ASCII operators of the class>)<suffix>+ | (<remaining class
    // chars>)<suffix>*`. The first alternative only fires with at least one
    // suffix character, so bare `+`, `⊗`, `≤`, ... keep their dedicated tokens
    // (and their existing lexing boundaries); the second gives the previously
    // unknown characters an operator token. `priority = 12` outranks the
    // `Identifier` regex, whose start class overlaps the math-symbol blocks.
    // The colon class takes no suffix (upstream's `no-suffix?` set).
    #[regex(r"([\u{2190}\u{2192}\u{2194}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]+|([\u{219A}\u{219B}\u{219C}\u{219D}\u{219E}\u{21A0}\u{21A2}\u{21A3}\u{21A4}\u{21A6}\u{21A9}\u{21AA}\u{21AB}\u{21AC}\u{21AE}\u{21B6}\u{21B7}\u{21BA}\u{21BB}\u{21BC}\u{21BD}\u{21C0}\u{21C1}\u{21C4}\u{21C6}\u{21C7}\u{21C9}\u{21CB}\u{21CC}\u{21CD}\u{21CE}\u{21CF}\u{21D0}\u{21D2}\u{21D4}\u{21DA}\u{21DB}\u{21DC}\u{21DD}\u{21E0}\u{21E2}\u{21F4}\u{21F6}\u{21F7}\u{21F8}\u{21F9}\u{21FA}\u{21FB}\u{21FC}\u{21FD}\u{21FE}\u{21FF}\u{27F5}\u{27F6}\u{27F7}\u{27F9}\u{27FA}\u{27FB}\u{27FC}\u{27FD}\u{27FE}\u{27FF}\u{2900}\u{2901}\u{2902}\u{2903}\u{2904}\u{2905}\u{2906}\u{2907}\u{290C}\u{290D}\u{290E}\u{290F}\u{2910}\u{2911}\u{2914}\u{2915}\u{2916}\u{2917}\u{2918}\u{291D}\u{291E}\u{291F}\u{2920}\u{2944}\u{2945}\u{2946}\u{2947}\u{2948}\u{294A}\u{294B}\u{294E}\u{2950}\u{2952}\u{2953}\u{2956}\u{2957}\u{295A}\u{295B}\u{295E}\u{295F}\u{2962}\u{2964}\u{2966}\u{2967}\u{2968}\u{2969}\u{296A}\u{296B}\u{296C}\u{296D}\u{2970}\u{2977}\u{297A}\u{29F4}\u{2B30}\u{2B31}\u{2B32}\u{2B33}\u{2B34}\u{2B35}\u{2B36}\u{2B37}\u{2B38}\u{2B39}\u{2B3A}\u{2B3B}\u{2B3C}\u{2B3D}\u{2B3E}\u{2B3F}\u{2B40}\u{2B41}\u{2B42}\u{2B43}\u{2B44}\u{2B47}\u{2B48}\u{2B49}\u{2B4A}\u{2B4B}\u{2B4C}\u{FFE9}\u{FFEB}\u{1F8B2}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 12)]
    UnicodeOpArrow,

    #[regex(r"([<>~\u{2208}\u{2209}\u{220B}\u{220C}\u{2248}\u{2249}\u{2260}\u{2261}\u{2262}\u{2264}\u{2265}\u{2272}\u{2282}\u{2283}\u{2284}\u{2285}\u{2286}\u{2287}\u{2288}\u{2289}\u{228A}\u{228B}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]+|([\u{220A}\u{220D}\u{221D}\u{2225}\u{2226}\u{2237}\u{223A}\u{223B}\u{223D}\u{223E}\u{2241}\u{2242}\u{2243}\u{2244}\u{2245}\u{2246}\u{2247}\u{224A}\u{224B}\u{224C}\u{224D}\u{224E}\u{2250}\u{2251}\u{2252}\u{2253}\u{2256}\u{2257}\u{2258}\u{2259}\u{225A}\u{225B}\u{225C}\u{225D}\u{225E}\u{225F}\u{2263}\u{2266}\u{2267}\u{2268}\u{2269}\u{226A}\u{226B}\u{226C}\u{226D}\u{226E}\u{226F}\u{2270}\u{2271}\u{2273}\u{2274}\u{2275}\u{2276}\u{2277}\u{2278}\u{2279}\u{227A}\u{227B}\u{227C}\u{227D}\u{227E}\u{227F}\u{2280}\u{2281}\u{228F}\u{2290}\u{2291}\u{2292}\u{229C}\u{22A2}\u{22A3}\u{22A9}\u{22AC}\u{22AE}\u{22B0}\u{22B1}\u{22B2}\u{22B3}\u{22B4}\u{22B5}\u{22B6}\u{22B7}\u{22CD}\u{22D0}\u{22D1}\u{22D5}\u{22D6}\u{22D7}\u{22D8}\u{22D9}\u{22DA}\u{22DB}\u{22DC}\u{22DD}\u{22DE}\u{22DF}\u{22E0}\u{22E1}\u{22E2}\u{22E3}\u{22E4}\u{22E5}\u{22E6}\u{22E7}\u{22E8}\u{22E9}\u{22EA}\u{22EB}\u{22EC}\u{22ED}\u{22F2}\u{22F3}\u{22F4}\u{22F5}\u{22F6}\u{22F7}\u{22F8}\u{22F9}\u{22FA}\u{22FB}\u{22FC}\u{22FD}\u{22FE}\u{22FF}\u{27C2}\u{27C8}\u{27C9}\u{27D2}\u{29B7}\u{29C0}\u{29C1}\u{29E1}\u{29E3}\u{29E4}\u{29E5}\u{2A66}\u{2A67}\u{2A6A}\u{2A6B}\u{2A6C}\u{2A6D}\u{2A6E}\u{2A6F}\u{2A70}\u{2A71}\u{2A72}\u{2A73}\u{2A75}\u{2A76}\u{2A77}\u{2A78}\u{2A79}\u{2A7A}\u{2A7B}\u{2A7C}\u{2A7D}\u{2A7E}\u{2A7F}\u{2A80}\u{2A81}\u{2A82}\u{2A83}\u{2A84}\u{2A85}\u{2A86}\u{2A87}\u{2A88}\u{2A89}\u{2A8A}\u{2A8B}\u{2A8C}\u{2A8D}\u{2A8E}\u{2A8F}\u{2A90}\u{2A91}\u{2A92}\u{2A93}\u{2A94}\u{2A95}\u{2A96}\u{2A97}\u{2A98}\u{2A99}\u{2A9A}\u{2A9B}\u{2A9C}\u{2A9D}\u{2A9E}\u{2A9F}\u{2AA0}\u{2AA1}\u{2AA2}\u{2AA3}\u{2AA4}\u{2AA5}\u{2AA6}\u{2AA7}\u{2AA8}\u{2AA9}\u{2AAA}\u{2AAB}\u{2AAC}\u{2AAD}\u{2AAE}\u{2AAF}\u{2AB0}\u{2AB1}\u{2AB2}\u{2AB3}\u{2AB4}\u{2AB5}\u{2AB6}\u{2AB7}\u{2AB8}\u{2AB9}\u{2ABA}\u{2ABB}\u{2ABC}\u{2ABD}\u{2ABE}\u{2ABF}\u{2AC0}\u{2AC1}\u{2AC2}\u{2AC3}\u{2AC4}\u{2AC5}\u{2AC6}\u{2AC7}\u{2AC8}\u{2AC9}\u{2ACA}\u{2ACB}\u{2ACC}\u{2ACD}\u{2ACE}\u{2ACF}\u{2AD0}\u{2AD1}\u{2AD2}\u{2AD3}\u{2AD4}\u{2AD5}\u{2AD6}\u{2AD7}\u{2AD8}\u{2AD9}\u{2AEA}\u{2AEB}\u{2AF7}\u{2AF8}\u{2AF9}\u{2AFA}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 12)]
    UnicodeOpComparison,

    #[regex(r"([+\-|\u{2212}\u{2228}\u{222A}\u{2294}\u{2295}\u{2296}\u{22BB}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]+|([\u{00A6}\u{00B1}\u{2213}\u{2214}\u{2238}\u{224F}\u{228E}\u{229E}\u{229F}\u{22BD}\u{22CE}\u{22D3}\u{27C7}\u{29FA}\u{29FB}\u{2A08}\u{2A22}\u{2A23}\u{2A24}\u{2A25}\u{2A26}\u{2A27}\u{2A28}\u{2A29}\u{2A2A}\u{2A2B}\u{2A2C}\u{2A2D}\u{2A2E}\u{2A39}\u{2A3A}\u{2A41}\u{2A42}\u{2A45}\u{2A4A}\u{2A4C}\u{2A4F}\u{2A50}\u{2A52}\u{2A54}\u{2A56}\u{2A57}\u{2A5B}\u{2A5D}\u{2A61}\u{2A62}\u{2A63}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 12)]
    UnicodeOpPlus,

    #[regex(r"([*/\\%&\u{00D7}\u{00F7}\u{2218}\u{2227}\u{2229}\u{2293}\u{2297}\u{2298}\u{2299}\u{22C5}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]+|([\u{00B7}\u{0387}\u{214B}\u{2217}\u{2219}\u{2224}\u{2240}\u{228D}\u{229A}\u{229B}\u{22A0}\u{22A1}\u{22BC}\u{22C4}\u{22C6}\u{22C7}\u{22C9}\u{22CA}\u{22CB}\u{22CC}\u{22CF}\u{22D2}\u{233F}\u{25B7}\u{27D1}\u{27D5}\u{27D6}\u{27D7}\u{29B8}\u{29BC}\u{29BE}\u{29BF}\u{29F6}\u{29F7}\u{2A07}\u{2A1D}\u{2A1F}\u{2A30}\u{2A31}\u{2A32}\u{2A33}\u{2A34}\u{2A35}\u{2A36}\u{2A37}\u{2A38}\u{2A3B}\u{2A3C}\u{2A3D}\u{2A40}\u{2A43}\u{2A44}\u{2A4B}\u{2A4D}\u{2A4E}\u{2A51}\u{2A53}\u{2A55}\u{2A58}\u{2A5A}\u{2A5C}\u{2A5E}\u{2A5F}\u{2A60}\u{2ADB}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 12)]
    UnicodeOpTimes,

    #[regex(r"([\^\u{2191}\u{2193}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]+|([\u{21F5}\u{27F0}\u{27F1}\u{2908}\u{2909}\u{290A}\u{290B}\u{2912}\u{2913}\u{2949}\u{294C}\u{294D}\u{294F}\u{2951}\u{2954}\u{2955}\u{2958}\u{2959}\u{295C}\u{295D}\u{2960}\u{2961}\u{2963}\u{2965}\u{296E}\u{296F}\u{FFEA}\u{FFEC}])[\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 12)]
    UnicodeOpPower,

    #[regex(r"[\u{205D}\u{22EE}\u{22EF}\u{22F0}\u{22F1}]", priority = 12)]
    UnicodeOpColon,

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
    #[token(".<:")]
    DotSubtype,
    #[token(".>:")]
    DotSupertype,
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
    #[token(".===")]
    DotEqEqEq,
    #[token(".!=")]
    DotNotEq,
    #[token(".!==")]
    DotNotEqEq,
    #[token(".!")]
    DotNot,
    #[token(".~")]
    DotTilde,
    #[token(".<<")]
    DotLtLt,
    #[token(".>>")]
    DotGtGt,
    #[token(".>>>")]
    DotGtGtGt,
    #[token(".&")]
    DotAmp,
    #[token(".|")]
    DotPipe,
    #[token(".&&")]
    DotAndAnd,
    #[token(".||")]
    DotOrOr,
    // Dotted (broadcast) Unicode operators, per upstream precedence class
    // (Issue #11110): upstream's `add-dots` gives a dotted operator the SAME
    // precedence as its base operator, so `.⊗` is times-precedence, not the
    // catch-all comparison precedence `DotOtherUnicodeOperator` gives. Classes
    // derived from the same `julia-parser.scm` tables as the non-dotted
    // variants above; `priority = 14` outranks the catch-all below, which stays
    // as the fallback for dotted characters outside upstream's tables.
    #[regex(r"\.[\u{2190}\u{2192}\u{2194}\u{219A}\u{219B}\u{219C}\u{219D}\u{219E}\u{21A0}\u{21A2}\u{21A3}\u{21A4}\u{21A6}\u{21A9}\u{21AA}\u{21AB}\u{21AC}\u{21AE}\u{21B6}\u{21B7}\u{21BA}\u{21BB}\u{21BC}\u{21BD}\u{21C0}\u{21C1}\u{21C4}\u{21C6}\u{21C7}\u{21C9}\u{21CB}\u{21CC}\u{21CD}\u{21CE}\u{21CF}\u{21D0}\u{21D2}\u{21D4}\u{21DA}\u{21DB}\u{21DC}\u{21DD}\u{21E0}\u{21E2}\u{21F4}\u{21F6}\u{21F7}\u{21F8}\u{21F9}\u{21FA}\u{21FB}\u{21FC}\u{21FD}\u{21FE}\u{21FF}\u{27F5}\u{27F6}\u{27F7}\u{27F9}\u{27FA}\u{27FB}\u{27FC}\u{27FD}\u{27FE}\u{27FF}\u{2900}\u{2901}\u{2902}\u{2903}\u{2904}\u{2905}\u{2906}\u{2907}\u{290C}\u{290D}\u{290E}\u{290F}\u{2910}\u{2911}\u{2914}\u{2915}\u{2916}\u{2917}\u{2918}\u{291D}\u{291E}\u{291F}\u{2920}\u{2944}\u{2945}\u{2946}\u{2947}\u{2948}\u{294A}\u{294B}\u{294E}\u{2950}\u{2952}\u{2953}\u{2956}\u{2957}\u{295A}\u{295B}\u{295E}\u{295F}\u{2962}\u{2964}\u{2966}\u{2967}\u{2968}\u{2969}\u{296A}\u{296B}\u{296C}\u{296D}\u{2970}\u{2977}\u{297A}\u{29F4}\u{2B30}\u{2B31}\u{2B32}\u{2B33}\u{2B34}\u{2B35}\u{2B36}\u{2B37}\u{2B38}\u{2B39}\u{2B3A}\u{2B3B}\u{2B3C}\u{2B3D}\u{2B3E}\u{2B3F}\u{2B40}\u{2B41}\u{2B42}\u{2B43}\u{2B44}\u{2B47}\u{2B48}\u{2B49}\u{2B4A}\u{2B4B}\u{2B4C}\u{FFE9}\u{FFEB}\u{1F8B2}][\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 14)]
    DotUnicodeOpArrow,

    #[regex(r"\.[\u{2208}\u{2209}\u{220A}\u{220B}\u{220C}\u{220D}\u{221D}\u{2225}\u{2226}\u{2237}\u{223A}\u{223B}\u{223D}\u{223E}\u{2241}\u{2242}\u{2243}\u{2244}\u{2245}\u{2246}\u{2247}\u{2248}\u{2249}\u{224A}\u{224B}\u{224C}\u{224D}\u{224E}\u{2250}\u{2251}\u{2252}\u{2253}\u{2256}\u{2257}\u{2258}\u{2259}\u{225A}\u{225B}\u{225C}\u{225D}\u{225E}\u{225F}\u{2260}\u{2261}\u{2262}\u{2263}\u{2264}\u{2265}\u{2266}\u{2267}\u{2268}\u{2269}\u{226A}\u{226B}\u{226C}\u{226D}\u{226E}\u{226F}\u{2270}\u{2271}\u{2272}\u{2273}\u{2274}\u{2275}\u{2276}\u{2277}\u{2278}\u{2279}\u{227A}\u{227B}\u{227C}\u{227D}\u{227E}\u{227F}\u{2280}\u{2281}\u{2282}\u{2283}\u{2284}\u{2285}\u{2286}\u{2287}\u{2288}\u{2289}\u{228A}\u{228B}\u{228F}\u{2290}\u{2291}\u{2292}\u{229C}\u{22A2}\u{22A3}\u{22A9}\u{22AC}\u{22AE}\u{22B0}\u{22B1}\u{22B2}\u{22B3}\u{22B4}\u{22B5}\u{22B6}\u{22B7}\u{22CD}\u{22D0}\u{22D1}\u{22D5}\u{22D6}\u{22D7}\u{22D8}\u{22D9}\u{22DA}\u{22DB}\u{22DC}\u{22DD}\u{22DE}\u{22DF}\u{22E0}\u{22E1}\u{22E2}\u{22E3}\u{22E4}\u{22E5}\u{22E6}\u{22E7}\u{22E8}\u{22E9}\u{22EA}\u{22EB}\u{22EC}\u{22ED}\u{22F2}\u{22F3}\u{22F4}\u{22F5}\u{22F6}\u{22F7}\u{22F8}\u{22F9}\u{22FA}\u{22FB}\u{22FC}\u{22FD}\u{22FE}\u{22FF}\u{27C2}\u{27C8}\u{27C9}\u{27D2}\u{29B7}\u{29C0}\u{29C1}\u{29E1}\u{29E3}\u{29E4}\u{29E5}\u{2A66}\u{2A67}\u{2A6A}\u{2A6B}\u{2A6C}\u{2A6D}\u{2A6E}\u{2A6F}\u{2A70}\u{2A71}\u{2A72}\u{2A73}\u{2A75}\u{2A76}\u{2A77}\u{2A78}\u{2A79}\u{2A7A}\u{2A7B}\u{2A7C}\u{2A7D}\u{2A7E}\u{2A7F}\u{2A80}\u{2A81}\u{2A82}\u{2A83}\u{2A84}\u{2A85}\u{2A86}\u{2A87}\u{2A88}\u{2A89}\u{2A8A}\u{2A8B}\u{2A8C}\u{2A8D}\u{2A8E}\u{2A8F}\u{2A90}\u{2A91}\u{2A92}\u{2A93}\u{2A94}\u{2A95}\u{2A96}\u{2A97}\u{2A98}\u{2A99}\u{2A9A}\u{2A9B}\u{2A9C}\u{2A9D}\u{2A9E}\u{2A9F}\u{2AA0}\u{2AA1}\u{2AA2}\u{2AA3}\u{2AA4}\u{2AA5}\u{2AA6}\u{2AA7}\u{2AA8}\u{2AA9}\u{2AAA}\u{2AAB}\u{2AAC}\u{2AAD}\u{2AAE}\u{2AAF}\u{2AB0}\u{2AB1}\u{2AB2}\u{2AB3}\u{2AB4}\u{2AB5}\u{2AB6}\u{2AB7}\u{2AB8}\u{2AB9}\u{2ABA}\u{2ABB}\u{2ABC}\u{2ABD}\u{2ABE}\u{2ABF}\u{2AC0}\u{2AC1}\u{2AC2}\u{2AC3}\u{2AC4}\u{2AC5}\u{2AC6}\u{2AC7}\u{2AC8}\u{2AC9}\u{2ACA}\u{2ACB}\u{2ACC}\u{2ACD}\u{2ACE}\u{2ACF}\u{2AD0}\u{2AD1}\u{2AD2}\u{2AD3}\u{2AD4}\u{2AD5}\u{2AD6}\u{2AD7}\u{2AD8}\u{2AD9}\u{2AEA}\u{2AEB}\u{2AF7}\u{2AF8}\u{2AF9}\u{2AFA}][\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 14)]
    DotUnicodeOpComparison,

    #[regex(r"\.[\u{00A6}\u{00B1}\u{2212}\u{2213}\u{2214}\u{2228}\u{222A}\u{2238}\u{224F}\u{228E}\u{2294}\u{2295}\u{2296}\u{229E}\u{229F}\u{22BB}\u{22BD}\u{22CE}\u{22D3}\u{27C7}\u{29FA}\u{29FB}\u{2A08}\u{2A22}\u{2A23}\u{2A24}\u{2A25}\u{2A26}\u{2A27}\u{2A28}\u{2A29}\u{2A2A}\u{2A2B}\u{2A2C}\u{2A2D}\u{2A2E}\u{2A39}\u{2A3A}\u{2A41}\u{2A42}\u{2A45}\u{2A4A}\u{2A4C}\u{2A4F}\u{2A50}\u{2A52}\u{2A54}\u{2A56}\u{2A57}\u{2A5B}\u{2A5D}\u{2A61}\u{2A62}\u{2A63}][\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 14)]
    DotUnicodeOpPlus,

    #[regex(r"\.[\u{00B7}\u{00D7}\u{00F7}\u{0387}\u{214B}\u{2217}\u{2218}\u{2219}\u{2224}\u{2227}\u{2229}\u{2240}\u{228D}\u{2293}\u{2297}\u{2298}\u{2299}\u{229A}\u{229B}\u{22A0}\u{22A1}\u{22BC}\u{22C4}\u{22C5}\u{22C6}\u{22C7}\u{22C9}\u{22CA}\u{22CB}\u{22CC}\u{22CF}\u{22D2}\u{233F}\u{25B7}\u{27D1}\u{27D5}\u{27D6}\u{27D7}\u{29B8}\u{29BC}\u{29BE}\u{29BF}\u{29F6}\u{29F7}\u{2A07}\u{2A1D}\u{2A1F}\u{2A30}\u{2A31}\u{2A32}\u{2A33}\u{2A34}\u{2A35}\u{2A36}\u{2A37}\u{2A38}\u{2A3B}\u{2A3C}\u{2A3D}\u{2A40}\u{2A43}\u{2A44}\u{2A4B}\u{2A4D}\u{2A4E}\u{2A51}\u{2A53}\u{2A55}\u{2A58}\u{2A5A}\u{2A5C}\u{2A5E}\u{2A5F}\u{2A60}\u{2ADB}][\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 14)]
    DotUnicodeOpTimes,

    #[regex(r"\.[\u{2191}\u{2193}\u{21F5}\u{27F0}\u{27F1}\u{2908}\u{2909}\u{290A}\u{290B}\u{2912}\u{2913}\u{2949}\u{294C}\u{294D}\u{294F}\u{2951}\u{2954}\u{2955}\u{2958}\u{2959}\u{295C}\u{295D}\u{2960}\u{2961}\u{2963}\u{2965}\u{296E}\u{296F}\u{FFEA}\u{FFEC}][\u{0300}-\u{036F}\u{0483}-\u{0489}\u{1AB0}-\u{1AFF}\u{1DC0}-\u{1DFF}\u{20D0}-\u{20F0}\u{FE00}-\u{FE0F}\u{FE20}-\u{FE2F}\u{00B4}\u{02B9}-\u{02BF}\u{A700}-\u{A71F}\u{00B2}-\u{00B3}\u{00B9}\u{02B0}\u{02B2}-\u{02B3}\u{02B7}-\u{02B8}\u{02E1}-\u{02E3}\u{1D2C}\u{1D2E}\u{1D30}-\u{1D31}\u{1D33}-\u{1D3A}\u{1D3C}\u{1D3E}-\u{1D43}\u{1D47}-\u{1D49}\u{1D4D}\u{1D4F}-\u{1D50}\u{1D52}\u{1D56}-\u{1D58}\u{1D5B}\u{1D5D}-\u{1D6A}\u{1D9C}\u{1DA0}\u{1DA5}-\u{1DA6}\u{1DAB}\u{1DB0}\u{1DB8}\u{1DBB}\u{1DBF}\u{2032}-\u{2037}\u{2057}\u{2070}-\u{2071}\u{2074}-\u{208E}\u{2090}-\u{2093}\u{2095}-\u{209C}\u{2C7C}-\u{2C7D}\u{A71B}-\u{A71C}]*", priority = 14)]
    DotUnicodeOpPower,

    #[regex(r"\.[\u{00B1}\u{00D7}\u{00F7}\u{2200}-\u{22FF}\u{27C0}-\u{27FF}\u{2900}-\u{297F}\u{2A00}-\u{2AFF}\u{2B00}-\u{2BFF}\u{2300}-\u{23FF}]")]
    DotOtherUnicodeOperator,

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
    // - Quote literal: ''' (Julia's accepted spelling for the single quote char)
    // - Octal escapes: '\033' (1-3 octal digits)
    // - Hex escapes: '\x41' (1-2 hex digits), including escaped UTF-8 bytes
    // - Unicode escapes: '\u0041' (1-4 hex digits)
    // - Unicode escapes (long): '\U00000041' (1-8 hex digits)
    // - Named escapes: '\N{GREEK SMALL LETTER ALPHA}'
    #[regex(
        r"'''|'([^'\\]|\\x[c-fC-F][0-9a-fA-F](\\x[0-9a-fA-F]{2})+|\\x[0-9a-fA-F]{1,2}|\\u[0-9a-fA-F]{1,4}|\\U[0-9a-fA-F]{1,8}|\\N\{[^}]+\}|\\[0-7]{1,3}|\\[^\n])'"
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
    // - Prime suffix marks (U+2032-U+2037)
    // - Emoji / symbol identifiers used by upstream tests
    // - Subscript digits (U+2080-U+2089) and letters (U+2090-U+209C)
    // - Superscript digits (U+2070-U+2079 plus legacy U+00B2/U+00B3/U+00B9)
    //   and letters
    // - ! as an identifier continuation character (sort!, foo!bar, etc.)
    // Note: Excludes √∛∜ (U+221A-U+221C) which are unary operators
    // The greedy `!` continuation is required so a `keyword!` name (e.g. `in!`)
    // and names like `push!`/`foo!bar` out-match the bare keyword / base
    // identifier. The `a!=b` ambiguity (don't fold the `!` of `!=` into the
    // name) is resolved in the lexer wrapper, which can rewind via
    // `restart_from` (Issues #8194 / #10713).
    // Issue #11083: the math-symbol blocks below are NOT wholesale identifier
    // start characters — every codepoint upstream Julia lists in an operator
    // precedence table (`julia/src/julia-parser.scm`) is punched out, so `⊛`,
    // `⊞`, `⊗`, … lex as operators even with no surrounding whitespace
    // (`a⊛b`), while non-operator math symbols (`∞`, `∇`, `∅`, `⊝`) stay
    // identifier characters exactly as before. The punched-out ranges are
    // derived mechanically from the same upstream tables as the operator
    // tokens above.
    #[regex(r"[_\p{XID_Start}\u{1F000}-\u{1F8B1}\u{1F8B3}-\u{1FAFF}\u{2600}-\u{26FF}\u{2200}-\u{2207}\u{220E}-\u{2211}\u{2215}-\u{2216}\u{221E}-\u{2223}\u{222B}-\u{2236}\u{2239}\u{223C}\u{223F}\u{228C}\u{229D}\u{22A4}-\u{22A8}\u{22AA}-\u{22AB}\u{22AD}\u{22AF}\u{22B8}-\u{22BA}\u{22BE}-\u{22C3}\u{22C8}\u{22D4}\u{2A00}-\u{2A06}\u{2A09}-\u{2A1C}\u{2A1E}\u{2A20}-\u{2A21}\u{2A2F}\u{2A3E}-\u{2A3F}\u{2A46}-\u{2A49}\u{2A59}\u{2A64}-\u{2A65}\u{2A68}-\u{2A69}\u{2ADA}\u{2ADC}-\u{2AE9}\u{2AEC}-\u{2AF6}\u{2AFB}-\u{2AFF}][_\p{XID_Continue}!\u{00B4}\u{02B9}-\u{02BF}\u{2032}-\u{2037}\u{2080}-\u{209C}\u{2070}-\u{207F}\u{00B2}\u{00B3}\u{00B9}]*")]
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
        )
    }

    /// Check if this token is an operator
    pub fn is_operator(&self) -> bool {
        matches!(
            self,
            Token::Plus
                | Token::PlusPlus
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
                | Token::GreaterEqual
                | Token::LessEqual
                | Token::EqEq
                | Token::EqEqEq
                | Token::NotEq
                | Token::NotEqEq
                | Token::Identical
                | Token::NotEqual
                | Token::Approx
                | Token::NotApprox
                | Token::NotIdentical
                | Token::ElementOf
                | Token::NotElementOf
                | Token::Contains
                | Token::NotContains
                | Token::SubsetEq
                | Token::NotSubsetEq
                | Token::Subset
                | Token::NotSubset
                | Token::StrictSubset
                | Token::SupersetEq
                | Token::NotSupersetEq
                | Token::Superset
                | Token::NotSuperset
                | Token::StrictSuperset
                | Token::LessSimilar
                | Token::Subtype
                | Token::Supertype
                | Token::AndAnd
                | Token::OrOr
                | Token::Not
                | Token::LogicalNot
                | Token::Tilde
                | Token::LtLt
                | Token::GtGt
                | Token::GtGtGt
                | Token::SlashSlash
                | Token::MinusSign
                | Token::CirclePlus
                | Token::CircleMinus
                | Token::Union
                | Token::LogicalOr
                | Token::SquareUnion
                | Token::Times
                | Token::Divide
                | Token::DotOperator
                | Token::RingOperator
                | Token::Intersection
                | Token::LogicalAnd
                | Token::CircleTimes
                | Token::CircleDivide
                | Token::CircleDot
                | Token::SquareIntersection
                | Token::UpArrow
                | Token::DownArrow
                | Token::SquareRoot
                | Token::CubeRoot
                | Token::FourthRoot
                | Token::PipeRight
                | Token::UnicodeOpArrow
                | Token::UnicodeOpComparison
                | Token::UnicodeOpPlus
                | Token::UnicodeOpTimes
                | Token::UnicodeOpPower
                | Token::UnicodeOpColon
                | Token::PipeLeft
                | Token::LeftArrow2
                | Token::RightArrow2
                | Token::LeftRightArrow2
                | Token::LeftArrow
                | Token::RightArrow
                | Token::LeftRightArrow
                | Token::Arrow
                | Token::FatArrow
                | Token::DotPlus
                | Token::DotMinus
                | Token::DotStar
                | Token::DotSlash
                | Token::DotBackslash
                | Token::DotCaret
                | Token::DotPercent
                | Token::DotSubtype
                | Token::DotSupertype
                | Token::DotLt
                | Token::DotGt
                | Token::DotLtEq
                | Token::DotGtEq
                | Token::DotEqEq
                | Token::DotEqEqEq
                | Token::DotNotEq
                | Token::DotNotEqEq
                | Token::DotNot
                | Token::DotTilde
                | Token::DotRightArrow2
                | Token::DotLeftRightArrow2
                | Token::DotLtLt
                | Token::DotGtGt
                | Token::DotGtGtGt
                | Token::DotAmp
                | Token::DotPipe
                | Token::DotAndAnd
                | Token::DotOrOr
                | Token::DotUnicodeOpArrow
                | Token::DotUnicodeOpComparison
                | Token::DotUnicodeOpPlus
                | Token::DotUnicodeOpTimes
                | Token::DotUnicodeOpPower
                | Token::DotOtherUnicodeOperator
                | Token::Xor
                | Token::DoubleColon
                // Issue #8759: `..` (DotDot) is the range extension operator; allow it
                // in operator contexts so `:(..)` and `x .. y` parse correctly.
                // Note: `++` (PlusPlus) is already listed above.
                | Token::DotDot
        )
    }

    /// Operator tokens that are special grammar forms, not function names —
    /// upstream `julia/src/julia-parser.scm`'s `syntactic-operators`.
    ///
    /// Upstream's full list is `&& || = += -= *= /= //= \= ^= ÷= %= <<= >>=
    /// >>>= |= &= ⊻=` (plus dotted variants) and `:= $= . ... ->`. Most of
    /// those are lexed here as assignment tokens (`is_assignment`) or as the
    /// structural `Dot`/`Ellipsis` tokens, none of which are classified
    /// `is_operator()` to begin with. This predicate covers exactly the
    /// members that ARE operator tokens in this lexer: `->`, `&&`, `||`,
    /// `.&&`, and `.||` (Issues #10917, #10932).
    pub fn is_syntactic_operator(&self) -> bool {
        matches!(
            self,
            Token::Arrow | Token::AndAnd | Token::OrOr | Token::DotAndAnd | Token::DotOrOr
        )
    }

    /// Whether this token can appear as an unquoted operator identifier.
    ///
    /// Julia treats the `syntactic-operators` (`->`, `&&`, `||`, `.&&`,
    /// `.||`) as grammar markers, not operator identifiers. They remain
    /// operator tokens for infix precedence and quoted symbol forms
    /// (`:->`, `:(&&)`), but a bare one cannot name or denote a function
    /// (Issues #10917, #10932).
    ///
    /// `::` is upstream's syntactic-*unary* operator: it is not an
    /// identifier/value either, but unlike the syntactic operators it is not
    /// rejected as an invalid identifier — its unary grammar form
    /// (`::T`, and recursively `::::T`) consumes it instead (Issue #10915).
    pub fn is_operator_identifier(&self) -> bool {
        self.is_operator() && !self.is_syntactic_operator() && !matches!(self, Token::DoubleColon)
    }

    /// Check if this token is a keyword that also denotes a first-class
    /// function/operator value in upstream Julia (`isa`, `in`). These are
    /// lexed as keyword tokens but can appear as quoted operator names
    /// (`Base.:(isa)`, `Base.:isa`, `:(in)`) (Issue #5115).
    pub fn is_operator_keyword(&self) -> bool {
        matches!(self, Token::KwIsa | Token::KwIn)
    }

    /// Check if this token can be quoted as an operator-like symbol inside
    /// `:(...)` or a qualified field form such as `Base.:(:)`.
    pub fn is_quoted_operator_symbol(&self) -> bool {
        self.is_operator()
            || self.is_operator_keyword()
            || matches!(
                self,
                Token::Colon
                    | Token::Dot
                    | Token::Ellipsis
                    | Token::DotDot
                    | Token::HorizontalEllipsis
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
                | Token::DotLtLtEq
                | Token::DotGtGtEq
                | Token::DotGtGtGtEq
                | Token::DotAmpEq
                | Token::DotPipeEq
                | Token::MinusSignEq
                | Token::DivisionSignEq
                | Token::XorEq
                | Token::DotDivisionSignEq
                | Token::DotXorEq
                | Token::ColonEquals
                | Token::DoubleColonEquals
                | Token::EqualsColon
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
                | Token::DotLtLtEq
                | Token::DotGtGtEq
                | Token::DotGtGtGtEq
                | Token::DotAmpEq
                | Token::DotPipeEq
                | Token::MinusSignEq
                | Token::DivisionSignEq
                | Token::XorEq
                | Token::DotDivisionSignEq
                | Token::DotXorEq
                | Token::ColonEquals
                | Token::DoubleColonEquals
                | Token::EqualsColon
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
                | Token::DotSubtype
                | Token::DotSupertype
                | Token::DotLt
                | Token::DotGt
                | Token::DotLtEq
                | Token::DotGtEq
                | Token::DotEqEq
                | Token::DotEqEqEq
                | Token::DotNotEq
                | Token::DotNotEqEq
                | Token::DotNot
                | Token::DotTilde
                | Token::DotRightArrow2
                | Token::DotLeftRightArrow2
                | Token::DotLtLt
                | Token::DotGtGt
                | Token::DotGtGtGt
                | Token::DotAmp
                | Token::DotPipe
                | Token::DotAndAnd
                | Token::DotOrOr
                | Token::DotUnicodeOpArrow
                | Token::DotUnicodeOpComparison
                | Token::DotUnicodeOpPlus
                | Token::DotUnicodeOpTimes
                | Token::DotUnicodeOpPower
                | Token::DotOtherUnicodeOperator
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
            Token::DotSubtype => Some("<:"),
            Token::DotSupertype => Some(">:"),
            Token::DotLt => Some("<"),
            Token::DotGt => Some(">"),
            Token::DotLtEq => Some("<="),
            Token::DotGtEq => Some(">="),
            Token::DotEqEq => Some("=="),
            Token::DotEqEqEq => Some("==="),
            Token::DotNotEq => Some("!="),
            Token::DotNotEqEq => Some("!=="),
            Token::DotNot => Some("!"),
            Token::DotTilde => Some("~"),
            Token::DotRightArrow2 => Some("-->"),
            Token::DotLeftRightArrow2 => Some("<-->"),
            Token::DotLtLt => Some("<<"),
            Token::DotGtGt => Some(">>"),
            Token::DotGtGtGt => Some(">>>"),
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
            Token::True => Some("true"),
            Token::False => Some("false"),
            _ => None,
        }
    }
}

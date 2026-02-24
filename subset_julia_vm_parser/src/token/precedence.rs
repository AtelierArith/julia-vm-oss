//! Operator precedence and associativity definitions

use super::Token;

/// Operator precedence levels
///
/// Based on tree-sitter-julia grammar.js:11-40
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i8)]
pub enum Precedence {
    /// Lowest precedence (macro arguments)
    MacroArg = -4,
    /// Statement level
    Stmt = -3,
    /// Assignment
    Assign = -2,
    /// Array/Tuple
    ArrayTuple = -1,
    /// Arrow function: x -> expr
    Afunc = 10,
    /// Pair: =>
    Pair = 11,
    /// Conditional: ? :
    Conditional = 12,
    /// Arrow operators: <--, -->
    Arrow = 13,
    /// Lazy or: ||
    LazyOr = 14,
    /// Lazy and: &&
    LazyAnd = 15,
    /// Where clause
    Where = 16,
    /// Comparison: <, >, ==, etc.
    Comparison = 17,
    /// Pipe left: <|
    PipeLeft = 18,
    /// Pipe right: |>
    PipeRight = 19,
    /// Colon (range)
    Colon = 20,
    /// Plus: +, -
    Plus = 21,
    /// Times: *, /
    Times = 22,
    /// Rational: //
    Rational = 23,
    /// Bitshift: <<, >>
    Bitshift = 24,
    /// Prefix (unary)
    Prefix = 25,
    /// Power: ^
    Power = 26,
    /// Declaration: ::
    Decl = 27,
    /// Dot (field access)
    Dot = 28,
}

impl TryFrom<i8> for Precedence {
    type Error = ();

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            -4 => Ok(Precedence::MacroArg),
            -3 => Ok(Precedence::Stmt),
            -2 => Ok(Precedence::Assign),
            -1 => Ok(Precedence::ArrayTuple),
            10 => Ok(Precedence::Afunc),
            11 => Ok(Precedence::Pair),
            12 => Ok(Precedence::Conditional),
            13 => Ok(Precedence::Arrow),
            14 => Ok(Precedence::LazyOr),
            15 => Ok(Precedence::LazyAnd),
            16 => Ok(Precedence::Where),
            17 => Ok(Precedence::Comparison),
            18 => Ok(Precedence::PipeLeft),
            19 => Ok(Precedence::PipeRight),
            20 => Ok(Precedence::Colon),
            21 => Ok(Precedence::Plus),
            22 => Ok(Precedence::Times),
            23 => Ok(Precedence::Rational),
            24 => Ok(Precedence::Bitshift),
            25 => Ok(Precedence::Prefix),
            26 => Ok(Precedence::Power),
            27 => Ok(Precedence::Decl),
            28 => Ok(Precedence::Dot),
            _ => Err(()),
        }
    }
}

/// Operator associativity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Associativity {
    Left,
    Right,
    None,
}

impl Token {
    /// Get the precedence and associativity of a binary operator
    pub fn binary_precedence(&self) -> Option<(Precedence, Associativity)> {
        use Associativity::{Left, Right};
        use Precedence::*;

        Some(match self {
            // Assignment operators (lowest precedence, right associative)
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
            | Token::MinusSignEq
            | Token::DivisionSignEq
            | Token::XorEq => (Assign, Right),

            // Pair
            Token::FatArrow => (Pair, Right),

            // Arrow operators
            Token::Arrow
            | Token::LeftArrow
            | Token::RightArrow
            | Token::LeftRightArrow
            | Token::LeftArrow2
            | Token::RightArrow2
            | Token::LeftRightArrow2 => (Arrow, Right),

            // Lazy boolean (and broadcast variants, Issue #2545)
            Token::OrOr | Token::DotOrOr => (LazyOr, Left),
            Token::AndAnd | Token::DotAndAnd => (LazyAnd, Left),

            // Where clause (type constraints)
            Token::KwWhere => (Where, Left),

            // Comparison
            Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
            | Token::EqEq
            | Token::EqEqEq
            | Token::NotEq
            | Token::NotEqEq
            | Token::Subtype
            | Token::Supertype
            | Token::GreaterEqual
            | Token::LessEqual
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
            | Token::KwIn
            | Token::KwIsa
            // Broadcast comparison
            | Token::DotLt
            | Token::DotGt
            | Token::DotLtEq
            | Token::DotGtEq
            | Token::DotEqEq
            | Token::DotNotEq => (Comparison, Left),

            // Pipe
            Token::PipeLeft => (PipeLeft, Right),
            Token::PipeRight => (PipeRight, Left),

            // Colon/Range
            Token::Colon | Token::DotDot | Token::HorizontalEllipsis => (Colon, Left),

            // Plus
            Token::Plus
            | Token::Minus
            | Token::PlusPlus
            | Token::Pipe
            | Token::DotPlus
            | Token::DotMinus
            | Token::DotPipe
            | Token::MinusSign
            | Token::CirclePlus
            | Token::CircleMinus
            | Token::Union
            | Token::LogicalOr
            | Token::SquareUnion => (Plus, Left),

            // Times
            Token::Star
            | Token::Slash
            | Token::Percent
            | Token::Amp
            | Token::Backslash
            | Token::DotStar
            | Token::DotSlash
            | Token::DotBackslash
            | Token::DotPercent
            | Token::DotAmp
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
            | Token::Xor => (Times, Left),

            // Rational
            Token::SlashSlash => (Rational, Left),

            // Bitshift
            Token::LtLt | Token::GtGt | Token::GtGtGt => (Bitshift, Left),

            // Power
            Token::Caret | Token::DotCaret | Token::UpArrow | Token::DownArrow => (Power, Right),

            _ => return None,
        })
    }

    /// Get the precedence of a unary operator
    pub fn unary_precedence(&self) -> Option<Precedence> {
        match self {
            Token::Plus
            | Token::Minus
            | Token::Not
            | Token::LogicalNot
            | Token::Tilde
            | Token::SquareRoot
            | Token::CubeRoot
            | Token::FourthRoot
            | Token::Subtype
            | Token::Supertype
            | Token::Dollar => Some(Precedence::Prefix), // $ for interpolation/unquote
            _ => None,
        }
    }
}

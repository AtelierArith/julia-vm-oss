//! Backend-neutral broadcast element IR.
//!
//! Julia lowers `x .+ y` to `materialize(Broadcasted(op, (x, y)))` and fuses
//! nested broadcasts into a single tree. Resolving the callee and the element
//! result type here keeps backends free of callee-name matching and of Julia's
//! promotion rules.

use super::{AotBuiltinOp, AotExpr, BinOpKind};
use crate::aot::types::StaticType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastOp {
    Binary(BinOpKind),
    Builtin(AotBuiltinOp),
}

#[derive(Debug, Clone)]
pub enum BroadcastNode {
    Element,
    Scalar(AotExpr),
    Apply {
        op: BroadcastOp,
        args: Vec<BroadcastNode>,
    },
}

#[derive(Debug, Clone)]
pub struct BroadcastPlan {
    pub source: AotExpr,
    pub node: BroadcastNode,
    pub elem_ty: StaticType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BroadcastReject {
    MultipleArrayOperands(usize),
    NoArrayOperand,
    UnknownCallee,
    UnsupportedElementTypes,
    UnrankedArray,
}

impl BroadcastReject {
    pub fn message(&self) -> &'static str {
        match self {
            Self::MultipleArrayOperands(_) => {
                "Wasm AoT broadcast supports exactly one array operand; multi-array broadcast is rewritten by shared AoT specialization before reaching this backend"
            }
            Self::NoArrayOperand => "Wasm AoT broadcast requires one array operand",
            Self::UnknownCallee => "Wasm AoT broadcast callee is not a supported element operation",
            Self::UnsupportedElementTypes => {
                "Wasm AoT broadcast operand types have no statically known Julia result type"
            }
            Self::UnrankedArray => "Wasm AoT broadcast requires a statically ranked array operand",
        }
    }
}

impl BroadcastOp {
    pub fn resolve(callee: &str) -> Option<Self> {
        Some(match callee {
            "op_add" => Self::Binary(BinOpKind::Add),
            "op_sub" => Self::Binary(BinOpKind::Sub),
            "op_mul" => Self::Binary(BinOpKind::Mul),
            "op_div" => Self::Binary(BinOpKind::Div),
            "op_pow" => Self::Binary(BinOpKind::Pow),
            "op_mod" => Self::Binary(BinOpKind::Rem),
            "op_eq" => Self::Binary(BinOpKind::Eq),
            "op_ne" => Self::Binary(BinOpKind::Ne),
            "op_lt" => Self::Binary(BinOpKind::Lt),
            "op_le" => Self::Binary(BinOpKind::Le),
            "op_gt" => Self::Binary(BinOpKind::Gt),
            "op_ge" => Self::Binary(BinOpKind::Ge),
            "abs" => Self::Builtin(AotBuiltinOp::Abs),
            "sqrt" => Self::Builtin(AotBuiltinOp::Sqrt),
            "exp" => Self::Builtin(AotBuiltinOp::Exp),
            "log" => Self::Builtin(AotBuiltinOp::Log),
            "floor" => Self::Builtin(AotBuiltinOp::Floor),
            "ceil" => Self::Builtin(AotBuiltinOp::Ceil),
            "trunc" => Self::Builtin(AotBuiltinOp::Trunc),
            "round" => Self::Builtin(AotBuiltinOp::Round),
            "min" => Self::Builtin(AotBuiltinOp::Min),
            "max" => Self::Builtin(AotBuiltinOp::Max),
            "clamp" => Self::Builtin(AotBuiltinOp::Clamp),
            _ => return None,
        })
    }

    /// Julia's element result type for this operation over `args`.
    pub fn result_ty(self, args: &[StaticType]) -> Option<StaticType> {
        if args.is_empty() || !args.iter().all(is_supported_element) {
            return None;
        }
        match self {
            Self::Binary(op) if is_comparison(op) => Some(StaticType::Bool),
            Self::Binary(BinOpKind::Div) => float_result(&unify(args)?),
            Self::Binary(_) => unify(args),
            Self::Builtin(AotBuiltinOp::Sqrt | AotBuiltinOp::Exp | AotBuiltinOp::Log) => {
                float_result(&unify(args)?)
            }
            Self::Builtin(
                AotBuiltinOp::Abs
                | AotBuiltinOp::Floor
                | AotBuiltinOp::Ceil
                | AotBuiltinOp::Trunc
                | AotBuiltinOp::Round
                | AotBuiltinOp::Min
                | AotBuiltinOp::Max
                | AotBuiltinOp::Clamp,
            ) => unify(args),
            Self::Builtin(_) => None,
        }
    }

    /// Type every operand is converted to before the operation is applied.
    pub fn operand_ty(self, args: &[StaticType]) -> Option<StaticType> {
        match self {
            Self::Binary(op) if is_comparison(op) => unify(args),
            _ => self.result_ty(args),
        }
    }
}

fn is_comparison(op: BinOpKind) -> bool {
    matches!(
        op,
        BinOpKind::Eq
            | BinOpKind::Ne
            | BinOpKind::Lt
            | BinOpKind::Le
            | BinOpKind::Gt
            | BinOpKind::Ge
    )
}

fn is_supported_element(ty: &StaticType) -> bool {
    matches!(
        ty,
        StaticType::U8
            | StaticType::I32
            | StaticType::I64
            | StaticType::F32
            | StaticType::F64
            | StaticType::Bool
    )
}

/// Julia's promotion over the operand types, or `None` when it is not static.
fn unify(args: &[StaticType]) -> Option<StaticType> {
    let first = args.first()?;
    if args.iter().all(|ty| ty == first) {
        return Some(first.clone());
    }
    StaticType::promote_numeric_args(args)
}

/// `/`, `sqrt`, `exp` and `log` stay in Float32 only when every operand already is.
fn float_result(ty: &StaticType) -> Option<StaticType> {
    match ty {
        StaticType::F32 => Some(StaticType::F32),
        StaticType::U8 | StaticType::I32 | StaticType::I64 | StaticType::F64 => {
            Some(StaticType::F64)
        }
        _ => None,
    }
}

/// Build a shape plan from an intact `materialize(Broadcasted(..))` tree.
pub fn plan(expr: &AotExpr) -> Result<Option<BroadcastPlan>, BroadcastReject> {
    let Some(call) = broadcasted_args(expr) else {
        return Ok(None);
    };
    let mut arrays = Vec::new();
    let node = build(call, &mut arrays)?;
    let source = match arrays.len() {
        0 => return Err(BroadcastReject::NoArrayOperand),
        1 => arrays[0].clone(),
        count => return Err(BroadcastReject::MultipleArrayOperands(count)),
    };
    let element = match &source.get_type() {
        StaticType::Array {
            element,
            ndims: Some(_),
        } => (**element).clone(),
        _ => return Err(BroadcastReject::UnrankedArray),
    };
    let elem_ty = node_ty(&node, &element).ok_or(BroadcastReject::UnsupportedElementTypes)?;
    Ok(Some(BroadcastPlan {
        source,
        node,
        elem_ty,
    }))
}

fn broadcasted_args(expr: &AotExpr) -> Option<&[AotExpr]> {
    let (function, args) = match expr {
        AotExpr::CallDynamic { function, args } => (function, args),
        AotExpr::CallStatic { function, args, .. } => (function, args),
        AotExpr::Convert { value, .. } => return broadcasted_args(value),
        _ => return None,
    };
    match function.as_str() {
        "materialize" if args.len() == 1 => broadcasted_args(&args[0]),
        "Broadcasted" if args.len() == 2 => Some(args),
        _ => None,
    }
}

fn build(call: &[AotExpr], arrays: &mut Vec<AotExpr>) -> Result<BroadcastNode, BroadcastReject> {
    let callee = match &call[0] {
        AotExpr::Var { name, .. } => name.as_str(),
        _ => return Err(BroadcastReject::UnknownCallee),
    };
    let op = BroadcastOp::resolve(callee).ok_or(BroadcastReject::UnknownCallee)?;
    let AotExpr::TupleLit { elements } = &call[1] else {
        return Err(BroadcastReject::UnknownCallee);
    };
    let args = elements
        .iter()
        .map(|element| operand(element, arrays))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BroadcastNode::Apply { op, args })
}

fn operand(expr: &AotExpr, arrays: &mut Vec<AotExpr>) -> Result<BroadcastNode, BroadcastReject> {
    if let Some(nested) = broadcasted_args(expr) {
        return build(nested, arrays);
    }
    if matches!(expr.get_type(), StaticType::Array { .. }) {
        arrays.push(expr.clone());
        return Ok(BroadcastNode::Element);
    }
    Ok(BroadcastNode::Scalar(expr.clone()))
}

pub fn node_ty(node: &BroadcastNode, element: &StaticType) -> Option<StaticType> {
    match node {
        BroadcastNode::Element => Some(element.clone()),
        BroadcastNode::Scalar(expr) => Some(expr.get_type()),
        BroadcastNode::Apply { op, args } => {
            let arg_types = args
                .iter()
                .map(|arg| node_ty(arg, element))
                .collect::<Option<Vec<_>>>()?;
            op.result_ty(&arg_types)
        }
    }
}

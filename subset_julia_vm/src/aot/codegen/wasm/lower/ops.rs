use crate::aot::ir::{AotBinOp, AotUnaryOp, BinOpKind, UnaryOpKind};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};

use super::super::types::unsupported;

pub(super) fn ensure_type(ty: &StaticType) -> AotResult<()> {
    match ty {
        StaticType::I64
        | StaticType::I32
        | StaticType::F32
        | StaticType::F64
        | StaticType::Bool
        | StaticType::Str
        | StaticType::U8 => Ok(()),
        StaticType::Tuple(_) | StaticType::NamedTuple(_) | StaticType::Struct { .. } => Ok(()),
        StaticType::Array {
            element,
            ndims: Some(rank),
        } if *rank <= super::super::types::MAX_RANK => {
            super::super::types::descriptor_layout(ty).map(|_| ())
        }
        other => Err(unsupported(format!(
            "Wasm AoT does not support type `{}`",
            other.julia_type_name()
        ))),
    }
}

pub(super) fn ensure_return_type(ty: &StaticType) -> AotResult<()> {
    if *ty == StaticType::Nothing {
        Ok(())
    } else {
        ensure_type(ty)
    }
}

pub(super) fn map_binop(op: AotBinOp) -> AotResult<BinOpKind> {
    Ok(match op {
        AotBinOp::Add => BinOpKind::Add,
        AotBinOp::Sub => BinOpKind::Sub,
        AotBinOp::Mul => BinOpKind::Mul,
        AotBinOp::Div | AotBinOp::IntDiv => BinOpKind::Div,
        AotBinOp::Mod => BinOpKind::Rem,
        AotBinOp::Eq | AotBinOp::Egal => BinOpKind::Eq,
        AotBinOp::Ne | AotBinOp::NotEgal => BinOpKind::Ne,
        AotBinOp::Lt => BinOpKind::Lt,
        AotBinOp::Le => BinOpKind::Le,
        AotBinOp::Gt => BinOpKind::Gt,
        AotBinOp::Ge => BinOpKind::Ge,
        AotBinOp::And => BinOpKind::And,
        AotBinOp::Or => BinOpKind::Or,
        AotBinOp::BitAnd => BinOpKind::BitAnd,
        AotBinOp::BitOr => BinOpKind::BitOr,
        AotBinOp::BitXor => BinOpKind::BitXor,
        AotBinOp::Shl => BinOpKind::Shl,
        AotBinOp::Shr => BinOpKind::Shr,
        AotBinOp::Pow => BinOpKind::Pow,
        AotBinOp::Subtype => {
            return Err(unsupported(format!(
                "Wasm AoT does not support binary operator `{op}`"
            )))
        }
    })
}

pub(super) fn map_unary(op: AotUnaryOp) -> AotResult<UnaryOpKind> {
    match op {
        AotUnaryOp::Neg => Ok(UnaryOpKind::Neg),
        AotUnaryOp::Not => Ok(UnaryOpKind::Not),
        AotUnaryOp::BitNot => Ok(UnaryOpKind::BitNot),
        AotUnaryOp::Pos => Err(AotError::InternalError(
            "unary plus must be removed before Wasm IR".to_string(),
        )),
    }
}

//! One-shot typed payloads carried from VM errors into Julia exception structs.

use crate::vm::value::{FunctionValue, TupleValue};
use crate::vm::{Value, VmError};

pub(in crate::vm) enum PendingExceptionPayload {
    Method {
        message: String,
        f: Value,
        args: Vec<Value>,
    },
    Domain {
        message: String,
        val: Value,
    },
    Type {
        message: String,
        func: Value,
        context: Value,
        expected: Value,
        got: Value,
    },
    StringIndex {
        index: i64,
        valid_indices: (i64, i64),
        string: Value,
    },
    Parse {
        message: String,
        detail: Value,
    },
    FieldIndex {
        index: usize,
        field_count: usize,
        receiver: Value,
    },
}

impl PendingExceptionPayload {
    pub(in crate::vm) fn method_error(message: String, f_name: &str, args: &[Value]) -> Self {
        Self::Method {
            message,
            f: Value::Function(FunctionValue::new(f_name.to_string())),
            args: args.to_vec(),
        }
    }

    fn error(&self) -> VmError {
        match self {
            Self::Method { message, .. } => VmError::MethodError(message.clone()),
            Self::Domain { message, .. } => VmError::DomainError(message.clone()),
            Self::Type { message, .. } => VmError::TypeError(message.clone()),
            Self::StringIndex {
                index,
                valid_indices,
                ..
            } => VmError::StringIndexError {
                index: *index,
                valid_indices: *valid_indices,
            },
            Self::Parse { message, .. } => VmError::ParseError(message.clone()),
            Self::FieldIndex {
                index, field_count, ..
            } => VmError::FieldIndexOutOfBounds {
                index: *index,
                field_count: *field_count,
            },
        }
    }

    fn matches(&self, err: &VmError) -> bool {
        match (self, err) {
            (Self::Method { message, .. }, VmError::MethodError(err_message))
            | (Self::Domain { message, .. }, VmError::DomainError(err_message))
            | (Self::Type { message, .. }, VmError::TypeError(err_message))
            | (Self::Parse { message, .. }, VmError::ParseError(err_message)) => {
                message == err_message
            }
            (
                Self::StringIndex {
                    index,
                    valid_indices,
                    ..
                },
                VmError::StringIndexError {
                    index: err_index,
                    valid_indices: err_valid_indices,
                },
            ) => index == err_index && valid_indices == err_valid_indices,
            (
                Self::FieldIndex {
                    index, field_count, ..
                },
                VmError::FieldIndexOutOfBounds {
                    index: err_index,
                    field_count: err_field_count,
                },
            ) => index == err_index && field_count == err_field_count,
            _ => false,
        }
    }

    fn into_fields(self) -> Vec<Value> {
        match self {
            Self::Method { f, args, .. } => vec![f, Value::Tuple(TupleValue::new(args))],
            Self::Domain { message, val } => vec![val, Value::str_new(message)],
            Self::Type {
                func,
                context,
                expected,
                got,
                ..
            } => vec![func, context, expected, got],
            Self::StringIndex { index, string, .. } => {
                vec![string, Value::I64(index)]
            }
            Self::Parse { message, detail } => vec![Value::str_new(message), detail],
            Self::FieldIndex {
                index, receiver, ..
            } => vec![receiver, Value::I64(index as i64)],
        }
    }
}

#[derive(Default)]
pub(in crate::vm) struct PendingExceptionPayloadCarrier {
    pending: Option<PendingExceptionPayload>,
}

impl PendingExceptionPayloadCarrier {
    pub(in crate::vm) fn park_and_construct(
        &mut self,
        payload: PendingExceptionPayload,
    ) -> VmError {
        let err = payload.error();
        self.pending = Some(payload);
        err
    }

    pub(in crate::vm) fn park_for_existing(
        &mut self,
        payload: PendingExceptionPayload,
        err: &VmError,
    ) {
        if payload.matches(err) {
            self.pending = Some(payload);
        }
    }

    pub(in crate::vm) fn take_fields_for(&mut self, err: &VmError) -> Option<Vec<Value>> {
        let payload = self.pending.take()?;
        payload.matches(err).then(|| payload.into_fields())
    }

    pub(in crate::vm) fn clear(&mut self) {
        self.pending = None;
    }
}

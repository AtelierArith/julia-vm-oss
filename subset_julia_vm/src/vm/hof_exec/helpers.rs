//! HOF utility functions for calling functions with arguments.

use crate::rng::RngLike;

use crate::vm::error::VmError;
use crate::vm::frame::Frame;
use crate::vm::util::bind_value_to_slot;
use crate::vm::value::{Value, ValueType};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Call a function with a single argument
    pub(in crate::vm) fn call_function_with_arg(
        &mut self,
        func_index: usize,
        arg: f64,
    ) -> Result<(), VmError> {
        let func = self.get_function_checked(func_index)?;
        let local_slot_count = func.local_slot_count;
        let first_param_type = func.params.first().map(|(_, ty)| ty.clone());
        let first_slot = func.param_slots.first().copied();
        let entry = func.entry;

        let mut frame = Frame::new_with_slots(local_slot_count, Some(func_index));
        if let Some(ty) = first_param_type {
            let val = match ty {
                ValueType::I64 => Value::I64(arg as i64),
                _ => Value::F64(arg),
            };
            if let Some(slot) = first_slot {
                bind_value_to_slot(&mut frame, slot, val, &mut self.struct_heap);
            }
        }

        self.return_ips.push(self.ip);
        self.frames.push(frame);
        self.ip = entry;
        Ok(())
    }

    /// Call a function with two arguments (for reduce)
    pub(in crate::vm) fn call_function_with_two_args(
        &mut self,
        func_index: usize,
        arg1: f64,
        arg2: f64,
    ) -> Result<(), VmError> {
        let func = self.get_function_checked(func_index)?;
        let local_slot_count = func.local_slot_count;
        let param1_type = func.params.first().map(|(_, ty)| ty.clone());
        let param2_type = func.params.get(1).map(|(_, ty)| ty.clone());
        let slot1 = func.param_slots.first().copied();
        let slot2 = func.param_slots.get(1).copied();
        let entry = func.entry;

        let mut frame = Frame::new_with_slots(local_slot_count, Some(func_index));
        if let (Some(ty1), Some(s1)) = (param1_type, slot1) {
            let val = match ty1 {
                ValueType::I64 => Value::I64(arg1 as i64),
                _ => Value::F64(arg1),
            };
            bind_value_to_slot(&mut frame, s1, val, &mut self.struct_heap);
        }
        if let (Some(ty2), Some(s2)) = (param2_type, slot2) {
            let val = match ty2 {
                ValueType::I64 => Value::I64(arg2 as i64),
                _ => Value::F64(arg2),
            };
            bind_value_to_slot(&mut frame, s2, val, &mut self.struct_heap);
        }

        self.return_ips.push(self.ip);
        self.frames.push(frame);
        self.ip = entry;
        Ok(())
    }

    // Note: call_function_with_two_values removed - was used by value-based reduce
    // which is now Pure Julia (base/iterators.jl)

    /// Call a function with first arg (possibly complex) and extra args
    pub(in crate::vm) fn call_function_with_extra_args(
        &mut self,
        func_index: usize,
        first_arg: Value,
        extra_args: &[Value],
    ) -> Result<(), VmError> {
        let func = self.get_function_checked(func_index)?;
        let local_slot_count = func.local_slot_count;
        let params: Vec<(String, ValueType)> = func.params.clone();
        let param_slots: Vec<usize> = func.param_slots.clone();
        let entry = func.entry;

        let mut frame = Frame::new_with_slots(local_slot_count, Some(func_index));

        if let (Some((_, ty)), Some(&slot)) = (params.first(), param_slots.first()) {
            let val = match (&first_arg, ty) {
                (Value::F64(v), ValueType::I64) => Value::I64(*v as i64),
                (Value::I64(v), ValueType::F64) => Value::F64(*v as f64),
                _ => first_arg.clone(),
            };
            bind_value_to_slot(&mut frame, slot, val, &mut self.struct_heap);
        }

        // Set extra parameters
        for (i, arg) in extra_args.iter().enumerate() {
            if let (Some((_, ty)), Some(&slot)) = (params.get(i + 1), param_slots.get(i + 1)) {
                let val = match (arg, ty) {
                    (Value::F64(v), ValueType::I64) => Value::I64(*v as i64),
                    (Value::I64(v), ValueType::F64) => Value::F64(*v as f64),
                    _ => arg.clone(),
                };
                bind_value_to_slot(&mut frame, slot, val, &mut self.struct_heap);
            }
        }

        self.return_ips.push(self.ip);
        self.frames.push(frame);
        self.ip = entry;
        Ok(())
    }
}

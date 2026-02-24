use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::shared::{
    array_intersect, array_setdiff, array_symdiff, array_union, normalize_memory, pop_set,
    pop_set_or_array, vec_to_array, vec_to_set, SetOrArray,
};
use crate::vm::error::VmError;
use crate::vm::stack_ops::StackOps;
use crate::vm::value::{DictKey, SetValue, Value};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_set_ops(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<bool, VmError> {
        match builtin {
            BuiltinId::SetNew => {
                self.stack.push(Value::Set(SetValue::new()));
                Ok(true)
            }
            BuiltinId::SetPush => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "push! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let elem = self.stack.pop_value()?;
                let key = DictKey::from_value(&elem)?;
                let mut set = pop_set(&mut self.stack)?;
                set.insert(key);
                self.stack.push(Value::Set(set));
                Ok(true)
            }
            BuiltinId::SetDelete => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "delete! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let elem = self.stack.pop_value()?;
                let key = DictKey::from_value(&elem)?;
                let mut set = pop_set(&mut self.stack)?;
                set.remove(&key);
                self.stack.push(Value::Set(set));
                Ok(true)
            }
            BuiltinId::SetIn => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "in requires 2 arguments, got {}",
                        argc
                    )));
                }
                let set = pop_set(&mut self.stack)?;
                let elem = self.stack.pop_value()?;
                let key = DictKey::from_value(&elem)?;
                self.stack.push(Value::Bool(set.contains(&key)));
                Ok(true)
            }
            BuiltinId::SetUnion => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "union requires 2 arguments, got {}",
                        argc
                    )));
                }
                let b = pop_set_or_array(&mut self.stack)?;
                let a = pop_set_or_array(&mut self.stack)?;
                let result = match (a, b) {
                    (SetOrArray::Set(sa), SetOrArray::Set(sb)) => Value::Set(sa.union(&sb)),
                    (SetOrArray::Array(va), SetOrArray::Array(vb)) => {
                        vec_to_array(array_union(&va, &vb))
                    }
                    (SetOrArray::Set(sa), SetOrArray::Array(vb)) => {
                        Value::Set(sa.union(&vec_to_set(&vb)))
                    }
                    (SetOrArray::Array(va), SetOrArray::Set(sb)) => {
                        Value::Set(vec_to_set(&va).union(&sb))
                    }
                };
                self.stack.push(result);
                Ok(true)
            }
            BuiltinId::SetIntersect => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "intersect requires 2 arguments, got {}",
                        argc
                    )));
                }
                let b = pop_set_or_array(&mut self.stack)?;
                let a = pop_set_or_array(&mut self.stack)?;
                let result = match (a, b) {
                    (SetOrArray::Set(sa), SetOrArray::Set(sb)) => Value::Set(sa.intersect(&sb)),
                    (SetOrArray::Array(va), SetOrArray::Array(vb)) => {
                        vec_to_array(array_intersect(&va, &vb))
                    }
                    (SetOrArray::Set(sa), SetOrArray::Array(vb)) => {
                        Value::Set(sa.intersect(&vec_to_set(&vb)))
                    }
                    (SetOrArray::Array(va), SetOrArray::Set(sb)) => {
                        Value::Set(vec_to_set(&va).intersect(&sb))
                    }
                };
                self.stack.push(result);
                Ok(true)
            }
            BuiltinId::SetSetdiff => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "setdiff requires 2 arguments, got {}",
                        argc
                    )));
                }
                let b = pop_set_or_array(&mut self.stack)?;
                let a = pop_set_or_array(&mut self.stack)?;
                let result = match (a, b) {
                    (SetOrArray::Set(sa), SetOrArray::Set(sb)) => Value::Set(sa.setdiff(&sb)),
                    (SetOrArray::Array(va), SetOrArray::Array(vb)) => {
                        vec_to_array(array_setdiff(&va, &vb))
                    }
                    (SetOrArray::Set(sa), SetOrArray::Array(vb)) => {
                        Value::Set(sa.setdiff(&vec_to_set(&vb)))
                    }
                    (SetOrArray::Array(va), SetOrArray::Set(sb)) => {
                        Value::Set(vec_to_set(&va).setdiff(&sb))
                    }
                };
                self.stack.push(result);
                Ok(true)
            }
            BuiltinId::SetSymdiff => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "symdiff requires 2 arguments, got {}",
                        argc
                    )));
                }
                let b = pop_set_or_array(&mut self.stack)?;
                let a = pop_set_or_array(&mut self.stack)?;
                let result = match (a, b) {
                    (SetOrArray::Set(sa), SetOrArray::Set(sb)) => {
                        let a_minus_b = sa.setdiff(&sb);
                        let b_minus_a = sb.setdiff(&sa);
                        Value::Set(a_minus_b.union(&b_minus_a))
                    }
                    (SetOrArray::Array(va), SetOrArray::Array(vb)) => {
                        vec_to_array(array_symdiff(&va, &vb))
                    }
                    (SetOrArray::Set(sa), SetOrArray::Array(vb)) => {
                        let sb = vec_to_set(&vb);
                        let a_minus_b = sa.setdiff(&sb);
                        let b_minus_a = sb.setdiff(&sa);
                        Value::Set(a_minus_b.union(&b_minus_a))
                    }
                    (SetOrArray::Array(va), SetOrArray::Set(sb)) => {
                        let sa = vec_to_set(&va);
                        let a_minus_b = sa.setdiff(&sb);
                        let b_minus_a = sb.setdiff(&sa);
                        Value::Set(a_minus_b.union(&b_minus_a))
                    }
                };
                self.stack.push(result);
                Ok(true)
            }
            BuiltinId::SetIssubset => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "issubset requires 2 arguments, got {}",
                        argc
                    )));
                }
                let val_b = normalize_memory(self.stack.pop_value()?);
                let val_a = normalize_memory(self.stack.pop_value()?);
                let is_subset = match (&val_a, &val_b) {
                    (Value::Set(set_a), Value::Set(set_b)) => {
                        set_a.iter().all(|elem| set_b.contains(elem))
                    }
                    (Value::Array(arr_a), Value::Array(arr_b)) => {
                        let arr_a_ref = arr_a.borrow();
                        let arr_b_ref = arr_b.borrow();
                        let vec_a = arr_a_ref.try_as_f64_vec()?;
                        let vec_b = arr_b_ref.try_as_f64_vec()?;
                        vec_a.iter().all(|elem_a| {
                            vec_b.iter().any(|elem_b| (elem_a - elem_b).abs() < 1e-10)
                        })
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "issubset requires two Sets or two Arrays, got {:?} and {:?}",
                            crate::vm::util::value_type_name(&val_a),
                            crate::vm::util::value_type_name(&val_b)
                        )));
                    }
                };
                self.stack.push(Value::Bool(is_subset));
                Ok(true)
            }
            BuiltinId::SetIsdisjoint => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "isdisjoint requires 2 arguments, got {}",
                        argc
                    )));
                }
                let val_b = normalize_memory(self.stack.pop_value()?);
                let val_a = normalize_memory(self.stack.pop_value()?);
                let is_disjoint = match (&val_a, &val_b) {
                    (Value::Set(set_a), Value::Set(set_b)) => {
                        !set_a.iter().any(|elem| set_b.contains(elem))
                    }
                    (Value::Array(arr_a), Value::Array(arr_b)) => {
                        let arr_a_ref = arr_a.borrow();
                        let arr_b_ref = arr_b.borrow();
                        let vec_a = arr_a_ref.try_as_f64_vec()?;
                        let vec_b = arr_b_ref.try_as_f64_vec()?;
                        let set_a: std::collections::HashSet<_> =
                            vec_a.iter().map(|&x| x.to_bits()).collect();
                        let set_b: std::collections::HashSet<_> =
                            vec_b.iter().map(|&x| x.to_bits()).collect();
                        set_a.is_disjoint(&set_b)
                    }
                    (Value::Set(set_a), Value::Array(arr_b)) => {
                        let arr_b_ref = arr_b.borrow();
                        let vec_b = arr_b_ref.try_as_f64_vec()?;
                        let mut found = false;
                        for &v in &vec_b {
                            let as_i64 = v as i64;
                            if v == as_i64 as f64 && set_a.contains(&DictKey::I64(as_i64)) {
                                found = true;
                                break;
                            }
                        }
                        !found
                    }
                    (Value::Array(arr_a), Value::Set(set_b)) => {
                        let arr_a_ref = arr_a.borrow();
                        let vec_a = arr_a_ref.try_as_f64_vec()?;
                        let mut found = false;
                        for &v in &vec_a {
                            let as_i64 = v as i64;
                            if v == as_i64 as f64 && set_b.contains(&DictKey::I64(as_i64)) {
                                found = true;
                                break;
                            }
                        }
                        !found
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isdisjoint requires two Sets or two Arrays, got {:?} and {:?}",
                            crate::vm::util::value_type_name(&val_a),
                            crate::vm::util::value_type_name(&val_b)
                        )));
                    }
                };
                self.stack.push(Value::Bool(is_disjoint));
                Ok(true)
            }
            BuiltinId::SetIssetequal => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "issetequal requires 2 arguments, got {}",
                        argc
                    )));
                }
                let val_b = normalize_memory(self.stack.pop_value()?);
                let val_a = normalize_memory(self.stack.pop_value()?);
                let is_equal = match (&val_a, &val_b) {
                    (Value::Set(set_a), Value::Set(set_b)) => {
                        set_a.len() == set_b.len() && set_a.iter().all(|elem| set_b.contains(elem))
                    }
                    (Value::Array(arr_a), Value::Array(arr_b)) => {
                        let arr_a_ref = arr_a.borrow();
                        let arr_b_ref = arr_b.borrow();
                        let vec_a = arr_a_ref.try_as_f64_vec()?;
                        let vec_b = arr_b_ref.try_as_f64_vec()?;
                        let set_a: std::collections::HashSet<_> =
                            vec_a.iter().map(|&x| x.to_bits()).collect();
                        let set_b: std::collections::HashSet<_> =
                            vec_b.iter().map(|&x| x.to_bits()).collect();
                        set_a == set_b
                    }
                    (Value::Set(set_a), Value::Array(arr_b)) => {
                        let arr_b_ref = arr_b.borrow();
                        let vec_b = arr_b_ref.try_as_f64_vec()?;
                        let mut set_from_arr = SetValue::new();
                        for &v in &vec_b {
                            set_from_arr.insert(DictKey::I64(v.to_bits() as i64));
                        }
                        set_a.len() == set_from_arr.len()
                            && set_a.iter().all(|elem| set_from_arr.contains(elem))
                    }
                    (Value::Array(arr_a), Value::Set(set_b)) => {
                        let arr_a_ref = arr_a.borrow();
                        let vec_a = arr_a_ref.try_as_f64_vec()?;
                        let mut set_from_arr = SetValue::new();
                        for &v in &vec_a {
                            set_from_arr.insert(DictKey::I64(v.to_bits() as i64));
                        }
                        set_from_arr.len() == set_b.len()
                            && set_from_arr.iter().all(|elem| set_b.contains(elem))
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "issetequal requires two Sets or two Arrays, got {:?} and {:?}",
                            crate::vm::util::value_type_name(&val_a),
                            crate::vm::util::value_type_name(&val_b)
                        )));
                    }
                };
                self.stack.push(Value::Bool(is_equal));
                Ok(true)
            }
            BuiltinId::SetEmpty => {
                if argc != 1 {
                    return Err(VmError::TypeError(format!(
                        "empty! requires 1 argument, got {}",
                        argc
                    )));
                }
                let _ = pop_set(&mut self.stack)?;
                self.stack.push(Value::Set(SetValue::new()));
                Ok(true)
            }
            BuiltinId::SetUnionMut => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "union! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let itr = normalize_memory(self.stack.pop_value()?);
                let mut set = pop_set(&mut self.stack)?;

                match itr {
                    Value::Set(other_set) => {
                        for key in other_set.iter() {
                            set.insert(key.clone());
                        }
                    }
                    Value::Array(arr) => {
                        let arr_ref = arr.borrow();
                        let vec = arr_ref.try_as_f64_vec()?;
                        for &v in &vec {
                            let as_i64 = v as i64;
                            if v == as_i64 as f64 {
                                set.insert(DictKey::I64(as_i64));
                            }
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "union! requires Set or Array as second argument, got {:?}",
                            crate::vm::util::value_type_name(&itr)
                        )));
                    }
                }
                self.stack.push(Value::Set(set));
                Ok(true)
            }
            BuiltinId::SetIntersectMut => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "intersect! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let itr = normalize_memory(self.stack.pop_value()?);
                let set = pop_set(&mut self.stack)?;

                let keep_set: SetValue = match &itr {
                    Value::Set(other_set) => other_set.clone(),
                    Value::Array(arr) => {
                        let arr_ref = arr.borrow();
                        let vec = arr_ref.try_as_f64_vec()?;
                        let mut temp_set = SetValue::new();
                        for &v in &vec {
                            let as_i64 = v as i64;
                            if v == as_i64 as f64 {
                                temp_set.insert(DictKey::I64(as_i64));
                            }
                        }
                        temp_set
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "intersect! requires Set or Array as second argument, got {:?}",
                            crate::vm::util::value_type_name(&itr)
                        )));
                    }
                };

                self.stack.push(Value::Set(set.intersect(&keep_set)));
                Ok(true)
            }
            BuiltinId::SetSetdiffMut => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "setdiff! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let itr = normalize_memory(self.stack.pop_value()?);
                let mut set = pop_set(&mut self.stack)?;

                match itr {
                    Value::Set(other_set) => {
                        for key in other_set.iter() {
                            set.remove(key);
                        }
                    }
                    Value::Array(arr) => {
                        let arr_ref = arr.borrow();
                        let vec = arr_ref.try_as_f64_vec()?;
                        for &v in &vec {
                            let as_i64 = v as i64;
                            if v == as_i64 as f64 {
                                set.remove(&DictKey::I64(as_i64));
                            }
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "setdiff! requires Set or Array as second argument, got {:?}",
                            crate::vm::util::value_type_name(&itr)
                        )));
                    }
                }
                self.stack.push(Value::Set(set));
                Ok(true)
            }
            BuiltinId::SetSymdiffMut => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "symdiff! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let itr = normalize_memory(self.stack.pop_value()?);
                let mut set = pop_set(&mut self.stack)?;

                let itr_set: SetValue = match &itr {
                    Value::Set(other_set) => other_set.clone(),
                    Value::Array(arr) => {
                        let arr_ref = arr.borrow();
                        let vec = arr_ref.try_as_f64_vec()?;
                        let mut temp_set = SetValue::new();
                        for &v in &vec {
                            let as_i64 = v as i64;
                            if v == as_i64 as f64 {
                                temp_set.insert(DictKey::I64(as_i64));
                            }
                        }
                        temp_set
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "symdiff! requires Set or Array as second argument, got {:?}",
                            crate::vm::util::value_type_name(&itr)
                        )));
                    }
                };

                for key in itr_set.iter() {
                    if set.contains(key) {
                        set.remove(key);
                    } else {
                        set.insert(key.clone());
                    }
                }

                self.stack.push(Value::Set(set));
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

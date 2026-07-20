use super::super::{escape_rust_ident, AotCodeGenerator};
use super::ResolvedCAbiExport;
use crate::aot::abi::{AotAbiClass, AotAbiValue};
use crate::aot::codegen::CAbiExport;
use crate::aot::ir::{AotFunction, AotProgram};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};
use std::collections::HashSet;

impl AotCodeGenerator {
    pub(in crate::aot::codegen::aot_codegen) fn resolve_c_abi_exports(
        &self,
        program: &AotProgram,
    ) -> AotResult<Vec<ResolvedCAbiExport>> {
        let mut resolved = Vec::new();
        let mut seen_export_names = HashSet::new();
        let generated_names: HashSet<_> = program
            .functions
            .iter()
            .map(|func| self.emitted_function_name(func))
            .collect();

        for request in &self.config.c_abi_exports {
            if !Self::is_c_symbol_name(&request.export_name) {
                return Err(AotError::CodegenError(format!(
                    "C ABI export symbol `{}` is not a valid C/Rust symbol name",
                    request.export_name
                )));
            }
            if !seen_export_names.insert(request.export_name.clone()) {
                return Err(AotError::CodegenError(format!(
                    "duplicate C ABI export symbol `{}`",
                    request.export_name
                )));
            }

            let candidates: Vec<_> = program
                .functions
                .iter()
                .filter_map(|func| {
                    let rust_func_name = self.emitted_function_name(func);
                    let name_matches = func.name == request.function_name
                        || rust_func_name == request.function_name;
                    let signature_matches = request.arg_types.as_ref().is_none_or(|arg_types| {
                        func.params.iter().map(|(_, ty)| ty).eq(arg_types.iter())
                    });
                    if name_matches && signature_matches {
                        Some((func, rust_func_name))
                    } else {
                        None
                    }
                })
                .collect();

            let [(func, rust_func_name)] = candidates.as_slice() else {
                return Err(AotError::CodegenError(Self::c_abi_resolution_error(
                    request,
                    candidates.len(),
                )));
            };

            if request.export_name != *rust_func_name
                && generated_names.contains(&request.export_name)
            {
                return Err(AotError::CodegenError(format!(
                    "C ABI export symbol `{}` conflicts with an existing generated Rust function; use a distinct export name",
                    request.export_name
                )));
            }

            Self::validate_c_abi_export(request, func)?;
            resolved.push(ResolvedCAbiExport {
                export_name: request.export_name.clone(),
                rust_func_name: rust_func_name.clone(),
                func: (*func).clone(),
            });
        }

        Ok(resolved)
    }

    fn c_abi_resolution_error(request: &CAbiExport, candidate_count: usize) -> String {
        if candidate_count == 0 {
            format!(
                "C ABI export `{}` could not find function `{}`",
                request.export_name, request.function_name
            )
        } else {
            format!(
                "C ABI export `{}` is ambiguous for function `{}`; use `symbol=function(Int64,Float64)` or a generated method name such as `name_i64_i64`",
                request.export_name, request.function_name
            )
        }
    }

    fn validate_c_abi_export(request: &CAbiExport, func: &AotFunction) -> AotResult<()> {
        let abi = func.call_abi();
        if !abi.is_fully_native() {
            return Err(AotError::CodegenError(format!(
                "C ABI export `{}` for `{}` requires a fully native AoT ABI; boxed runtime `Value` boundaries are not C ABI stable",
                request.export_name, func.name
            )));
        }

        for (idx, param) in abi.params().iter().enumerate() {
            if !Self::c_abi_value_is_stable(param, false) {
                return Err(AotError::CodegenError(format!(
                    "C ABI export `{}` for `{}` has non-C-stable parameter {} of type `{}`",
                    request.export_name,
                    func.name,
                    idx + 1,
                    param.julia_type()
                )));
            }
        }

        if !Self::c_abi_value_is_stable(abi.ret(), true) {
            return Err(AotError::CodegenError(format!(
                "C ABI export `{}` for `{}` has non-C-stable return type `{}`",
                request.export_name,
                func.name,
                abi.ret().julia_type()
            )));
        }

        Ok(())
    }

    fn c_abi_value_is_stable(value: &AotAbiValue, allow_nothing: bool) -> bool {
        if value.class() != AotAbiClass::UnboxedScalar {
            return false;
        }
        matches!(
            value.julia_type(),
            StaticType::I64
                | StaticType::I32
                | StaticType::I16
                | StaticType::I8
                | StaticType::U64
                | StaticType::U32
                | StaticType::U16
                | StaticType::U8
                | StaticType::F64
                | StaticType::F32
                | StaticType::Bool
        ) || (allow_nothing && matches!(value.julia_type(), StaticType::Nothing))
    }

    pub(in crate::aot::codegen::aot_codegen) fn emit_c_abi_wrappers(
        &mut self,
        exports: &[ResolvedCAbiExport],
    ) -> AotResult<()> {
        for export in exports {
            if export.export_name == export.rust_func_name {
                continue;
            }

            let params: Vec<_> = export
                .func
                .params
                .iter()
                .map(|(name, ty)| format!("{}: {}", escape_rust_ident(name), self.type_to_rust(ty)))
                .collect();
            let args: Vec<_> = export
                .func
                .params
                .iter()
                .map(|(name, _)| escape_rust_ident(name))
                .collect();
            let export_ident = escape_rust_ident(&export.export_name);
            let return_ty = self.type_to_rust(&export.func.return_type);

            self.write_line("#[no_mangle]");
            self.write_line(&format!(
                "pub extern \"C\" fn {}({}) -> {} {{",
                export_ident,
                params.join(", "),
                return_ty
            ));
            self.indent();
            if export.func.return_type == StaticType::Nothing {
                self.write_line(&format!("{}({});", export.rust_func_name, args.join(", ")));
            } else {
                self.write_line(&format!("{}({})", export.rust_func_name, args.join(", ")));
            }
            self.dedent();
            self.write_line("}");
            self.blank_line();
        }

        Ok(())
    }
}

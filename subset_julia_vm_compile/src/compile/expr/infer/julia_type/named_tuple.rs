use crate::types::JuliaType;

#[derive(Clone)]
pub(super) struct StaticNamedTupleField {
    pub(super) name: String,
    pub(super) type_name: Option<String>,
}

pub(super) fn static_named_tuple_fields_from_julia_type(
    ty: &JuliaType,
) -> Option<Vec<StaticNamedTupleField>> {
    let JuliaType::Struct(name) = ty else {
        return None;
    };
    let body = name.strip_prefix("@NamedTuple{")?.strip_suffix('}')?.trim();
    if body.is_empty() {
        return Some(Vec::new());
    }
    split_static_named_tuple_fields(body)
        .into_iter()
        .map(|field| {
            let (name, type_name) = field
                .split_once("::")
                .map_or((field.trim(), None), |(name, ty)| {
                    (name.trim(), Some(ty.trim().to_string()))
                });
            if name.is_empty() {
                None
            } else {
                Some(StaticNamedTupleField {
                    name: name.to_string(),
                    type_name,
                })
            }
        })
        .collect()
}

pub(super) fn merge_static_named_tuple_fields(
    arg_fields: &[Vec<StaticNamedTupleField>],
) -> Vec<StaticNamedTupleField> {
    let mut merged = Vec::<StaticNamedTupleField>::new();
    for fields in arg_fields {
        for field in fields {
            if let Some(existing) = merged
                .iter_mut()
                .find(|candidate| candidate.name == field.name)
            {
                *existing = field.clone();
            } else {
                merged.push(field.clone());
            }
        }
    }
    merged
}

fn split_static_named_tuple_fields(body: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in body.char_indices() {
        match ch {
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                let field = body[start..idx].trim();
                if !field.is_empty() {
                    fields.push(field);
                }
                start = idx + 1;
            }
            _ => {}
        }
    }
    let last = body[start..].trim();
    if !last.is_empty() {
        fields.push(last);
    }
    fields
}

pub(super) fn split_named_tuple_constructor_params(inner: &str) -> Vec<&str> {
    let mut params = Vec::new();
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ',' if brace_depth == 0 && paren_depth == 0 => {
                let param = inner[start..idx].trim();
                if !param.is_empty() {
                    params.push(param);
                }
                start = idx + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        params.push(last);
    }
    params
}

pub(super) fn parse_named_tuple_constructor_names(param: &str) -> Option<Vec<String>> {
    let inner = param.strip_prefix('(')?.strip_suffix(')')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    inner
        .split(',')
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(|raw| raw.strip_prefix(':').map(str::to_string))
        .collect()
}

pub(super) fn parse_named_tuple_constructor_tuple_types(param: &str) -> Option<Vec<String>> {
    let inner = param.strip_prefix("Tuple{")?.strip_suffix('}')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    Some(
        split_static_named_tuple_fields(inner)
            .into_iter()
            .map(|ty| {
                JuliaType::from_name_or_struct(ty.trim())
                    .name()
                    .into_owned()
            })
            .collect(),
    )
}

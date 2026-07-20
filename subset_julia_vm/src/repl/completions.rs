//! REPL tab completion engine.
//!
//! This module is intentionally independent of rustyline so completion behavior
//! can be unit-tested without the `repl` feature or terminal integration.

use std::sync::OnceLock;

use crate::julia::{base, packages, stdlib::available_modules};
use crate::unicode::{completions_for_prefix, latex_to_unicode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompletionKind {
    Keyword,
    Keyval,
    Module,
    BaseExport,
    Global,
    Field,
    Latex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionItem {
    pub text: String,
    pub display: String,
    pub kind: CompletionKind,
}

#[derive(Clone, Debug, Default)]
pub struct CompletionContext<'a> {
    pub variable_names: &'a [String],
    pub function_names: &'a [String],
    pub imported_module_names: &'a [String],
    pub field_names_by_object: &'a [(String, Vec<String>)],
}

const SORTED_KEYWORDS: &[&str] = &[
    "abstract",
    "abstract type",
    "as",
    "baremodule",
    "begin",
    "break",
    "catch",
    "const",
    "continue",
    "do",
    "else",
    "elseif",
    "end",
    "export",
    "finally",
    "for",
    "function",
    "global",
    "if",
    "import",
    "in",
    "isa",
    "let",
    "local",
    "macro",
    "module",
    "mutable",
    "mutable struct",
    "outer",
    "primitive",
    "primitive type",
    "public",
    "quote",
    "return",
    "struct",
    "try",
    "using",
    "where",
    "while",
];

const SORTED_KEYVALS: &[&str] = &["false", "true"];

pub fn complete(
    line: &str,
    pos: usize,
    ctx: &CompletionContext<'_>,
) -> (usize, Vec<CompletionItem>) {
    if pos > line.len() || !line.is_char_boundary(pos) {
        return (pos.min(line.len()), Vec::new());
    }

    if let Some((start, items)) = complete_latex(line, pos) {
        return (start, items);
    }

    if let Some((start, items)) = complete_qualified(line, pos, ctx) {
        return (start, items);
    }

    if let Some((start, prefix)) = loading_module_context(line, pos) {
        let mut items = Vec::new();
        extend_modules(&mut items, prefix);
        finish_items(&mut items);
        return (start, items);
    }

    let start = identifier_start(line, pos);
    let prefix = &line[start..pos];
    if prefix.is_empty() {
        return (pos, Vec::new());
    }

    let mut items = Vec::new();
    extend_from_sorted_list(&mut items, SORTED_KEYWORDS, prefix, CompletionKind::Keyword);
    extend_from_sorted_list(&mut items, SORTED_KEYVALS, prefix, CompletionKind::Keyval);
    extend_modules(&mut items, prefix);
    extend_from_sorted_list(
        &mut items,
        base_exports(),
        prefix,
        CompletionKind::BaseExport,
    );
    extend_from_names(
        &mut items,
        ctx.variable_names,
        prefix,
        CompletionKind::Global,
    );
    extend_from_names(
        &mut items,
        ctx.function_names,
        prefix,
        CompletionKind::Global,
    );
    extend_imported_module_exports(&mut items, ctx.imported_module_names, prefix);
    finish_items(&mut items);

    (start, items)
}

fn complete_qualified(
    line: &str,
    pos: usize,
    ctx: &CompletionContext<'_>,
) -> Option<(usize, Vec<CompletionItem>)> {
    let before_cursor = &line[..pos];
    let dot_pos = before_cursor.rfind('.')?;
    let object_start = identifier_start(line, dot_pos);
    if object_start == dot_pos {
        return None;
    }
    let object = &line[object_start..dot_pos];
    if object.is_empty() {
        return None;
    }
    let prefix_start = dot_pos + '.'.len_utf8();
    let prefix = &line[prefix_start..pos];
    if !prefix.chars().all(is_identifier_completion_char) {
        return None;
    }

    let mut items = Vec::new();
    if let Some(exports) = exports_for_module(object) {
        extend_from_sorted_list(&mut items, exports, prefix, CompletionKind::BaseExport);
    }
    if let Some((_, fields)) = ctx
        .field_names_by_object
        .iter()
        .find(|(name, _)| name == object)
    {
        extend_from_names(&mut items, fields, prefix, CompletionKind::Field);
    }
    if items.is_empty() {
        None
    } else {
        finish_items(&mut items);
        Some((prefix_start, items))
    }
}

fn loading_module_context(line: &str, pos: usize) -> Option<(usize, &str)> {
    let before_cursor = &line[..pos];
    let leading_ws = before_cursor.len() - before_cursor.trim_start().len();
    let rest = &before_cursor[leading_ws..];
    let after_keyword = if starts_with_keyword(rest, "using") {
        leading_ws + "using".len()
    } else if starts_with_keyword(rest, "import") {
        leading_ws + "import".len()
    } else {
        return None;
    };

    if !before_cursor[after_keyword..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
    {
        return None;
    }

    let loading_tail = &before_cursor[after_keyword..];
    if loading_tail.contains(':') {
        return None;
    }

    let last_comma = loading_tail
        .rfind(',')
        .map(|idx| after_keyword + idx + ','.len_utf8())
        .unwrap_or(after_keyword);
    let segment_start = skip_ascii_whitespace(line, last_comma, pos);
    let start = identifier_start(line, pos).max(segment_start);
    Some((start, &line[start..pos]))
}

fn starts_with_keyword(rest: &str, keyword: &str) -> bool {
    rest.strip_prefix(keyword)
        .is_some_and(|tail| tail.chars().next().is_some_and(char::is_whitespace))
}

fn skip_ascii_whitespace(line: &str, mut start: usize, end: usize) -> usize {
    while start < end {
        let Some(ch) = line[start..end].chars().next() else {
            break;
        };
        if !ch.is_ascii_whitespace() {
            break;
        }
        start += ch.len_utf8();
    }
    start
}

fn finish_items(items: &mut Vec<CompletionItem>) {
    items.sort_by(|a, b| {
        a.text
            .cmp(&b.text)
            .then_with(|| kind_rank(&a.kind).cmp(&kind_rank(&b.kind)))
    });
    items.dedup_by(|a, b| a.text == b.text);
}

fn complete_latex(line: &str, pos: usize) -> Option<(usize, Vec<CompletionItem>)> {
    let before_cursor = &line[..pos];
    let backslash_pos = before_cursor.rfind('\\')?;
    let prefix = &before_cursor[backslash_pos..];
    if !is_valid_latex_prefix(prefix) {
        return None;
    }

    if let Some(unicode) = latex_to_unicode(prefix) {
        return Some((
            backslash_pos,
            vec![CompletionItem {
                text: unicode.to_string(),
                display: format!("{} -> {}", prefix, unicode),
                kind: CompletionKind::Latex,
            }],
        ));
    }

    let items: Vec<_> = completions_for_prefix(prefix)
        .into_iter()
        .map(|(latex, unicode)| CompletionItem {
            text: unicode.to_string(),
            display: format!("{} -> {}", latex, unicode),
            kind: CompletionKind::Latex,
        })
        .collect();
    if items.is_empty() {
        None
    } else {
        Some((backslash_pos, items))
    }
}

fn is_valid_latex_prefix(prefix: &str) -> bool {
    prefix.len() > 1
        && prefix[1..]
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '^')
}

fn identifier_start(line: &str, pos: usize) -> usize {
    let mut start = pos;
    for (idx, ch) in line[..pos].char_indices().rev() {
        if is_identifier_completion_char(ch) {
            start = idx;
        } else {
            break;
        }
    }
    start
}

fn is_identifier_completion_char(ch: char) -> bool {
    ch == '_' || ch == '!' || ch.is_alphanumeric() || !ch.is_ascii()
}

fn extend_from_sorted_list(
    items: &mut Vec<CompletionItem>,
    sorted: &[impl AsRef<str>],
    prefix: &str,
    kind: CompletionKind,
) {
    let start = sorted.partition_point(|candidate| candidate.as_ref() < prefix);
    for candidate in &sorted[start..] {
        let candidate = candidate.as_ref();
        if !candidate.starts_with(prefix) {
            break;
        }
        items.push(CompletionItem {
            text: candidate.to_string(),
            display: candidate.to_string(),
            kind: kind.clone(),
        });
    }
}

fn extend_modules(items: &mut Vec<CompletionItem>, prefix: &str) {
    extend_from_sorted_list(items, module_names(), prefix, CompletionKind::Module);
}

fn extend_imported_module_exports(
    items: &mut Vec<CompletionItem>,
    imported_module_names: &[String],
    prefix: &str,
) {
    for module in imported_module_names {
        if let Some(exports) = exports_for_module(module) {
            extend_from_sorted_list(items, exports, prefix, CompletionKind::BaseExport);
        }
    }
}

fn extend_from_names(
    items: &mut Vec<CompletionItem>,
    names: &[String],
    prefix: &str,
    kind: CompletionKind,
) {
    for name in names {
        if name.starts_with(prefix) {
            items.push(CompletionItem {
                text: name.clone(),
                display: name.clone(),
                kind: kind.clone(),
            });
        }
    }
}

fn module_names() -> &'static [String] {
    static MODULE_NAMES: OnceLock<Vec<String>> = OnceLock::new();
    MODULE_NAMES
        .get_or_init(|| {
            let mut names: Vec<String> = available_modules()
                .into_iter()
                .chain(packages::bundled_package_names())
                .chain(["Base", "Core"])
                .map(str::to_string)
                .collect();
            names.sort();
            names.dedup();
            names
        })
        .as_slice()
}

fn base_exports() -> &'static [String] {
    base::exported_names()
}

fn exports_for_module(module: &str) -> Option<&'static [String]> {
    static BASE64_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static BROADCAST_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static DATES_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static INTERACTIVE_UTILS_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static ITERATORS_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static LINEAR_ALGEBRA_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static PRINTF_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static EXAMPLE_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static JSXGRAPH_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static PLOTS_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static PRIMES_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static RANDOM_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static STATISTICS_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();
    static TEST_EXPORTS: OnceLock<Vec<String>> = OnceLock::new();

    match module {
        "Base" => Some(base_exports()),
        "Base64" => Some(
            BASE64_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::BASE64_JL))
                .as_slice(),
        ),
        "Broadcast" => Some(
            BROADCAST_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::BROADCAST_JL))
                .as_slice(),
        ),
        "Dates" => Some(
            DATES_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::DATES_JL))
                .as_slice(),
        ),
        "InteractiveUtils" => Some(
            INTERACTIVE_UTILS_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::INTERACTIVE_UTILS_JL))
                .as_slice(),
        ),
        "Iterators" => Some(
            ITERATORS_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::ITERATORS_JL))
                .as_slice(),
        ),
        "LinearAlgebra" => Some(
            LINEAR_ALGEBRA_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::LINEAR_ALGEBRA_JL))
                .as_slice(),
        ),
        "Printf" => Some(
            PRINTF_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::PRINTF_JL))
                .as_slice(),
        ),
        "Example" => Some(
            EXAMPLE_EXPORTS
                .get_or_init(|| parse_base_exports(packages::EXAMPLE_JL))
                .as_slice(),
        ),
        "JSXGraph" => Some(
            JSXGRAPH_EXPORTS
                .get_or_init(|| parse_base_exports(packages::JSXGRAPH_JL))
                .as_slice(),
        ),
        "Plots" => Some(
            PLOTS_EXPORTS
                .get_or_init(|| parse_base_exports(packages::PLOTS_JL))
                .as_slice(),
        ),
        "Primes" => Some(
            PRIMES_EXPORTS
                .get_or_init(|| parse_base_exports(packages::PRIMES_JL))
                .as_slice(),
        ),
        "Random" => Some(
            RANDOM_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::RANDOM_JL))
                .as_slice(),
        ),
        "Statistics" => Some(
            STATISTICS_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::STATISTICS_JL))
                .as_slice(),
        ),
        "Test" => Some(
            TEST_EXPORTS
                .get_or_init(|| parse_base_exports(crate::julia::stdlib::TEST_JL))
                .as_slice(),
        ),
        _ => None,
    }
}

fn parse_base_exports(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut collecting_export = false;
    for line in src.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let line = if let Some(rest) = line.strip_prefix("export") {
            let rest = rest.trim();
            if rest.is_empty() {
                collecting_export = true;
                continue;
            }
            rest
        } else if collecting_export {
            line
        } else {
            continue;
        };
        for name in line
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            names.push(name.to_string());
        }
        collecting_export = line.ends_with(',');
    }
    names.sort();
    names.dedup();
    names
}

fn kind_rank(kind: &CompletionKind) -> u8 {
    match kind {
        CompletionKind::Keyword => 0,
        CompletionKind::Keyval => 1,
        CompletionKind::Module => 2,
        CompletionKind::BaseExport => 3,
        CompletionKind::Global => 4,
        CompletionKind::Field => 5,
        CompletionKind::Latex => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|item| item.text.as_str()).collect()
    }

    #[test]
    fn keyword_completion_includes_using() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("usin", 4, &ctx);
        assert_eq!(start, 0);
        assert!(texts(&items).contains(&"using"));
    }

    #[test]
    fn keyword_completion_includes_operator_keywords() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("is", 2, &ctx);
        assert_eq!(start, 0);
        assert!(items
            .iter()
            .any(|item| item.text == "isa" && item.kind == CompletionKind::Keyword));
    }

    #[test]
    fn keyword_completion_includes_composite_keywords() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("mut", 3, &ctx);
        assert_eq!(start, 0);
        assert!(texts(&items).contains(&"mutable"));
        assert!(texts(&items).contains(&"mutable struct"));
    }

    #[test]
    fn keyval_completion_includes_true_false() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("fa", 2, &ctx);
        assert_eq!(start, 0);
        assert!(items
            .iter()
            .any(|item| item.text == "false" && item.kind == CompletionKind::Keyval));
    }

    #[test]
    fn using_context_completes_module_names() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("using Line", 10, &ctx);
        assert_eq!(start, 6);
        // `LineSearches` (bundled Optim dependency) and `LinearAlgebra` (stdlib)
        // both share the `Line` prefix.
        assert_eq!(texts(&items), vec!["LineSearches", "LinearAlgebra"]);
        assert!(items.iter().all(|item| item.kind == CompletionKind::Module));
    }

    #[test]
    fn using_context_completes_bundled_package_names() {
        let ctx = CompletionContext::default();

        let (start, items) = complete("using Pri", 9, &ctx);
        assert_eq!(start, 6);
        assert!(texts(&items).contains(&"Primes"));

        let (start, items) = complete("using Plo", 9, &ctx);
        assert_eq!(start, 6);
        assert_eq!(texts(&items), vec!["Plots"]);

        let (start, items) = complete("using JSX", 9, &ctx);
        assert_eq!(start, 6);
        assert_eq!(texts(&items), vec!["JSXGraph"]);
    }

    #[test]
    fn import_context_completes_module_names_after_comma() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("import LinearAlgebra, Ran", 25, &ctx);
        assert_eq!(start, 22);
        // `RandomExtensions` became a bundled package (PR #7907), so completing
        // `Ran` now correctly offers both `Random` and `RandomExtensions`
        // (alphabetically ordered). See Issue #7909.
        assert_eq!(texts(&items), vec!["Random", "RandomExtensions"]);
    }

    #[test]
    fn bare_identifier_completion_includes_modules_and_base_exports() {
        let ctx = CompletionContext::default();

        let (start, items) = complete("Line", 4, &ctx);
        assert_eq!(start, 0);
        assert!(texts(&items).contains(&"LinearAlgebra"));

        let (start, items) = complete("pri", 3, &ctx);
        assert_eq!(start, 0);
        let texts = texts(&items);
        assert!(texts.contains(&"print"));
        assert!(texts.contains(&"println"));
    }

    #[test]
    fn bare_identifier_completion_includes_repl_globals_and_functions() {
        let variable_names = vec!["session_value".to_string()];
        let function_names = vec!["session_func".to_string()];
        let ctx = CompletionContext {
            variable_names: &variable_names,
            function_names: &function_names,
            imported_module_names: &[],
            field_names_by_object: &[],
        };

        let (start, items) = complete("session_", 8, &ctx);
        assert_eq!(start, 0);
        let texts = texts(&items);
        assert!(texts.contains(&"session_value"));
        assert!(texts.contains(&"session_func"));
    }

    #[test]
    fn bare_identifier_completion_uses_imported_package_exports() {
        let imported_module_names = vec!["Primes".to_string()];
        let ctx = CompletionContext {
            variable_names: &[],
            function_names: &[],
            imported_module_names: &imported_module_names,
            field_names_by_object: &[],
        };

        let (start, items) = complete("nextpri", 7, &ctx);
        assert_eq!(start, 0);
        assert_eq!(texts(&items), vec!["nextprime"]);

        let ctx = CompletionContext::default();
        let (_, items) = complete("nextpri", 7, &ctx);
        assert!(items.is_empty());
    }

    #[test]
    fn qualified_module_completion_uses_exports() {
        let ctx = CompletionContext::default();

        let (start, items) = complete("Base.pri", 8, &ctx);
        assert_eq!(start, 5);
        let item_texts = texts(&items);
        assert!(item_texts.contains(&"print"));
        assert!(item_texts.contains(&"println"));

        let (start, items) = complete("LinearAlgebra.no", 16, &ctx);
        assert_eq!(start, 14);
        assert_eq!(texts(&items), vec!["norm", "normalize"]);
    }

    #[test]
    fn qualified_bundled_package_completion_uses_exports() {
        let ctx = CompletionContext::default();

        let (start, items) = complete("Primes.nextpri", 14, &ctx);
        assert_eq!(start, 7);
        assert_eq!(texts(&items), vec!["nextprime"]);

        let (start, items) = complete("Plots.plot", 10, &ctx);
        assert_eq!(start, 6);
        assert!(texts(&items).contains(&"plot!"));

        let (start, items) = complete("JSXGraph.curve", 14, &ctx);
        assert_eq!(start, 9);
        let item_texts = texts(&items);
        assert!(item_texts.contains(&"curve3d"));
    }

    #[test]
    fn field_completion_uses_context_fields() {
        let fields = vec![("point".to_string(), vec!["x".to_string(), "y".to_string()])];
        let ctx = CompletionContext {
            variable_names: &[],
            function_names: &[],
            imported_module_names: &[],
            field_names_by_object: &fields,
        };

        let (start, items) = complete("point.", 6, &ctx);
        assert_eq!(start, 6);
        assert_eq!(texts(&items), vec!["x", "y"]);

        let (start, items) = complete("point.x", 7, &ctx);
        assert_eq!(start, 6);
        assert_eq!(texts(&items), vec!["x"]);
    }

    #[test]
    fn empty_prefix_returns_no_candidates_for_space_insertion_fallback() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("", 0, &ctx);
        assert_eq!(start, 0);
        assert!(items.is_empty());

        let (start, items) = complete("    ", 4, &ctx);
        assert_eq!(start, 4);
        assert!(items.is_empty());
    }

    #[test]
    fn latex_completion_has_priority() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("\\alp", 4, &ctx);
        assert_eq!(start, 0);
        assert!(items.iter().any(|item| item.display.starts_with("\\alpha")));
    }

    #[test]
    fn exact_latex_completion_replaces_with_unicode() {
        let ctx = CompletionContext::default();
        let (start, items) = complete("\\alpha", 6, &ctx);
        assert_eq!(start, 0);
        assert_eq!(
            items,
            vec![CompletionItem {
                text: "α".to_string(),
                display: "\\alpha -> α".to_string(),
                kind: CompletionKind::Latex,
            }]
        );
    }
}

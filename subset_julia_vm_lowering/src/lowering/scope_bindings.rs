//! Structural binding provenance for one lexical scope.

use std::collections::HashSet;

use crate::ir::core::{Block, Expr, LocalDeclKind, Stmt};

/// Binding provenance without descending into nested loops, try clauses,
/// functions, comprehensions, or hard `let` scopes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct ScopeBindingInventory {
    pub explicit_locals: HashSet<String>,
    pub compiler_enclosing: HashSet<String>,
    pub soft_bindings: HashSet<String>,
    /// Assignment-backed soft bindings, excluding named function definitions.
    /// Consumers that rewrite value slots must not silently rewrite a hoisted
    /// generic-function identity as though it were a plain assignment.
    pub assignment_bindings: HashSet<String>,
    pub globals: HashSet<String>,
}

impl ScopeBindingInventory {
    pub fn collect(block: &Block) -> Self {
        let mut inventory = Self::default();
        inventory.collect_block(block);
        inventory
    }

    pub fn binding_names(&self) -> impl Iterator<Item = &String> {
        self.explicit_locals
            .iter()
            .chain(self.compiler_enclosing.iter())
            .chain(self.soft_bindings.iter())
            .filter(|name| !self.globals.contains(*name))
    }

    fn collect_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::AssignExpr { var, value, .. } => {
                self.soft_bindings.insert(var.to_string());
                self.assignment_bindings.insert(var.to_string());
                self.collect_expr(value);
            }
            Expr::BinaryOp { left, right, .. }
            | Expr::Pair {
                key: left,
                value: right,
                ..
            } => {
                self.collect_expr(left);
                self.collect_expr(right);
            }
            Expr::UnaryOp { operand, .. } | Expr::Convert { operand, .. } => {
                self.collect_expr(operand)
            }
            Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
                for arg in args {
                    self.collect_expr(arg);
                }
                for (_, value) in kwargs {
                    self.collect_expr(value);
                }
            }
            Expr::Builtin { args, .. } | Expr::New { args, .. } => {
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            Expr::Index { array, indices, .. } => {
                self.collect_expr(array);
                for index in indices {
                    self.collect_expr(index);
                }
            }
            Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
                for element in elements {
                    self.collect_expr(element);
                }
            }
            Expr::NamedTupleLiteral { fields, .. } => {
                for (_, value) in fields {
                    self.collect_expr(value);
                }
            }
            Expr::Range {
                start, step, stop, ..
            } => {
                self.collect_expr(start);
                if let Some(step) = step {
                    self.collect_expr(step);
                }
                self.collect_expr(stop);
            }
            // The first iterator expression executes outside the comprehension's
            // hard scope. Body/filter and later multi-iterator expressions execute
            // inside it and cannot bind the containing clause (Issue #11304).
            Expr::Comprehension { iter, .. } | Expr::Generator { iter, .. } => {
                self.collect_expr(iter)
            }
            Expr::MultiComprehension { iterations, .. } => {
                if let Some((_, iter)) = iterations.first() {
                    self.collect_expr(iter);
                }
            }
            Expr::FieldAccess { object, .. } => self.collect_expr(object),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                self.collect_expr(condition);
                self.collect_expr(then_expr);
                self.collect_expr(else_expr);
            }
            Expr::LetBlock { bindings, body, .. } => {
                for (_, value) in bindings {
                    self.collect_expr(value);
                }
                if bindings.is_empty() {
                    self.collect_block(body);
                }
            }
            Expr::StringConcat { parts, .. } => {
                for part in parts {
                    self.collect_expr(part);
                }
            }
            Expr::DictLiteral { pairs, .. } => {
                for (key, value) in pairs {
                    self.collect_expr(key);
                    self.collect_expr(value);
                }
            }
            Expr::QuoteLiteral { constructor, .. } => self.collect_expr(constructor),
            Expr::ReturnExpr { value, .. } => {
                if let Some(value) = value {
                    self.collect_expr(value);
                }
            }
            Expr::DynamicTypeConstruct {
                base_expr,
                type_args,
                ..
            } => {
                if let Some(base_expr) = base_expr {
                    self.collect_expr(base_expr);
                }
                for type_arg in type_args {
                    self.collect_expr(type_arg);
                }
            }
            Expr::Var(_, _)
            | Expr::Literal(_, _)
            | Expr::FunctionRef { .. }
            | Expr::TypedEmptyArray { .. }
            | Expr::SliceAll { .. }
            | Expr::BreakExpr { .. }
            | Expr::ContinueExpr { .. } => {}
        }
    }

    fn collect_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.collect_stmt(stmt);
        }
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
        if self.collect_enclosing_loop_exprs(stmt) {
            return;
        }

        match stmt {
            Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
                self.soft_bindings.insert(var.to_string());
                self.assignment_bindings.insert(var.to_string());
                if let Some(inner) = transparent_block(value) {
                    self.collect_block(inner);
                } else {
                    self.collect_expr(value);
                }
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                self.soft_bindings.extend(targets.iter().cloned());
                self.assignment_bindings.extend(targets.iter().cloned());
                self.collect_expr(value);
            }
            Stmt::FunctionDef { func, .. } => {
                self.soft_bindings.insert(func.name.clone());
            }
            Stmt::Global { names, .. } => self.globals.extend(names.iter().cloned()),
            Stmt::LocalDecl { var, kind, .. } => match kind {
                LocalDeclKind::Explicit => {
                    self.explicit_locals.insert(var.clone());
                }
                LocalDeclKind::CompilerEnclosing => {
                    self.compiler_enclosing.insert(var.clone());
                }
            },
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_expr(condition);
                self.collect_block(then_branch);
                if let Some(block) = else_branch {
                    self.collect_block(block);
                }
            }
            Stmt::Block(block) | Stmt::Timed { body: block, .. } => self.collect_block(block),
            Stmt::Expr { expr, .. } => {
                if let Some(inner) = transparent_block(expr) {
                    self.collect_block(inner);
                } else {
                    self.collect_expr(expr);
                }
            }
            Stmt::Return {
                value: Some(expr), ..
            } => self.collect_expr(expr),
            Stmt::Test { condition, .. } => self.collect_expr(condition),
            Stmt::TestThrows { expr, .. } => self.collect_expr(expr),
            Stmt::IndexAssign { indices, value, .. } => {
                for index in indices {
                    self.collect_expr(index);
                }
                self.collect_expr(value);
            }
            Stmt::FieldAssign { value, .. } => self.collect_expr(value),
            Stmt::DictAssign { key, value, .. } => {
                self.collect_expr(key);
                self.collect_expr(value);
            }
            _ => {}
        }
    }

    /// Collect expressions evaluated before entering a loop body's nested soft
    /// scope. These are bindings of the containing clause, not of the body.
    fn collect_enclosing_loop_exprs(&mut self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::For {
                start, end, step, ..
            } => {
                self.collect_expr(start);
                self.collect_expr(end);
                if let Some(step) = step {
                    self.collect_expr(step);
                }
            }
            Stmt::ForEach { iterable, .. } | Stmt::ForEachTuple { iterable, .. } => {
                self.collect_expr(iterable);
            }
            Stmt::While { condition, .. } => self.collect_expr(condition),
            _ => return false,
        }
        true
    }
}

#[doc(hidden)]
pub fn collect_scope_level_bindings(block: &Block, out: &mut HashSet<String>) {
    out.extend(
        ScopeBindingInventory::collect(block)
            .binding_names()
            .cloned(),
    );
}

fn transparent_block(expr: &Expr) -> Option<&Block> {
    match expr {
        Expr::LetBlock { bindings, body, .. } if bindings.is_empty() => Some(body),
        _ => None,
    }
}

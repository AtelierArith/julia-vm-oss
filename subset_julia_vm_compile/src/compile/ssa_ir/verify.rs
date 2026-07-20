//! SSA well-formedness verifier (Issue #8550).
//!
//! Checked invariants:
//!
//! * Block ids are dense indices; terminator targets exist; each block's
//!   `succs` equals its terminator targets and `preds` is the exact inverse
//!   (ascending by id); the entry block has no predecessors.
//! * Every definition id is unique; every `Def` operand refers to an existing
//!   definition; `Argument` indices are in range.
//! * Phi statements form a block prefix; phi `edges`/`values` arity matches
//!   the predecessor list exactly.
//! * Defs dominate uses: within a block by statement order, across blocks by
//!   the dominator tree (iterative Cooper–Harvey–Kennedy on reverse
//!   post-order, shared with the optimization passes via [`super::dom`]).
//!   Phi operands must dominate the *end* of their edge's predecessor.
//!   Unreachable blocks are checked structurally but skipped for dominance,
//!   matching the construction that keeps dead user code.
//!
//! [`super::build_function`] runs this verifier in debug builds after every
//! construction.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use super::dom::{compute_idoms, compute_reachable, dominates};
use super::model::{BlockId, SsaFunction, SsaOp, SsaValue, SsaValueId};

/// Verify SSA well-formedness; `Err` carries a human-readable description of
/// the first violation found.
pub fn verify(func: &SsaFunction) -> Result<(), String> {
    if func.blocks.is_empty() {
        return Err("SSA function has no blocks".to_string());
    }
    if func.block(func.entry).is_none() {
        return Err(format!("entry block b{} does not exist", func.entry.0));
    }
    for (index, block) in func.blocks.iter().enumerate() {
        if block.id.0 as usize != index {
            return Err(format!("block at index {index} has id b{}", block.id.0));
        }
    }
    if let Some(entry) = func.block(func.entry) {
        if !entry.preds.is_empty() {
            return Err(format!(
                "entry block b{} must not have predecessors",
                func.entry.0
            ));
        }
    }

    verify_edges(func)?;
    let def_sites = collect_def_sites(func)?;
    verify_phi_shape(func)?;

    let reachable = compute_reachable(func);
    let idoms = compute_idoms(func, &reachable);
    verify_dominance(func, &def_sites, &reachable, &idoms)
}

fn verify_edges(func: &SsaFunction) -> Result<(), String> {
    let mut expected_preds: Vec<Vec<BlockId>> = vec![Vec::new(); func.blocks.len()];
    for block in &func.blocks {
        let targets = block.terminator.targets();
        for target in &targets {
            if func.block(*target).is_none() {
                return Err(format!(
                    "b{}: terminator targets missing block b{}",
                    block.id.0, target.0
                ));
            }
        }
        if block.succs != targets {
            return Err(format!(
                "b{}: successor list {:?} does not match terminator targets {:?}",
                block.id.0, block.succs, targets
            ));
        }
        for target in &targets {
            expected_preds[target.0 as usize].push(block.id);
        }
    }
    for block in &func.blocks {
        let expected = &expected_preds[block.id.0 as usize];
        if &block.preds != expected {
            return Err(format!(
                "b{}: predecessor list {:?} does not match edges {:?} (ascending by id)",
                block.id.0, block.preds, expected
            ));
        }
    }
    Ok(())
}

type DefSites = BTreeMap<SsaValueId, (BlockId, usize)>;

fn collect_def_sites(func: &SsaFunction) -> Result<DefSites, String> {
    let mut def_sites = DefSites::new();
    for block in &func.blocks {
        for (index, stmt) in block.stmts.iter().enumerate() {
            if def_sites.insert(stmt.id, (block.id, index)).is_some() {
                return Err(format!(
                    "b{}[{index}]: duplicate definition of %{}",
                    block.id.0, stmt.id.0
                ));
            }
        }
    }
    Ok(def_sites)
}

fn verify_phi_shape(func: &SsaFunction) -> Result<(), String> {
    for block in &func.blocks {
        let mut seen_non_phi = false;
        for (index, stmt) in block.stmts.iter().enumerate() {
            let SsaOp::Phi(phi) = &stmt.op else {
                seen_non_phi = true;
                continue;
            };
            if seen_non_phi {
                return Err(format!(
                    "b{}[{index}]: phi %{} after a non-phi statement",
                    block.id.0, stmt.id.0
                ));
            }
            if phi.edges.len() != phi.values.len() {
                return Err(format!(
                    "b{}[{index}]: phi %{} has {} edges but {} values",
                    block.id.0,
                    stmt.id.0,
                    phi.edges.len(),
                    phi.values.len()
                ));
            }
            if phi.edges != block.preds {
                return Err(format!(
                    "b{}[{index}]: phi %{} edges {:?} do not match predecessors {:?}",
                    block.id.0, stmt.id.0, phi.edges, block.preds
                ));
            }
        }
    }
    Ok(())
}

fn verify_dominance(
    func: &SsaFunction,
    def_sites: &DefSites,
    reachable: &BTreeSet<BlockId>,
    idoms: &BTreeMap<BlockId, BlockId>,
) -> Result<(), String> {
    for block in &func.blocks {
        if !reachable.contains(&block.id) {
            continue;
        }
        for (index, stmt) in block.stmts.iter().enumerate() {
            if let SsaOp::Phi(phi) = &stmt.op {
                for (edge, value) in phi.edges.iter().zip(&phi.values) {
                    let Some(value) = value else { continue };
                    if !reachable.contains(edge) {
                        continue;
                    }
                    check_phi_operand(func, def_sites, idoms, value, block.id, index, *edge)?;
                }
            } else {
                for operand in stmt.op.operands() {
                    check_operand(func, def_sites, idoms, operand, block.id, index)?;
                }
            }
        }
        for operand in block.terminator.operands() {
            check_operand(func, def_sites, idoms, operand, block.id, block.stmts.len())?;
        }
    }
    Ok(())
}

fn check_operand(
    func: &SsaFunction,
    def_sites: &DefSites,
    idoms: &BTreeMap<BlockId, BlockId>,
    operand: &SsaValue,
    use_block: BlockId,
    use_index: usize,
) -> Result<(), String> {
    match operand {
        SsaValue::Const(_) => Ok(()),
        SsaValue::Argument(index) => {
            if *index < func.params.len() {
                Ok(())
            } else {
                Err(format!(
                    "b{}[{use_index}]: argument index {index} out of range ({} params)",
                    use_block.0,
                    func.params.len()
                ))
            }
        }
        SsaValue::Def(id) => {
            let Some(&(def_block, def_index)) = def_sites.get(id) else {
                return Err(format!(
                    "b{}[{use_index}]: use of unknown def %{}",
                    use_block.0, id.0
                ));
            };
            if def_block == use_block {
                if def_index < use_index {
                    Ok(())
                } else {
                    Err(format!(
                        "b{}[{use_index}]: %{} used before its definition at [{def_index}]",
                        use_block.0, id.0
                    ))
                }
            } else if dominates(def_block, use_block, func.entry, idoms) {
                Ok(())
            } else {
                Err(format!(
                    "b{}[{use_index}]: definition of %{} in b{} does not dominate the use",
                    use_block.0, id.0, def_block.0
                ))
            }
        }
    }
}

fn check_phi_operand(
    func: &SsaFunction,
    def_sites: &DefSites,
    idoms: &BTreeMap<BlockId, BlockId>,
    operand: &SsaValue,
    phi_block: BlockId,
    phi_index: usize,
    edge_pred: BlockId,
) -> Result<(), String> {
    match operand {
        SsaValue::Const(_) => Ok(()),
        SsaValue::Argument(index) => {
            if *index < func.params.len() {
                Ok(())
            } else {
                Err(format!(
                    "b{}[{phi_index}]: phi argument index {index} out of range ({} params)",
                    phi_block.0,
                    func.params.len()
                ))
            }
        }
        SsaValue::Def(id) => {
            let Some(&(def_block, _)) = def_sites.get(id) else {
                return Err(format!(
                    "b{}[{phi_index}]: phi uses unknown def %{}",
                    phi_block.0, id.0
                ));
            };
            // The value is read at the end of the predecessor, so the def
            // must dominate the predecessor block itself.
            if dominates(def_block, edge_pred, func.entry, idoms) {
                Ok(())
            } else {
                Err(format!(
                    "b{}[{phi_index}]: phi operand %{} (defined in b{}) does not dominate \
                     predecessor b{}",
                    phi_block.0, id.0, def_block.0, edge_pred.0
                ))
            }
        }
    }
}

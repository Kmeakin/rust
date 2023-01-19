use rustc_index::bit_set::BitSet;
use rustc_index::vec::IndexVec;
use rustc_middle::mir::visit::*;
use rustc_middle::mir::*;
use rustc_middle::ty::TyCtxt;

use crate::ssa::SsaLocals;
use crate::MirPass;

/// Unify locals that copy each other.
///
/// We consider patterns of the form
///   _a = rvalue
///   _b = move? _a
///   _c = move? _a
///   _d = move? _c
/// where each of the locals is only assigned once.
///
/// We want to replace all those locals by `_a`, either copied or moved.
pub struct CopyProp;

impl<'tcx> MirPass<'tcx> for CopyProp {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        sess.mir_opt_level() >= 4
    }

    #[instrument(level = "trace", skip(self, tcx, body))]
    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        debug!(def_id = ?body.source.def_id());
        propagate_ssa(tcx, body);
    }
}

fn propagate_ssa<'tcx>(tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
    let param_env = tcx.param_env_reveal_all_normalized(body.source.def_id());
    let ssa = SsaLocals::new(tcx, param_env, body);

    let fully_moved = fully_moved_locals(&ssa, body);
    debug!(?fully_moved);

    let mut storage_to_remove = BitSet::new_empty(fully_moved.domain_size());
    for (local, &head) in ssa.copy_classes().iter_enumerated() {
        if local != head {
            storage_to_remove.insert(head);
            storage_to_remove.insert(local);
        }
    }

    let any_replacement = ssa.copy_classes().iter_enumerated().any(|(l, &h)| l != h);

    Replacer { tcx, copy_classes: &ssa.copy_classes(), fully_moved, storage_to_remove }
        .visit_body_preserves_cfg(body);

    if any_replacement {
        crate::simplify::remove_unused_definitions(body);
    }
}

/// `SsaLocals` computed equivalence classes between locals considering copy/move assignments.
///
/// This function also returns whether all the `move?` in the pattern are `move` and not copies.
/// A local which is in the bitset can be replaced by `move _a`. Otherwise, it must be
/// replaced by `copy _a`, as we cannot move multiple times from `_a`.
///
/// If an operand copies `_c`, it must happen before the assignment `_d = _c`, otherwise it is UB.
/// This means that replacing it by a copy of `_a` if ok, since this copy happens before `_c` is
/// moved, and therefore that `_d` is moved.
#[instrument(level = "trace", skip(ssa, body))]
fn fully_moved_locals(ssa: &SsaLocals, body: &Body<'_>) -> BitSet<Local> {
    let mut fully_moved = BitSet::new_filled(body.local_decls.len());

    for (_, rvalue) in ssa.assignments(body) {
        let (Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) | Rvalue::CopyForDeref(place))
            = rvalue
        else { continue };

        let Some(rhs) = place.as_local() else { continue };
        if !ssa.is_ssa(rhs) {
            continue;
        }

        if let Rvalue::Use(Operand::Copy(_)) | Rvalue::CopyForDeref(_) = rvalue {
            fully_moved.remove(rhs);
        }
    }

    ssa.meet_copy_equivalence(&mut fully_moved);

    fully_moved
}

/// Utility to help performing subtitution of `*pattern` by `target`.
struct Replacer<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    fully_moved: BitSet<Local>,
    storage_to_remove: BitSet<Local>,
    copy_classes: &'a IndexVec<Local, Local>,
}

impl<'tcx> MutVisitor<'tcx> for Replacer<'_, 'tcx> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }

    fn visit_local(&mut self, local: &mut Local, _: PlaceContext, _: Location) {
        *local = self.copy_classes[*local];
    }

    fn visit_operand(&mut self, operand: &mut Operand<'tcx>, loc: Location) {
        if let Operand::Move(place) = *operand
            && let Some(local) = place.as_local()
            && !self.fully_moved.contains(local)
        {
            *operand = Operand::Copy(place);
        }
        self.super_operand(operand, loc);
    }

    fn visit_statement(&mut self, stmt: &mut Statement<'tcx>, loc: Location) {
        if let StatementKind::StorageLive(l) | StatementKind::StorageDead(l) = stmt.kind
            && self.storage_to_remove.contains(l)
        {
            stmt.make_nop();
        }
        if let StatementKind::Assign(box (ref place, _)) = stmt.kind
            && let Some(l) = place.as_local()
            && self.copy_classes[l] != l
        {
            stmt.make_nop();
        }
        self.super_statement(stmt, loc);
    }
}

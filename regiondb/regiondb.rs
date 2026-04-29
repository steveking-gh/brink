// Pre-layout region intersection data and validation.
//
// RegionDb::build runs once after IRDb, before the layout iterate loop begins.
// The build function walks irdb.ir_vec in order, tracking section nesting via
// a scope stack, and computes the EffectiveRegion for every section.  Because
// const_eval evaluates region addr and size before IRDb exists, the resulting
// intersections are stable and do not change across layout passes.
//
// Placing this computation before layout (rather than recomputing on every pass)
// correctly signals that region geometry resolves fully before layout begins
// and prevents the layout iterate loop from implying that region data might
// converge over iterations.
//
// Order of operations: build RegionDb after IRDb and before LayoutPhase.
// process.rs calls RegionDb::build, then passes the result to LayoutPhase::build.
//

use diags::Diags;
use ir::{EffectiveRegion, IRKind, RegionBinding};
use irdb::IRDb;
use std::collections::{HashMap, HashSet};

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

/// Computes the intersection of two region bindings. Returns Some(intersection)
/// when the regions overlap, None when disjoint. The intersection name is
/// "{parent} & {direct}" for diagnostics.
fn intersect_regions(parent: &RegionBinding, direct: &RegionBinding) -> Option<RegionBinding> {
    let addr = parent.addr.max(direct.addr);
    let end_p = parent.addr + parent.size;
    let end_d = direct.addr + direct.size;
    let end = end_p.min(end_d);
    if end <= addr {
        return None;
    }
    Some(RegionBinding {
        addr,
        size: end - addr,
        name: format!("{} & {}", parent.name, direct.name),
        src_loc: direct.src_loc.clone(),
    })
}

/// Pre-computed region intersection data, keyed by section name.
/// RegionDb::build produces this once after IRDb; LayoutPhase reads the map for
/// enforcement, and validate_section_regions reads the map for post-convergence
/// size checks.
pub struct RegionDb {
    pub effective_regions: HashMap<String, EffectiveRegion>,
}

impl RegionDb {
    /// Walk irdb.ir_vec once, computing the EffectiveRegion for every section.
    /// Returns None on any error.
    pub fn build(irdb: &IRDb, diags: &mut Diags) -> Option<RegionDb> {
        let mut effective_regions: HashMap<String, EffectiveRegion> = HashMap::new();
        // Scope stack: one entry per active section, innermost last.
        // None means the section has no region constraint.
        let mut scope_stack: Vec<Option<EffectiveRegion>> = Vec::new();
        // Tracks region-bound sections already entered -- detects re-use.
        let mut seen_region_sections: HashSet<String> = HashSet::new();
        let mut ok = true;

        // The only IR elements that matter for region intersection are
        // SectionStart and SectionEnd.
        for ir in &irdb.ir_vec {
            match ir.kind {
                IRKind::SectionStart => {
                    let sec_name = irdb.get_opnd_as_identifier(ir, 0);
                    let parent_effective = scope_stack.last().and_then(|e| e.as_ref());
                    let direct = irdb.region_for_section(sec_name);

                    // Region-bound sections anchor to a fixed address; re-inclusion conflicts.
                    if direct.is_some() && !seen_region_sections.insert(sec_name.to_string()) {
                        let msg = format!(
                            "Section '{}' is bound to a region and cannot be included more \
                             than once.  Region-bound sections anchor to a fixed address; \
                             a second inclusion always produces an address conflict.",
                            sec_name
                        );
                        diags.err1("EXEC_79", &msg, ir.src_loc.clone());
                        ok = false;
                    }

                    // Build contributor list: inherit parent's, then append direct.
                    let mut contributors: Vec<RegionBinding> = parent_effective
                        .map(|e| e.contributors.clone())
                        .unwrap_or_default();
                    if let Some(d) = direct {
                        contributors.push(d.clone());
                    }

                    let effective = match (parent_effective.map(|e| &e.binding), direct) {
                        (None, None) => None,
                        (Some(p), None) => Some(EffectiveRegion {
                            binding: p.clone(),
                            contributors,
                        }),
                        (None, Some(d)) => Some(EffectiveRegion {
                            binding: d.clone(),
                            contributors,
                        }),
                        (Some(p), Some(d)) => {
                            match intersect_regions(p, d) {
                                Some(b) => {
                                    // The section anchors to d.addr; d.addr must fall
                                    // within the parent constraint.
                                    if d.addr < b.addr {
                                        let msg = format!(
                                            "Section '{}': region '{}' starts at {:#X}, which \
                                             is before the enclosing region '{}' start {:#X}. \
                                             The starting address must lie within the \
                                             intersection [{:#X}, {:#X}).",
                                            sec_name,
                                            d.name,
                                            d.addr,
                                            p.name,
                                            p.addr,
                                            b.addr,
                                            b.addr + b.size, // No overflow, bare addition is safe.
                                        );
                                        diags.err2(
                                            "EXEC_78",
                                            &msg,
                                            d.src_loc.clone(),
                                            p.src_loc.clone(),
                                        );
                                        ok = false;
                                    }
                                    Some(EffectiveRegion {
                                        binding: b,
                                        contributors,
                                    })
                                }
                                None => {
                                    // Regions are completely disjoint.
                                    let msg = format!(
                                        "Section '{}': region '{}' [{:#X}, {:#X}) does not \
                                         intersect with enclosing region '{}' [{:#X}, {:#X}).",
                                        sec_name,
                                        d.name,
                                        d.addr,
                                        d.addr + d.size, // No overflow, bare addition is safe.
                                        p.name,
                                        p.addr,
                                        p.addr + p.size, // No overflow, bare addition is safe.
                                    );
                                    diags.err2(
                                        "EXEC_77",
                                        &msg,
                                        d.src_loc.clone(),
                                        p.src_loc.clone(),
                                    );
                                    ok = false;
                                    // Fallback: push direct binding to keep the scope stack
                                    // structurally consistent for child sections.
                                    Some(EffectiveRegion {
                                        binding: d.clone(),
                                        contributors,
                                    })
                                }
                            }
                        }
                    };

                    if let Some(ref e) = effective {
                        effective_regions.insert(sec_name.to_string(), e.clone());
                    }
                    scope_stack.push(effective);
                }

                IRKind::SectionEnd => {
                    scope_stack.pop();
                }

                _ => {}
            }
        }

        if ok {
            Some(RegionDb { effective_regions })
        } else {
            None
        }
    }
}

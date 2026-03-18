// Output map construction for brink.
//
// MapDb collects the semantic payload for all map output formats.
// All data derives from the Engine and IRDb after iteration completes.
// No additional compiler passes run.
//
// Three format functions render MapDb to string output:
//   format_human  — tabular, human-readable
//   format_gnu    — GNU linker memory-map style
//   format_json   — structured JSON
//
// Order of operations: MapDb::new runs after engine.execute() in process.rs.

use engine::Engine;
use ir::ParameterValue;
use irdb::IRDb;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

/// One occurrence of a section write in the output image.
/// A section written N times via `wr` produces N entries.
/// Entries sort by `img_start` (output order).
#[derive(Clone, Debug)]
pub struct SectionEntry {
    pub name: String,
    pub img_start: u64,
    pub abs_start: u64,
    pub size: u64,
}

/// Position of a label in the output image.
/// Entries sort by `img_offset` (output order).
#[derive(Clone, Debug)]
pub struct LabelEntry {
    pub name: String,
    pub img_offset: u64,
    pub abs_addr: u64,
}

/// A resolved const name/value pair.
/// Entries sort alphabetically by name.
#[derive(Clone, Debug)]
pub struct ConstEntry {
    pub name: String,
    pub value: ParameterValue,
}

/// Complete semantic payload of the output map.
#[derive(Clone, Debug)]
pub struct MapDb {
    pub output_file: String,
    pub base_addr: u64,
    pub total_size: u64,
    pub sections: Vec<SectionEntry>,
    pub labels: Vec<LabelEntry>,
    pub consts: Vec<ConstEntry>,
}

impl MapDb {
    /// Constructs a MapDb from the post-iterate engine and irdb.
    /// `output_file` is the path of the output binary, used for display only.
    pub fn new(engine: &Engine, irdb: &IRDb, output_file: &str) -> MapDb {
        let base_addr = engine.start_addr;

        let mut sections: Vec<SectionEntry> = engine
            .wr_dispatches
            .iter()
            .map(|wd| SectionEntry {
                name: wd.name.clone(),
                img_start: wd.img_start,
                abs_start: base_addr + wd.img_start,
                size: wd.size,
            })
            .collect();
        sections.sort_by_key(|s| s.img_start);

        let mut labels: Vec<LabelEntry> = engine
            .label_dispatches
            .iter()
            .map(|ld| LabelEntry {
                name: ld.name.clone(),
                img_offset: ld.img_offset,
                abs_addr: base_addr + ld.img_offset,
            })
            .collect();
        labels.sort_by_key(|l| l.img_offset);

        let mut consts: Vec<ConstEntry> = irdb
            .const_values
            .iter()
            .map(|(name, pv)| ConstEntry {
                name: name.clone(),
                value: pv.clone(),
            })
            .collect();
        consts.sort_by(|a, b| a.name.cmp(&b.name));

        // Total output size: maximum extent of any section (img_start + size).
        let total_size = sections
            .iter()
            .map(|s| s.img_start + s.size)
            .max()
            .unwrap_or(0);

        MapDb {
            output_file: output_file.to_string(),
            base_addr,
            total_size,
            sections,
            labels,
            consts,
        }
    }
}

// Binary generation and extension execution.
//
// ExecPhase forms the final stage of the compiler pipeline.  ExecPhase consumes
// LocationDb and MapDb to construct the output binary file.  Core operations
// include writing inline data, padding bytes, and referenced file contents.
//
// ExecPhase invokes compiler extensions, granting direct memory-mapped write
// access to the binary output.  Extension calls evaluate sequentially after
// core operations complete.

use anyhow::{Result, anyhow};
use diags::{Diags, SourceSpan};
use extension_registry::{ExtensionRegistry, ParamArg, ParamKind};
use ir::{DataType, IR, IRKind, ParameterValue};
use irdb::IRDb;
use locationdb::LocationDb;
use mapdb::MapDb;
use argvaldb::ParmValDb;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Write};

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

type WrittenRanges = BTreeMap<u64, (u64, SourceSpan)>;

fn get_wrx_byte_width(ir: &IR) -> usize {
    match ir.kind {
        IRKind::Wr(w) => w as usize,
        bad => panic!("Called get_wrx_byte_width with {:?}", bad),
    }
}

pub struct ExecPhase {}

impl ExecPhase {
    pub fn execute(
        location_db: &LocationDb,
        argvaldb: &ParmValDb,
        map_db: &MapDb,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
        ext_registry: &ExtensionRegistry,
    ) -> Result<()> {
        trace!("Engine::execute:");
        let mut written_ranges = BTreeMap::new();

        Self::execute_core_operations(
            location_db,
            argvaldb,
            &mut written_ranges,
            irdb,
            diags,
            file,
            ext_registry,
        )?;
        Self::execute_extensions(
            location_db,
            argvaldb,
            map_db,
            irdb,
            diags,
            file,
            ext_registry,
        )?;

        Ok(())
    }

    fn execute_print(
        argvaldb: &ParmValDb,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        _file: &File,
    ) -> Result<()> {
        trace!("Engine::execute_print:");
        if diags.noprint {
            debug!("Suppressing print statements.");
            return Ok(());
        }

        let Some(xstr) = Self::evaluate_string_expr(argvaldb, ir, irdb, diags) else {
            let msg = "Evaluating string expression failed.".to_string();
            diags.err1("EXEC_16", &msg, ir.src_loc.clone());
            return Err(anyhow!("Wrs failed"));
        };
        print!("{}", xstr);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_wrs(
        location_db: &LocationDb,
        argvaldb: &ParmValDb,
        written_ranges: &mut WrittenRanges,
        lid: usize,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
    ) -> Result<()> {
        trace!("Engine::execute_wrs:");
        let Some(xstr) = Self::evaluate_string_expr(argvaldb, ir, irdb, diags) else {
            let msg = "Evaluating string expression failed.".to_string();
            diags.err1("EXEC_15", &msg, ir.src_loc.clone());
            return Err(anyhow!("Wrs failed"));
        };
        let size = xstr.len() as u64;
        let loc = &location_db.ir_locs[lid];
        let addr = loc.addr.addr_base + loc.addr.addr_offset;
        if !Self::check_and_record_range(written_ranges, addr, size, ir.src_loc.clone(), diags) {
            return Err(anyhow!("Address overwrite detected"));
        }

        let bufs = xstr.as_bytes();
        // the map_error lambda just converts io::error to a std::error
        let result = file.write_all(bufs).map_err(|err| err.into());
        if result.is_err() {
            let msg = "Writing string failed".to_string();
            diags.err1("EXEC_3", &msg, ir.src_loc.clone());
        }

        result
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_wrf(
        location_db: &LocationDb,
        argvaldb: &ParmValDb,
        written_ranges: &mut WrittenRanges,
        lid: usize,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
    ) -> Result<()> {
        trace!("Engine::execute_wrf:");

        let path = argvaldb.parms[ir.operands[0]].to_str().to_owned();

        // we already verified this is a legit file path,
        // so unwrap is ok.
        let file_size = irdb.files.get(path.as_str()).unwrap().size;

        let loc = &location_db.ir_locs[lid];
        let addr = loc.addr.addr_base + loc.addr.addr_offset;
        if !Self::check_and_record_range(written_ranges, addr, file_size, ir.src_loc.clone(), diags)
        {
            return Err(anyhow!("Address overwrite detected"));
        }

        let mut source_file = match File::open(path.as_str()) {
            Ok(f) => f,
            Err(err) => {
                let msg = format!(
                    "Opening file '{path}' failed with OS error '{:?}'.",
                    err.raw_os_error()
                );
                diags.err1("EXEC_33", &msg, ir.src_loc.clone());
                return Err(anyhow!(err));
            }
        };

        // read/write in 64K chunks
        // TODO don't hardcode this number
        let mut buf = [0u8; 0x10000];
        let mut total_bytes = 0;
        loop {
            // the map_error lambda just converts io::error to a std::error
            let bytes_read = match source_file.read(&mut buf) {
                Ok(bytes) => bytes,
                Err(err) => {
                    let msg = format!(
                        "Reading file '{path}' failed with OS error '{:?}'.",
                        err.raw_os_error()
                    );
                    diags.err1("EXEC_34", &msg, ir.src_loc.clone());
                    return Err(anyhow!(err));
                }
            };

            total_bytes += bytes_read;
            let write_result = file
                .write_all(&buf[0..bytes_read])
                .map_err(|err| err.into());
            if write_result.is_err() {
                let msg = "Writing buffer failed".to_string();
                diags.err1("EXEC_35", &msg, ir.src_loc.clone());
                return write_result;
            }

            if bytes_read < buf.len() {
                break; // source file is exhausted, nothing more to write
            }
        }

        assert!(total_bytes as u64 == file_size);

        Ok(())
    }

    fn execute_wrx(
        location_db: &LocationDb,
        argvaldb: &ParmValDb,
        written_ranges: &mut WrittenRanges,
        lid: usize,
        ir: &IR,
        diags: &mut Diags,
        file: &mut File,
    ) -> Result<()> {
        trace!("{}", format!("Engine::execute_wrx: {:?}", ir.kind).as_str());
        let byte_size = get_wrx_byte_width(ir);

        let opnd_num = ir.operands[0];
        trace!(
            "{}",
            format!("engine::execute_wrx: checking operand {}", opnd_num).as_str()
        );
        let parm = &argvaldb.parms[opnd_num];

        // Extract bytes as little-endian.  One a big-endian machine, the LSB will
        // bit the highest address location, which is wrong since we're writing
        // from the lowest address.
        let buf = match parm.data_type() {
            DataType::Integer | DataType::I64 => {
                let val = parm.to_i64();
                val.to_le_bytes()
            }
            DataType::U64 => {
                let val = parm.to_u64();
                val.to_le_bytes()
            }
            bad => {
                panic!("Unexpected parameter type {:?} in execute_wrx", bad);
            }
        };

        let mut repeat_count = 1;

        if ir.operands.len() == 2 {
            // Yes, we have a repeat count
            // We already validated the operands in IRDB.
            let repeat_opnd_num = ir.operands[1];
            let op = &argvaldb.parms[repeat_opnd_num];
            repeat_count = op.to_u64();
        }

        trace!("{}", format!("Repeat count = {}", repeat_count).as_str());
        let total_size = (byte_size as u64) * repeat_count;
        let loc = &location_db.ir_locs[lid];
        let addr = loc.addr.addr_base + loc.addr.addr_offset;
        if !Self::check_and_record_range(
            written_ranges,
            addr,
            total_size,
            ir.src_loc.clone(),
            diags,
        ) {
            return Err(anyhow!("Address overwrite detected"));
        }

        // The map_error lambda just converts io::error to a std::error
        // Write only the number of bytes required for the width of the wrx
        while repeat_count > 0 {
            let result = file.write_all(&buf[0..byte_size]).map_err(|err| err.into());
            if result.is_err() {
                let msg = format!("{:?} failed", ir.kind);
                diags.err1("EXEC_18", &msg, ir.src_loc.clone());
                return result;
            }
            repeat_count -= 1;
        }

        Ok(())
    }

    fn execute_core_operations(
        location_db: &LocationDb,
        argvaldb: &ParmValDb,
        written_ranges: &mut WrittenRanges,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
        ext_registry: &ExtensionRegistry,
    ) -> Result<()> {
        trace!("Engine::execute_core_operations:");
        let mut result;
        let mut error_count = 0;
        for (lid, ir) in irdb.ir_vec.iter().enumerate() {
            result = match ir.kind {
                IRKind::Wr(_) => Self::execute_wrx(location_db, argvaldb, written_ranges, lid, ir, diags, file),
                IRKind::Print => Self::execute_print(argvaldb, ir, irdb, diags, file),
                IRKind::Wrs => Self::execute_wrs(location_db, argvaldb, written_ranges, lid, ir, irdb, diags, file),
                IRKind::Wrf => Self::execute_wrf(location_db, argvaldb, written_ranges, lid, ir, irdb, diags, file),
                // Assert evaluates in the validation phase, before byte generation.
                IRKind::Assert => Ok(()),
                // the rest of these operations are computed during iteration
                IRKind::SetSecOffset
                | IRKind::SetAddrOffset
                | IRKind::SetAddr
                | IRKind::SetFileOffset
                | IRKind::Align
                | IRKind::Addr
                | IRKind::Const
                | IRKind::AddrOffset
                | IRKind::SecOffset
                | IRKind::FileOffset
                | IRKind::Label
                | IRKind::Sizeof
                | IRKind::SizeofExt
                | IRKind::BuiltinOutputSize
                | IRKind::BuiltinOutputAddr
                | IRKind::BuiltinVersionString
                | IRKind::BuiltinVersionMajor
                | IRKind::BuiltinVersionMinor
                | IRKind::BuiltinVersionPatch
                | IRKind::ToI64
                | IRKind::ToU64
                | IRKind::Eq
                | IRKind::NEq
                | IRKind::GEq
                | IRKind::LEq
                | IRKind::Gt
                | IRKind::Lt
                | IRKind::DoubleEq
                | IRKind::I64
                | IRKind::U64
                | IRKind::BitAnd
                | IRKind::LogicalAnd
                | IRKind::BitOr
                | IRKind::LogicalOr
                | IRKind::Multiply
                | IRKind::Modulo
                | IRKind::Divide
                | IRKind::Add
                | IRKind::Subtract
                | IRKind::SectionStart
                | IRKind::SectionEnd
                | IRKind::LeftShift
                | IRKind::RightShift
                // if/else IR only lives in const_ir_vec; never reaches the engine.
                | IRKind::ConstDeclare
                | IRKind::IfBegin
                | IRKind::ElseBegin
                | IRKind::IfEnd
                | IRKind::BareAssign => Ok(()),
                IRKind::ExtensionCall => {
                    // Pre-pad zeroed bytes to expand the output file to cover
                    // the extension's output region before memory mapping.
                    let ext_name = argvaldb.parms[ir.operands[0]].identifier_to_str();
                    if let Some(entry) = ext_registry.get(ext_name) {
                        let buf = vec![0u8; entry.cached_size];
                        file.write_all(&buf).map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to pre-pad space for extension '{}': {}",
                                ext_name,
                                e
                            )
                        })
                    } else {
                        Ok(())
                    }
                }
            };

            if result.is_err() {
                error_count += 1;
                if error_count > 10 {
                    // todo parameterize max 10 errors
                    break;
                }
            }
        }

        if error_count > 0 {
            return Err(anyhow!("Error detected"));
        }
        Ok(())
    }

    fn execute_extensions(
        location_db: &LocationDb,
        argvaldb: &ParmValDb,
        map_db: &MapDb,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
        ext_registry: &ExtensionRegistry,
    ) -> Result<()> {
        trace!("Engine::execute_extensions:");

        // Scope extraction: we isolate ONLY the extension calls
        // decoupled from the core pipeline logic.
        let mut extension_nodes = Vec::new();
        for (idx, ir) in irdb.ir_vec.iter().enumerate() {
            if ir.kind == IRKind::ExtensionCall {
                extension_nodes.push(idx);
            }
        }

        if extension_nodes.is_empty() {
            return Ok(());
        }

        use memmap2::MmapOptions;

        // We memory-map the output file to allow extensions zero-copy access to the fully generated image,
        // and allow zero-copy patching of the extension's execution output without re-reading from disk.
        // We synchronize the file first to ensure the OS sees all written data/padding before mapping.
        if let Err(e) = file.sync_all() {
            return Err(anyhow!(
                "Failed to sync output file before memory mapping: {}",
                e
            ));
        }

        // This is the only bit of unsafe code in brink.
        let mut mmap = match unsafe { MmapOptions::new().map_mut(&*file) } {
            Ok(m) => m,
            Err(e) => return Err(anyhow!("Failed to memory map output file: {}", e)),
        };

        // Maps each section name to a list of indices into wr_dispatches.
        // Slice param resolution uses the map to check for ambiguity and to
        // resolve file_offset in O(1) instead of two linear scans per call.
        let mut sec_dispatch_map: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, d) in map_db.sections.iter().enumerate() {
            sec_dispatch_map.entry(d.name.as_str()).or_default().push(i);
        }

        // Sequentially execute extensions and patch the output file.
        let mut error_count = 0;
        for &idx in &extension_nodes {
            let ir = &irdb.ir_vec[idx];
            let name = argvaldb.parms[ir.operands[0]].identifier_to_str();

            let Some(entry) = ext_registry.get(name) else {
                unreachable!("Extension '{}' not found in registry", name);
            };
            let byte_width = entry.cached_size;

            // Build the ParamArg list and call the extension.
            //
            // All extension calls use IRKind::ExtensionCall with operand layout:
            //   [name, user_arg0..., output]
            //
            // The trailing output operand is a type-checking placeholder and
            // must not be passed to the extension.  last = len-1 excludes it.
            //
            // When the extension declares params() (cached_params non-empty), the
            // engine resolves Slice-kinded params to ParamArg::Slice and passes
            // remaining params as ParamArg::Int or ParamArg::Str.  Operands arrive in
            // declaration order (irdb canonicalized them).
            //
            // When cached_params is empty (legacy opt-out), the engine applies the
            // old heuristic: if the first user arg is an Identifier that names a
            // known section, resolve it to ParamArg::Slice.
            //
            // ParamArg::Slice holds &mmap[..], an immutable borrow.  Pre-resolve
            // all section lookups before that scope so error handling can use `continue`.
            let last = ir.operands.len() - 1;
            let cached_params = &entry.cached_params;

            // Per-param section resolutions, indexed parallel to cached_params.
            // Each entry is Some((file_offset, size, slice_start, slice_end)) for
            // Slice params, or None for Int/Str params.
            // For the legacy path a single entry covers user arg 0.
            let mut resolved_sections: Vec<Option<(u64, u64, usize, usize)>> = Vec::new();
            let mut section_resolve_failed = false;

            if cached_params.is_empty() {
                // Legacy heuristic: if user arg 0 is an Identifier matching a section,
                // resolve it to ParamArg::Slice.
                if last > 1 {
                    if let ParameterValue::Identifier(ref sec_name) =
                        argvaldb.parms[ir.operands[1]]
                    {
                        let indices = sec_dispatch_map
                            .get(sec_name.as_str())
                            .map(Vec::as_slice)
                            .unwrap_or(&[]);
                        if indices.len() > 1 {
                            diags.err1(
                                "EXEC_56",
                                &format!(
                                    "Section '{}' appears {} times in the output; \
                                     section-name form is ambiguous. Wrap with a unique \
                                     section name or use `wr {}(section_name)` on a single \
                                     occurrence.",
                                    sec_name,
                                    indices.len(),
                                    name,
                                ),
                                ir.src_loc.clone(),
                            );
                            error_count += 1;
                            section_resolve_failed = true;
                        } else if let Some(&di) = indices.first() {
                            let d = &map_db.sections[di];
                            let start = d.file_offset as usize;
                            resolved_sections.push(Some((
                                d.file_offset,
                                d.size,
                                start,
                                start + d.size as usize,
                            )));
                        } else {
                            resolved_sections.push(None);
                        }
                    } else {
                        resolved_sections.push(None);
                    }
                }
            } else {
                // Named-arg/positional path: resolve each Slice param.
                // Operands arrive in declaration order after irdb canonicalization.
                for (i, p) in cached_params.iter().enumerate() {
                    if p.kind == ParamKind::Slice {
                        let sec_name = argvaldb.parms[ir.operands[1 + i]]
                            .identifier_to_str()
                            .to_string();
                        let indices = sec_dispatch_map
                            .get(sec_name.as_str())
                            .map(Vec::as_slice)
                            .unwrap_or(&[]);
                        if indices.len() > 1 {
                            diags.err1(
                                "EXEC_56",
                                &format!(
                                    "Extension '{}': section '{}' for parameter '{}' appears {} \
                                     times in the output and is ambiguous.",
                                    name,
                                    sec_name,
                                    p.name,
                                    indices.len(),
                                ),
                                ir.src_loc.clone(),
                            );
                            error_count += 1;
                            section_resolve_failed = true;
                            break;
                        }
                        let Some(&di) = indices.first() else {
                            return Err(anyhow!(
                                "Extension '{}': section '{}' for parameter '{}' not found \
                                 in dispatch table. This is a compiler bug.",
                                name,
                                sec_name,
                                p.name
                            ));
                        };
                        let d = &map_db.sections[di];
                        let start = d.file_offset as usize;
                        resolved_sections.push(Some((
                            d.file_offset,
                            d.size,
                            start,
                            start + d.size as usize,
                        )));
                    } else {
                        resolved_sections.push(None);
                    }
                }
            }

            if section_resolve_failed {
                continue;
            }

            // Build ext_args in a scope that isolates the immutable mmap borrow
            // held by ParamArg::Slice.  The scope produces only an owned Vec<u8>,
            // so the borrow drops before the mutable patch write below.
            let exec_result: Result<Vec<u8>, String> = {
                let mut ext_args: Vec<ParamArg<'_>> = Vec::new();

                if cached_params.is_empty() {
                    // Legacy path: section at user arg 0 (if any), then remaining args.
                    let user_arg_start = if let Some(Some((_file_offset, _len, start, end))) =
                        resolved_sections.first().copied()
                    {
                        ext_args.push(ParamArg::Slice {
                            data: &mmap[start..end],
                        });
                        2
                    } else {
                        1
                    };
                    for &op in &ir.operands[user_arg_start..last] {
                        let parm = &argvaldb.parms[op];
                        let arg = match parm {
                            ParameterValue::U64(v) => ParamArg::Int(*v),
                            ParameterValue::I64(v) | ParameterValue::Integer(v) => {
                                ParamArg::Int(*v as u64)
                            }
                            ParameterValue::QuotedString(s) => ParamArg::Str(s.as_str()),
                            _ => {
                                unreachable!("unexpected extension arg type {:?}", parm.data_type())
                            }
                        };
                        ext_args.push(arg);
                    }
                } else {
                    // Named-arg/positional path: build args from declared params in order.
                    for (i, p) in cached_params.iter().enumerate() {
                        if p.kind == ParamKind::Slice {
                            if let Some((_file_offset, _len, start, end)) = resolved_sections[i] {
                                ext_args.push(ParamArg::Slice {
                                    data: &mmap[start..end],
                                });
                            }
                        } else {
                            let parm = &argvaldb.parms[ir.operands[1 + i]];
                            let arg = match parm {
                                ParameterValue::U64(v) => ParamArg::Int(*v),
                                ParameterValue::I64(v) | ParameterValue::Integer(v) => {
                                    ParamArg::Int(*v as u64)
                                }
                                ParameterValue::QuotedString(s) => ParamArg::Str(s.as_str()),
                                _ => unreachable!(
                                    "unexpected extension arg type {:?}",
                                    parm.data_type()
                                ),
                            };
                            ext_args.push(arg);
                        }
                    }
                }

                let mut out = vec![0u8; byte_width];
                entry.extension.execute(&ext_args, &mut out).map(|_| out)
            };

            let out_buffer = match exec_result {
                Ok(buf) => buf,
                Err(e) => {
                    let msg = format!("Extension '{}' execution failed: {}", name, e);
                    diags.err1("EXEC_47", &msg, ir.src_loc.clone());
                    return Err(anyhow!(msg));
                }
            };

            // Patch the file at the exact image offset of this extension call.
            // ir_locs[idx] holds the file offset before this IR executes, which
            // is the start of the zeroed placeholder written during generate.
            let loc = &location_db.ir_locs[idx];
            let abs_offset = loc.file_offset as usize;

            if abs_offset + byte_width > mmap.len() {
                return Err(anyhow!(
                    "Extension bounded write exceeds file bounds. This is a severe compiler bug."
                ));
            }

            // Zero-copy assignment back into OS memory map.
            mmap[abs_offset..abs_offset + byte_width].copy_from_slice(&out_buffer);
        }

        if error_count > 0 {
            return Err(anyhow!("Error detected in extension execution"));
        }

        if let Err(e) = mmap.flush() {
            return Err(anyhow!("Failed to flush memory-mapped file patches: {}", e));
        }

        Ok(())
    }

    fn check_and_record_range(
        written_ranges: &mut WrittenRanges,
        start: u64,
        size: u64,
        src_loc: SourceSpan,
        diags: &mut Diags,
    ) -> bool {
        if size == 0 {
            return true;
        }
        let end = start + size; // safe: callers already checked for overflow

        // Check against the range that starts at or just before `start`.
        let overlap = written_ranges
            .range(..=start)
            .next_back()
            .filter(|(_, (prev_end, _))| *prev_end > start)
            .map(|(&prev_start, (prev_end, prev_loc))| (prev_start, *prev_end, prev_loc.clone()))
            .or_else(|| {
                // Also check the range that starts strictly after `start` ???
                // it overlaps if its start is less than our end.
                written_ranges
                    .range(start + 1..)
                    .next()
                    .filter(|&(&next_start, _)| next_start < end)
                    .map(|(&next_start, (next_end, next_loc))| {
                        (next_start, *next_end, next_loc.clone())
                    })
            });

        written_ranges.insert(start, (end, src_loc.clone()));

        if let Some((prev_start, prev_end, prev_loc)) = overlap {
            let msg = format!(
                "Address range {:#x}..{:#x} overlaps previously written range {:#x}..{:#x}",
                start, end, prev_start, prev_end,
            );
            diags.err2("EXEC_55", &msg, src_loc, prev_loc);
            return false;
        }

        true
    }

    fn evaluate_string_expr(
        argvaldb: &ParmValDb,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
    ) -> Option<String> {
        let mut result = true;
        let mut xstr = String::new();
        for (local_op_num, &op_num) in ir.operands.iter().enumerate() {
            let op = &argvaldb.parms[op_num];
            debug!(
                "Processing string expr operand {} with data type {:?}",
                local_op_num,
                op.data_type()
            );
            match op.data_type() {
                DataType::QuotedString => {
                    xstr.push_str(op.to_str());
                }
                DataType::U64 => {
                    xstr.push_str(format!("{:#X}", op.to_u64()).as_str());
                }
                DataType::Integer | DataType::I64 => {
                    xstr.push_str(format!("{}", op.to_i64()).as_str());
                }
                bad => {
                    let msg = format!("Cannot stringify type '{:?}'", bad);
                    let src_loc = irdb.parms[op_num].src_loc.clone();
                    diags.err1("EXEC_67", &msg, src_loc);
                    result = false;
                }
            }
        }

        // If stringifying succeeded, return the String
        if result { Some(xstr) } else { None }
    }

}

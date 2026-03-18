// Top-level pipeline orchestrator for brink.
//
// The process function is the single entry point that drives the entire
// compiler pipeline.  It sequences the four stages in order — Ast, LinearDb,
// IRDb and Engine — passing each stage's output as input to the next, and
// converting any stage-level Err(()) result into an anyhow error so that the
// caller receives a descriptive failure message.  It also handles the output
// file name, creating the file before handing it to Engine for writing.
//
// Order of operations: process.rs sits above all four pipeline stages.
// main.rs calls process() once per invocation after reading the source file.

use anyhow::{Context, Result, anyhow};
use std::fs::File;
use std::io::Write;

// Local libraries
use ast::{Ast, AstDb};
use diags::Diags;
use engine::Engine;
use irdb::IRDb;
use lineardb::LinearDb;
use map::{MapDb, format_human};

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

/// Entry point for all processing on the input source file.
/// `name`        — source file path
/// `fstr`        — source file contents
/// `output_file` — binary output path (default: "output.bin")
/// `verbosity`   — log level (0 = quiet, 1 = default, 2+ = verbose)
/// `noprint`     — suppress print statements in source
/// `map_hf`      — human-friendly map destination: None = skip,
///                 Some("-") = stdout, Some(path) = file
pub fn process(
    name: &str,
    fstr: &str,
    output_file: Option<&str>,
    verbosity: u64,
    noprint: bool,
    map_hf: Option<&str>,
) -> Result<()> {
    info!("Processing {}", name);
    debug!("File contains: {}", fstr);

    let mut diags = Diags::new(name, fstr, verbosity, noprint);

    let ast =
        Ast::new(fstr, &mut diags).map_err(|_| anyhow!("[PROC_1]: Error detected, halting."))?;

    if verbosity > 2 {
        ast.dump("ast.dot")?;
    }

    let ast_db = AstDb::new(&mut diags, &ast)?;
    let linear_db = LinearDb::new(&mut diags, &ast, &ast_db)
        .map_err(|_| anyhow!("[PROC_2]: Error detected, halting."))?;
    if verbosity > 2 {
        linear_db.dump();
    }

    let ir_db = IRDb::new(&linear_db, &mut diags)
        .map_err(|_| anyhow!("[PROC_3]: Error detected, halting."))?;

    debug!("Dumping ir_db");
    if verbosity > 2 {
        ir_db.dump();
    }

    let engine = Engine::new(&ir_db, &mut diags, 0)
        .map_err(|_| anyhow!("[PROC_5]: Error detected, halting."))?;
    if verbosity > 2 {
        engine.dump_locations();
    }
    // Determine if the user specified an output file on the command line
    // Trim whitespace
    let fname_str = String::from(output_file.unwrap_or("output.bin").trim_matches(' '));
    debug!("process: output file name is {}", fname_str);

    let mut file =
        File::create(&fname_str).context(format!("Unable to create output file {}", fname_str))?;

    if engine.execute(&ir_db, &mut diags, &mut file).is_err() {
        return Err(anyhow!("[PROC_4]: Error detected, halting."));
    }

    // Generate map output if requested.  MapDb derives all data from the
    // post-iterate engine and irdb; no additional compiler passes run.
    if map_hf.is_some() || false /* reserved for map_gnu / map_json */ {
        let map_db = MapDb::new(&engine, &ir_db, &fname_str);
        emit_map(map_hf, &format_human(&map_db))?;
    }
    Ok(())
}

/// Writes `content` to stdout when `dest` is `Some("-")`, or to the named
/// file when `dest` is `Some(path)`.  Does nothing when `dest` is `None`.
fn emit_map(dest: Option<&str>, content: &str) -> Result<()> {
    match dest {
        None => {}
        Some("-") => print!("{content}"),
        Some(path) => {
            let mut f = File::create(path)
                .context(format!("Unable to create map file {path}"))?;
            f.write_all(content.as_bytes())
                .context(format!("Unable to write map file {path}"))?;
        }
    }
    Ok(())
}

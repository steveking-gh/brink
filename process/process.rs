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

// Local libraries
use ast::{Ast, AstDb};
use diags::Diags;
use engine::Engine;
use irdb::IRDb;
use lineardb::LinearDb;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

/// Entry point for all processing on the input source file
/// name: The name of the file
/// fstr: A string containing the file
pub fn process(
    name: &str,
    fstr: &str,
    output_file: Option<&str>,
    verbosity: u64,
    noprint: bool,
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
    Ok(())
}

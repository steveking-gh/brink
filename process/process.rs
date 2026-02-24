use anyhow::{Context, Result, anyhow};
use std::fs::File;
extern crate clap;

// Local libraries
use ast::{Ast, AstDb};
use diags::Diags;
use engine::Engine;
use irdb::IRDb;
use lineardb::LinearDb;

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

/// Entry point for all processing on the input source file
/// name: The name of the file
/// fstr: A string containing the file
pub fn process(
    name: &str,
    fstr: &str,
    args: &clap::ArgMatches,
    verbosity: u64,
    noprint: bool,
) -> Result<()> {
    info!("Processing {}", name);
    debug!("File contains: {}", fstr);

    let mut diags = Diags::new(name, fstr, verbosity, noprint);

    let ast = Ast::new(fstr, &mut diags);
    if ast.is_none() {
        return Err(anyhow!("[PROC_1]: Error detected, halting."));
    }

    let ast = ast.unwrap();

    if verbosity > 2 {
        ast.dump("ast.dot")?;
    }

    let ast_db = AstDb::new(&mut diags, &ast)?;
    let linear_db = LinearDb::new(&mut diags, &ast, &ast_db);
    if linear_db.is_none() {
        return Err(anyhow!("[PROC_2]: Error detected, halting."));
    }
    let linear_db = linear_db.unwrap();
    if verbosity > 2 {
        linear_db.dump();
    }
    let ir_db = IRDb::new(&linear_db, &mut diags);
    if ir_db.is_none() {
        return Err(anyhow!("[PROC_3]: Error detected, halting."));
    }
    let ir_db = ir_db.unwrap();

    debug!("Dumping ir_db");
    if verbosity > 2 {
        ir_db.dump();
    }

    let engine = Engine::new(&ir_db, &mut diags, 0);
    if engine.is_none() {
        return Err(anyhow!("[PROC_5]: Error detected, halting."));
    }

    let engine = engine.unwrap();
    if verbosity > 2 {
        engine.dump_locations();
    }
    // Determine if the user specified an output file on the command line
    // Trim whitespace
    let fname_str = String::from(
        args.value_of("output")
            .unwrap_or("output.bin")
            .trim_matches(' '),
    );
    debug!("process: output file name is {}", fname_str);

    let mut file =
        File::create(&fname_str).context(format!("Unable to create output file {}", fname_str))?;

    if engine.execute(&ir_db, &mut diags, &mut file).is_err() {
        return Err(anyhow!("[PROC_4]: Error detected, halting."));
    }
    Ok(())
}

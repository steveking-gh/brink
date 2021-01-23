use std::fs::File;
use anyhow::{Result,Context,anyhow};
extern crate clap;

// Local libraries
use diags::Diags;
use ast::{Ast,AstDb};
use lineardb::LinearDb;
use irdb::IRDb;
use engine::Engine;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

/// Entry point for all processing on the input source file
/// name: The name of the file
/// fstr: A string containing the file
pub fn process(name: &str, fstr: &str, args: &clap::ArgMatches, verbosity: u64)
               -> Result<()> {
    info!("Processing {}", name);
    debug!("File contains: {}", fstr);

    let mut diags = Diags::new(name,fstr,verbosity);

    let ast = Ast::new(fstr, &mut diags);
    if ast.is_none() {
        return Err(anyhow!("[PROC_1]: Failed to construct the abstract syntax tree."));
    }

    let ast = ast.unwrap();

    ast.dump("ast.dot")?;

    let ast_db = AstDb::new(&mut diags, &ast)?;
    let linear_db = LinearDb::new(&mut diags, &ast, &ast_db);
    if linear_db.is_none() {
        return Err(anyhow!("[PROC_2]: Failed to construct the linear database."));
    }
    let linear_db = linear_db.unwrap();
    linear_db.dump();
    let ir_db = IRDb::new(&linear_db, &mut diags);
    if ir_db.is_none() {
        return Err(anyhow!("[PROC_3]: Failed to construct the IR database."));
    }
    let ir_db = ir_db.unwrap();

    debug!("Dumping ir_db");
    ir_db.dump();

    let engine = Engine::new(&ir_db, &mut diags, 0);

    // Determine if the user specified an output file on the command line
    // Trim whitespace
    let fname_str = String::from(args.value_of("output")
                                            .unwrap_or("output.bin")
                                            .trim_matches(' '));
    debug!("process: output file name is {}", fname_str);

    let mut file = File::create(&fname_str)
            .context(format!("Unable to create output file {}", fname_str))?;

    if engine.execute(&ir_db, &mut diags, &mut file).is_err() {
        return Err(anyhow!("[PROC_4]: output file creation failed."));
    }
    Ok(())
}
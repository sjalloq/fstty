//! Simple example to test wellen's hierarchy loading
//!
//! Usage: cargo run -p fstty-core --example load_hierarchy -- <path_to_fst_file>

use std::env;
use std::time::Instant;

use wellen::{viewers, LoadOptions};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <waveform_file>", args[0]);
        std::process::exit(1);
    }

    let filename = &args[1];
    println!("Loading header from: {}", filename);

    // Time the header load
    let start = Instant::now();

    let load_opts = LoadOptions {
        multi_thread: false,  // Single-threaded
        remove_scopes_with_empty_name: false,
    };
    let header = viewers::read_header_from_file(filename, &load_opts)?;

    let elapsed = start.elapsed();
    println!("Header loaded in {:.2?}", elapsed);

    let hierarchy = &header.hierarchy;

    // Print some stats
    println!("\n=== Hierarchy Stats ===");
    println!("File format: {:?}", header.file_format);
    println!("Body length: {} bytes", header.body_len);
    println!("Unique signals: {}", hierarchy.num_unique_signals());

    // Print first 10 top-level scopes
    println!("\n=== Top-level Scopes (first 10) ===");
    for (i, scope_ref) in hierarchy.scopes().take(10).enumerate() {
        let scope = &hierarchy[scope_ref];
        println!("  {}. {} ({:?})", i + 1, scope.name(hierarchy), scope.scope_type());
    }

    // Print first 10 top-level variables
    println!("\n=== Top-level Variables (first 10) ===");
    for (i, var_ref) in hierarchy.vars().take(10).enumerate() {
        let var = &hierarchy[var_ref];
        println!("  {}. {} ({:?}, {:?})",
            i + 1,
            var.name(hierarchy),
            var.var_type(),
            var.length()
        );
    }

    // Count total scopes and vars
    println!("\n=== Counting (may be slow for large files) ===");
    let count_start = Instant::now();
    let scope_count = hierarchy.iter_scopes().count();
    let var_count = hierarchy.iter_vars().count();
    println!("Total scopes: {} (counted in {:.2?})", scope_count, count_start.elapsed());
    println!("Total variables: {}", var_count);

    // Walk into first scope's child scopes
    if let Some(first_scope_ref) = hierarchy.scopes().next() {
        let first_scope = &hierarchy[first_scope_ref];
        println!("\n=== First scope: {} ===", first_scope.full_name(hierarchy));

        println!("Child scopes (first 10):");
        for (i, child_ref) in first_scope.scopes(hierarchy).take(10).enumerate() {
            let child = &hierarchy[child_ref];
            println!("  {}. [Scope] {}", i + 1, child.name(hierarchy));
        }

        println!("Child vars (first 10):");
        for (i, var_ref) in first_scope.vars(hierarchy).take(10).enumerate() {
            let var = &hierarchy[var_ref];
            println!("  {}. [Var] {} ({:?})", i + 1, var.name(hierarchy), var.length());
        }
    }

    Ok(())
}

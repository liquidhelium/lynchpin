use clap::Parser as _;
use tinymist_world::{args::CompileOnceArgs, print_diagnostics};

fn main() {
    let args = CompileOnceArgs::parse();
    let universe = args.resolve_system().expect("failed to resolve universe");
    let world = universe.snapshot();
    let result = lynchpin::compile_ng::compile(&world);
    if let Err(e2) = print_diagnostics(
        &world,
        result.warnings.iter(),
        tinymist_world::DiagnosticFormat::Human,
    ) {
        println!("Error: Cannot print diagnostics: {}", e2)
    }
    let doc = match result.output {
        Ok(doc) => doc,
        Err(e) => {
            let _ = print_diagnostics(&world, e.iter(), tinymist_world::DiagnosticFormat::Human);
            return;
        }
    };
    for page in &doc {
        eprintln!("DEBUG: page inner items: {}", page.inner.items().len());
        for (pos, item) in page.inner.items() {
            eprintln!("DEBUG:   item at ({:?}, {:?}): {:?}", pos.col, pos.row, std::mem::discriminant(item));
        }
        let grid = page.inner.render();
        let ansi = grid.to_ansi();
        eprintln!("DEBUG: ansi output len: {}", ansi.len());
        print!("{}", ansi);
    }
}

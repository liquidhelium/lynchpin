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
        let grid = page.inner.render();
        print!("{}", grid.to_ansi());
    }
}

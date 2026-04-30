use clap::Parser as _;
use tinymist_world::{args::CompileOnceArgs, print_diagnostics};
use typst::layout::PagedDocument;
use typterm::transform;

fn main() {
    let args = CompileOnceArgs::parse();
    let universe = args.resolve_system().expect("failed to resolve universe");
    let world = universe.snapshot();
    let result = typst::compile::<PagedDocument>(&world);
    if let Err(e2) = print_diagnostics(&world, result.warnings, tinymist_world::DiagnosticFormat::Human) {
        println!("Error: Cannot print diagnostics: {}", e2)
    }
    let s = match result.output {
        Ok(pages) => transform(&pages),
        Err(e) => {
            if let Err(e2) = print_diagnostics(&world, e, tinymist_world::DiagnosticFormat::Human) {
                println!("Error: Cannot print diagnostics: {}", e2)
            }
            return
        }
    };
    print!("{s}")
}

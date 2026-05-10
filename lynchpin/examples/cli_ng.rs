use clap::Parser as _;
use tinymist_world::{args::CompileOnceArgs, print_diagnostics};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")))
        .with_writer(std::io::stderr)
        .init();
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

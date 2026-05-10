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
        // Debug: inspect page frame structure BEFORE rendering
        {
            use lynchpin_library_ng::{TermFrameItem, TermScalar};
            eprintln!("Page frame: size={}x{} items={}",
                page.inner.cols().get(), page.inner.rows().get(), page.inner.items().len());
            fn dump_frame(frame: &lynchpin_library_ng::TermFrame, indent: usize) {
                let prefix = "  ".repeat(indent);
                for (pos, item) in frame.items() {
                    match item {
                        TermFrameItem::Text(t, _) => {
                            eprintln!("{prefix}text '{}' at ({},{}) size={}x{}", t, pos.col.get(), pos.row.get(), frame.cols().get(), frame.rows().get());
                        }
                        TermFrameItem::Frame(f) => {
                            eprintln!("{prefix}subframe size={}x{} at ({},{}) parent_size={}x{}",
                                f.cols().get(), f.rows().get(), pos.col.get(), pos.row.get(),
                                frame.cols().get(), frame.rows().get());
                            dump_frame(f, indent + 1);
                        }
                        _ => eprintln!("{prefix}other at ({},{})", pos.col.get(), pos.row.get()),
                    }
                }
            }
            dump_frame(&page.inner, 0);
        }
        let grid = page.inner.render();
        // Also dump grid to compare - check specific positions
        {
            use lynchpin_library_ng::TermScalar;
            eprintln!("Grid dump: size={}x{}", grid.cols.get(), grid.rows.get());
            // Check where delimiters actually are
            for r in 0..grid.rows.get() {
                eprint!("  row {}:", r);
                for c in 0..grid.cols.get() {
                    let cell = grid.get(TermScalar::new(c), TermScalar::new(r));
                    if !cell.is_blank() {
                        eprint!(" ({},{})='{}'", c, r, cell.ch);
                    }
                }
                eprintln!();
            }
            // Specifically check (1,0) and (5,0)
            let c1 = grid.get(TermScalar::new(1), TermScalar::new(0));
            let c5 = grid.get(TermScalar::new(5), TermScalar::new(0));
            let c28 = grid.get(TermScalar::new(28), TermScalar::new(0));
            eprintln!("  check (1,0): ch='{}' is_blank={}", c1.ch, c1.is_blank());
            eprintln!("  check (5,0): ch='{}' is_blank={}", c5.ch, c5.is_blank());
            eprintln!("  check (28,0): ch='{}' is_blank={}", c28.ch, c28.is_blank());
            // Check raw cells using the new debug method
            for i in 0..12.min(grid.cell_count()) {
                let cell = grid.cell_at(i);
                if !cell.is_blank() {
                    eprintln!("  cells[{}] = '{}'", i, cell.ch);
                }
            }
        }
        print!("{}", grid.to_ansi());
    }
}

//! Composed large operators for display-mode math.
//!
//! When a large operator (Σ, ∫, Π, …) appears in display mode, we construct
//! a multi-row multi-column rendered form from component characters rather
//! than using the single Unicode codepoint.
//!
//! The design follows the Diagon / Math.cpp approach:
//!
//! * **Sum** (Σ):  `╲───╱` + top bar + bottom bar, proportional to content width
//! * **Integral** (∫):  `⌠│⌡` stretched to match content height
//! * **Product** (Π):  `┃┳┃` + top bar + bottom bar

use crossterm::style::ContentStyle;
use ecow::EcoString;
use lynchpin_library::config::RenderMode;
use lynchpin_library::frame::{Col, Row, TermFrame, TermPoint, TermSize};

// ── Public API ────────────────────────────────────────────────────────────────

/// Build a display-mode summation operator (large Σ).
///
/// Returns a frame whose baseline is at the centre of the operator body.
/// The caller composes this with upper/lower limits and the content.
pub fn build_sum_operator(mode: RenderMode, _content_hint_rows: Row) -> TermFrame {
    let sc = sum_chars(mode);
    // Fixed-height sum symbol: always 4 rows × 3 cols (like Math.cpp).
    let height: Row = Row::new(4);
    let width: Col = Col::new(3);

    let mut sigma = TermFrame::new(TermSize::new(width, height));
    sigma.set_baseline(Row::new(2)); // height / 2

    // Row 0: top bar
    for x in 0..3 {
        sigma.push_text(
            TermPoint::new(Col::new(x), Row::ZERO),
            EcoString::from(sc.top),
            ContentStyle::default(),
        );
    }

    // Row 1: diag_top at col 0
    sigma.push_text(
        TermPoint::new(Col::ZERO, Row::new(1)),
        EcoString::from(sc.diag_top),
        ContentStyle::default(),
    );
    // Row 2: diag_bot at col 0
    sigma.push_text(
        TermPoint::new(Col::ZERO, Row::new(2)),
        EcoString::from(sc.diag_bot),
        ContentStyle::default(),
    );

    // Row 3: bottom bar
    for x in 0..3 {
        sigma.push_text(
            TermPoint::new(Col::new(x), Row::new(3)),
            EcoString::from(sc.bot),
            ContentStyle::default(),
        );
    }

    sigma
}

/// Build a display-mode product operator (large Π).
pub fn build_prod_operator(mode: RenderMode) -> TermFrame {
    let pc = prod_chars(mode);
    let height: Row = Row::new(3);
    let width: Col = Col::new(3);

    let mut prod = TermFrame::new(TermSize::new(width, height));
    prod.set_baseline(Row::new(1)); // height / 2

    // Row 0: left connect, top bar, right connect
    prod.push_text(
        TermPoint::new(Col::ZERO, Row::ZERO),
        EcoString::from(pc.connect),
        ContentStyle::default(),
    );
    prod.push_text(
        TermPoint::new(Col::new(1), Row::ZERO),
        EcoString::from(pc.top),
        ContentStyle::default(),
    );
    prod.push_text(
        TermPoint::new(Col::new(2), Row::ZERO),
        EcoString::from(pc.connect),
        ContentStyle::default(),
    );

    // Rows 1-2: vertical bars at edges
    for y in 1..3 {
        prod.push_text(
            TermPoint::new(Col::ZERO, Row::new(y)),
            EcoString::from(pc.vert),
            ContentStyle::default(),
        );
        prod.push_text(
            TermPoint::new(Col::new(2), Row::new(y)),
            EcoString::from(pc.vert),
            ContentStyle::default(),
        );
    }

    prod
}

/// Build a display-mode integral operator (∫ stretched to `content_rows`).
///
/// The integral sign uses single-width Unicode characters (⌠│⌡) or a
/// multi-column ASCII construction.
pub fn build_integral_operator(mode: RenderMode, content_rows: Row) -> TermFrame {
    let ic = integral_chars(mode);
    let min_height: Row = Row::new(3);
    let height = content_rows.max(min_height);
    let h = height.get();

    // Width depends on mode: 1 for Unicode (⌠│⌡), 3 for ASCII
    let width: Col = if mode.is_unicode() {
        Col::new(1)
    } else {
        Col::new(3)
    };

    let mut integral = TermFrame::new(TermSize::new(width, height));
    integral.set_baseline(height.max(Row::new(1)) - Row::new(1)); // bottom-aligned for integrand

    if mode.is_unicode() {
        // Single-column Unicode integral: ⌠ at top, │ repeated, ⌡ at bottom.
        integral.push_text(
            TermPoint::new(Col::ZERO, Row::ZERO),
            EcoString::from(ic.top),
            ContentStyle::default(),
        );
        for y in 1..(h - 1) {
            integral.push_text(
                TermPoint::new(Col::ZERO, Row::new(y)),
                EcoString::from(ic.mid),
                ContentStyle::default(),
            );
        }
        if h > 1 {
            integral.push_text(
                TermPoint::new(Col::ZERO, Row::new(h - 1)),
                EcoString::from(ic.bot),
                ContentStyle::default(),
            );
        }
    } else {
        // ASCII 3-column integral pattern (from Math.cpp):
        let top_chars: &[char] = &[' ', '.', '-'];
        let mid_chars: &[char] = &[' ', '|', ' '];
        let bot_chars: &[char] = &['-', '\'', ' '];

        for (i, &ch) in top_chars.iter().enumerate() {
            integral.push_text(
                TermPoint::new(Col::new(i as i32), Row::ZERO),
                EcoString::from(ch),
                ContentStyle::default(),
            );
        }
        for y in 1..(h - 1) {
            for (i, &ch) in mid_chars.iter().enumerate() {
                integral.push_text(
                    TermPoint::new(Col::new(i as i32), Row::new(y)),
                    EcoString::from(ch),
                    ContentStyle::default(),
                );
            }
        }
        if h > 1 {
            for (i, &ch) in bot_chars.iter().enumerate() {
                integral.push_text(
                    TermPoint::new(Col::new(i as i32), Row::new(h - 1)),
                    EcoString::from(ch),
                    ContentStyle::default(),
                );
            }
        }
    }

    integral
}

// ── Character maps ────────────────────────────────────────────────────────────

struct SumChars {
    top: char,
    diag_top: char,
    diag_bot: char,
    bot: char,
}

fn sum_chars(mode: RenderMode) -> SumChars {
    if mode.is_unicode() {
        SumChars {
            top: '_',
            diag_top: '╲',
            diag_bot: '╱',
            bot: '‾',
        }
    } else {
        SumChars {
            top: '_',
            diag_top: '\\',
            diag_bot: '/',
            bot: '-',
        }
    }
}

struct ProdChars {
    top: char,
    vert: char,
    connect: char,
}

fn prod_chars(mode: RenderMode) -> ProdChars {
    if mode.is_unicode() {
        ProdChars {
            top: '━',
            vert: '┃',
            connect: '┳',
        }
    } else {
        ProdChars {
            top: '_',
            vert: '|',
            connect: '_',
        }
    }
}

struct IntegralChars {
    top: char,
    mid: char,
    bot: char,
}

fn integral_chars(mode: RenderMode) -> IntegralChars {
    if mode.is_unicode() {
        IntegralChars {
            top: '⌠',
            mid: '⎮',
            bot: '⌡',
        }
    } else {
        IntegralChars {
            top: '/',
            mid: '|',
            bot: '/',
        }
    }
}

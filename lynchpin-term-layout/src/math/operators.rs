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

use crate::config::RenderMode;
use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};

// ── Public API ────────────────────────────────────────────────────────────────

/// Build a display-mode summation operator (large Σ).
///
/// Returns a frame whose baseline is at the centre of the operator body.
/// The caller composes this with upper/lower limits and the content.
pub fn build_sum_operator(mode: RenderMode, _content_hint_rows: Row) -> TermFrame {
    let sc = sum_chars(mode);
    // Fixed-height sum symbol: always 4 rows × 3 cols (like Math.cpp).
    let height: Row = 4;
    let width: Col = 3;

    let mut sigma = TermFrame::new(TermSize::new(width, height));
    sigma.set_baseline(height / 2);

    // Row 0: top bar (all summation_top)
    for x in 0..width {
        sigma.push_text(
            TermPoint::new(x, 0),
            sc.top.to_string(),
            ContentStyle::default(),
        );
    }

    // Row 1: diag_top at col 0, mid char at col 1
    sigma.push_text(
        TermPoint::new(0, 1),
        sc.diag_top.to_string(),
        ContentStyle::default(),
    );
    // Row 2: diag_bot at col 0
    sigma.push_text(
        TermPoint::new(0, 2),
        sc.diag_bot.to_string(),
        ContentStyle::default(),
    );

    // Row 3: bottom bar (all summation_bottom)
    for x in 0..width {
        sigma.push_text(
            TermPoint::new(x, 3),
            sc.bot.to_string(),
            ContentStyle::default(),
        );
    }

    // Fill remaining cells with spaces so frame is solid
    // (row 1 cols 1.., row 2 cols 1.. are blank → TermFrame handles this)

    sigma
}

/// Build a display-mode product operator (large Π).
pub fn build_prod_operator(mode: RenderMode) -> TermFrame {
    let pc = prod_chars(mode);
    let height: Row = 3;
    let width: Col = 3;

    let mut prod = TermFrame::new(TermSize::new(width, height));
    prod.set_baseline(height / 2);

    // Row 0: top bar
    prod.push_text(
        TermPoint { col: 0, row: 0 },
        pc.connect,
        ContentStyle::default(),
    );
    for x in 1..width - 1 {
        prod.push_text(
            TermPoint::new(x, 0),
            pc.top.to_string(),
            ContentStyle::default(),
        );
    }
    prod.push_text(
        TermPoint {
            col: width - 1,
            row: 0,
        },
        pc.connect,
        ContentStyle::default(),
    );

    // Rows 1-2: vertical bars at edges
    for y in 1..3 {
        prod.push_text(
            TermPoint::new(0, y),
            pc.vert.to_string(),
            ContentStyle::default(),
        );
        prod.push_text(
            TermPoint::new(2, y),
            pc.vert.to_string(),
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
    let min_height: Row = 3;
    let height = content_rows.max(min_height);

    // Width depends on mode: 1 for Unicode (⌠│⌡), 3 for ASCII (3-col pattern)
    let width: Col = if mode.is_unicode() { 1 } else { 3 };

    let mut integral = TermFrame::new(TermSize::new(width, height));
    integral.set_baseline(height.max(1) - 1); // bottom-aligned for integrand

    if mode.is_unicode() {
        // Single-column Unicode integral: ⌠ at top, │ repeated, ⌡ at bottom.
        integral.push_text(
            TermPoint::new(0, 0),
            ic.top.to_string(),
            ContentStyle::default(),
        );
        for y in 1..(height - 1) {
            integral.push_text(
                TermPoint::new(0, y),
                ic.mid.to_string(),
                ContentStyle::default(),
            );
        }
        if height > 1 {
            integral.push_text(
                TermPoint::new(0, height - 1),
                ic.bot.to_string(),
                ContentStyle::default(),
            );
        }
    } else {
        // ASCII 3-column integral pattern (from Math.cpp):
        // top:    " .-"  or similar
        // middle: " | "  repeated
        // bottom: "-' "
        let top_chars: &[char] = &[' ', '.', '-'];
        let mid_chars: &[char] = &[' ', '|', ' '];
        let bot_chars: &[char] = &['-', '\'', ' '];

        for (i, &ch) in top_chars.iter().enumerate() {
            integral.push_text(
                TermPoint::new(i as Col, 0),
                ch.to_string(),
                ContentStyle::default(),
            );
        }
        for y in 1..(height - 1) {
            for (i, &ch) in mid_chars.iter().enumerate() {
                integral.push_text(
                    TermPoint::new(i as Col, y),
                    ch.to_string(),
                    ContentStyle::default(),
                );
            }
        }
        if height > 1 {
            for (i, &ch) in bot_chars.iter().enumerate() {
                integral.push_text(
                    TermPoint::new(i as Col, height - 1),
                    ch.to_string(),
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

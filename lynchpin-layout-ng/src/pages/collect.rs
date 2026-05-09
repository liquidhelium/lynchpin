//! Collect realized pairs into page-level [`Item`]s.
//!
//! Terminal equivalent of `lynchpin-layout/src/pages/collect.rs`.
//!
//! The page collector splits the flat realized pair stream into
//! page runs (consecutive non-pagebreak content), tags, and parity
//! instructions.  This mirrors the paged version but is simplified
//! for terminal rendering.

use rustc_hash::FxHashSet;

use typst::foundations::StyleChain;
use typst::introspection::{Locator, SplitLocator, Tag, TagElem};
use typst::layout::{PagebreakElem, Parity};
use typst::routines::Pair;

// ── Item ─────────────────────────────────────────────────────────────────────

/// An item in page layout.
pub enum Item<'a> {
    /// A page run containing content.
    Run(&'a [Pair<'a>], StyleChain<'a>, Locator<'a>),
    /// Tags in between pages.
    Tags(&'a [Pair<'a>]),
    /// An instruction to possibly add a page for parity.
    Parity(Parity, StyleChain<'a>, Locator<'a>),
}

// ── Collect ──────────────────────────────────────────────────────────────────

/// Slices up the children into logical parts.
pub fn collect<'a>(
    mut children: &'a mut [Pair<'a>],
    locator: &mut SplitLocator<'a>,
    mut initial: StyleChain<'a>,
) -> Vec<Item<'a>> {
    let mut items: Vec<Item<'a>> = vec![];
    let mut staged_empty_page = true;

    while let Some(&(elem, styles)) = children.first() {
        if let Some(pagebreak) = elem.to_packed::<PagebreakElem>() {
            let strong = !pagebreak.weak.get(styles);
            if strong && staged_empty_page {
                let locator = locator.next(&elem.span());
                items.push(Item::Run(&[], initial, locator));
            }

            if let Some(parity) = pagebreak.to.get(styles) {
                let locator = locator.next(&elem.span());
                items.push(Item::Parity(parity, styles, locator));
            }

            if !pagebreak.boundary.get(styles) {
                initial = styles;
            }

            staged_empty_page |= strong;
            children = &mut children[1..];
        } else {
            // Find the end of the consecutive non-pagebreak run.
            let end = children
                .iter()
                .take_while(|(c, _)| !c.is::<PagebreakElem>())
                .count();

            let end = migrate_unterminated_tags(children, end);
            if end == 0 {
                continue;
            }

            let (group, rest) = children.split_at_mut(end);
            children = rest;

            // If all that is left are tags, don't create a page just for them.
            if group.iter().all(|(c, _)| c.is::<TagElem>())
                && !(staged_empty_page
                    && children.iter().all(|&(c, s)| {
                        c.to_packed::<PagebreakElem>()
                            .is_some_and(|c| c.boundary.get(s))
                    }))
            {
                items.push(Item::Tags(group));
                continue;
            }

            let locator = locator.next(&elem.span());
            items.push(Item::Run(group, initial, locator));
            staged_empty_page = false;
        }
    }

    if staged_empty_page {
        items.push(Item::Run(&[], initial, locator.next(&())));
    }

    items
}

// ── Tag migration ────────────────────────────────────────────────────────────

/// Migrates trailing start tags without accompanying end tags from before
/// a pagebreak to after it. Returns the position right after the last
/// non-migrated tag.
fn migrate_unterminated_tags(children: &mut [Pair], mid: usize) -> usize {
    let (before, after) = children.split_at(mid);
    let start = mid
        - before
            .iter()
            .rev()
            .take_while(|&(c, _)| c.is::<TagElem>())
            .count();
    let end = mid
        + after
            .iter()
            .take_while(|&(c, _)| c.is::<PagebreakElem>())
            .count();

    let excluded: FxHashSet<_> = children[start..mid]
        .iter()
        .filter_map(|(c, _)| match c.to_packed::<TagElem>()?.tag {
            Tag::Start(..) => None,
            Tag::End(loc, ..) => Some(loc),
        })
        .collect();

    let key = |(c, _): &Pair| match c.to_packed::<TagElem>() {
        Some(elem) => {
            if excluded.contains(&elem.tag.location()) {
                -1
            } else {
                1
            }
        }
        None => 0,
    };

    children[start..end].sort_by_key(key);

    start
        + children[start..end]
            .iter()
            .take_while(|pair| key(pair) == -1)
            .count()
}

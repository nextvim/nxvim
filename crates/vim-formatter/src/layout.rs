use std::borrow::Cow;

use unicode_width::UnicodeWidthChar;

use crate::{CompiledFormat, FormatResolver, RenderItem, ResolveError, StyleId, TablineTarget};

#[derive(Clone, Debug)]
enum Atom {
    Glyph {
        ch: char,
        width: usize,
        style: Option<StyleId>,
    },
    Align,
    Truncate,
    ClickTarget(TablineTarget),
}

/// Applies target-width alignment and truncation to resolved render items.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutEngine;

impl LayoutEngine {
    pub fn layout(items: &[RenderItem<'_>], target_width: usize) -> Vec<RenderItem<'static>> {
        let mut atoms = flatten(items);
        let truncation_point = atoms.iter().position(|atom| matches!(atom, Atom::Truncate));

        if content_width(&atoms) > target_width {
            truncate_to_width(&mut atoms, truncation_point.unwrap_or(0), target_width);
        }

        apply_alignment(&mut atoms, target_width);
        build_render_items(atoms)
    }
}

/// Convenience wrapper around [`LayoutEngine::layout`].
pub fn layout(items: &[RenderItem<'_>], target_width: usize) -> Vec<RenderItem<'static>> {
    LayoutEngine::layout(items, target_width)
}

impl CompiledFormat {
    /// Resolves editor state and applies final layout in one pass.
    pub fn render<R: FormatResolver + ?Sized>(
        &self,
        resolver: &R,
        target_width: usize,
    ) -> Result<Vec<RenderItem<'static>>, ResolveError> {
        let items = self.resolve(resolver)?;
        Ok(LayoutEngine::layout(&items, target_width))
    }
}

fn flatten(items: &[RenderItem<'_>]) -> Vec<Atom> {
    let mut atoms = Vec::new();
    for item in items {
        match item {
            RenderItem::Text { text, style } => {
                atoms.extend(text.chars().map(|ch| Atom::Glyph {
                    ch,
                    width: UnicodeWidthChar::width(ch).unwrap_or(0),
                    style: *style,
                }));
            }
            RenderItem::Align => atoms.push(Atom::Align),
            RenderItem::Truncate => atoms.push(Atom::Truncate),
            RenderItem::ClickTarget { target } => atoms.push(Atom::ClickTarget(*target)),
        }
    }
    atoms
}

fn content_width(atoms: &[Atom]) -> usize {
    atoms
        .iter()
        .map(|atom| match atom {
            Atom::Glyph { width, .. } => *width,
            _ => 0,
        })
        .sum()
}

fn truncate_to_width(atoms: &mut Vec<Atom>, marker: usize, target_width: usize) {
    if target_width == 0 {
        atoms.retain(|atom| matches!(atom, Atom::ClickTarget(_)));
        return;
    }

    let indicator_style = atoms[marker.min(atoms.len())..]
        .iter()
        .find_map(atom_style)
        .or_else(|| {
            atoms[..marker.min(atoms.len())]
                .iter()
                .rev()
                .find_map(atom_style)
        });
    let required = content_width(atoms)
        .saturating_sub(target_width)
        .saturating_add(1);
    let mut removed = 0;
    let mut index = marker.min(atoms.len());

    // Remove from the truncation point toward the right. This preserves the
    // rightmost status section while left-truncating the marked region.
    while index < atoms.len() && removed < required {
        match atoms[index] {
            Atom::Glyph { width, .. } => {
                removed += width;
                atoms.remove(index);
            }
            Atom::Align | Atom::Truncate => {
                atoms.remove(index);
            }
            Atom::ClickTarget(_) => index += 1,
        }
    }

    // A marker near the end may not provide enough removable content. Fall
    // back toward the left while retaining the earliest possible prefix.
    while content_width(atoms).saturating_add(1) > target_width {
        let Some(index) = atoms
            .iter()
            .position(|atom| matches!(atom, Atom::Glyph { .. }))
        else {
            break;
        };
        atoms.remove(index);
    }

    let insertion = marker.min(atoms.len());
    atoms.insert(
        insertion,
        Atom::Glyph {
            ch: '<',
            width: 1,
            style: indicator_style,
        },
    );

    // Removing a double-width glyph can undershoot the target by one column;
    // alignment below intentionally fills that space when an align marker exists.
    while content_width(atoms) > target_width {
        let Some(index) = atoms
            .iter()
            .position(|atom| matches!(atom, Atom::Glyph { .. }))
        else {
            break;
        };
        atoms.remove(index);
    }
}

fn apply_alignment(atoms: &mut Vec<Atom>, target_width: usize) {
    let align_count = atoms
        .iter()
        .filter(|atom| matches!(atom, Atom::Align))
        .count();
    if align_count == 0 {
        atoms.retain(|atom| !matches!(atom, Atom::Truncate));
        return;
    }

    let available = target_width.saturating_sub(content_width(atoms));
    let base = available / align_count;
    let remainder = available % align_count;
    let mut seen = 0;
    let mut result = Vec::with_capacity(atoms.len() + available);

    for atom in std::mem::take(atoms) {
        match atom {
            Atom::Align => {
                let count = base + usize::from(seen < remainder);
                result.extend((0..count).map(|_| Atom::Glyph {
                    ch: ' ',
                    width: 1,
                    style: None,
                }));
                seen += 1;
            }
            Atom::Truncate => {}
            atom => result.push(atom),
        }
    }
    *atoms = result;
}

fn atom_style(atom: &Atom) -> Option<StyleId> {
    match atom {
        Atom::Glyph { style, .. } => *style,
        _ => None,
    }
}

fn build_render_items(atoms: Vec<Atom>) -> Vec<RenderItem<'static>> {
    let mut output = Vec::new();
    for atom in atoms {
        match atom {
            Atom::Glyph { ch, style, .. } => {
                if let Some(RenderItem::Text {
                    text,
                    style: previous_style,
                }) = output.last_mut()
                    && *previous_style == style
                {
                    text.to_mut().push(ch);
                } else {
                    output.push(RenderItem::Text {
                        text: Cow::Owned(ch.to_string()),
                        style,
                    });
                }
            }
            Atom::ClickTarget(target) => output.push(RenderItem::ClickTarget { target }),
            Atom::Align | Atom::Truncate => {}
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::layout;
    use crate::{RenderItem, StyleId, TablineTarget};

    fn text(value: &str, style: Option<StyleId>) -> RenderItem<'_> {
        RenderItem::Text {
            text: Cow::Borrowed(value),
            style,
        }
    }

    fn joined(items: &[RenderItem<'_>]) -> String {
        items
            .iter()
            .filter_map(|item| match item {
                RenderItem::Text { text, .. } => Some(text.as_ref()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn fills_a_single_alignment_section() {
        let output = layout(
            &[text("left", None), RenderItem::Align, text("right", None)],
            14,
        );
        assert_eq!(joined(&output), "left     right");
    }

    #[test]
    fn distributes_space_across_multiple_alignment_points() {
        let output = layout(
            &[
                text("a", None),
                RenderItem::Align,
                text("b", None),
                RenderItem::Align,
                text("c", None),
            ],
            8,
        );
        assert_eq!(joined(&output), "a   b  c");
    }

    #[test]
    fn truncates_from_an_explicit_marker_and_preserves_suffix() {
        let output = layout(
            &[
                text("pre:", None),
                RenderItem::Truncate,
                text("very-long-name", Some(StyleId(2))),
                RenderItem::Align,
                text("99%", None),
            ],
            14,
        );
        assert_eq!(joined(&output), "pre:<g-name99%");
        assert!(matches!(
            &output[1],
            RenderItem::Text {
                style: Some(StyleId(2)),
                ..
            }
        ));
    }

    #[test]
    fn wide_unicode_uses_terminal_columns() {
        let output = layout(&[text("界界", None), RenderItem::Align, text("x", None)], 7);
        assert_eq!(joined(&output), "界界  x");
    }

    #[test]
    fn preserves_tabline_targets_through_layout() {
        let output = layout(
            &[
                RenderItem::ClickTarget {
                    target: TablineTarget::Tab(2),
                },
                text("two", None),
                RenderItem::ClickTarget {
                    target: TablineTarget::Reset,
                },
            ],
            10,
        );
        assert!(matches!(
            output[0],
            RenderItem::ClickTarget {
                target: TablineTarget::Tab(2)
            }
        ));
        assert!(matches!(
            output[2],
            RenderItem::ClickTarget {
                target: TablineTarget::Reset
            }
        ));
    }

    #[test]
    fn zero_width_target_emits_no_text() {
        let output = layout(&[text("hello", None), RenderItem::Truncate], 0);
        assert_eq!(joined(&output), "");
    }
}

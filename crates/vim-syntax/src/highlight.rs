//! Highlight group identity, links, and colorscheme style resolution.

use std::collections::HashMap;

use vim_colorscheme::{ColorScheme, Style};

/// An interned highlight group identifier.
///
/// IDs are local to the [`HighlightGroups`] that created them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupId(u32);

impl GroupId {
    /// Returns the zero-based numeric representation of this ID.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Interns Vim highlight group names.
///
/// Names are compared using ASCII case folding, as Vim group names are, while
/// [`name`](Self::name) retains the spelling used by the first call to
/// [`intern`](Self::intern).
#[derive(Debug, Clone, Default)]
pub struct HighlightGroups {
    names: Vec<String>,
    ids: HashMap<String, GroupId>,
}

impl HighlightGroups {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the existing ID for `name`, or creates one if it is new.
    pub fn intern(&mut self, name: impl AsRef<str>) -> GroupId {
        let name = name.as_ref();
        let key = fold_name(name);
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }

        let index = self.names.len();
        let raw = u32::try_from(index).expect("too many highlight groups");
        let id = GroupId(raw);
        self.names.push(name.to_owned());
        self.ids.insert(key, id);
        id
    }

    /// Looks up a previously interned name using ASCII-insensitive identity.
    pub fn get(&self, name: &str) -> Option<GroupId> {
        self.ids.get(&fold_name(name)).copied()
    }

    /// Returns the retained display spelling for `id`.
    pub fn name(&self, id: GroupId) -> Option<&str> {
        self.names.get(id.index()).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// How a highlight link interacts with an existing link or direct style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMode {
    /// Install only when no link exists; a direct colorscheme style wins during
    /// resolution. This models `:highlight default link`.
    Default,
    /// Replace an existing link and override a direct style on the source.
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HighlightLink {
    target: GroupId,
    mode: LinkMode,
}

/// Links from highlight groups to other highlight groups.
#[derive(Debug, Clone, Default)]
pub struct HighlightLinks {
    links: Vec<Option<HighlightLink>>,
}

impl HighlightLinks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `source` to link to `target`.
    ///
    /// Returns `false` when a default link was ignored because `source` already
    /// had a link; otherwise returns `true`.
    pub fn set(&mut self, source: GroupId, target: GroupId, mode: LinkMode) -> bool {
        let index = source.index();
        if self.links.len() <= index {
            self.links.resize(index + 1, None);
        }
        if mode == LinkMode::Default && self.links[index].is_some() {
            return false;
        }
        self.links[index] = Some(HighlightLink { target, mode });
        true
    }

    pub fn set_default(&mut self, source: GroupId, target: GroupId) -> bool {
        self.set(source, target, LinkMode::Default)
    }

    pub fn set_replace(&mut self, source: GroupId, target: GroupId) {
        self.set(source, target, LinkMode::Replace);
    }

    /// Removes a link, returning whether one was present.
    pub fn clear(&mut self, source: GroupId) -> bool {
        self.links
            .get_mut(source.index())
            .and_then(Option::take)
            .is_some()
    }

    pub fn target(&self, source: GroupId) -> Option<GroupId> {
        self.link(source).map(|link| link.target)
    }

    pub fn mode(&self, source: GroupId) -> Option<LinkMode> {
        self.link(source).map(|link| link.mode)
    }

    fn link(&self, source: GroupId) -> Option<HighlightLink> {
        self.links.get(source.index()).copied().flatten()
    }
}

/// Resolves a group through links into a colorscheme style.
///
/// Colorscheme keys are matched ASCII-case-insensitively. Missing groups and
/// link cycles resolve to `Style::default()`. Resolution is also bounded by the
/// number of interned groups, protecting callers from malformed foreign IDs.
pub fn resolve_style(
    group: GroupId,
    groups: &HighlightGroups,
    links: &HighlightLinks,
    scheme: &ColorScheme,
) -> Style {
    let mut current = group;
    let mut visited = vec![false; groups.len()];

    loop {
        let Some(name) = groups.name(current) else {
            return Style::default();
        };
        if std::mem::replace(&mut visited[current.index()], true) {
            return Style::default();
        }

        let direct = scheme_style(scheme, name);
        match links.link(current) {
            Some(link) if link.mode == LinkMode::Replace => current = link.target,
            Some(link) if direct.is_none() => current = link.target,
            _ => return direct.copied().unwrap_or_default(),
        }
    }
}

fn fold_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn scheme_style<'a>(scheme: &'a ColorScheme, name: &str) -> Option<&'a Style> {
    scheme.get_style(name).or_else(|| {
        scheme
            .styles
            .iter()
            .find_map(|(candidate, style)| candidate.eq_ignore_ascii_case(name).then_some(style))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vim_colorscheme::{Color, Metadata};

    fn scheme(styles: &[(&str, Style)]) -> ColorScheme {
        let mut scheme = ColorScheme::new(Metadata::default());
        for &(name, style) in styles {
            scheme.insert_style(name, style);
        }
        scheme
    }

    #[test]
    fn interning_is_ascii_case_insensitive_and_retains_first_spelling() {
        let mut groups = HighlightGroups::new();
        let first = groups.intern("VimComment");

        assert_eq!(groups.intern("vimcomment"), first);
        assert_eq!(groups.intern("VIMCOMMENT"), first);
        assert_eq!(groups.get("vImCoMmEnT"), Some(first));
        assert_eq!(groups.name(first), Some("VimComment"));
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn identity_folds_ascii_only() {
        let mut groups = HighlightGroups::new();
        let upper_ascii = groups.intern("CAFÉ");
        let lower_ascii = groups.intern("cafÉ");
        let lower_unicode = groups.intern("café");

        assert_eq!(upper_ascii, lower_ascii);
        assert_ne!(upper_ascii, lower_unicode);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn unknown_ids_and_names_are_safe() {
        let groups = HighlightGroups::new();
        assert_eq!(groups.get("Missing"), None);
        assert_eq!(groups.name(GroupId(99)), None);
        assert_eq!(
            resolve_style(GroupId(99), &groups, &HighlightLinks::new(), &scheme(&[])),
            Style::default()
        );
    }

    #[test]
    fn default_link_does_not_replace_an_existing_link() {
        let mut groups = HighlightGroups::new();
        let source = groups.intern("Source");
        let first = groups.intern("First");
        let second = groups.intern("Second");
        let mut links = HighlightLinks::new();

        assert!(links.set_default(source, first));
        assert!(!links.set_default(source, second));
        assert_eq!(links.target(source), Some(first));
        assert_eq!(links.mode(source), Some(LinkMode::Default));
    }

    #[test]
    fn replacement_link_overwrites_any_existing_link() {
        let mut groups = HighlightGroups::new();
        let source = groups.intern("Source");
        let first = groups.intern("First");
        let second = groups.intern("Second");
        let mut links = HighlightLinks::new();

        links.set_default(source, first);
        links.set_replace(source, second);
        assert_eq!(links.target(source), Some(second));
        assert_eq!(links.mode(source), Some(LinkMode::Replace));
        assert!(links.clear(source));
        assert!(!links.clear(source));
    }

    #[test]
    fn resolves_direct_styles_and_colorscheme_names_case_insensitively() {
        let mut groups = HighlightGroups::new();
        let comment = groups.intern("Comment");
        let expected = Style::with_fg(Color::Green).italic();
        let scheme = scheme(&[("cOMMent", expected)]);

        assert_eq!(
            resolve_style(comment, &groups, &HighlightLinks::new(), &scheme),
            expected
        );
    }

    #[test]
    fn resolves_a_chain_of_default_links() {
        let mut groups = HighlightGroups::new();
        let a = groups.intern("A");
        let b = groups.intern("B");
        let c = groups.intern("C");
        let mut links = HighlightLinks::new();
        links.set_default(a, b);
        links.set_default(b, c);
        let expected = Style::with_fg(Color::Blue).bold();

        assert_eq!(
            resolve_style(a, &groups, &links, &scheme(&[("C", expected)])),
            expected
        );
    }

    #[test]
    fn direct_style_wins_over_a_default_link() {
        let mut groups = HighlightGroups::new();
        let source = groups.intern("Source");
        let target = groups.intern("Target");
        let mut links = HighlightLinks::new();
        links.set_default(source, target);
        let source_style = Style::with_fg(Color::Red);
        let target_style = Style::with_fg(Color::Blue);

        assert_eq!(
            resolve_style(
                source,
                &groups,
                &links,
                &scheme(&[("Source", source_style), ("Target", target_style)])
            ),
            source_style
        );
    }

    #[test]
    fn replacement_link_wins_over_a_direct_style() {
        let mut groups = HighlightGroups::new();
        let source = groups.intern("Source");
        let target = groups.intern("Target");
        let mut links = HighlightLinks::new();
        links.set_replace(source, target);
        let target_style = Style::with_fg(Color::Blue).underline();

        assert_eq!(
            resolve_style(
                source,
                &groups,
                &links,
                &scheme(&[
                    ("Source", Style::with_fg(Color::Red)),
                    ("Target", target_style)
                ])
            ),
            target_style
        );
    }

    #[test]
    fn missing_terminal_style_is_default() {
        let mut groups = HighlightGroups::new();
        let source = groups.intern("Source");
        let missing = groups.intern("Missing");
        let mut links = HighlightLinks::new();
        links.set_replace(source, missing);

        assert_eq!(
            resolve_style(source, &groups, &links, &scheme(&[])),
            Style::default()
        );
    }

    #[test]
    fn self_link_is_cycle_safe() {
        let mut groups = HighlightGroups::new();
        let group = groups.intern("Loop");
        let mut links = HighlightLinks::new();
        links.set_replace(group, group);

        assert_eq!(
            resolve_style(group, &groups, &links, &scheme(&[])),
            Style::default()
        );
    }

    #[test]
    fn multi_group_cycle_is_cycle_safe_even_when_styles_exist() {
        let mut groups = HighlightGroups::new();
        let a = groups.intern("A");
        let b = groups.intern("B");
        let c = groups.intern("C");
        let mut links = HighlightLinks::new();
        links.set_replace(a, b);
        links.set_replace(b, c);
        links.set_replace(c, a);

        assert_eq!(
            resolve_style(
                a,
                &groups,
                &links,
                &scheme(&[("B", Style::with_fg(Color::Green))])
            ),
            Style::default()
        );
    }
}

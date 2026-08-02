/// Selects the Vim formatting-language variant accepted by the compiler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FormatDialect {
    #[default]
    StatusLine,
    TabLine,
    WinBar,
    Ruler,
    Title,
}

/// A tabline mouse target activated by subsequent rendered text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TablineTarget {
    /// Select the numbered tab page (`%NT`).
    Tab(u32),
    /// Clear the active tab target (`%T`).
    Reset,
    /// Close a tab page, where zero means the current tab (`%NX`/`%X`).
    Close(u32),
}

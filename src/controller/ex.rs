#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub start_at_cursor: Option<bool>,
    pub end_at_cursor: Option<bool>,
    pub start_pattern: Option<String>,
    pub end_pattern: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ex {
    // --- File Management & Lifecycle ---
    /// `:w[rite] [file]` - Writes the current buffer to a file (defaults to active file).
    Write,
    /// `:q[uit]` - Quits the current window/editor.
    Quit,
    /// `:wq [file]` - Writes the current buffer to a file and quits.
    Wq,
    /// `:x[it]` or `:wq` - Writes the current buffer to a file (only if modified) and quits.
    Xit,
    /// `:up[date]` - Writes the current buffer to a file only if it has been modified.
    Update,

    // --- Buffer & File Loading ---
    /// `:e[dit] [file]` - Loads/edits a file in the current window.
    Edit,
    /// `:r[ead] [file]` - Reads/inserts a file's content below the cursor line.
    Read,

    // --- Argument List Navigation ---
    /// `:n[ext]` - Navigates to the next file in the argument list.
    Next,
    /// `:prev[ious]` or `:N` - Navigates to the previous file in the argument list.
    Prev,
    /// `:fir[st]` - Navigates to the first file in the argument list.
    First,
    /// `:la[st]` - Navigates to the last file in the argument list.
    Last,

    // --- Buffer Management ---
    /// `:b[uffer] [N|name]` - Switches to buffer N or buffer named 'name'.
    Buffer,
    /// `:bn[ext]` - Switches to the next buffer in the buffer list.
    Bnext,
    /// `:bp[revious]` - Switches to the previous buffer in the buffer list.
    Bprev,
    /// `:bd[elete] [N]` - Deletes/closes buffer N (defaults to active buffer).
    Bdelete,
    /// `:ls` or `:files` or `:buffers` - Lists all loaded buffers.
    Buffers,

    // --- Tab Management ---
    /// `:tabe[dit]` or `:tabnew` `[file]` - Opens a file in a new tab page.
    Tabnew,
    /// `:tabn[ext]` - Switches to the next tab page.
    Tabnext,
    /// `:tabp[revious]` or `:tabN` - Switches to the previous tab page.
    Tabprev,
    /// `:tabc[lose]` - Closes the current tab page.
    Tabclose,
    /// `:tabo[nly]` - Closes all other tab pages except the current one.
    Tabonly,

    // --- Window Splitting ---
    /// `:sp[lit] [file]` - Splits the current window horizontally.
    Split,
    /// `:vs[plit] [file]` - Splits the current window vertically.
    Vsplit,
    /// `:clo[se]` - Closes the current window.
    Close,
    /// `:on[ly]` - Closes all other windows except the current one.
    Only,

    // --- Editing & Manipulation ---
    /// `:[range]d[elete] [register] [count]` - Deletes lines in range.
    Delete,
    /// `:[range]y[ank] [register] [count]` - Yanks lines in range.
    Yank,
    /// `:[line]pu[t] [register]` - Puts/pastes register contents below line.
    Put,
    /// `:[range]j[oin]` - Joins lines in range.
    Join,
    /// `:[range]s[ubstitute]/pat/rep/flags` - Replaces matches of 'pat' with 'rep'.
    Substitute,

    // --- Search & Execution ---
    /// `:[range]g[lobal]/pat/cmd` - Runs an ex command on all lines matching 'pat'.
    Global,
    /// `:[range]v[global]/pat/cmd` - Runs an ex command on all lines NOT matching 'pat'.
    Vglobal,

    // --- Display & Inspection ---
    /// `:[range]p[rint]` - Prints/displays lines in range.
    Print,
    /// `:[range]nu[mber]` or `:#` - Prints lines in range with line numbers.
    Number,
    /// `:[range]l[ist]` - Prints lines showing trailing whitespaces and tabs.
    List,
    /// `:marks` - Lists all active marks.
    Marks,
    /// `:reg[isters]` - Displays the contents of all registers.
    Registers,

    // --- Undo & Redo ---
    /// `:u[ndo]` - Undoes the last change(s).
    Undo,
    /// `:red[o]` - Redoes the last undone change(s).
    Redo,

    // --- Configuration & Help ---
    /// `:se[t] [option]` - Configures or views editor settings.
    Set,
    /// `:colorschemes [name]` - Loads a colorscheme by name.
    Colorschemes,
    /// `:syn[tax] {on|off}` - Enables or disables syntax highlighting.
    Syntax,
    /// `:h[elp] [subject]` - Opens help documentation for a subject.
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExCommand {
    pub op: Ex,
    pub arguments: Option<Vec<String>>,
    pub range: Option<Range>,
}

impl std::fmt::Display for Ex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ex::Write => write!(f, "write"),
            Ex::Quit => write!(f, "quit"),
            Ex::Wq => write!(f, "wq"),
            Ex::Xit => write!(f, "xit"),
            Ex::Update => write!(f, "update"),
            Ex::Edit => write!(f, "edit"),
            Ex::Read => write!(f, "read"),
            Ex::Next => write!(f, "next"),
            Ex::Prev => write!(f, "prev"),
            Ex::First => write!(f, "first"),
            Ex::Last => write!(f, "last"),
            Ex::Buffer => write!(f, "buffer"),
            Ex::Bnext => write!(f, "bnext"),
            Ex::Bprev => write!(f, "bprev"),
            Ex::Bdelete => write!(f, "bdelete"),
            Ex::Buffers => write!(f, "buffers"),
            Ex::Tabnew => write!(f, "tabnew"),
            Ex::Tabnext => write!(f, "tabnext"),
            Ex::Tabprev => write!(f, "tabprev"),
            Ex::Tabclose => write!(f, "tabclose"),
            Ex::Tabonly => write!(f, "tabonly"),
            Ex::Split => write!(f, "split"),
            Ex::Vsplit => write!(f, "vsplit"),
            Ex::Close => write!(f, "close"),
            Ex::Only => write!(f, "only"),
            Ex::Delete => write!(f, "delete"),
            Ex::Yank => write!(f, "yank"),
            Ex::Put => write!(f, "put"),
            Ex::Join => write!(f, "join"),
            Ex::Substitute => write!(f, "substitute"),
            Ex::Global => write!(f, "global"),
            Ex::Vglobal => write!(f, "vglobal"),
            Ex::Print => write!(f, "print"),
            Ex::Number => write!(f, "number"),
            Ex::List => write!(f, "list"),
            Ex::Marks => write!(f, "marks"),
            Ex::Registers => write!(f, "registers"),
            Ex::Undo => write!(f, "undo"),
            Ex::Redo => write!(f, "redo"),
            Ex::Set => write!(f, "set"),
            Ex::Colorschemes => write!(f, "colorschemes"),
            Ex::Syntax => write!(f, "syntax"),
            Ex::Help => write!(f, "help"),
        }
    }
}

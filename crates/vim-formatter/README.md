# vim-formatter

A small Rust lexer, parser, compiler, and renderer for Vim-style statusline and tabline format strings.

Format strings are compiled once into bytecode, then resolved against editor state on each redraw.

```text
source → lexer → parser → AST → compiler → bytecode → resolver → layout
```

## Features

- Vim statusline escapes such as `%f`, `%l`, `%c`, `%m`, `%y`, and `%p`
- Minimum/maximum widths and left alignment: `%-10.20f`
- Alignment (`%=`) and truncation (`%<`)
- Highlight groups: `%#StatusLine#` and `%*`
- Expressions: `%{mode()}`
- Nested groups: `%20(%t%m%)`
- Tabline click targets: `%1T`, `%T`, `%X`, and `%3X`
- Unicode terminal-column measurement
- Source spans and structured errors
- Backend-neutral styled render items

## Usage

```toml
[dependencies]
vim-formatter = { git = "ssh://git@github.com/icedman/vim-formatter" }
```

Implement `FormatResolver` to provide editor state:

```rust
use std::{borrow::Cow, error::Error};
use vim_formatter::{
    CompiledFormat, FormatDialect, FormatResolver, parse,
};

struct EditorState;

impl FormatResolver for EditorState {
    fn file_name(&self) -> Cow<'_, str> {
        Cow::Borrowed("src/main.rs")
    }

    fn line(&self) -> usize {
        128
    }

    fn column(&self) -> usize {
        17
    }

    fn total_lines(&self) -> usize {
        512
    }

    fn is_modified(&self) -> bool {
        true
    }

    fn file_type(&self) -> Cow<'_, str> {
        Cow::Borrowed("rust")
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let source = "%f %m%= %y | %l:%c | %p%%";
    let ast = parse(source, FormatDialect::StatusLine)?;
    let compiled = CompiledFormat::compile(&ast)?;

    // Resolve editor data and lay it out at 60 terminal columns.
    let items = compiled.render(&EditorState, 60)?;

    for item in items {
        println!("{item:?}");
    }

    Ok(())
}
```

## Samples

Statusline:

```vim
%#StatusLine# %f %m%= %y | %l:%c | %p%%
```

```text
 src/main.rs [+]                       rust | 128:17 | 25%
```

Statusline with an expression, group, and file metadata:

```vim
%#StatusLine# %{mode()} %20(%t%m%)%=%e[%o] %l/%L
```

```text
 NORMAL main.rs[+]                         utf-8[unix] 128/512
```

Long path with explicit truncation:

```vim
%#StatusLine# %<%F%= %n:%y %l:%c:%v %P
```

```text
 </Developer/rust/vim-formatter/src/main.rs 3:rust 128:17:20 25
```

Clickable tabline:

```vim
%#TabLine#%1T 1: README.md %2T%#TabLineSel# 2: main.rs %T%= %X ×
```

```text
 1: README.md  2: main.rs                                  ×
```

Tabline click regions are returned as typed `RenderItem::ClickTarget` values, allowing a UI backend to select or close tabs.

## Example program

The included demo renders true-color ANSI statusline and tabline samples. Kanagawa is the default; pass a theme name to swap palettes:

```sh
cargo run
cargo run -- kanagawa
cargo run -- catppuccin
```

Run checks with:

```sh
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

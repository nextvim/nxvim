# nxvim

`nxvim` is a **Vim-inspired terminal text editor** written in Rust, powered by Zed's ultra-high-performance Rope + SumTree-backed text buffers and concurrent snapshot technologies.

It bridges the speed and safety of Rust, the rock-solid collaborative/concurrent editing foundation of Zed, and the extensive editing features, selection models, and scripting capabilities of Vim.

![nxvim screenshot](https://raw.githubusercontent.com/nextvim/nxvim/main/screenshots/Screenshot%20from%202026-08-24%2008-59-21.png)

## Features

nxvim is under active development. A lot is still missing, and the feature set is not yet intended to be a complete or drop-in replacement for Vim or Neovim. The current implementation includes:

- Modal editing with normal, insert, replace, visual, visual-line, visual-block, and command-line modes.
- Vim-style motions, counts, operators, and operator/motion composition.
- Text objects for applying operations to structured regions of text.
- Registers, yanking and putting text, and recorded/replayed macros.
- Undo and redo, repeat, search and replace, folding, indentation, and case transformations.
- Tree-sitter structural navigation, including `]]` for the next block boundary and `]f` for the next function.
- Multiple buffers and windows, cursor and selection handling, and terminal-oriented rendering.
- Basic Vimscript command execution, including startup commands and sourced scripts.
- A Vim-compatible regular-expression engine used by editor search and scripting features.
- TextMate grammar parsing and syntax highlighting, with incremental highlighting support.
- A Rust implementation built on Zed's rope, SumTree, and snapshot-based text infrastructure.

Many Vim features, commands, integrations, and edge cases remain to be implemented or may only be partially compatible. Treat compatibility as evolving rather than guaranteed.

## Running

`nxvim` can be run interactively or headlessly for testing and scripting.

```sh
nxvim [options] [files...]
```

| Argument / Option | Description |
| --- | --- |
| `[files...]` | File paths to open into separate buffers at startup. |
| `--cmd <command>` | Execute `<command>` (Vimscript statement) *before* loading any files. |
| `-c <command>` | Execute `<command>` (Vimscript statement) *after* loading the first file. |
| `-S <script_file>` | Source and execute the Vimscript file `<script_file>` at startup. |
| `--` | Explicitly separates arguments/options from file paths. |

---

## Getting Started

### Installation

#### Fedora (COPR)

Install the latest packaged build from the nxvim COPR repository:

```sh
sudo dnf install 'dnf-command(copr)'
sudo dnf copr enable icedman/nxvim
sudo dnf install nxvim
```

Then run:

```sh
nxvim
```

#### Arch Linux (from `PKGBUILD`)

The package can be tested and installed locally before an AUR package is published. Clone the nxvim repository and build the package with Arch's `makepkg`:

```sh
git clone https://github.com/nextvim/nxvim.git
cd nxvim/packaging
makepkg -si
```

`makepkg` downloads and verifies the pinned source archive, builds nxvim, and prompts for installation. It must be run as a normal user, not as root.

### Prerequisites

- [Rust Toolchain](https://rustup.rs/) (supporting Edition 2024).

### Running Interactively

Run the text editor inside your terminal:

```sh
cargo run
```

## Configuration

nxvim reads its startup configuration from `~/.config/nxvim/init.vim`. For example:

```vim
colorscheme kanagawa
set nonumber
set cursorline
treesitter on
syntax on
indexer on
inspect off
set inspect=treesitter
```

## Inspecting syntax

Inspection shows information about the syntax at the cursor in the status area. Enable it with:

```vim
:inspect on
```

Choose which parser or indexer to inspect with the `inspect` option:

```vim
:set inspect=treesitter
:set inspect=textmate
```

Use `:inspect off` to hide the inspection information. Tree-sitter inspection requires Tree-sitter to be enabled with `:treesitter on`; TextMate inspection requires syntax highlighting to be enabled with `:syntax on`.

## Colorschemes

nxvim includes built-in colorschemes with both dark and light variants. Change the active scheme with the Vim command `:colorscheme <name>`; run `:colorscheme` without a name to show the current scheme. The default scheme is `tokyonight` (the TokyoNight Moon variant).

Available colorschemes:

- `carbonfox`
- `catppuccin`, `catppuccin-mocha`, `catppuccin-frappe`, `catppuccin-latte`, `catppuccin-macchiato`
- `dawnfox`, `dayfox`, `duskfox`, `nightfox`, `nordfox`, `terafox`
- `dracula`
- `gruvbox-material`
- `kanagawa`
- `onedark`
- `rose-pine`, `rose-pine-dawn`, `rose-pine-moon`
- `tokyonight`, `tokyonight-day`, `tokyonight-night`, `tokyonight-storm`

The built-in schemes are defined as TOML resources in [`crates/vim-colorscheme/src/schemes`](crates/vim-colorscheme/src/schemes). Contributions for additional schemes are welcome.

## Licenses and Provenance

- First-party workspace code in `nxvim`, `crates/vim-buffer`, `crates/vim-script`, `crates/vim-formatter`, etc., is licensed under the same terms as **Vim** or custom compatible licensing, permitting distribution and modification.
- Extracted Zed foundations under `crates/zed` preserve upstream licensing: `clock`, `rope`, and `text` are under **GPL-3.0-or-later**, while `sum_tree` is licensed under **Apache-2.0**. Distribution of binary artifacts combining GPL dependencies must remain compatible with GPL-3.0 terms.

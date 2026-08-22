# nxvim

`nxvim` is a **Vim-inspired terminal text editor** written in Rust, powered by Zed's ultra-high-performance Rope + SumTree-backed text buffers and concurrent snapshot technologies.

It bridges the speed and safety of Rust, the rock-solid collaborative/concurrent editing foundation of Zed, and the extensive editing features, selection models, and scripting capabilities of Vim.

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

### Prerequisites

- [Rust Toolchain](https://rustup.rs/) (supporting Edition 2024).

### Running Interactively

Run the text editor inside your terminal:

```sh
cargo run
```

## Licenses and Provenance

- First-party workspace code in `nxvim`, `crates/vim-buffer`, `crates/vim-script`, `crates/vim-formatter`, etc., is licensed under the same terms as **Vim** or custom compatible licensing, permitting distribution and modification.
- Extracted Zed foundations under `crates/zed` preserve upstream licensing: `clock`, `rope`, and `text` are under **GPL-3.0-or-later**, while `sum_tree` is licensed under **Apache-2.0**. Distribution of binary artifacts combining GPL dependencies must remain compatible with GPL-3.0 terms.

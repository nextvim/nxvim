# Vim compatibility oracle

`nxvim` is pinned to upstream Vim **9.2, patches 1–843** (`v9.2.0843`) for differential behavior tests. This matches the oracle used by `nextvim/vim-regex`.

The machine-readable pin is in `vim-version.json`. The annotated tag resolves to commit `975e191dc817d8d00abca7197c4529a417c2f805`; CI should check out that commit and verify that tag `v9.2.0843` still resolves to it before building.

Build the oracle on Linux with:

```sh
git clone https://github.com/vim/vim.git vendor-vim
git -C vendor-vim checkout 975e191dc817d8d00abca7197c4529a417c2f805
cd vendor-vim
./configure \
  --with-features=huge \
  --enable-multibyte \
  --disable-gui \
  --without-x
make -j2
```

Differential tests must start Vim without user configuration, plugins, swap, or viminfo and use UTF-8. Test output should compare stable state and Vim error identifiers, not localized message text.

Any future pin update must be coordinated with `vim-regex`, `vim-script`, and `vim-formatter`. Update the machine-readable pin, copied help/source references, fixture provenance, and expected snapshots in the same reviewed change.

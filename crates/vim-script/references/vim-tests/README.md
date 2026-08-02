# Selected upstream Vim regression references

These files are unmodified snapshots downloaded from Vim's upstream `src/testdir` on 2026-08-02:

- [`test_eval_stuff.vim`](https://github.com/vim/vim/blob/master/src/testdir/test_eval_stuff.vim)
- [`test_functions.vim`](https://github.com/vim/vim/blob/master/src/testdir/test_functions.vim)
- [`test_listdict.vim`](https://github.com/vim/vim/blob/master/src/testdir/test_listdict.vim)
- [`test_trycatch.vim`](https://github.com/vim/vim/blob/master/src/testdir/test_trycatch.vim)
- [`test_vimscript.vim`](https://github.com/vim/vim/blob/master/src/testdir/test_vimscript.vim)

The upstream files depend on Vim's test framework and are retained as executable-specification references. Standalone, deterministic adaptations live in `tests/fixtures/`. The Rust compatibility suite always compares those fixtures with committed expected snapshots and additionally runs them against the system `vim` executable when available.

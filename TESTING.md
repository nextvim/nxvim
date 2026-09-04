# Running Vim's Test Suite on nxvim (TESTING.md)

This document maps out the strategy, requirements, and ordered roadmap for executing Vim's official test suite (`src/testdir/runtest.vim` and `test_*.vim`) against `nxvim`.

---

## 🎯 Goal

Enable `nxvim` to run standard VimScript test files (`test_*.vim`) natively, simulating keystrokes via `feedkeys()`, capturing test failures in `v:errors`, and reporting results identically to Vim's native `make test` pipeline.

---

## 🏗️ Architecture & How Vim Runs Tests

Vim executes tests headlessly using its own binary as the runner:
```bash
nxvim -u NONE -S runtest.vim test_assert.vim
```

1. **Headless Execution:** Vim starts without a TUI/GUI, loads `runtest.vim`, which scans for functions starting with `Test_`.
2. **Setup & Teardown:** For each test function, `SetUp()` is invoked, then `Test_*()`, followed by `TearDown()`.
3. **Assertions & `v:errors`:** Assertion failures do not crash the runner; they append error messages to the global list `v:errors`.
4. **Keystroke Simulation:** Key inputs are fed into Vim's type-ahead buffer using `feedkeys()` (or `term_sendkeys()` for terminal screen dumps).
5. **Log Generation:** Results are written to `messages` and `test.log`. The exit code reflects test outcome.

---

## 🗺️ Feature Implementation Roadmap (In Order)

### Phase 1: CLI Flags & Headless Test Harness
**Objective:** Provide the CLI switches and headless execution mode necessary to launch tests non-interactively.

- [ ] **CLI Switches (`src/main.rs` & `src/app/`):**
  - Support `-u NONE` (skip user configs / vimrc).
  - Support `-S <script>` (source script file at startup).
  - Support `-c <command>` and `--cmd <command>` (execute Ex commands at startup/pre-init).
  - Support `-g` / `--headless` (run in headless mode without initializing TUI/Winit).
- [ ] **Headless Batch Mode:**
  - Create a non-rendering app mode that processes events, Ex commands, and script execution synchronously until completion or `:qa!`.
  - Set process exit code to `1` if `test.log` contains errors or `0` on success.

---

### Phase 2: VimScript Error Tracking & Core Assertion Builtins
**Objective:** Implement `v:errors` and the core `assert_*` functions required by `runtest.vim`.

- [ ] **Special Variable `v:errors` (`crates/vim-script`):**
  - Implement global `v:errors` list variable.
  - Automatically initialize `v:errors` as an empty list (`[]`) in script evaluation context.
- [ ] **Core Assertion Builtin Functions:**
  - `assert_equal(expected, actual [, msg])`
  - `assert_notequal(expected, actual [, msg])`
  - `assert_true(actual [, msg])`
  - `assert_false(actual [, msg])`
  - `assert_fails(cmd [, expected_error [, msg]])`
  - `assert_inrange(lower, upper, actual [, msg])`
  - `assert_match(pattern, actual [, msg])`
  - `assert_report(msg)`
- [ ] **Exception Handling (`try` / `catch` / `finally`):**
  - Support `v:exception`, `v:errmsg`, and `v:throwpoint`.
  - Ensure `assert_fails()` correctly intercepts runtime exceptions triggered by Ex commands.

---

### Phase 3: Input Simulation (`feedkeys()`) & Key Queue Integration
**Objective:** Enable VimScript tests to simulate user keystrokes in normal, insert, and visual modes.

- [ ] **Key Queue Infrastructure (`crates/vim-input` & `src/app/script_host.rs`):**
  - Create a high-priority input queue in `InputEngine` for injected keystrokes.
- [ ] **`feedkeys({string} [, {mode}])` Implementation:**
  - Support `{string}` parsing (converting key notation like `<Esc>`, `<CR>`, `<C-W>`, `<F4>` into key events).
  - **Mode `'t'`:** Treat input as typed by the user (subject to mapping unless `'n'` is specified).
  - **Mode `'m'`:** Remap keys (default mode).
  - **Mode `'n'`:** Do not remap keys.
  - **Mode `'x'` (Synchronous Execution):** Execute feedkeys buffer immediately and exhaust the input queue synchronously before `feedkeys()` returns.
  - **Mode `'L'`:** Low-level input buffer insertion.

---

### Phase 4: Sourcing Engine & Builtin Inventory for `runtest.vim`
**Objective:** Provide all builtin functions, commands, and options that `runtest.vim` relies upon.

- [ ] **Essential Builtin Functions:**
  - `execute({command})`: Run an Ex command and return its output as a string.
  - `eval({string})`: Evaluate an expression string.
  - `mode([expr])`: Return active editor mode string (`"n"`, `"i"`, `"v"`, `"V"`, `"\<C-V>"`, `"c"`).
  - `getbufline()`, `setbufline()`, `deletebufline()`, `append()`.
  - `bufnr()`, `bufname()`, `bufexists()`, `getbufinfo()`.
  - `filereadable()`, `filewritable()`, `delete()`, `writefile()`, `readfile()`.
  - `reltime()`, `reltimestr()`, `reltimefloat()`.
  - `changenr()`, `undotree()`.
- [ ] **Option Infrastructure (`set` & `&option` accessors):**
  - Enable setting/querying options used by tests (`&cpoptions`, `&tabstop`, `&shiftwidth`, `&cmdheight`).

---

### Phase 5: Internal Override & Testing Hooks (`test_override()`)
**Objective:** Support internal state overrides used in Vim's unit test scripts.

- [ ] **`test_override({expr}, {val})` Implementation:**
  - Override `char_avail`: Simulate key availability in input loop.
  - Override `redraw`: Control screen update timing during script execution.
  - Override `starting`: Control initialization phase flags.
  - Override `alloc_fail`: Simulate allocation failures for robustness testing.

---

### Phase 6: Screen Dump & Terminal Tests (Advanced)
**Objective:** Support visual regression and terminal screen dump tests.

- [ ] **Terminal Buffer Integration (`+terminal`):**
  - Implement `term_start()`, `term_sendkeys()`, `term_wait()`, `term_getline()`.
- [ ] **Screen Dump Testing:**
  - Support `term_dumptest()` and `term_dumpdiff()` for comparing screen buffer state against `.dump` files.

---

## 🧪 Quick Start Verification Matrix

| Milestone | Command | Target Test File | Expected Result |
| :--- | :--- | :--- | :--- |
| **Milestone 1** | `nxvim -u NONE -S runtest.vim test_assert.vim` | `test_assert.vim` | Evaluates basic `assert_equal()` and `assert_true()`. |
| **Milestone 2** | `nxvim -u NONE -S runtest.vim test_feedkeys.vim` | `test_feedkeys.vim` | `feedkeys()` synchronously modifies buffer text. |
| **Milestone 3** | `nxvim -u NONE -S runtest.vim test_cmdline.vim` | `test_cmdline.vim` | Command line navigation and completion tests pass. |

---

## 📁 Related Documents

- [SCRIPT.md](file:///home/iceman/Developer/rust/nextvim/nxvim/docs/SCRIPT.md) – Current VimScript engine state & command spec inventory.
- [VIM.md](file:///home/iceman/Developer/rust/nextvim/nxvim/docs/VIM.md) – Vim compatibility goals & engine architecture.

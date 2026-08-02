# vim-input

`vim-input` is an editor-agnostic Vim input resolver. It consumes normalized key presses, keeps the small amount of state required by Vim's input grammar, and resolves key sequences defined by a `Keymap` into `Action` values.

The initial implementation is extracted from `.tmp-dzed/src/controller/{input,keymap,actions}.rs`, but the crate must not depend on dzed, an editor buffer, a renderer, or a terminal event library.

```text
backend event                vim-input                         editor
(crossterm/winit/GUI)  ->  KeyInput -> Resolver -> Outcome  -> execute Action
                                      ^
                                      |
                                    Keymap
```

## Goals

- Resolve single keys and multi-key sequences such as `j`, `gg`, and `<C-w>h`.
- Support Vim editing modes, counts, operators, motions, text objects, and registers.
- Keep bindings in a configurable `Keymap`, separate from the resolver algorithm.
- Preserve the existing `Mode` and `Action` enums during extraction so dzed can migrate without rewriting its executor.
- Expose pending input for status lines and diagnostics.
- Be deterministic, synchronous, cheap to reset, and straightforward to test.
- Remain independent of document contents. The crate describes intent; the editor applies it.

## Non-goals

- Moving cursors or modifying text.
- Owning registers, clipboard contents, marks, or search state.
- Recording or replaying macros. Macro recording captures resolved editor commands and therefore belongs to the host.
- Reading terminal events directly in the core API.
- Implementing Command mode, command-line parsing, or Ex commands.
- Deciding whether an action is valid for a particular document or editor layout.

## Proposed public API

```rust
use vim_input::{Key, Keymap, Mode, ResolveOutcome, Resolver};

let keymap = Keymap::vim_defaults()?;
let mut input = Resolver::new(Mode::Normal);

match input.feed(Key::char('j'), &keymap) {
    ResolveOutcome::Resolved(resolved) => {
        // resolved.action is Action::MoveDown { count: 1, select: false }
        // resolved.register contains the selected register, when applicable.
        editor.execute(resolved);
    }
    ResolveOutcome::Pending => update_status(input.pending()),
    ResolveOutcome::Ignored => {}
    ResolveOutcome::Invalid(invalid) => show_invalid_sequence(invalid),
}
```

The important types should have approximately these responsibilities:

```rust
pub struct Resolver { /* private grammar state */ }

impl Resolver {
    pub fn new(mode: Mode) -> Self;
    pub fn feed(&mut self, key: Key, keymap: &Keymap) -> ResolveOutcome;
    pub fn mode(&self) -> Mode;
    pub fn set_mode(&mut self, mode: Mode);
    pub fn pending(&self) -> PendingInput<'_>;
    pub fn is_pending(&self) -> bool;
    pub fn reset(&mut self);
}

pub enum ResolveOutcome {
    Resolved(ResolvedAction),
    Pending,
    Ignored,
    Invalid(InvalidSequence),
}

pub struct ResolvedAction {
    pub action: Action,
    pub register: Option<char>,
}
```

`ResolvedAction` carries the register alongside the action instead of exposing dzed's timing-sensitive `last_register` field. `Pending` replaces the ambiguous use of `Action::NoOp` for an incomplete sequence. `Ignored` covers events that intentionally produce no command, while `Invalid` makes failed mappings observable. A frontend may choose to treat either as a no-op.

Mode changes caused by resolved actions remain immediate: after resolving `i`, `mode()` reports `Insert` before the caller executes the action. Direct editor-driven changes use `set_mode`.

## Input representation

The core crate owns a small backend-neutral key model:

```rust
pub struct Key {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

pub enum KeyCode {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    Function(u8),
}
```

`Modifiers` should be a bitflag containing at least Shift, Control, Alt, and Super. Character normalization must have one documented rule: printable characters retain their character and Shift is ignored for matching when the character already encodes case; non-character keys retain Shift. This prevents backends from disagreeing about whether `A` is represented as `Char('A')` or `Shift + Char('a')`.

Backend conversion belongs outside the core. The workspace can provide `From<crossterm::event::KeyEvent>` behind an optional `crossterm` feature, or dzed can keep a tiny adapter. Key-release filtering also belongs in that adapter because the resolver only receives logical key presses.

Mappings use a parseable notation compatible with the source implementation:

```text
j
5j
<C-w>h
<Esc>
gg
f{char}
```

Parsing returns a structured `KeyParseError`; library code must not panic on invalid user mappings.

## Keymap design

A keymap is data, not a hard-coded resolver:

```rust
let mut map = Keymap::empty();
map.bind(BindingContext::Motion, "w", Action::MoveToWord {
    count: 1,
    select: false,
})?;
map.bind(BindingContext::Normal, "<C-w>h", Action::FocusLeftWindow)?;
map.unbind(BindingContext::Normal, "Q")?;
```

The initial contexts preserve dzed's current behavior:

- `Operator`
- `Motion`
- `Normal`
- `Mode`
- `Insert`
- `Visual`
- `TextObject`

`Keymap::vim_defaults()` builds the shipped Vim-like map. `Keymap::empty()` supports applications and tests that want no defaults. Binding methods should detect duplicate exact mappings and malformed patterns. Rebinding should be explicit (`replace`) rather than silently dependent on insertion order.

Internally, use a prefix trie (or an equivalent indexed structure), not a linear scan over every `HashMap` entry for every key. Each lookup must distinguish:

- no match;
- valid prefix;
- complete match;
- complete match that is also a prefix of a longer mapping.

Complete mappings resolve eagerly, including when the same sequence is a prefix of a longer mapping. There is no timeout or deferred exact fallback. Consequently, applications should reject or warn about longer mappings made unreachable by an eager exact prefix.

### Context precedence

When more than one context can match, resolution is deterministic and follows the extracted behavior:

1. visual overrides in a visual mode;
2. text objects while visual or after an operator;
3. motions;
4. operators when no operator is pending;
5. normal actions;
6. mode changes.

Insert mode uses its own bindings first, then emits printable text. Command mode is excluded: an action that requests command-line input is emitted to the host, which owns the command-line component and subsequent input. This precedence is part of the public behavior and requires tests.

## Resolver state and grammar

The resolver privately tracks:

- current `Mode`;
- count before an operator and count before its motion;
- buffered key sequence;
- pending operator and its originating sequence;
- selected register and whether the next character names a register.

Representative resolutions:

| Input | Result |
|---|---|
| `j` | `MoveDown { count: 1, .. }` |
| `5k` | `MoveUp { count: 5, .. }` |
| `gg` | `MoveToStartOfDocument { count: 1, .. }` |
| `d` | `Pending` with a delete operator |
| `dw` | `DeleteMotion { count: 1, motion: MoveToWord { count: 1, .. } }` |
| `2d3w` | operator count `2`, motion count `3` |
| `dd` | `DeleteLine { count: 1 }` |
| `"ap` | put using register `a` |
| `fx` | next-character motion with `ch: 'x'` |
| `i`, then `a` | enter Insert, then `InsertText("a")` |

Counts must use checked parsing. Overflow should saturate at `u32::MAX` or produce a documented invalid outcome; it must not silently fall back to `1`.

Invalid pending commands follow Vim's Normal-mode behavior: consume the invalid continuation, cancel the entire pending command, clear its count/operator/register-prefix state, and return `ResolveOutcome::Invalid`. Do not reinterpret a suffix of the failed sequence as a new command. For example, an invalid continuation after `z` is consumed; only the next subsequently supplied key starts a fresh command. This command-grammar rule is distinct from Vim's mapping expansion behavior, where input that fails to match a mapping may still be processed normally.

`pending()` should return structured state rather than requiring callers to inspect public fields:

```rust
pub struct PendingInput<'a> {
    pub count: Option<u32>,
    pub operator: Option<&'a Action>,
    pub keys: &'a [Key],
    pub waiting_for_register: bool,
}
```

A `Display` implementation may provide the status-line string currently produced by `pending_keys_str()`.

## `Action` and `Mode` compatibility

The first release should copy and maintain the existing `Action` and `Mode` enums, including variant names and payloads. This is deliberate: the extraction should change dependency boundaries, not simultaneously force a rewrite of dzed's action executor. `Action` remains the crate's resolved-command vocabulary and dzed matches on it as before.

The resolver currently relies on `Action::with_count`, `Action::with_select`, `Action::with_char`, and `Action::count`. Keep these methods during migration and add exhaustive tests for every applicable variant. Inapplicable transformations should not quietly return surprising values.

### Recommended improvements after extraction

The existing enum works, but it mixes several concepts and repeats metadata across many variants. Improve it only in a separate, reviewable change after compatibility tests pass:

1. **Separate grammar from execution commands.** Represent an operator, motion, and text object as typed values while resolving, then produce an executable `Action`. This removes temporary `Action::Delete { .. }` values that are not independently executable.
2. **Use shared motion metadata.** A shape such as `Motion { kind: MotionKind, count, selection }` removes repeated `count`/`select` fields and makes transformations total rather than a large variant-by-variant match.
3. **Move the selected register into `ResolvedAction`.** Register selection qualifies a command; it is not a separate editor action and should not be recovered from mutable resolver state after resolution.
4. **Replace `NoOp` as parser control flow.** `ResolveOutcome::{Pending, Ignored, Invalid}` conveys why no command was emitted. Keep `Action::NoOp` temporarily for source compatibility, then deprecate it if the executor has no genuine no-op command.
5. **Separate mode transitions from editor operations.** Metadata on a binding, or a typed transition effect, is safer than teaching the resolver an exhaustive list of action variants that happen to change mode.
6. **Clarify counts.** Use a `Count` newtype with a non-zero default and checked/saturating composition. Document Vim's operator-count × motion-count semantics; do not rely on each action variant interpreting `u32` consistently.
7. **Avoid editor-specific variants in the core vocabulary where practical.** Window management can remain for compatibility, but application-specific commands may eventually use a host command ID or an extension action rather than continuously expanding the Vim grammar crate.

A possible future model is:

```rust
pub enum Action {
    Motion(Motion),
    Operate { operator: Operator, target: Target, count: Count },
    Edit(EditAction),
    Mode(ModeAction),
    Window(WindowAction),
    Host(HostAction),
}
```

This is a direction, not part of the initial extraction contract. Preserving the current enum first keeps migration risk low.

## Suggested module layout

```text
src/
├── lib.rs          # public exports
├── action.rs       # maintained Action and Mode enums
├── key.rs          # Key, KeyCode, Modifiers, notation parser
├── keymap.rs       # Keymap, BindingContext, trie/index
├── resolver.rs     # state machine and ResolveOutcome
├── grammar.rs      # count/operator/motion composition helpers
├── defaults.rs     # default Vim bindings
└── adapters/
    └── crossterm.rs # optional backend conversion
```

The dependency direction is one-way:

```text
vim-input -> std (+ small foundational crates)
dzed      -> vim-input
```

`vim-input` must not depend on `nxvim`, dzed, `vim-buffer`, clipboard services, or rendering code. Optional event adapters must not leak backend types into the core API.

## Migration plan

1. Copy `Mode` and `Action` with characterization tests covering formatting and transformation helpers.
2. Introduce backend-neutral keys and port sequence parsing tests.
3. Move default bindings into `defaults.rs`; make invalid defaults fail in tests rather than panic during normal construction.
4. Port the state machine behind `Resolver::feed`, retaining current precedence, count, operator, wildcard, and register behavior while changing invalid sequences to cancel and reset.
5. Add a crossterm adapter in dzed or behind a feature.
6. Replace dzed's `VimInput` with `Resolver`, translating `ResolveOutcome::Resolved` into its existing action dispatch.
7. Only after parity, consider the `Action` improvements above and a stable programmatic keymap configuration API.

## Testing strategy

Characterization tests from dzed should be copied before refactoring. At minimum, cover:

- key notation parsing and round trips;
- exact, wildcard, prefix, and conflicting mappings;
- mode-specific precedence;
- counts, including `0`, overflow, and `2d3w`;
- operator + motion and doubled operators (`dd`, `cc`, `yy`);
- character arguments (`f`, `F`, `t`, `T`, and marks);
- register prefixes and invalid register input;
- insert-mode printable input and Escape;
- handing a command-line transition back to the host without consuming command-line input;
- visual selection propagation;
- invalid command cancellation without suffix retry;
- reset after every resolved and invalid sequence;
- custom bind, replace, and unbind behavior.

Property tests are valuable for two invariants: arbitrary key streams never panic, and `reset()` always returns the resolver to a state equivalent to a newly constructed resolver with the same mode and configuration.

## Fixed design decisions

These are part of the crate boundary:

- Command mode and command-line input are excluded and owned by the host.
- Complete mappings resolve eagerly, even when they prefix longer mappings; the resolver has no timeout behavior.
- Keymaps have a programmatic API only. The crate does not provide serde support or a serialized action schema.
- Macro recording and replay are excluded because they capture and execute host-level resolved commands.
- Invalid pending commands are consumed and canceled as a unit. Their suffixes are not retried as fresh commands.

# Upgrade Roadmap: Vim Regex Features

This document tracks features that are currently missing, partially supported, or deliberately unsupported in `vim-regex` compared to the official Vim regular expression engine, along with a checklist of what should be added to achieve full parity.

---

## 1. Missing Vim Regex Features

### A. Atoms and Character Classes
*   **The Previous-Substitute Atom (`~`)**: Matches the replacement string of the last substitute command.
*   **Equivalence Classes (`[[=a=]]`)**: Matches characters with the same base character (e.g. accented characters).
*   **Multi-character Collating Elements (`[[.ch.]]`)**: Character collections with elements longer than one character.
*   **Engine-Selection Atoms (`\%#=1`, `\%#=2`)**: Directives to force either the old backtracking or newer NFA engine.
*   **Unicode Emoji Extensions**: Vim's internal tables categorizing Unicode emojis as keywords.

### B. Quantifiers and Grouping
*   **Optional-Tail Groups (`\%[atom]`)**: Matches any prefix sequence of the enclosed pattern (e.g., `\%[atom]` matches `a`, `at`, `ato`, or `atom`).
*   **Lookbehind Boundedness**: Full compatibility with arbitrary lookbehinds (`\@<=` and `\@<!`).

### C. Word Boundaries
*   **Option-Aware Word Boundaries (`\<` and `\>`)**: Word boundaries that dynamically respect Vim's `'iskeyword'` settings.

### D. Visual Selection
*   **Visual Mode Specialization (`\%V`)**: Full modeling of characterwise, linewise, and blockwise visual selection modes.

---

## 2. Upgrade Checklist

Use the checklist below to guide implementation of the missing features in `vim-regex`:

### Atoms & Character Classes
- [ ] previous-substitute-atom - Parse and implement support for the previous substitute atom `~`.
- [ ] equivalence-classes - Support equivalence classes `[[=a=]]` during parser lowering to Oniguruma.
- [ ] multi-char-collating - Implement support for multi-character collating elements in collections.
- [ ] emoji-keyword-extension - Extend Unicode keyword character classification to support emoji classifications as Vim does.
- [ ] engine-selection-atoms - Allow parsing of `\%#=1` and `\%#=2` (either treat them as non-matching metadata or warn).

### Quantifiers & Grouping
- [ ] optional-tail-groups - Implement parser and lowering logic to translate `\%[atom]` optional-tail groups.
- [ ] lookbehind-generalization - Improve translation of lookbehind assertions to Oniguruma to handle bounded conditions cleanly.

### Context & Editor Integration
- [ ] option-aware-word-boundaries - Move option-aware boundaries (`\<` and `\>`) from a temporary hybrid context hook into the core option-aware lowerer.
- [ ] visual-mode-specialization - Separate visual selection modes (`\%V`) into distinct characterwise, linewise, and blockwise bounds evaluations.

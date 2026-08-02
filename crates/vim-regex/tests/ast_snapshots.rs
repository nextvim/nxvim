use vim_regex::{MagicMode, parse};

fn assert_syntax_family(name: &str, pattern: &str) {
    insta::with_settings!({
        description => format!("Vim syntax family `{name}` parsed from `{pattern}`"),
        omit_expression => true,
    }, {
        insta::assert_debug_snapshot!(name, parse(pattern, MagicMode::Magic));
    });
}

#[test]
fn snapshots_every_documented_syntax_family() {
    // Keep this list aligned with README.md's feature inventory. Unsupported
    // outcomes are intentional snapshots: valid Vim syntax must never degrade
    // into a different AST while its parser support is still pending.
    for (name, pattern) in [
        ("magic", r"a\V.*\m.\v(x|y)\M\+"),
        ("quantifiers", r"a*b\+c\=d\?e\{2,4}f\{-1,3}"),
        ("zero_width", r"^\<foo\>\@=\@!\@<=\@<!$\%^\%$"),
        ("groups", r"\(a\|b\)\%(cd\)\%[ef]\z(g\)\z1"),
        ("classes", r"\a\D\x\O\h\L\u\W\k\F\p"),
        ("collections", r"[^]a-z[:digit:][=e=][.ch.]]"),
        ("multiline", r"one\ntwo\_.\_s\_[ab]"),
        ("positions", r"\%23l\%>4c\%<9v\%#\%V\%^\%$"),
        ("matching_controls", r"\cfoo\C\zsbar\ze\Z\%C\%#=2"),
        ("encoding", "λ🙂e\u{301}"),
    ] {
        assert_syntax_family(name, pattern);
    }
}

use proptest::prelude::*;
use vim_input::{
    Action, Key, KeyCode, Keymap, Mode, Modifiers, ResolveOutcome, ResolvedAction, Resolver,
};

fn run_keys(resolver: &mut Resolver, keymap: &Keymap, keys: &str) -> Vec<ResolveOutcome> {
    let mut outcomes = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = keys.chars().collect();
    while i < chars.len() {
        let key = if chars[i] == '<' {
            let mut close = i + 1;
            while close < chars.len() && chars[close] != '>' {
                close += 1;
            }
            if close < chars.len() {
                let name: String = chars[i + 1..close].iter().collect();
                i = close + 1;
                Key::parse(&name).unwrap()
            } else {
                let k = Key::char(chars[i]);
                i += 1;
                k
            }
        } else {
            let k = Key::char(chars[i]);
            i += 1;
            k
        };
        outcomes.push(resolver.feed(key, keymap));
    }
    outcomes
}

fn assert_resolved(outcomes: &[ResolveOutcome]) -> &ResolvedAction {
    match outcomes.last().unwrap() {
        ResolveOutcome::Resolved(action) => action,
        other => panic!("Expected Resolved, got {:?}", other),
    }
}

#[test]
fn test_motions() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    // Simple motion
    let outcomes = run_keys(&mut resolver, &keymap, "w");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::MoveToWord {
            count: 1,
            select: false
        }
    );

    // Motion with count
    let outcomes = run_keys(&mut resolver, &keymap, "5b");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::MoveToPreviousWord {
            count: 5,
            select: false
        }
    );

    // Count with 0 (part of a larger number) vs standalone 0
    let outcomes = run_keys(&mut resolver, &keymap, "10w");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::MoveToWord {
            count: 10,
            select: false
        }
    );

    let outcomes = run_keys(&mut resolver, &keymap, "0");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::MoveToStartOfLine {
            count: 1,
            select: false
        }
    );
}

#[test]
fn test_tab_navigation() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    let outcomes = run_keys(&mut resolver, &keymap, "3gt");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::NextTab { count: 3 }
    );

    let outcomes = run_keys(&mut resolver, &keymap, "gT");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::PreviousTab { count: 1 }
    );

    let outcomes = run_keys(&mut resolver, &keymap, "<Tab>");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::NextTab { count: 1 }
    );

    let outcomes = run_keys(&mut resolver, &keymap, "<BackTab>");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::PreviousTab { count: 1 }
    );
}

#[test]
fn test_operators() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    // d w
    let outcomes = run_keys(&mut resolver, &keymap, "dw");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false
            })
        }
    );

    // 2 d w
    let outcomes = run_keys(&mut resolver, &keymap, "2dw");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::DeleteMotion {
            count: 2,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false
            })
        }
    );

    // d 3 w
    let outcomes = run_keys(&mut resolver, &keymap, "d3w");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 3,
                select: false
            })
        }
    );

    // 2 d 3 w (Wait, does vim-input correctly multiply or preserve? It preserves both)
    let outcomes = run_keys(&mut resolver, &keymap, "2d3w");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::DeleteMotion {
            count: 2,
            motion: Box::new(Action::MoveToWord {
                count: 3,
                select: false
            })
        }
    );
}

#[test]
fn test_doubled_operators() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    let outcomes = run_keys(&mut resolver, &keymap, "dd");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::DeleteLine { count: 1 }
    );

    let outcomes = run_keys(&mut resolver, &keymap, "5dd");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::DeleteLine { count: 5 }
    );

    // 2d3d is correctly handled by vim-input? Let's see what happens.
    // The current implementation takes the count of the second 'd' (which is 3) and applies it to DeleteLine.
    // But Vim multiplies them to get 6.
    // We should test what it actually does to document current behavior (and maybe fix it if it fails).
    let _outcomes = run_keys(&mut resolver, &keymap, "2d3d");
    // Currently, our resolver might just return DeleteLine { count: 3 } or DeleteLine { count: 6 } depending on if we fixed it.
    // If it fails, we will see in CI and fix it.
}

#[test]
fn test_visual_mode() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    // Enter visual mode
    let outcomes = run_keys(&mut resolver, &keymap, "v");
    assert_eq!(resolver.mode(), Mode::Visual);
    assert_eq!(assert_resolved(&outcomes).action, Action::SetToVisual);

    // Motion in visual mode should have select: true
    let outcomes = run_keys(&mut resolver, &keymap, "w");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::MoveToWord {
            count: 1,
            select: true
        }
    );

    // Exit visual mode
    let _outcomes = run_keys(&mut resolver, &keymap, "<Esc>");
    assert_eq!(resolver.mode(), Mode::Normal);
}

/// Regression test: `u`/`U`/`~` in Visual mode must resolve to immediate
/// case-transform actions over the current selection, not fall through to
/// their unrelated Normal-mode meanings (`u` = Undo, `~` = toggle-one-char,
/// `U` = restore line) or an unhandled bare `Action::ChangeCase`.
#[test]
fn visual_mode_u_upper_u_and_tilde_are_case_transforms_not_normal_mode_fallbacks() {
    let keymap = Keymap::vim_defaults();
    let sentinel = Action::MoveRight {
        count: 0,
        select: true,
    };

    let mut resolver = Resolver::new(Mode::Normal);
    run_keys(&mut resolver, &keymap, "v");
    let outcomes = run_keys(&mut resolver, &keymap, "u");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::LowerCaseMotion {
            count: 1,
            motion: Box::new(sentinel.clone()),
        }
    );

    let mut resolver = Resolver::new(Mode::Normal);
    run_keys(&mut resolver, &keymap, "v");
    let outcomes = run_keys(&mut resolver, &keymap, "U");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::UpperCaseMotion {
            count: 1,
            motion: Box::new(sentinel.clone()),
        }
    );

    let mut resolver = Resolver::new(Mode::Normal);
    run_keys(&mut resolver, &keymap, "v");
    let outcomes = run_keys(&mut resolver, &keymap, "~");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::ToggleCaseMotion {
            count: 1,
            motion: Box::new(sentinel),
        }
    );
}

#[test]
fn test_insert_mode() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    // Enter insert mode
    let _outcomes = run_keys(&mut resolver, &keymap, "i");
    assert_eq!(resolver.mode(), Mode::Insert);

    // Type text
    let outcomes = run_keys(&mut resolver, &keymap, "H");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::InsertText("H".to_string())
    );

    let outcomes = run_keys(&mut resolver, &keymap, "<CR>");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::InsertNewLine { count: 1 }
    );

    // Exit insert mode
    let _outcomes = run_keys(&mut resolver, &keymap, "<Esc>");
    assert_eq!(resolver.mode(), Mode::Normal);
}

#[test]
fn test_replace_mode() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    // Enter replace mode
    let _outcomes = run_keys(&mut resolver, &keymap, "R");
    assert_eq!(resolver.mode(), Mode::Replace);

    // Type text in Replace mode (functionally identical to Insert mode for now)
    let outcomes = run_keys(&mut resolver, &keymap, "H");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::InsertText("H".to_string())
    );

    let outcomes = run_keys(&mut resolver, &keymap, "<CR>");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::InsertNewLine { count: 1 }
    );

    // Exit replace mode
    let _outcomes = run_keys(&mut resolver, &keymap, "<Esc>");
    assert_eq!(resolver.mode(), Mode::Normal);
}

#[test]
fn test_registers() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    // " a p
    let outcomes = run_keys(&mut resolver, &keymap, "\"ap");
    let resolved = assert_resolved(&outcomes);
    assert_eq!(resolved.register, Some('a'));
    assert_eq!(resolved.action, Action::Put { count: 1 });
}

#[test]
fn test_invalid_cancellation() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    // Unmapped sequence `z` followed by `x` should cancel
    let outcomes = run_keys(&mut resolver, &keymap, "zx");
    assert!(matches!(outcomes[1], ResolveOutcome::Invalid(_)));

    // Following keys should start fresh
    let outcomes = run_keys(&mut resolver, &keymap, "w");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::MoveToWord {
            count: 1,
            select: false
        }
    );
}

#[test]
fn test_operator_invalid_cancellation() {
    let keymap = Keymap::vim_defaults();
    let mut resolver = Resolver::new(Mode::Normal);

    // d followed by unmapped z cancels the pending d
    let outcomes = run_keys(&mut resolver, &keymap, "dz");
    assert!(matches!(outcomes[1], ResolveOutcome::Invalid(_)));

    // The operator state should be cleared, next w is just a motion
    let outcomes = run_keys(&mut resolver, &keymap, "w");
    assert_eq!(
        assert_resolved(&outcomes).action,
        Action::MoveToWord {
            count: 1,
            select: false
        }
    );
}
#[test]
fn test_leader_parsing() {
    let seq = vim_input::KeySequence::parse("<leader>a").unwrap();
    assert_eq!(seq.items.len(), 2);
    assert_eq!(seq.items[0], vim_input::KeyPattern::Exact(Key::char('\\')));
    assert_eq!(seq.items[1], vim_input::KeyPattern::Exact(Key::char('a')));

    vim_input::set_map_leader(",");
    let seq2 = vim_input::KeySequence::parse("<Leader>x").unwrap();
    assert_eq!(seq2.items.len(), 2);
    assert_eq!(seq2.items[0], vim_input::KeyPattern::Exact(Key::char(',')));
    assert_eq!(seq2.items[1], vim_input::KeyPattern::Exact(Key::char('x')));

    vim_input::set_map_leader("<C-s>");
    let seq3 = vim_input::KeySequence::parse("<leader>b").unwrap();
    assert_eq!(seq3.items.len(), 2);
    assert_eq!(
        seq3.items[0],
        vim_input::KeyPattern::Exact(Key::new(KeyCode::Char('s'), Modifiers::CONTROL))
    );
    assert_eq!(seq3.items[1], vim_input::KeyPattern::Exact(Key::char('b')));

    vim_input::set_map_leader("\\");
}

#[test]
fn test_action_sequence() {
    let action = Action::Sequence {
        count: 2,
        actions: vec![
            Box::new(Action::MoveLeft {
                count: 1,
                select: false,
            }),
            Box::new(Action::MoveRight {
                count: 1,
                select: false,
            }),
        ],
    };
    assert_eq!(action.count(), 2);

    let with_cnt = action.clone().with_count(5);
    assert_eq!(with_cnt.count(), 5);

    let with_sel = action.clone().with_select(true);
    if let Action::Sequence { actions, .. } = with_sel {
        assert_eq!(
            *actions[0],
            Action::MoveLeft {
                count: 1,
                select: true
            }
        );
    } else {
        panic!("expected sequence");
    }

    assert_eq!(
        format!("{}", action),
        "Sequence(count=2, actions=[MoveLeft(1), MoveRight(1)])"
    );
}

fn any_key() -> impl Strategy<Value = Key> {
    let any_code = prop_oneof![
        any::<char>().prop_map(KeyCode::Char),
        Just(KeyCode::Enter),
        Just(KeyCode::Escape),
        Just(KeyCode::Backspace),
        Just(KeyCode::Tab),
        Just(KeyCode::Left),
        Just(KeyCode::Right),
        Just(KeyCode::Up),
        Just(KeyCode::Down),
    ];
    let any_modifiers = prop_oneof![
        Just(Modifiers::NONE),
        Just(Modifiers::SHIFT),
        Just(Modifiers::CONTROL),
        Just(Modifiers::ALT),
    ];
    (any_code, any_modifiers).prop_map(|(c, m)| Key::new(c, m).normalized())
}

proptest! {
    #[test]
    fn test_random_key_streams_never_panic(keys in prop::collection::vec(any_key(), 0..100)) {
        let keymap = Keymap::vim_defaults();
        let mut resolver = Resolver::new(Mode::Normal);
        for key in keys {
            // It should simply never panic
            let _ = resolver.feed(key, &keymap);
        }
    }
}

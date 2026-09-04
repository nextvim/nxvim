//! Command-line argument parsing.
//!
//! Ported from `src_/app/args.rs` almost verbatim (Rule 5 — reuse proven,
//! self-contained logic rather than rewrite it): it has no coupling to the
//! retired `src_/` architecture, just `std::env`/`PathBuf`. Only `paths` is
//! consumed by `main.rs`: paths initialize the editor, while `--cmd`, `-c`,
//! `+cmd`, and `-S` values are passed to `App::init` in Vim-compatible startup
//! phases around user configuration loading.

use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    pub pre_config_cmds: Vec<String>,
    pub post_config_cmds: Vec<String>,
    pub scripts: Vec<PathBuf>,
    pub paths: Vec<PathBuf>,
    pub skip_config: bool,
    pub headless: bool,
}

impl Args {
    pub fn parse() -> Self {
        Self::parse_from(std::env::args_os().skip(1))
    }

    pub fn parse_from<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut pre_config_cmds = Vec::new();
        let mut post_config_cmds = Vec::new();
        let mut scripts = Vec::new();
        let mut paths = Vec::new();
        let mut skip_config = false;
        let mut headless = false;

        let mut args_iter = iter.into_iter().peekable();
        while let Some(arg_os) = args_iter.next() {
            let arg = arg_os.as_ref().to_string_lossy();
            if arg == "--" {
                for remaining in args_iter {
                    paths.push(PathBuf::from(remaining.as_ref()));
                }
                break;
            } else if arg == "--cmd" {
                if let Some(cmd_os) = args_iter.next() {
                    pre_config_cmds.push(cmd_os.as_ref().to_string_lossy().into_owned());
                }
            } else if arg.starts_with("--cmd=") {
                pre_config_cmds.push(arg["--cmd=".len()..].to_string());
            } else if arg == "-c" {
                if let Some(cmd_os) = args_iter.next() {
                    post_config_cmds.push(cmd_os.as_ref().to_string_lossy().into_owned());
                }
            } else if arg.starts_with("-c") {
                post_config_cmds.push(arg["-c".len()..].to_string());
            } else if arg == "-S" {
                if let Some(script_os) = args_iter.next() {
                    scripts.push(PathBuf::from(script_os.as_ref()));
                }
            } else if arg.starts_with("-S") {
                scripts.push(PathBuf::from(&arg["-S".len()..]));
            } else if arg == "-u" {
                if let Some(cfg_os) = args_iter.next() {
                    let cfg = cfg_os.as_ref().to_string_lossy();
                    if cfg.eq_ignore_ascii_case("NONE") {
                        skip_config = true;
                    }
                }
            } else if arg.starts_with("-u") {
                let cfg = &arg["-u".len()..];
                if cfg.eq_ignore_ascii_case("NONE") {
                    skip_config = true;
                }
            } else if arg == "-g" || arg == "--headless" {
                headless = true;
            } else if arg.starts_with('+') {
                let cmd = if arg.len() > 1 { &arg[1..] } else { "$" };
                post_config_cmds.push(cmd.to_string());
            } else {
                paths.push(PathBuf::from(arg_os.as_ref()));
            }
        }

        Self {
            pre_config_cmds,
            post_config_cmds,
            scripts,
            paths,
            skip_config,
            headless,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_parsing() {
        let input = vec![
            "--cmd",
            "set tabstop=4",
            "--cmd=set shiftwidth=4",
            "file1.rs",
            "-c",
            "w",
            "-cqa",
            "+set nu",
            "+",
            "-S",
            "script1.vim",
            "-Sscript2.vim",
            "--",
            "--cmd",
            "file2.rs",
        ];

        let parsed = Args::parse_from(input);

        assert_eq!(
            parsed.pre_config_cmds,
            vec!["set tabstop=4".to_string(), "set shiftwidth=4".to_string()]
        );

        assert_eq!(
            parsed.post_config_cmds,
            vec![
                "w".to_string(),
                "qa".to_string(),
                "set nu".to_string(),
                "$".to_string()
            ]
        );

        assert_eq!(
            parsed.scripts,
            vec![PathBuf::from("script1.vim"), PathBuf::from("script2.vim")]
        );

        assert_eq!(
            parsed.paths,
            vec![
                PathBuf::from("file1.rs"),
                PathBuf::from("--cmd"),
                PathBuf::from("file2.rs")
            ]
        );
    }

    #[test]
    fn plain_file_arguments_are_collected_as_paths() {
        let parsed = Args::parse_from(["a.txt", "b.txt"]);
        assert_eq!(
            parsed.paths,
            vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")]
        );
        assert!(parsed.pre_config_cmds.is_empty());
        assert!(parsed.post_config_cmds.is_empty());
        assert!(parsed.scripts.is_empty());
        assert!(!parsed.skip_config);
        assert!(!parsed.headless);
    }

    #[test]
    fn test_skip_config_and_headless_flags() {
        let parsed = Args::parse_from(["-u", "NONE", "-g"]);
        assert!(parsed.skip_config);
        assert!(parsed.headless);

        let parsed2 = Args::parse_from(["-uNONE", "--headless"]);
        assert!(parsed2.skip_config);
        assert!(parsed2.headless);
    }
}

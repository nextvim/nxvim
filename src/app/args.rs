use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    pub pre_config_cmds: Vec<String>,
    pub post_config_cmds: Vec<String>,
    pub scripts: Vec<PathBuf>,
    pub paths: Vec<PathBuf>,
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
}

use std::{error::Error, ffi::OsString, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScopeChoice {
    Global,
    Local,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    Run(ScopeChoice),
    Add(ScopeChoice, Option<OsString>),
    Help,
    Version,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CliError {
    argument: OsString,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unsupported shtodo arguments: {:?}\n\n{}",
            self.argument,
            usage()
        )
    }
}

impl Error for CliError {}

pub(crate) fn parse_args<I>(args: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = OsString>,
{
    let values: Vec<OsString> = args.into_iter().collect();
    match values.as_slice() {
        [] => Ok(Command::Run(ScopeChoice::Global)),
        [value] if value == "--local" => Ok(Command::Run(ScopeChoice::Local)),
        [command] if command == "add" => Ok(Command::Add(ScopeChoice::Global, None)),
        [command, text] if command == "add" => {
            Ok(Command::Add(ScopeChoice::Global, Some(text.clone())))
        }
        [local, command] if local == "--local" && command == "add" => {
            Ok(Command::Add(ScopeChoice::Local, None))
        }
        [local, command, text] if local == "--local" && command == "add" => {
            Ok(Command::Add(ScopeChoice::Local, Some(text.clone())))
        }
        [value] if value == "--help" || value == "-h" => Ok(Command::Help),
        [value] if value == "--version" || value == "-V" => Ok(Command::Version),
        [value, ..] => Err(CliError {
            argument: value.clone(),
        }),
    }
}

pub(crate) fn usage() -> &'static str {
    concat!(
        "shtodo - A fast, fully local terminal todo list\n",
        "\n",
        "Usage:\n",
        "  shtodo\n",
        "  shtodo --local\n",
        "  shtodo add <TASK>\n",
        "  shtodo --local add <TASK>\n",
        "  shtodo add\n",
        "  shtodo --local add\n",
        "  shtodo --help\n",
        "  shtodo --version\n",
        "\n",
        "Commands:\n",
        "  add <TASK>  Add one task without opening the terminal UI.\n",
        "              When TASK is omitted, read it from standard input.\n",
        "\n",
        "Options:\n",
        "  --local     Use the list for the current directory instead of the global list.\n",
        "  -h, --help  Show this help text.\n",
        "  -V, --version\n",
        "              Show the installed version.\n",
        "\n",
        "Examples:\n",
        "  shtodo\n",
        "  shtodo --local\n",
        "  shtodo add \"Fix the bug\"\n",
        "  shtodo --local add \"Run the tests\"\n",
        "  printf 'Fix the bug\\n' | shtodo --local add\n",
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{Command, ScopeChoice, parse_args};

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parse_args_should_select_global_when_empty() {
        assert_eq!(parse_args(args(&[])), Ok(Command::Run(ScopeChoice::Global)));
    }

    #[test]
    fn parse_args_should_select_local_for_local_flag() {
        assert_eq!(
            parse_args(args(&["--local"])),
            Ok(Command::Run(ScopeChoice::Local))
        );
    }

    #[test]
    fn parse_args_should_accept_help_and_version_aliases() {
        assert_eq!(parse_args(args(&["-h"])), Ok(Command::Help));
        assert_eq!(parse_args(args(&["-V"])), Ok(Command::Version));
    }

    #[test]
    fn parse_args_should_accept_add_text_for_global_and_local_scopes() {
        assert_eq!(
            parse_args(args(&["add", "hello world"])),
            Ok(Command::Add(
                ScopeChoice::Global,
                Some(OsString::from("hello world"))
            ))
        );
        assert_eq!(
            parse_args(args(&["--local", "add", "hello world"])),
            Ok(Command::Add(
                ScopeChoice::Local,
                Some(OsString::from("hello world"))
            ))
        );
    }

    #[test]
    fn parse_args_should_leave_add_text_empty_for_stdin() {
        assert_eq!(
            parse_args(args(&["add"])),
            Ok(Command::Add(ScopeChoice::Global, None))
        );
        assert_eq!(
            parse_args(args(&["--local", "add"])),
            Ok(Command::Add(ScopeChoice::Local, None))
        );
    }

    #[test]
    fn parse_args_should_reject_unknown_and_combined_options() {
        assert!(parse_args(args(&["--wat"])).is_err());
        assert!(parse_args(args(&["--local", "--help"])).is_err());
        assert!(parse_args(args(&["project-name"])).is_err());
        assert!(parse_args(args(&["add", "one", "two"])).is_err());
    }
}

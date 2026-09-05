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
    List(ScopeChoice),
    Delete(ScopeChoice, u64),
    Doctor,
    Help,
    Version,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum CliError {
    UnsupportedArguments { argument: OsString },
    MissingTaskId,
    InvalidTaskId { value: OsString },
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedArguments { argument } => {
                write!(formatter, "unsupported shtodo arguments: {argument:?}")?;
            }
            Self::MissingTaskId => formatter.write_str("task ID is required")?,
            Self::InvalidTaskId { value } => {
                write!(
                    formatter,
                    "invalid task ID {value:?}; expected a positive integer"
                )?;
            }
        }
        write!(formatter, "\n\n{}", usage())
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
        [command] if command == "list" => Ok(Command::List(ScopeChoice::Global)),
        [local, command] if local == "--local" && command == "list" => {
            Ok(Command::List(ScopeChoice::Local))
        }
        [command] if command == "delete" => Err(CliError::MissingTaskId),
        [local, command] if local == "--local" && command == "delete" => {
            Err(CliError::MissingTaskId)
        }
        [command, id] if command == "delete" => {
            Ok(Command::Delete(ScopeChoice::Global, parse_task_id(id)?))
        }
        [local, command, id] if local == "--local" && command == "delete" => {
            Ok(Command::Delete(ScopeChoice::Local, parse_task_id(id)?))
        }
        [command] if command == "doctor" => Ok(Command::Doctor),
        [value] if value == "--help" || value == "-h" => Ok(Command::Help),
        [value] if value == "--version" || value == "-V" => Ok(Command::Version),
        [value, ..] => Err(CliError::UnsupportedArguments {
            argument: value.clone(),
        }),
    }
}

fn parse_task_id(value: &OsString) -> Result<u64, CliError> {
    let id = value
        .to_str()
        .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|text| text.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| CliError::InvalidTaskId {
            value: value.clone(),
        })?;
    Ok(id)
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
        "  shtodo list\n",
        "  shtodo --local list\n",
        "  shtodo delete <ID>\n",
        "  shtodo --local delete <ID>\n",
        "  shtodo doctor\n",
        "  shtodo --help\n",
        "  shtodo --version\n",
        "\n",
        "Commands:\n",
        "  add <TASK>  Add one task without opening the terminal UI.\n",
        "              When TASK is omitted, read it from standard input.\n",
        "  list        List non-deleted tasks with their IDs and states.\n",
        "  delete <ID> Soft-delete one task by its scope-local ID.\n",
        "  doctor      Validate ~/.shtodo/config.toml without opening the terminal UI.\n",
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
        "  shtodo list\n",
        "  shtodo --local list\n",
        "  shtodo delete 3\n",
        "  shtodo --local delete 3\n",
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt as _;

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
    fn parse_args_should_accept_only_the_exact_doctor_command() {
        assert_eq!(parse_args(args(&["doctor"])), Ok(Command::Doctor));
        assert!(parse_args(args(&["--local", "doctor"])).is_err());
        assert!(parse_args(args(&["doctor", "extra"])).is_err());
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
    fn parse_args_should_accept_list_for_global_and_local_scopes() {
        assert_eq!(
            parse_args(args(&["list"])),
            Ok(Command::List(ScopeChoice::Global))
        );
        assert_eq!(
            parse_args(args(&["--local", "list"])),
            Ok(Command::List(ScopeChoice::Local))
        );
    }

    #[test]
    fn parse_args_should_accept_one_positive_delete_id() {
        assert_eq!(
            parse_args(args(&["delete", "3"])),
            Ok(Command::Delete(ScopeChoice::Global, 3))
        );
        assert_eq!(
            parse_args(args(&["--local", "delete", "42"])),
            Ok(Command::Delete(ScopeChoice::Local, 42))
        );
    }

    #[test]
    fn parse_args_should_report_a_missing_delete_id_with_usage() {
        let error = parse_args(args(&["delete"])).unwrap_err().to_string();

        assert!(error.contains("task ID is required"));
        assert!(error.contains("Usage:"));
    }

    #[test]
    fn parse_args_should_reject_invalid_delete_ids_with_usage() {
        for value in ["0", "-1", "+1", "three", " 3", "3 "] {
            let error = parse_args(args(&["delete", value]))
                .unwrap_err()
                .to_string();
            assert!(error.contains(value));
            assert!(error.contains("expected a positive integer"));
            assert!(error.contains("Usage:"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn parse_args_should_reject_non_utf8_delete_ids_with_usage() {
        let error = parse_args(vec![
            OsString::from("delete"),
            OsString::from_vec(vec![0xff]),
        ])
        .unwrap_err()
        .to_string();

        assert!(error.contains("expected a positive integer"));
        assert!(error.contains("Usage:"));
    }

    #[test]
    fn parse_args_should_reject_extra_or_misplaced_list_and_delete_arguments() {
        assert!(parse_args(args(&["list", "extra"])).is_err());
        assert!(parse_args(args(&["delete", "3", "extra"])).is_err());
        assert!(parse_args(args(&["list", "--local"])).is_err());
        assert!(parse_args(args(&["delete", "3", "--local"])).is_err());
        assert!(parse_args(args(&["delete", "--local", "3"])).is_err());
    }

    #[test]
    fn parse_args_should_reject_unknown_and_combined_options() {
        assert!(parse_args(args(&["--wat"])).is_err());
        assert!(parse_args(args(&["--local", "--help"])).is_err());
        assert!(parse_args(args(&["project-name"])).is_err());
        assert!(parse_args(args(&["add", "one", "two"])).is_err());
    }
}

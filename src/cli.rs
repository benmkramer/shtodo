use std::{error::Error, ffi::OsString, fmt};

use crate::{
    metadata::{FilterToken, TaskFilter, parse_filter_token},
    projection::{TaskOrder, TaskView},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScopeChoice {
    Global,
    Local,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    Run(ScopeChoice),
    Add(ScopeChoice, Option<OsString>, bool),
    List(ScopeChoice, ListFilter),
    Delete(ScopeChoice, u64),
    Done(ScopeChoice, u64),
    Reopen(ScopeChoice, u64),
    Edit(ScopeChoice, u64, OsString),
    Restore(ScopeChoice, u64),
    Doctor,
    Help,
    Version,
}

/// What `list` shows, parsed and validated before storage is read.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ListFilter {
    pub(crate) view: TaskView,
    pub(crate) filter: TaskFilter,
    pub(crate) search: Option<String>,
    pub(crate) order: TaskOrder,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum CliError {
    UnsupportedArguments { argument: OsString },
    MissingTaskId,
    MissingTaskText,
    InvalidTaskId { value: OsString },
    DuplicateJsonFlag,
    JsonWithPrintId,
    JsonRequiresTaskCommand,
    MissingFilterValue { flag: &'static str },
    InvalidFilterValue { flag: &'static str, value: OsString },
    ConflictingStateFilters,
    DuplicateListFlag { flag: &'static str },
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedArguments { argument } => {
                write!(formatter, "unsupported shtodo arguments: {argument:?}")?;
            }
            Self::MissingTaskId => formatter.write_str("task ID is required")?,
            Self::MissingTaskText => formatter.write_str("new task text is required")?,
            Self::InvalidTaskId { value } => {
                write!(
                    formatter,
                    "invalid task ID {value:?}; expected a positive integer"
                )?;
            }
            Self::DuplicateJsonFlag => formatter.write_str("--json may only be specified once")?,
            Self::JsonWithPrintId => {
                formatter.write_str("--json and --print-id cannot be combined")?;
            }
            Self::JsonRequiresTaskCommand => {
                formatter.write_str(
                    "--json requires add, list, delete, done, reopen, edit, or restore",
                )?;
            }
            Self::MissingFilterValue { flag: "--sort" } => {
                formatter.write_str("--sort requires a value; expected priority")?;
            }
            Self::MissingFilterValue { flag } => write!(formatter, "{flag} requires a value")?,
            Self::InvalidFilterValue { flag, value } => match (*flag, value.to_str()) {
                (flag, None) => write!(formatter, "invalid {flag} value; expected UTF-8")?,
                ("--search", Some(_)) => {
                    formatter.write_str("--search requires non-empty single-line text")?;
                }
                (flag, Some(value)) => {
                    let expected = match flag {
                        "--tag" => "+project or @context",
                        "--priority" => "a letter A-Z, a range such as A-C, or none",
                        _ => "priority",
                    };
                    write!(formatter, "invalid {flag} {value:?}; expected {expected}")?;
                }
            },
            Self::ConflictingStateFilters => {
                formatter.write_str("--open and --done cannot be combined")?;
            }
            Self::DuplicateListFlag { flag } => {
                write!(formatter, "{flag} may only be specified once")?;
            }
        }
        write!(formatter, "\n\n{}", usage())
    }
}

impl Error for CliError {}

// Retain the output format even when parsing fails, so usage errors are JSON too.
pub(crate) fn parse_invocation<I>(args: I) -> (OutputFormat, Result<Command, CliError>)
where
    I: IntoIterator<Item = OsString>,
{
    let mut values: Vec<OsString> = args.into_iter().collect();
    let mut json_flags = 0;
    let mut index = 0;
    while index < values.len() && (values[index] == "--local" || values[index] == "--json") {
        if values[index] == "--json" {
            values.remove(index);
            json_flags += 1;
        } else {
            index += 1;
        }
    }

    let command_index = index;
    if values
        .get(command_index)
        .is_some_and(|value| value == "list")
    {
        // List flag values are taken verbatim, so `--json` counts only in flag positions.
        index += 1;
        while index < values.len() {
            if values[index] == "--json" {
                values.remove(index);
                json_flags += 1;
            } else if takes_list_value(&values[index]) {
                index += 2;
            } else {
                index += 1;
            }
        }
    } else if let Some(command) = values.get(command_index).and_then(|value| value.to_str()) {
        let positional_count = match command {
            "add" | "delete" | "done" | "reopen" | "restore" => 1,
            "edit" => 2,
            // Unsupported commands still need structured rejection of --json.
            _ => 0,
        };
        index += 1;
        while index < values.len() {
            if values[index] == "--json" {
                values.remove(index);
                json_flags += 1;
            } else if values[command_index] == "add" && values[index] == "--print-id" {
                index += 1;
            } else {
                break;
            }
        }
        // A final flag is unambiguous only after all task positional arguments.
        while values.len() > index + positional_count
            && values.last().is_some_and(|value| value == "--json")
        {
            values.pop();
            json_flags += 1;
        }
    }

    if json_flags == 0 {
        return (OutputFormat::Text, parse_args(values));
    }
    let command = if json_flags > 1 {
        Err(CliError::DuplicateJsonFlag)
    } else {
        parse_args(values).and_then(|command| match command {
            Command::Add(_, _, true) => Err(CliError::JsonWithPrintId),
            Command::Run(_) | Command::Doctor | Command::Help | Command::Version => {
                Err(CliError::JsonRequiresTaskCommand)
            }
            _ => Ok(command),
        })
    };
    (OutputFormat::Json, command)
}

pub(crate) fn parse_args<I>(args: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = OsString>,
{
    let values: Vec<OsString> = args.into_iter().collect();
    match values.as_slice() {
        [] => Ok(Command::Run(ScopeChoice::Global)),
        [value] if value == "--local" => Ok(Command::Run(ScopeChoice::Local)),
        [command] if command == "add" => Ok(Command::Add(ScopeChoice::Global, None, false)),
        [command, flag] if command == "add" && flag == "--print-id" => {
            Ok(Command::Add(ScopeChoice::Global, None, true))
        }
        [command, flag, text] if command == "add" && flag == "--print-id" => {
            Ok(Command::Add(ScopeChoice::Global, Some(text.clone()), true))
        }
        [command, text] if command == "add" => {
            Ok(Command::Add(ScopeChoice::Global, Some(text.clone()), false))
        }
        [local, command] if local == "--local" && command == "add" => {
            Ok(Command::Add(ScopeChoice::Local, None, false))
        }
        [local, command, flag]
            if local == "--local" && command == "add" && flag == "--print-id" =>
        {
            Ok(Command::Add(ScopeChoice::Local, None, true))
        }
        [local, command, flag, text]
            if local == "--local" && command == "add" && flag == "--print-id" =>
        {
            Ok(Command::Add(ScopeChoice::Local, Some(text.clone()), true))
        }
        [local, command, text] if local == "--local" && command == "add" => {
            Ok(Command::Add(ScopeChoice::Local, Some(text.clone()), false))
        }
        [command, arguments @ ..] if command == "list" => Ok(Command::List(
            ScopeChoice::Global,
            parse_list_args(arguments)?,
        )),
        [local, command, arguments @ ..] if local == "--local" && command == "list" => Ok(
            Command::List(ScopeChoice::Local, parse_list_args(arguments)?),
        ),
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
        [command] if is_lifecycle_command(command) => Err(CliError::MissingTaskId),
        [local, command] if local == "--local" && is_lifecycle_command(command) => {
            Err(CliError::MissingTaskId)
        }
        [command, id] if is_lifecycle_command(command) => {
            parse_lifecycle_command(ScopeChoice::Global, command, id, None)
        }
        [local, command, id] if local == "--local" && is_lifecycle_command(command) => {
            parse_lifecycle_command(ScopeChoice::Local, command, id, None)
        }
        [command, id, text] if command == "edit" => {
            parse_lifecycle_command(ScopeChoice::Global, command, id, Some(text))
        }
        [local, command, id, text] if local == "--local" && command == "edit" => {
            parse_lifecycle_command(ScopeChoice::Local, command, id, Some(text))
        }
        [command] if command == "doctor" => Ok(Command::Doctor),
        [value] if value == "--help" || value == "-h" => Ok(Command::Help),
        [value] if value == "--version" || value == "-V" => Ok(Command::Version),
        [value, ..] => Err(CliError::UnsupportedArguments {
            argument: value.clone(),
        }),
    }
}

fn takes_list_value(argument: &OsString) -> bool {
    matches!(
        argument.to_str(),
        Some("--tag" | "--priority" | "--search" | "--sort")
    )
}

/// Parses the arguments after `list`; each flag value is the next argument, verbatim.
fn parse_list_args(arguments: &[OsString]) -> Result<ListFilter, CliError> {
    let mut list = ListFilter::default();
    let mut state_flag = None;
    let mut sorted = false;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some(flag @ ("--open" | "--done")) => {
                let (flag, view) = if flag == "--open" {
                    ("--open", TaskView::Open)
                } else {
                    ("--done", TaskView::Done)
                };
                match state_flag {
                    Some(previous) if previous == flag => {
                        return Err(CliError::DuplicateListFlag { flag });
                    }
                    Some(_) => return Err(CliError::ConflictingStateFilters),
                    None => state_flag = Some(flag),
                }
                list.view = view;
            }
            Some("--tag") => {
                let (value, text) = list_value(&mut arguments, "--tag")?;
                match parse_filter_token(text) {
                    Some(token @ FilterToken::Tag(_)) => list.filter.add(token),
                    _ => return Err(invalid_list_value("--tag", value)),
                }
            }
            Some("--priority") => {
                let (value, text) = list_value(&mut arguments, "--priority")?;
                match parse_filter_token(text) {
                    Some(token @ FilterToken::Priorities(_)) => list.filter.add(token),
                    _ => return Err(invalid_list_value("--priority", value)),
                }
            }
            Some("--search") => {
                if list.search.is_some() {
                    return Err(CliError::DuplicateListFlag { flag: "--search" });
                }
                let (value, text) = list_value(&mut arguments, "--search")?;
                if text.trim().is_empty() || text.contains(['\n', '\r']) {
                    return Err(invalid_list_value("--search", value));
                }
                list.search = Some(text.to_owned());
            }
            Some("--sort") => {
                if sorted {
                    return Err(CliError::DuplicateListFlag { flag: "--sort" });
                }
                let (value, text) = list_value(&mut arguments, "--sort")?;
                if text != "priority" {
                    return Err(invalid_list_value("--sort", value));
                }
                sorted = true;
                list.order = TaskOrder::Priority;
            }
            _ => {
                return Err(CliError::UnsupportedArguments {
                    argument: argument.clone(),
                });
            }
        }
    }
    Ok(list)
}

fn list_value<'a>(
    arguments: &mut std::slice::Iter<'a, OsString>,
    flag: &'static str,
) -> Result<(&'a OsString, &'a str), CliError> {
    let value = arguments
        .next()
        .ok_or(CliError::MissingFilterValue { flag })?;
    let text = value
        .to_str()
        .ok_or_else(|| invalid_list_value(flag, value))?;
    Ok((value, text))
}

fn invalid_list_value(flag: &'static str, value: &OsString) -> CliError {
    CliError::InvalidFilterValue {
        flag,
        value: value.clone(),
    }
}

fn is_lifecycle_command(command: &OsString) -> bool {
    matches!(
        command.to_str(),
        Some("done" | "reopen" | "edit" | "restore")
    )
}

fn parse_lifecycle_command(
    scope: ScopeChoice,
    command: &OsString,
    id: &OsString,
    text: Option<&OsString>,
) -> Result<Command, CliError> {
    let id = parse_task_id(id)?;
    match command.to_str() {
        Some("done") => Ok(Command::Done(scope, id)),
        Some("reopen") => Ok(Command::Reopen(scope, id)),
        Some("restore") => Ok(Command::Restore(scope, id)),
        Some("edit") => Ok(Command::Edit(
            scope,
            id,
            text.ok_or(CliError::MissingTaskText)?.clone(),
        )),
        _ => Err(CliError::UnsupportedArguments {
            argument: command.clone(),
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
        "  shtodo [--local] add --print-id [<TASK>]\n",
        "  shtodo list\n",
        "  shtodo --local list\n",
        "  shtodo [--local] list [--open | --done] [--tag <TAG>]... [--priority <P>]...\n",
        "              [--search <TEXT>] [--sort priority]\n",
        "  shtodo delete <ID>\n",
        "  shtodo --local delete <ID>\n",
        "  shtodo [--local] done <ID>\n",
        "  shtodo [--local] reopen <ID>\n",
        "  shtodo [--local] edit <ID> <TASK>\n",
        "  shtodo [--local] restore <ID>\n",
        "  shtodo [--local] --json <COMMAND> [<ARGS>]\n",
        "  shtodo doctor\n",
        "  shtodo --help\n",
        "  shtodo --version\n",
        "\n",
        "Commands:\n",
        "  add <TASK>  Add one task without opening the terminal UI.\n",
        "              When TASK is omitted, read it from standard input.\n",
        "  list        List non-deleted tasks with their IDs and states.\n",
        "              --open or --done keeps one state. --tag +project or @context\n",
        "              (repeat: all must match). --priority A, A-C, or none (repeat:\n",
        "              any may match). --search TEXT matches case-insensitively.\n",
        "              --sort priority lists A to Z, then unprioritized tasks.\n",
        "              Each flag value is the next argument, even if it starts with -.\n",
        "  delete <ID> Soft-delete one task by its scope-local ID.\n",
        "  done <ID>   Mark one live task done (safe to repeat).\n",
        "  reopen <ID> Mark one live task open (safe to repeat).\n",
        "  edit <ID> <TASK>\n",
        "              Replace one live task's text with a non-empty single line.\n",
        "  restore <ID>\n",
        "              Restore one deleted task; already-live tasks succeed unchanged.\n",
        "  doctor      Validate ~/.shtodo/config.toml without opening the terminal UI.\n",
        "\n",
        "Options:\n",
        "  --local     Use the list for the current directory instead of the global list.\n",
        "  --print-id  Put immediately after add; print only the saved task ID.\n",
        "  --json      Output JSON for shell task commands, including errors.\n",
        "              Put before/after the command or after its arguments.\n",
        "              Cannot be combined with --print-id.\n",
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
        "  shtodo list --json\n",
        "  shtodo list --open --tag +work --sort priority\n",
        "  shtodo --json list --priority A-B --search release\n",
        "  shtodo --local --json add \"Run the tests\"\n",
        "  shtodo delete 3\n",
        "  shtodo --local delete 3\n",
        "  shtodo done 3\n",
        "  shtodo reopen 3\n",
        "  shtodo edit 3 \"Fix the remaining bug\"\n",
        "  shtodo --local restore 3\n",
        "  shtodo add --print-id \"Fix the bug\"\n",
        "  printf 'Run the tests\\n' | shtodo --local add --print-id\n",
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt as _;

    use super::{
        CliError, Command, ListFilter, OutputFormat, ScopeChoice, parse_args, parse_invocation,
    };
    use crate::{
        metadata::{TaskFilter, parse_filter_token},
        projection::{TaskOrder, TaskView},
    };

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
                Some(OsString::from("hello world")),
                false
            ))
        );
        assert_eq!(
            parse_args(args(&["--local", "add", "hello world"])),
            Ok(Command::Add(
                ScopeChoice::Local,
                Some(OsString::from("hello world")),
                false
            ))
        );
    }

    #[test]
    fn parse_args_should_leave_add_text_empty_for_stdin() {
        assert_eq!(
            parse_args(args(&["add"])),
            Ok(Command::Add(ScopeChoice::Global, None, false))
        );
        assert_eq!(
            parse_args(args(&["--local", "add"])),
            Ok(Command::Add(ScopeChoice::Local, None, false))
        );
    }

    #[test]
    fn parse_args_should_accept_list_for_global_and_local_scopes() {
        assert_eq!(
            parse_args(args(&["list"])),
            Ok(Command::List(ScopeChoice::Global, ListFilter::default()))
        );
        assert_eq!(
            parse_args(args(&["--local", "list"])),
            Ok(Command::List(ScopeChoice::Local, ListFilter::default()))
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

    #[test]
    fn parse_args_should_accept_lifecycle_commands_in_both_scopes() {
        for (prefix, scope) in [
            (vec![], ScopeChoice::Global),
            (vec!["--local"], ScopeChoice::Local),
        ] {
            for (command, expected) in [
                ("done", Command::Done(scope, 3)),
                ("reopen", Command::Reopen(scope, 3)),
                ("restore", Command::Restore(scope, 3)),
            ] {
                let mut values = prefix.clone();
                values.extend([command, "3"]);
                assert_eq!(parse_args(args(&values)), Ok(expected));
            }
            let mut values = prefix;
            values.extend(["edit", "3", "new text"]);
            assert_eq!(
                parse_args(args(&values)),
                Ok(Command::Edit(scope, 3, OsString::from("new text")))
            );
        }
    }

    #[test]
    fn parse_args_should_report_missing_edit_text_with_usage() {
        for values in [vec!["edit", "3"], vec!["--local", "edit", "3"]] {
            let error = parse_args(args(&values)).unwrap_err().to_string();
            assert!(error.contains("new task text is required"));
            assert!(error.contains("shtodo [--local] edit <ID> <TASK>"));
        }
    }

    #[test]
    fn parse_args_should_accept_print_id_before_optional_add_text() {
        for (prefix, scope) in [
            (vec![], ScopeChoice::Global),
            (vec!["--local"], ScopeChoice::Local),
        ] {
            let mut values = prefix;
            values.extend(["add", "--print-id"]);
            assert_eq!(
                parse_args(args(&values)),
                Ok(Command::Add(scope, None, true))
            );
            values.push("new task");
            assert_eq!(
                parse_args(args(&values)),
                Ok(Command::Add(scope, Some(OsString::from("new task")), true))
            );
        }
        assert!(parse_args(args(&["--print-id", "add", "task"])).is_err());
        assert!(parse_args(args(&["add", "task", "--print-id"])).is_err());
        assert!(parse_args(args(&["add", "--print-id", "one", "two"])).is_err());
    }

    fn list(values: &[&str]) -> Result<ListFilter, CliError> {
        let mut arguments = vec!["list"];
        arguments.extend(values);
        match parse_args(args(&arguments))? {
            Command::List(ScopeChoice::Global, filter) => Ok(filter),
            command => panic!("unexpected {command:?}"),
        }
    }

    fn task_filter(tokens: &[&str]) -> TaskFilter {
        let mut filter = TaskFilter::default();
        for token in tokens {
            filter.add(parse_filter_token(token).unwrap());
        }
        filter
    }

    fn list_error(values: &[&str]) -> String {
        let error = list(values).unwrap_err().to_string();
        assert!(error.contains("Usage:"), "{error}");
        error
    }

    #[test]
    fn parse_list_args_should_accept_every_flag_in_any_order_in_both_scopes() {
        let expected = ListFilter {
            view: TaskView::Open,
            filter: task_filter(&["+work", "@home", "A-B", "none"]),
            search: Some("Fix It".into()),
            order: TaskOrder::Priority,
        };
        assert_eq!(
            list(&[
                "--tag",
                "+work",
                "--priority",
                "A-B",
                "--open",
                "--search",
                "Fix It",
                "--tag",
                "@home.",
                "--priority",
                "none",
                "--sort",
                "priority",
            ]),
            Ok(expected.clone())
        );
        assert_eq!(
            list(&[
                "--sort",
                "priority",
                "--priority",
                "a-b",
                "--priority",
                "NONE",
                "--search",
                "Fix It",
                "--tag",
                "@home",
                "--open",
                "--tag",
                "+work",
            ]),
            Ok(ListFilter {
                filter: task_filter(&["@home", "+work", "a-b", "none"]),
                ..expected.clone()
            })
        );
        assert_eq!(
            parse_args(args(&["--local", "list", "--done", "--tag", "+x"])),
            Ok(Command::List(
                ScopeChoice::Local,
                ListFilter {
                    view: TaskView::Done,
                    filter: task_filter(&["+x"]),
                    ..ListFilter::default()
                }
            ))
        );
        assert_eq!(list(&["--done"]).unwrap().view, TaskView::Done);
        assert_eq!(list(&[]).unwrap().order, TaskOrder::Manual);
        assert_eq!(
            list(&["--search", "--tag"]).unwrap().search.as_deref(),
            Some("--tag")
        );
        assert_eq!(
            list(&["--tag", "+x", "--tag", "+x."]).unwrap().filter.tags,
            ["+x"]
        );
    }

    #[test]
    fn parse_list_args_should_reject_every_error_row_with_usage() {
        for (values, message) in [
            (vec!["extra"], r#"unsupported shtodo arguments: "extra""#),
            (vec!["--wat"], r#"unsupported shtodo arguments: "--wat""#),
            (
                vec!["--tag=+x"],
                r#"unsupported shtodo arguments: "--tag=+x""#,
            ),
            (
                vec!["--local"],
                r#"unsupported shtodo arguments: "--local""#,
            ),
            (
                vec!["--open", "extra"],
                r#"unsupported shtodo arguments: "extra""#,
            ),
            (vec!["--tag"], "--tag requires a value"),
            (vec!["--priority"], "--priority requires a value"),
            (vec!["--search"], "--search requires a value"),
            (vec!["--sort"], "--sort requires a value; expected priority"),
            (
                vec!["--tag", "work"],
                r#"invalid --tag "work"; expected +project or @context"#,
            ),
            (
                vec!["--tag", "+"],
                r#"invalid --tag "+"; expected +project or @context"#,
            ),
            (
                vec!["--tag", "+."],
                r#"invalid --tag "+."; expected +project or @context"#,
            ),
            (
                vec!["--tag", "+a b"],
                r#"invalid --tag "+a b"; expected +project or @context"#,
            ),
            (
                vec!["--tag", "A"],
                r#"invalid --tag "A"; expected +project or @context"#,
            ),
            (
                vec!["--priority", "AA"],
                r#"invalid --priority "AA"; expected a letter A-Z, a range such as A-C, or none"#,
            ),
            (
                vec!["--priority", "C-A"],
                r#"invalid --priority "C-A"; expected"#,
            ),
            (vec!["--priority", ""], r#"invalid --priority ""; expected"#),
            (
                vec!["--priority", "+x"],
                r#"invalid --priority "+x"; expected"#,
            ),
            (
                vec!["--search", ""],
                "--search requires non-empty single-line text",
            ),
            (
                vec!["--search", " \t "],
                "--search requires non-empty single-line text",
            ),
            (
                vec!["--search", "a\nb"],
                "--search requires non-empty single-line text",
            ),
            (
                vec!["--search", "a\rb"],
                "--search requires non-empty single-line text",
            ),
            (
                vec!["--open", "--done"],
                "--open and --done cannot be combined",
            ),
            (
                vec!["--done", "--open"],
                "--open and --done cannot be combined",
            ),
            (
                vec!["--sort", "manual"],
                r#"invalid --sort "manual"; expected priority"#,
            ),
            (
                vec!["--sort", "x"],
                r#"invalid --sort "x"; expected priority"#,
            ),
            (
                vec!["--sort", "Priority"],
                r#"invalid --sort "Priority"; expected priority"#,
            ),
            (
                vec!["--sort", ""],
                r#"invalid --sort ""; expected priority"#,
            ),
            (
                vec!["--sort", "--json"],
                r#"invalid --sort "--json"; expected priority"#,
            ),
            (
                vec!["--open", "--open"],
                "--open may only be specified once",
            ),
            (
                vec!["--done", "--done"],
                "--done may only be specified once",
            ),
            (
                vec!["--search", "a", "--search", "b"],
                "--search may only be specified once",
            ),
            (
                vec!["--sort", "priority", "--sort", "priority"],
                "--sort may only be specified once",
            ),
        ] {
            let error = list_error(&values);
            assert!(error.starts_with(message), "{values:?}: {error}");
        }
        for values in [
            vec!["--sort"],
            vec!["--sort", "manual"],
            vec!["--sort", "--json"],
        ] {
            assert!(list_error(&values).contains("expected priority"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn parse_list_args_should_reject_non_utf8_filter_values() {
        for flag in ["--tag", "--priority", "--search", "--sort"] {
            let error = parse_args(vec![
                OsString::from("list"),
                OsString::from(flag),
                OsString::from_vec(vec![0xff]),
            ])
            .unwrap_err()
            .to_string();
            assert!(
                error.starts_with(&format!("invalid {flag} value; expected UTF-8")),
                "{error}"
            );
        }
    }

    #[test]
    fn parse_invocation_should_take_list_flag_values_verbatim_and_json_only_in_flag_positions() {
        let invoke = |values: &[&str]| parse_invocation(args(values));
        let search = |text: &str| {
            Ok(Command::List(
                ScopeChoice::Global,
                ListFilter {
                    search: Some(text.into()),
                    ..ListFilter::default()
                },
            ))
        };

        assert_eq!(
            invoke(&["list", "--search", "--json"]),
            (OutputFormat::Text, search("--json"))
        );
        for values in [
            vec!["--json", "list", "--search", "--json"],
            vec!["list", "--json", "--search", "--json"],
            vec!["list", "--search", "--json", "--json"],
        ] {
            assert_eq!(
                invoke(&values),
                (OutputFormat::Json, search("--json")),
                "{values:?}"
            );
        }
        assert_eq!(
            invoke(&["--local", "--json", "list", "--open", "--json"]).1,
            Err(CliError::DuplicateJsonFlag)
        );
        assert_eq!(
            invoke(&["list", "--open", "--json", "--sort", "priority"]),
            (
                OutputFormat::Json,
                Ok(Command::List(
                    ScopeChoice::Global,
                    ListFilter {
                        view: TaskView::Open,
                        order: TaskOrder::Priority,
                        ..ListFilter::default()
                    }
                ))
            )
        );
        let (format, error) = invoke(&["list", "--sort", "--json"]);
        assert_eq!(format, OutputFormat::Text);
        assert!(
            error
                .unwrap_err()
                .to_string()
                .starts_with(r#"invalid --sort "--json""#)
        );
        for values in [
            vec!["list", "--wat", "--json"],
            vec!["--json", "list", "--tag"],
            vec!["list", "--json", "--open", "--done"],
            vec!["--local", "list", "--json", "--sort", "manual"],
        ] {
            let (format, command) = invoke(&values);
            assert_eq!(format, OutputFormat::Json, "{values:?}");
            assert!(command.is_err(), "{values:?}");
        }
    }
}

use std::{
    fmt, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use crate::{
    app::Mode,
    input::{BindingId, BindingOverride, Keymap, KeymapIssue},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigSource {
    Defaults,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConfigDiagnostic {
    pub(crate) order: usize,
    pub(crate) path: String,
    pub(crate) message: String,
}

impl From<KeymapIssue> for ConfigDiagnostic {
    fn from(issue: KeymapIssue) -> Self {
        Self {
            order: issue.order,
            path: issue.path,
            message: issue.message,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ConfigError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Invalid {
        path: PathBuf,
        diagnostics: Vec<ConfigDiagnostic>,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "could not read config {}: {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(
                    formatter,
                    "could not parse config {}: {source}",
                    path.display()
                )
            }
            Self::Invalid { path, diagnostics } => {
                write!(formatter, "invalid config {}:", path.display())?;
                for diagnostic in diagnostics {
                    write!(formatter, "\n  {}: {}", diagnostic.path, diagnostic.message)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Invalid { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoadedKeymap {
    path: PathBuf,
    source: ConfigSource,
    keymap: Keymap,
}

impl LoadedKeymap {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) const fn source(&self) -> ConfigSource {
        self.source
    }

    pub(crate) fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    pub(crate) fn doctor_report(&self) -> String {
        let result = match self.source() {
            ConfigSource::Defaults => "OK: no config file; using defaults".to_owned(),
            ConfigSource::File => format!(
                "OK: {} configurable actions, {} active bindings",
                self.keymap().configurable_action_count(),
                self.keymap().active_binding_count()
            ),
        };
        format!("Config: {}\n{result}\n", self.path().display())
    }
}

impl ConfigError {
    pub(crate) fn doctor_report(&self) -> String {
        match self {
            Self::Invalid { path, diagnostics } => {
                let details = diagnostics
                    .iter()
                    .map(|diagnostic| format!("  {}: {}", diagnostic.path, diagnostic.message))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "Config: {}\nInvalid configuration:\n{details}",
                    path.display()
                )
            }
            Self::Read { path, source } => {
                format!(
                    "Config: {}\nCould not read configuration: {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                format!("Config: {}\nInvalid TOML: {source}", path.display())
            }
        }
    }
}

pub(crate) fn config_path(home: &Path) -> PathBuf {
    home.join(".shtodo/config.toml")
}

pub(crate) fn load(home: &Path) -> Result<LoadedKeymap, ConfigError> {
    let path = config_path(home);
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(LoadedKeymap {
                path,
                source: ConfigSource::Defaults,
                keymap: Keymap::defaults(),
            });
        }
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    let table = toml::from_str::<toml::Table>(&source).map_err(|source| ConfigError::Parse {
        path: path.clone(),
        source,
    })?;
    let (overrides, mut diagnostics) = parse_table(&table);
    let keymap = match Keymap::with_overrides(&overrides) {
        Ok(keymap) => Some(keymap),
        Err(issues) => {
            diagnostics.extend(issues.into_iter().map(ConfigDiagnostic::from));
            None
        }
    };
    diagnostics.sort_by_key(|diagnostic| diagnostic.order);
    if !diagnostics.is_empty() {
        return Err(ConfigError::Invalid { path, diagnostics });
    }
    let keymap = keymap.ok_or_else(|| ConfigError::Invalid {
        path: path.clone(),
        diagnostics: vec![ConfigDiagnostic {
            order: usize::MAX,
            path: "keybindings".into(),
            message: "validation failed without a diagnostic".into(),
        }],
    })?;
    Ok(LoadedKeymap {
        path,
        source: ConfigSource::File,
        keymap,
    })
}

fn parse_table(table: &toml::Table) -> (Vec<BindingOverride>, Vec<ConfigDiagnostic>) {
    let mut overrides = Vec::new();
    let mut diagnostics = Vec::new();
    let mut next_order = 0;

    for (name, value) in table {
        let order = take_order(&mut next_order);
        if name != "keybindings" {
            diagnostics.push(diagnostic(order, name, "unknown field"));
            consume_descendants(value, &mut next_order);
            continue;
        }
        let Some(modes) = value.as_table() else {
            diagnostics.push(diagnostic(order, "keybindings", "expected a table"));
            consume_descendants(value, &mut next_order);
            continue;
        };
        parse_modes(modes, &mut next_order, &mut overrides, &mut diagnostics);
    }

    (overrides, diagnostics)
}

fn parse_modes(
    modes: &toml::Table,
    next_order: &mut usize,
    overrides: &mut Vec<BindingOverride>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    for (name, value) in modes {
        let order = take_order(next_order);
        let path = format!("keybindings.{name}");
        let Some(mode) = parse_mode(name) else {
            diagnostics.push(diagnostic(order, path, "unknown mode"));
            consume_descendants(value, next_order);
            continue;
        };
        let Some(actions) = value.as_table() else {
            diagnostics.push(diagnostic(order, path, "expected a table"));
            consume_descendants(value, next_order);
            continue;
        };
        parse_actions(mode, actions, next_order, overrides, diagnostics);
    }
}

fn parse_actions(
    mode: Mode,
    actions: &toml::Table,
    next_order: &mut usize,
    overrides: &mut Vec<BindingOverride>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    for (name, value) in actions {
        let order = take_order(next_order);
        let path = format!("keybindings.{}.{}", mode_name(mode), name);
        let Some(id) = BindingId::from_config(mode, name) else {
            diagnostics.push(diagnostic(order, path, "unknown action"));
            consume_descendants(value, next_order);
            continue;
        };
        let Some(keys) = value.as_array() else {
            diagnostics.push(diagnostic(order, path, "expected an array"));
            consume_descendants(value, next_order);
            continue;
        };

        let mut values = Vec::with_capacity(keys.len());
        for (index, value) in keys.iter().enumerate() {
            let member_order = take_order(next_order);
            let Some(value) = value.as_str() else {
                diagnostics.push(diagnostic(
                    member_order,
                    format!("{path}[{index}]"),
                    "expected a string",
                ));
                continue;
            };
            values.push(value.to_owned());
        }
        overrides.push(BindingOverride {
            order,
            path,
            id,
            keys: values,
        });
    }
}

fn parse_mode(name: &str) -> Option<Mode> {
    match name {
        "normal" => Some(Mode::Normal),
        "insert" => Some(Mode::Insert),
        "help" => Some(Mode::Help),
        _ => None,
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "normal",
        Mode::Insert => "insert",
        Mode::Help => "help",
    }
}

fn diagnostic(
    order: usize,
    path: impl Into<String>,
    message: impl Into<String>,
) -> ConfigDiagnostic {
    ConfigDiagnostic {
        order,
        path: path.into(),
        message: message.into(),
    }
}

fn take_order(next_order: &mut usize) -> usize {
    let order = *next_order;
    *next_order += 1;
    order
}

fn consume_descendants(value: &toml::Value, next_order: &mut usize) {
    match value {
        toml::Value::Array(values) => {
            for value in values {
                take_order(next_order);
                consume_descendants(value, next_order);
            }
        }
        toml::Value::Table(table) => {
            for value in table.values() {
                take_order(next_order);
                consume_descendants(value, next_order);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::{ConfigError, ConfigSource, load};
    use crate::input::{BindingId, Keymap};

    fn configured_home(source: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        let directory = home.path().join(".shtodo");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("config.toml"), source).unwrap();
        home
    }

    fn labels_for(keymap: &Keymap, id: BindingId) -> Vec<&str> {
        keymap
            .bindings_for(id.mode())
            .find(|binding| binding.id() == id)
            .unwrap()
            .labels()
            .collect()
    }

    fn diagnostic_paths(error: ConfigError) -> Vec<String> {
        match error {
            ConfigError::Invalid { diagnostics, .. } => diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.path)
                .collect(),
            error => panic!("expected invalid configuration, got {error}"),
        }
    }

    #[test]
    fn load_should_use_defaults_without_creating_missing_config() {
        let home = tempfile::tempdir().unwrap();

        let loaded = load(home.path()).unwrap();

        assert_eq!(loaded.source(), ConfigSource::Defaults);
        assert_eq!(loaded.keymap().configurable_action_count(), 24);
        assert_eq!(loaded.keymap().active_binding_count(), 33);
        assert!(!home.path().join(".shtodo").exists());
    }

    #[test]
    fn load_should_apply_partial_action_replacements() {
        let home = configured_home(
            r#"
[keybindings.normal]
move_down = ["x", "ctrl-n"]
"#,
        );

        let loaded = load(home.path()).unwrap();

        assert_eq!(loaded.source(), ConfigSource::File);
        assert_eq!(loaded.keymap().active_binding_count(), 33);
        assert_eq!(
            labels_for(loaded.keymap(), BindingId::MoveDown),
            vec!["x", "Ctrl-n"]
        );
        assert_eq!(
            labels_for(loaded.keymap(), BindingId::MoveUp),
            vec!["k", "Up"]
        );
    }

    #[test]
    fn load_should_report_structural_and_keymap_issues_in_source_order() {
        let home = configured_home(
            r#"
[keybindings.normal]
move_down = ["dn"]
wat = ["x"]
add_task = ["d", "ctrl-c"]

[keybindings.unknown]
close_help = ["esc"]
"#,
        );

        let diagnostics = match load(home.path()).unwrap_err() {
            ConfigError::Invalid { diagnostics, .. } => diagnostics,
            error => panic!("expected invalid configuration, got {error}"),
        };

        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "keybindings.normal.move_down",
                "keybindings.normal.wat",
                "keybindings.normal.add_task",
                "keybindings.normal.add_task",
                "keybindings.unknown",
            ]
        );
    }

    #[test]
    fn load_should_accept_the_same_key_in_different_modes() {
        let home = configured_home(
            r#"
[keybindings.normal]
move_down = ["x"]

[keybindings.insert]
move_cursor_left = ["x"]
"#,
        );

        assert!(load(home.path()).is_ok());
    }

    #[test]
    fn load_should_reject_unknown_root_field() {
        let home = configured_home("wat = true");

        assert_eq!(
            diagnostic_paths(load(home.path()).unwrap_err()),
            vec!["wat"]
        );
    }

    #[test]
    fn load_should_reject_non_table_keybindings() {
        let home = configured_home("keybindings = []");

        assert_eq!(
            diagnostic_paths(load(home.path()).unwrap_err()),
            vec!["keybindings"]
        );
    }

    #[test]
    fn load_should_reject_non_array_action_value() {
        let home = configured_home("[keybindings.normal]\nmove_down = \"j\"");

        assert_eq!(
            diagnostic_paths(load(home.path()).unwrap_err()),
            vec!["keybindings.normal.move_down"]
        );
    }

    #[test]
    fn load_should_reject_non_string_array_member() {
        let home = configured_home("[keybindings.normal]\nmove_down = [\"j\", 1]");

        assert_eq!(
            diagnostic_paths(load(home.path()).unwrap_err()),
            vec!["keybindings.normal.move_down[1]"]
        );
    }

    #[test]
    fn load_should_report_semantic_issues_for_strings_in_mixed_arrays() {
        let home = configured_home("[keybindings.normal]\nmove_down = [\"bad\", 1, \"ctrl-c\"]");

        let diagnostics = match load(home.path()).unwrap_err() {
            ConfigError::Invalid { diagnostics, .. } => diagnostics,
            error => panic!("expected invalid configuration, got {error}"),
        };

        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.path, diagnostic.message))
                .collect::<Vec<_>>(),
            vec![
                "keybindings.normal.move_down: invalid key \"bad\": unknown key \"bad\"",
                "keybindings.normal.move_down: Ctrl-C is reserved and cannot be configured",
                "keybindings.normal.move_down[1]: expected a string",
            ]
        );
    }

    #[test]
    fn malformed_override_should_not_leave_its_default_active_for_conflicts() {
        let home = configured_home(
            r#"
[keybindings.normal]
move_down = ["dn"]
add_task = ["j"]
"#,
        );

        let diagnostics = match load(home.path()).unwrap_err() {
            ConfigError::Invalid { diagnostics, .. } => diagnostics,
            error => panic!("expected invalid configuration, got {error}"),
        };

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].path, "keybindings.normal.move_down");
        assert_eq!(
            diagnostics[0].message,
            "invalid key \"dn\": unknown key \"dn\""
        );
    }

    #[test]
    fn load_should_include_path_for_toml_syntax_error() {
        let home = configured_home("[keybindings.normal\nmove_down = [\"j\"]");
        let path = home.path().join(".shtodo/config.toml");

        let error = load(home.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Parse { .. }));
        assert!(error.to_string().contains(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn load_should_include_path_for_non_not_found_read_error() {
        let home = tempfile::tempdir().unwrap();
        let directory = home.path().join(".shtodo");
        fs::create_dir(&directory).unwrap();
        let path = directory.join("config.toml");
        fs::create_dir(&path).unwrap();

        let error = load(home.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Read { .. }));
        assert!(error.to_string().contains(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn config_path_should_resolve_under_the_given_home() {
        let home = Path::new("/example/home");

        assert_eq!(super::config_path(home), home.join(".shtodo/config.toml"));
    }
}

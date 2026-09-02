use std::path::Path;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{App, EditKind, Mode},
    input::{BindingId, Keymap, ResolvedBinding},
    task::{ListScope, Task},
};

const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 8;
const ROW_PREFIX_WIDTH: u16 = 4;

fn editor_window(buffer: &str, cursor: usize, width: u16) -> (&str, u16) {
    let mut cursor = cursor.min(buffer.len());
    while !buffer.is_char_boundary(cursor) {
        cursor = cursor.saturating_sub(1);
    }
    if width == 0 {
        return (&buffer[cursor..cursor], 0);
    }

    let max_cursor_width = usize::from(width.saturating_sub(1));
    let mut start = cursor;
    for (index, _) in buffer[..cursor].char_indices().rev() {
        if Line::from(&buffer[index..cursor]).width() > max_cursor_width {
            break;
        }
        start = index;
    }

    let cursor_column = Line::from(&buffer[start..cursor]).width();
    let mut end = cursor;
    for (index, character) in buffer[cursor..].char_indices() {
        let next = cursor + index + character.len_utf8();
        if Line::from(&buffer[start..next]).width() > usize::from(width) {
            break;
        }
        end = next;
    }

    (
        &buffer[start..end],
        cursor_column.min(usize::from(u16::MAX)) as u16,
    )
}

pub(crate) fn render(frame: &mut Frame<'_>, app: &App, keymap: &Keymap) {
    if app.celebration().is_some() {
        render_celebration(frame);
        return;
    }

    if frame.area().width < MIN_WIDTH || frame.area().height < MIN_HEIGHT {
        render_resize_message(frame);
        return;
    }

    let regions = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_header(frame, regions[0], app);
    render_content(frame, regions[1], app, keymap);
    render_footer(frame, regions[2], app, keymap);
    if app.mode() == Mode::Help {
        render_help(frame, regions[1], keymap);
    }
}

fn render_celebration(frame: &mut Frame<'_>) {
    const CARD_WIDTH: u16 = 36;
    const CARD_HEIGHT: u16 = 5;
    const EMOJI_WIDTH: u16 = 2;
    const MESSAGE: &str = "congrats, you got shit done";

    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.width < CARD_WIDTH || area.height < CARD_HEIGHT {
        render_resize_message(frame);
        return;
    }

    let card = Rect::new(
        area.x + (area.width - CARD_WIDTH) / 2,
        area.y + (area.height - CARD_HEIGHT) / 2,
        CARD_WIDTH,
        CARD_HEIGHT,
    );
    let text_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);

    for x in (card.x..card.right()).step_by(usize::from(EMOJI_WIDTH)) {
        frame.render_widget(Paragraph::new("💩"), Rect::new(x, card.y, EMOJI_WIDTH, 1));
        frame.render_widget(
            Paragraph::new("💩"),
            Rect::new(x, card.bottom() - 1, EMOJI_WIDTH, 1),
        );
    }
    for y in card.y + 1..card.bottom() - 1 {
        frame.render_widget(Paragraph::new("💩"), Rect::new(card.x, y, EMOJI_WIDTH, 1));
        frame.render_widget(
            Paragraph::new("💩"),
            Rect::new(card.right() - EMOJI_WIDTH, y, EMOJI_WIDTH, 1),
        );
    }

    frame.render_widget(
        Paragraph::new(MESSAGE)
            .style(text_style)
            .alignment(Alignment::Center),
        Rect::new(
            card.x + EMOJI_WIDTH,
            card.y + 2,
            card.width - EMOJI_WIDTH * 2,
            1,
        ),
    );
}

fn render_resize_message(frame: &mut Frame<'_>) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.width == 0 || area.height == 0 {
        return;
    }

    let message = "Resize terminal to at least 40x8";
    let characters = message.chars().collect::<Vec<_>>();
    let lines = characters
        .chunks(usize::from(area.width))
        .map(|chunk| Line::from(chunk.iter().collect::<String>()))
        .collect::<Vec<_>>();
    let line_count = lines.len().min(usize::from(u16::MAX)) as u16;
    let message_area = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(line_count) / 2,
        area.width,
        line_count.min(area.height),
    );
    let alignment = if line_count == 1 {
        Alignment::Center
    } else {
        Alignment::Left
    };
    frame.render_widget(Paragraph::new(lines).alignment(alignment), message_area);
}

fn render_header(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let visible_tasks = app.tasks().visible_tasks().collect::<Vec<_>>();
    let open_count = visible_tasks
        .iter()
        .filter(|task| !task.completed())
        .count();
    let done_count = visible_tasks.len() - open_count;
    let counts = format!("{open_count} open · {done_count} done");
    let columns = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(counts.chars().count() as u16),
    ])
    .split(area);

    let scope = scope_label(app.tasks().scope());
    let title = Line::from(vec![
        Span::styled(
            "shtodo",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {scope}")),
    ]);

    frame.render_widget(Paragraph::new(title), columns[0]);
    frame.render_widget(
        Paragraph::new(counts).alignment(ratatui::layout::Alignment::Right),
        columns[1],
    );
}

fn render_content(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App, keymap: &Keymap) {
    let visible_tasks = app.tasks().visible_tasks().collect::<Vec<_>>();
    let editor = app.editor();
    if visible_tasks.is_empty() && editor.is_none() {
        let mut text = "No tasks yet".to_owned();
        if let (Some(add), Some(help)) = (
            binding_label(keymap, Mode::Normal, BindingId::StartAdd),
            binding_label(keymap, Mode::Normal, BindingId::OpenHelp),
        ) {
            text.push_str(&format!("\nPress {add} to add · {help} for help"));
        }
        frame.render_widget(Paragraph::new(text), area);
        return;
    }

    let editor_width = area.width.saturating_sub(ROW_PREFIX_WIDTH);
    let editor_view = editor.map(|editor| {
        let (visible, cursor_column) =
            editor_window(editor.buffer(), editor.cursor(), editor_width);
        (editor.kind(), visible, cursor_column)
    });
    let mut selected_index = visible_tasks
        .iter()
        .position(|task| Some(task.id()) == app.selected());
    let mut items = visible_tasks
        .iter()
        .map(|task| match editor_view {
            Some((EditKind::Edit(id), visible, _)) if task.id() == id => editor_item(visible),
            _ => task_item(task),
        })
        .collect::<Vec<_>>();

    if let Some((EditKind::Add, visible, _)) = editor_view {
        selected_index = Some(items.len());
        items.push(editor_item(visible));
    }

    let list = List::new(items)
        .highlight_symbol("› ")
        .highlight_style(Style::default().bg(Color::DarkGray));
    let mut state = ListState::default().with_selected(selected_index);

    frame.render_stateful_widget(list, area, &mut state);

    if let (Some((_, _, cursor_column)), Some(selected_index)) = (editor_view, selected_index) {
        let visible_row = selected_index.saturating_sub(state.offset());
        let row = visible_row.min(usize::from(u16::MAX)) as u16;
        frame.set_cursor_position((
            area.x
                .saturating_add(ROW_PREFIX_WIDTH)
                .saturating_add(cursor_column),
            area.y.saturating_add(row),
        ));
    }
}

fn render_footer(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App, keymap: &Keymap) {
    let mut bindings = keymap
        .bindings_for(app.mode())
        .filter(|binding| binding.footer_priority().is_some())
        .collect::<Vec<_>>();
    bindings.sort_by_key(|binding| binding.footer_priority());
    let hints = bindings
        .into_iter()
        .map(footer_binding)
        .collect::<Vec<_>>()
        .join(" · ");
    let detail = match (app.message(), hints.is_empty()) {
        (Some(message), false) => format!("{message} · {hints}"),
        (Some(message), true) => message.to_owned(),
        (None, _) => hints,
    };
    let text = format!("{}  {detail}", mode_label(app.mode()));

    frame.render_widget(Paragraph::new(text), area);
}

fn render_help(frame: &mut Frame<'_>, area: Rect, keymap: &Keymap) {
    let block = Block::bordered().title("Keyboard help");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let normal = help_lines(keymap, Mode::Normal);
    let mut insert_and_help = help_lines(keymap, Mode::Insert);
    insert_and_help.push(Line::default());
    insert_and_help.extend(help_lines(keymap, Mode::Help));
    if let Some(normal_width) = help_column_width(&normal, &insert_and_help, inner) {
        let columns =
            Layout::horizontal([Constraint::Length(normal_width), Constraint::Min(1)]).split(inner);
        frame.render_widget(
            Paragraph::new(wrap_help_lines(&normal, columns[0].width)),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(wrap_help_lines(&insert_and_help, columns[1].width)),
            columns[1],
        );
    } else {
        let mut all_modes = normal;
        all_modes.push(Line::default());
        all_modes.extend(insert_and_help);
        frame.render_widget(
            Paragraph::new(wrap_help_lines(&all_modes, inner.width)),
            inner,
        );
    }
}

fn help_column_width(normal: &[Line<'_>], other: &[Line<'_>], area: Rect) -> Option<u16> {
    let mut best = None::<(usize, usize, u16, u16)>;
    for normal_width in 1..area.width {
        let other_width = area.width - normal_width;
        let normal_height = wrap_help_lines(normal, normal_width).len();
        let other_height = wrap_help_lines(other, other_width).len();
        if normal_height > usize::from(area.height) || other_height > usize::from(area.height) {
            continue;
        }
        let score = (
            normal_height.max(other_height),
            normal_height + other_height,
            normal_width.abs_diff(other_width),
            normal_width,
        );
        if best.is_none_or(|current| score < current) {
            best = Some(score);
        }
    }
    best.map(|(_, _, _, width)| width)
}

fn wrap_help_lines(lines: &[Line<'_>], width: u16) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut wrapped = Vec::new();
    for line in lines {
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        if text.is_empty() {
            wrapped.push(Line::default());
            continue;
        }
        let mut segment = String::new();
        let mut segment_width = 0_usize;
        for character in text.chars() {
            let character_width = Line::from(character.to_string()).width();
            if !segment.is_empty()
                && segment_width.saturating_add(character_width) > usize::from(width)
            {
                wrapped.push(Line::styled(segment, line.style));
                segment = character.to_string();
                segment_width = character_width;
            } else {
                segment.push(character);
                segment_width = segment_width.saturating_add(character_width);
            }
        }
        wrapped.push(Line::styled(segment, line.style));
    }
    wrapped
}

fn help_lines(keymap: &Keymap, mode: Mode) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        mode_name(mode),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    lines.extend(keymap.bindings_for(mode).map(|binding| {
        Line::from(format!(
            "{} {}",
            binding.labels().collect::<Vec<_>>().join(" / "),
            binding.description()
        ))
    }));
    lines
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "Normal",
        Mode::Insert => "Insert",
        Mode::Help => "Help",
    }
}

fn scope_label(scope: &ListScope) -> String {
    match scope {
        ListScope::Global => "global".into(),
        ListScope::Project { path } => Path::new(path)
            .file_name()
            .filter(|name| !name.is_empty())
            .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned()),
    }
}

fn task_item(task: &Task) -> ListItem<'static> {
    let marker = if task.completed() { "✓ " } else { "○ " };
    let text_style = if task.completed() {
        Style::default().add_modifier(Modifier::DIM | Modifier::CROSSED_OUT)
    } else {
        Style::default()
    };

    ListItem::new(Line::from(vec![
        Span::raw(marker),
        Span::styled(task.text().to_owned(), text_style),
    ]))
}

fn editor_item(text: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::raw("○ "),
        Span::raw(text.to_owned()),
    ]))
}

fn binding_label(keymap: &Keymap, mode: Mode, id: BindingId) -> Option<&str> {
    keymap
        .bindings_for(mode)
        .find(|binding| binding.id() == id)
        .map(ResolvedBinding::preferred_label)
}

fn footer_binding(binding: &ResolvedBinding) -> String {
    let description = match binding.description().strip_prefix("show ") {
        Some(description) => description,
        None => binding.description(),
    };
    format!("{} {description}", binding.preferred_label())
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Help => "HELP",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::{Buffer, Cell},
        style::{Color, Modifier},
    };

    use super::{binding_label, editor_window, render};
    use crate::{
        action::Action,
        app::App,
        input::{BindingId, BindingOverride, Keymap},
        task::{ListScope, TaskList},
    };

    fn render_app(app: &App, width: u16, height: u16) -> Buffer {
        let keymap = Keymap::defaults();
        render_app_with_keymap(app, &keymap, width, height)
    }

    fn render_app_with_keymap(app: &App, keymap: &Keymap, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app, keymap)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn override_for(id: BindingId, keys: &[&str]) -> BindingOverride {
        BindingOverride {
            order: 0,
            path: format!(
                "keybindings.{}.{}",
                match id.mode() {
                    crate::app::Mode::Normal => "normal",
                    crate::app::Mode::Insert => "insert",
                    crate::app::Mode::Help => "help",
                },
                id.config_name().unwrap()
            ),
            id,
            keys: keys.iter().map(|key| (*key).into()).collect(),
        }
    }

    fn buffer_text(buffer: &Buffer) -> String {
        buffer.content().iter().map(Cell::symbol).collect()
    }

    fn buffer_row(buffer: &Buffer, width: u16, row: u16) -> String {
        (0..width)
            .map(|column| buffer[(column, row)].symbol())
            .collect()
    }

    #[test]
    fn empty_list_should_render_brand_scope_mode_and_learning_hints() {
        let app = App::new(TaskList::new(ListScope::Global));
        let text = buffer_text(&render_app(&app, 80, 12));

        assert!(text.contains("shtodo"));
        assert!(text.contains("global"));
        assert!(text.contains("No tasks yet"));
        assert!(text.contains("i add"));
        assert!(text.contains("? help"));
        assert!(text.contains("NORMAL"));
    }

    #[test]
    fn binding_label_should_make_missing_bindings_explicit() {
        let keymap = Keymap::defaults();

        assert_eq!(
            binding_label(&keymap, crate::app::Mode::Normal, BindingId::StartAdd),
            Some("i")
        );
        assert_eq!(
            binding_label(&keymap, crate::app::Mode::Normal, BindingId::MoveCursorLeft),
            None
        );
    }

    #[test]
    fn custom_keymap_should_drive_footer_help_and_empty_state() {
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::StartAdd, &["a"]),
            override_for(BindingId::OpenHelp, &["h", "?"]),
        ])
        .unwrap();
        let mut app = App::new(TaskList::new(ListScope::Global));

        let normal = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 12));
        assert!(normal.contains("Press a to add · h for help"));
        assert!(normal.contains("a add task"));
        assert!(normal.contains("h help"));
        assert!(!normal.contains("i add task"));

        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 24));
        assert!(help.contains("h / ? show help"));
    }

    #[test]
    fn help_should_wrap_long_alias_lists_without_hiding_aliases_or_emergency_quit() {
        let aliases = [
            "α", "β", "γ", "δ", "ε", "ζ", "η", "θ", "λ", "μ", "ν", "ξ", "ο", "π", "ρ", "σ",
        ];
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::OpenHelp, &aliases),
            override_for(BindingId::CloseHelp, &["x", "esc"]),
        ])
        .unwrap();
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenHelp).unwrap();

        let buffer = render_app_with_keymap(&app, &keymap, 80, 24);
        let text = buffer_text(&buffer);

        for alias in aliases {
            assert!(text.contains(alias), "missing active Help alias: {alias}");
        }
        assert_eq!(text.matches("Ctrl-C").count(), 3);
        assert!(buffer_row(&buffer, 80, 0).contains("shtodo global"));
        assert!(buffer_row(&buffer, 80, 23).contains("HELP  x close help"));
    }

    #[test]
    fn custom_footer_should_prioritize_actions_instead_of_literal_keys() {
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::StartAdd, &["z"]),
            override_for(BindingId::OpenHelp, &["h"]),
        ])
        .unwrap();
        let app = App::new(TaskList::new(ListScope::Global));
        let buffer = render_app_with_keymap(&app, &keymap, 40, 8);
        let footer = buffer_row(&buffer, 40, 7);

        assert!(footer.contains("z add task"));
        assert!(footer.contains("h help"));
    }

    #[test]
    fn completed_selected_task_should_render_marker_glyph_and_style() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("finished").unwrap();
        list.toggle_complete(id).unwrap();
        let app = App::new(list);
        let buffer = render_app(&app, 80, 12);

        assert!(buffer.content().iter().any(|cell| cell.symbol() == "›"));
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "✓"));
        assert!(
            buffer.content().iter().any(|cell| {
                cell.symbol() == "f" && cell.modifier.contains(Modifier::CROSSED_OUT)
            })
        );
    }

    #[test]
    fn completing_the_final_open_task_should_render_a_centered_enclosed_card() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("last task").unwrap();
        let mut app = App::new(list);

        app.apply(Action::ToggleComplete).unwrap();
        let buffer = render_app(&app, 80, 12);

        for x in (22..58).step_by(2) {
            assert_eq!(buffer[(x, 3)].symbol(), "💩", "missing top edge at {x}");
            assert_eq!(buffer[(x, 7)].symbol(), "💩", "missing bottom edge at {x}");
        }
        for y in 4..7 {
            assert_eq!(buffer[(22, y)].symbol(), "💩", "missing left edge at {y}");
            assert_eq!(buffer[(56, y)].symbol(), "💩", "missing right edge at {y}");
        }
        assert!(
            buffer_row(&buffer, 80, 5).contains("congrats, you got shit done"),
            "missing celebration message"
        );
    }

    #[test]
    fn advancing_the_celebration_should_keep_the_card_stable() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("last task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();
        let first_frame = render_app(&app, 40, 8);

        app.advance_celebration();
        let second_frame = render_app(&app, 40, 8);

        assert_eq!(second_frame, first_frame);
    }

    #[test]
    fn populated_local_list_should_render_scope_counts_open_glyph_and_selection_style() {
        let mut list = TaskList::new(ListScope::Project {
            path: "/work/focus".into(),
        });
        list.add("open task").unwrap();
        let completed = list.add("done task").unwrap();
        list.toggle_complete(completed).unwrap();
        let app = App::new(list);
        let buffer = render_app(&app, 80, 12);

        let header = buffer_row(&buffer, 80, 0);
        assert!(header.contains("shtodo focus"));
        assert!(header.contains("1 open · 1 done"));
        assert_eq!(buffer[(2, 1)].symbol(), "○");
        assert_eq!(buffer[(2, 2)].symbol(), "✓");
        assert!(buffer[(4, 2)].modifier.contains(Modifier::DIM));
        assert_eq!(buffer[(0, 1)].bg, Color::DarkGray);
    }

    #[test]
    fn selected_task_should_remain_visible_when_list_scrolls() {
        let mut list = TaskList::new(ListScope::Global);
        for index in 0..10 {
            list.add(&format!("task {index}")).unwrap();
        }
        let mut app = App::new(list);
        for _ in 0..9 {
            app.apply(Action::MoveDown).unwrap();
        }

        let text = buffer_text(&render_app(&app, 80, 8));

        assert!(text.contains("task 9"));
        assert!(!text.contains("task 0"));
    }

    #[test]
    fn insert_mode_should_render_buffer_and_visible_cursor() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();
        for character in "new task".chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
        let backend = TestBackend::new(50, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let keymap = Keymap::defaults();

        terminal.draw(|frame| render(frame, &app, &keymap)).unwrap();

        terminal.backend_mut().assert_cursor_position((12, 1));
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("new task"));
        assert!(text.contains("INSERT"));
    }

    #[test]
    fn edit_mode_should_render_the_buffer_in_the_task_row_and_place_the_cursor_after_it() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("draft").unwrap();
        let mut app = App::new(list);
        app.apply(Action::StartEdit).unwrap();
        let backend = TestBackend::new(50, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let keymap = Keymap::defaults();

        terminal.draw(|frame| render(frame, &app, &keymap)).unwrap();

        assert_eq!(terminal.backend().buffer()[(4, 1)].symbol(), "d");
        assert_eq!(terminal.backend().buffer()[(8, 1)].symbol(), "t");
        terminal.backend_mut().assert_cursor_position((9, 1));
    }

    #[test]
    fn help_mode_should_render_bindings_from_keymap() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenHelp).unwrap();
        let text = buffer_text(&render_app(&app, 80, 24));

        assert!(text.contains("Keyboard help"));
        assert!(text.contains("J"));
        assert!(text.contains("move task down"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn help_mode_should_render_every_documented_binding_without_truncating_descriptions() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenHelp).unwrap();
        let buffer = render_app(&app, 80, 24);
        let lines = buffer
            .content()
            .chunks(80)
            .map(|cells| {
                cells
                    .iter()
                    .map(Cell::symbol)
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>();
        let expected_rows = [
            "Normal",
            "j / Down move down",
            "k / Up move up",
            "J move task down",
            "K move task up",
            "i add task",
            "e edit task",
            "Space toggle complete",
            "d delete task",
            "u restore latest",
            "? show help",
            "q / Ctrl-C quit",
            "Insert",
            "Left move cursor left",
            "Right move cursor right",
            "Home move cursor start",
            "End move cursor end",
            "Alt-Left / Alt-b move one word left",
            "Alt-Right / Alt-f move one word right",
            "Backspace delete before cursor",
            "Delete delete at cursor",
            "Alt-Backspace / Ctrl-w delete previous word",
            "Alt-Delete delete next word",
            "Enter save edit",
            "Esc cancel edit",
            "Ctrl-C quit",
            "Help",
            "? / Esc close help",
        ];

        for expected in expected_rows {
            assert!(
                lines.iter().any(|line| line.contains(expected)),
                "missing full help row: {expected}"
            );
        }
    }

    #[test]
    fn validation_message_should_precede_insert_save_and_cancel_hints() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();
        app.apply(Action::CommitEdit).unwrap();
        let text = buffer_text(&render_app(&app, 80, 10));

        let message = text.find("Task text cannot be empty").unwrap();
        let save = text.find("Enter save edit").unwrap();
        let cancel = text.find("Esc cancel edit").unwrap();
        assert!(message < save);
        assert!(save < cancel);
    }

    #[test]
    fn informational_message_should_render_before_filtered_footer_hints() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::RestoreLatest).unwrap();
        let buffer = render_app(&app, 80, 12);
        let footer = buffer_row(&buffer, 80, 11);

        let message = footer.find("Nothing to restore").unwrap();
        let add = footer.find("i add task").unwrap();
        assert!(message < add);
        assert!(footer.contains("? help"));
        assert!(!footer.contains("Down move down"));
        assert!(!footer.contains("Ctrl-C quit"));
    }

    #[test]
    fn narrow_footer_should_keep_prioritized_hints_when_clipping_the_remainder() {
        let app = App::new(TaskList::new(ListScope::Global));
        let buffer = render_app(&app, 40, 8);
        let footer = buffer_row(&buffer, 40, 7);

        assert!(footer.contains("NORMAL"));
        assert!(footer.contains("i add task"));
        assert!(footer.contains("? help"));
        assert!(!footer.contains("Down move down"));
        assert!(!footer.contains("Ctrl-C quit"));
    }

    #[test]
    fn undersized_terminal_should_render_only_resize_message() {
        let app = App::new(TaskList::new(ListScope::Global));
        let text = buffer_text(&render_app(&app, 30, 6));

        assert!(text.contains("Resize terminal to at least 40x8"));
        assert!(!text.contains("No tasks yet"));
    }

    #[test]
    fn editor_window_should_keep_end_cursor_visible() {
        let (visible, cursor_column) = editor_window("0123456789", 10, 5);

        assert_eq!(visible, "6789");
        assert_eq!(cursor_column, 4);
    }
}

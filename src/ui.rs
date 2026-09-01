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
    input::{Binding, bindings, bindings_for},
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

pub(crate) fn render(frame: &mut Frame<'_>, app: &App) {
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
    render_content(frame, regions[1], app);
    render_footer(frame, regions[2], app);
    if app.mode() == Mode::Help {
        render_help(frame);
    }
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

fn render_content(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let visible_tasks = app.tasks().visible_tasks().collect::<Vec<_>>();
    let editor = app.editor();
    if visible_tasks.is_empty() && editor.is_none() {
        frame.render_widget(
            Paragraph::new("No tasks yet\nPress i to add · ? for help"),
            area,
        );
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

fn render_footer(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let mut bindings = bindings_for(app.mode())
        .filter(|binding| binding.show_in_footer)
        .collect::<Vec<_>>();
    bindings.sort_by_key(|binding| footer_priority(app.mode(), binding.key_label));
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

fn render_help(frame: &mut Frame<'_>) {
    let area = centered_rect(frame.area(), 76, 19);
    let block = Block::bordered().title("Keyboard help");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let normal = help_lines(Mode::Normal);
    let mut insert_and_help = help_lines(Mode::Insert);
    insert_and_help.push(Line::default());
    insert_and_help.extend(help_lines(Mode::Help));
    let normal_width = line_width(&normal);
    let insert_and_help_width = line_width(&insert_and_help);
    let fits_two_columns = normal_width
        .saturating_add(1)
        .saturating_add(insert_and_help_width)
        <= inner.width
        && normal.len() <= usize::from(inner.height)
        && insert_and_help.len() <= usize::from(inner.height);

    if fits_two_columns {
        let columns = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(insert_and_help_width),
        ])
        .split(inner);
        frame.render_widget(Paragraph::new(normal), columns[0]);
        frame.render_widget(Paragraph::new(insert_and_help), columns[1]);
    } else {
        let mut all_modes = normal;
        all_modes.push(Line::default());
        all_modes.extend(insert_and_help);
        frame.render_widget(Paragraph::new(all_modes), inner);
    }
}

fn line_width(lines: &[Line<'_>]) -> u16 {
    lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .min(usize::from(u16::MAX)) as u16
}

fn centered_rect(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = max_width.min(area.width.saturating_sub(2));
    let height = max_height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn help_lines(mode: Mode) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        mode_name(mode),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    lines.extend(
        bindings()
            .filter(|binding| binding.mode == mode)
            .map(|binding| Line::from(format!("{} {}", binding.key_label, binding.description))),
    );
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

fn footer_binding(binding: &Binding) -> String {
    let description = match binding.description.strip_prefix("show ") {
        Some(description) => description,
        None => binding.description,
    };
    format!("{} {description}", binding.key_label)
}

fn footer_priority(mode: Mode, key_label: &str) -> u8 {
    match (mode, key_label) {
        (Mode::Normal, "i") | (Mode::Insert, "Enter") | (Mode::Help, "?") => 0,
        (Mode::Normal, "?") | (Mode::Insert, "Esc") | (Mode::Help, "Esc") => 1,
        _ => 2,
    }
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

    use super::{editor_window, render};
    use crate::{
        action::Action,
        app::App,
        task::{ListScope, TaskList},
    };

    fn render_app(app: &App, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal.backend().buffer().clone()
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

        terminal.draw(|frame| render(frame, &app)).unwrap();

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

        terminal.draw(|frame| render(frame, &app)).unwrap();

        assert_eq!(terminal.backend().buffer()[(4, 1)].symbol(), "d");
        assert_eq!(terminal.backend().buffer()[(8, 1)].symbol(), "t");
        terminal.backend_mut().assert_cursor_position((9, 1));
    }

    #[test]
    fn help_mode_should_render_bindings_from_table() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenHelp).unwrap();
        let text = buffer_text(&render_app(&app, 80, 24));

        assert!(text.contains("Keyboard help"));
        assert!(text.contains("J"));
        assert!(text.contains("move task down"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn help_mode_should_render_every_binding_without_truncating_descriptions() {
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
            "Ctrl-C quit",
            "j move down",
            "Down move down",
            "k move up",
            "Up move up",
            "J move task down",
            "K move task up",
            "i add task",
            "e edit task",
            "Space toggle complete",
            "d delete task",
            "u restore latest",
            "? show help",
            "q quit",
            "Left move cursor left",
            "Right move cursor right",
            "Home move cursor start",
            "End move cursor end",
            "Backspace delete before cursor",
            "Delete delete at cursor",
            "Enter save edit",
            "Esc cancel edit",
            "? close help",
            "Esc close help",
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

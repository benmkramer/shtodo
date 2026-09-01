#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::{Buffer, Cell},
        style::Modifier,
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
    bindings.sort_by_key(|binding| match binding.key_label {
        "i" => 0,
        "?" => 1,
        _ => 2,
    });
    let hints = bindings
        .into_iter()
        .map(footer_binding)
        .collect::<Vec<_>>()
        .join(" · ");
    let detail = app.message().unwrap_or(&hints);
    let text = format!("{}  {detail}", mode_label(app.mode()));

    frame.render_widget(Paragraph::new(text), area);
}

fn render_help(frame: &mut Frame<'_>) {
    let area = centered_rect(frame.area(), 76, 19);
    let block = Block::bordered().title("Keyboard help");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let columns = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(inner);
    for (column, mode) in columns.iter().zip([Mode::Normal, Mode::Insert, Mode::Help]) {
        frame.render_widget(Paragraph::new(help_lines(mode)), *column);
    }
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
            .map(|binding| {
                Line::from(format!("{:<10} {}", binding.key_label, binding.description))
            }),
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

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Help => "HELP",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::{Buffer, Cell},
        style::Modifier,
    };

    use super::render;
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
}

use std::path::Path;

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{App, Mode},
    input::{Binding, bindings_for},
    task::{ListScope, Task},
};

pub(crate) fn render(frame: &mut Frame<'_>, app: &App) {
    let regions = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_header(frame, regions[0], app);
    render_content(frame, regions[1], app);
    render_footer(frame, regions[2], app);
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
    if visible_tasks.is_empty() {
        frame.render_widget(
            Paragraph::new("No tasks yet\nPress i to add · ? for help"),
            area,
        );
        return;
    }

    let selected = app.selected();
    let selected_index = visible_tasks
        .iter()
        .position(|task| Some(task.id()) == selected);
    let items = visible_tasks
        .iter()
        .map(|task| task_item(task))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .highlight_symbol("› ")
        .highlight_style(Style::default().bg(Color::DarkGray));
    let mut state = ListState::default().with_selected(selected_index);

    frame.render_stateful_widget(list, area, &mut state);
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
    let text = format!("{}  {hints}", mode_label(app.mode()));

    frame.render_widget(Paragraph::new(text), area);
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

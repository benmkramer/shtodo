use std::{cmp::Ordering, path::Path};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
};

use crate::{
    app::{App, EditKind, Mode, Notice, TaskView},
    input::{BindingId, Keymap, ResolvedBinding},
    metadata::{self, Date},
    projection::{DisplayedRow, TaskOrder},
    task::ListScope,
};

const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 8;
const ROW_PREFIX_WIDTH: u16 = 4;
/// Columns a subtask row is indented before its marker.
const SUBTASK_INDENT: u16 = 2;
const PROJECT_COLOR: Color = Color::Cyan;
const CONTEXT_COLOR: Color = Color::Magenta;
const DUE_STYLE: Style = Style::new().fg(Color::Blue);
const OVERDUE_COLOR: Color = Color::Red;
const DUE_TODAY_COLOR: Color = Color::Yellow;

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

    let filter_mode = app.mode() == Mode::Filter;
    let regions = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(!app.is_trash_view())),
        Constraint::Min(1),
        // The tag hint sits above the footer, so footer messages are never displaced.
        Constraint::Length(u16::from(filter_mode)),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_header(frame, regions[0], app);
    if !app.is_trash_view() {
        render_view(frame, regions[1], app);
    }
    render_content(frame, regions[2], app, keymap);
    if filter_mode {
        render_filter_hint(frame, regions[3], app);
    }
    render_footer(frame, regions[4], app, keymap);
    if app.mode() == Mode::Help {
        // Help covers the header, view, and content rows; the footer below stays visible.
        let help_area = Rect::new(
            regions[0].x,
            regions[0].y,
            regions[0].width,
            regions[0].height + regions[1].height + regions[2].height,
        );
        render_help(frame, help_area, keymap, app);
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
    let counts = if app.is_trash_view() {
        format!("{} deleted", app.tasks().deleted_tasks().len())
    } else {
        format!("{open_count} open · {done_count} done")
    };
    let columns = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(counts.chars().count() as u16),
    ])
    .split(area);

    let title = Line::from(vec![
        Span::styled(
            "shtodo",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(header_detail(app)),
    ]);

    frame.render_widget(Paragraph::new(title), columns[0]);
    frame.render_widget(
        Paragraph::new(counts).alignment(ratatui::layout::Alignment::Right),
        columns[1],
    );
}

/// The header text after `shtodo`, which Help repeats in its title.
fn header_detail(app: &App) -> String {
    let scope = scope_label(app.tasks().scope());
    if app.is_trash_view() {
        format!(" TRASH · {scope} · newest first")
    } else {
        format!(" {scope}")
    }
}

fn render_view(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let tabs = [TaskView::All, TaskView::Open, TaskView::Done];
    let tab_width = tabs
        .iter()
        .map(|view| view.label().len() + 2)
        .sum::<usize>()
        + 2;
    let columns =
        Layout::horizontal([Constraint::Length(tab_width as u16), Constraint::Min(1)]).split(area);
    let selected = match app.view() {
        TaskView::All => 0,
        TaskView::Open => 1,
        TaskView::Done => 2,
    };
    frame.render_widget(
        Tabs::new(tabs.map(|view| {
            if view == app.view() {
                format!("[{}]", view.label())
            } else {
                format!(" {} ", view.label())
            }
        }))
        .select(selected)
        .padding("", "")
        .divider("│")
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Green)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        columns[0],
    );
    let area = columns[1];
    let matches = app.match_count();
    let Some(editor) = app
        .editor()
        .filter(|_| matches!(app.mode(), Mode::Search | Mode::Filter))
    else {
        let filter = Some(app.filter_text()).filter(|text| !text.is_empty());
        let status = status_line(matches, app.query(), filter, app.order(), area.width);
        frame.render_widget(Paragraph::new(status), area);
        return;
    };
    // An editor on the status row follows the match count and uses the rest of the row.
    let sigil = if app.mode() == Mode::Filter {
        "f:"
    } else {
        "/"
    };
    let prefix = format!(" · {matches} matches · {sigil}");
    let prefix_width = text_width(&prefix).min(usize::from(area.width)) as u16;
    let (buffer, cursor_column) = editor_window(
        editor.buffer(),
        editor.cursor(),
        area.width.saturating_sub(prefix_width),
    );
    frame.render_widget(Paragraph::new(format!("{prefix}{buffer}")), area);
    frame.set_cursor_position((area.x + prefix_width + cursor_column, area.y));
}

fn text_width(text: &str) -> usize {
    Line::from(text).width()
}

/// The match count is always kept; past `width`, the sort segment goes first, then the filter
/// is truncated with `…`. The empty `/` placeholder gives way to an active filter or sort.
fn status_line(
    matches: usize,
    query: &str,
    filter: Option<&str>,
    order: TaskOrder,
    width: u16,
) -> String {
    let sort = match order {
        TaskOrder::Manual => "",
        TaskOrder::Priority => " · by priority",
        TaskOrder::Due => " · by due",
    };
    let sorted = !sort.is_empty();
    const FILTER: &str = " · f:";
    let width = usize::from(width);
    let mut line = format!(" · {matches} matches");
    if !query.is_empty() || (filter.is_none() && !sorted) {
        line.push_str(" · /");
        line.push_str(query);
    }
    let used = text_width(&line);
    let Some(filter) = filter else {
        if used + text_width(sort) <= width {
            line.push_str(sort);
        }
        return line;
    };
    let full = used + text_width(FILTER) + text_width(filter);
    if full <= width {
        line.push_str(FILTER);
        line.push_str(filter);
        if full + text_width(sort) <= width {
            line.push_str(sort);
        }
    } else if let Some(room) = width.checked_sub(used + text_width(FILTER) + 1) {
        line.push_str(FILTER);
        let mut clipped = 0;
        for character in filter.chars() {
            clipped += text_width(character.encode_utf8(&mut [0; 4]));
            if clipped > room {
                break;
            }
            line.push(character);
        }
        line.push('…');
    }
    line
}

fn render_content(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App, keymap: &Keymap) {
    if app.is_trash_view() {
        render_trash_content(frame, area, app, keymap);
        return;
    }
    let rows = app.displayed_rows().collect::<Vec<_>>();
    let editor = app
        .editor()
        .filter(|editor| !matches!(editor.kind(), EditKind::Search | EditKind::Filter));
    if rows.is_empty() && editor.is_none() {
        let no_tasks = app.tasks().visible_tasks().next().is_none();
        let mut text = if no_tasks {
            "No tasks yet"
        } else {
            "No matching tasks"
        }
        .to_owned();
        if no_tasks
            && app.mode() == Mode::Normal
            && let (Some(add), Some(help)) = (
                binding_label(keymap, Mode::Normal, BindingId::StartAdd),
                binding_label(keymap, Mode::Normal, BindingId::OpenHelp),
            )
        {
            text.push_str(&format!("\nPress {add} to add · {help} for help"));
        } else if !no_tasks && app.mode() == Mode::Normal {
            // Esc clears the search first, then the filter.
            let clear = if app.query().is_empty() && !app.filter().is_empty() {
                "clear filter"
            } else {
                "clear search"
            };
            let hints = [
                (BindingId::ClearSearch, clear),
                (BindingId::CycleView, "next view"),
                (BindingId::PreviousView, "previous view"),
                (BindingId::StartSearch, "edit search"),
                (BindingId::EditFilter, "edit filter"),
            ]
            .into_iter()
            .filter_map(|(id, description)| {
                binding_label(keymap, Mode::Normal, id).map(|key| format!("{key} {description}"))
            })
            .collect::<Vec<_>>();
            if !hints.is_empty() {
                text.push_str(&format!("\n{}", hints.join(" · ")));
            }
        }
        if app.mode() == Mode::Normal
            && let Some(trash) = binding_label(keymap, Mode::Normal, BindingId::OpenTrash)
        {
            text.push_str(&format!("\nPress {trash} to browse trash"));
        }
        frame.render_widget(Paragraph::new(text), area);
        return;
    }

    // A subtask's draft starts two columns further right, under its indented marker.
    let editor_indent = editor.map_or(0, |editor| match editor.kind() {
        EditKind::AddSubtask(_) => SUBTASK_INDENT,
        EditKind::Edit(id)
            if app
                .tasks()
                .task(id)
                .is_some_and(|task| task.parent_id().is_some()) =>
        {
            SUBTASK_INDENT
        }
        _ => 0,
    });
    let editor_width = area.width.saturating_sub(ROW_PREFIX_WIDTH + editor_indent);
    let editor_view = editor.map(|editor| {
        let (visible, cursor_column) =
            editor_window(editor.buffer(), editor.cursor(), editor_width);
        (editor.kind(), visible, cursor_column)
    });
    // Where a detached draft row goes: a new subtask after its group's last displayed row,
    // anything else at the end.
    let detached_at = editor_view.and_then(|(kind, _, _)| match kind {
        EditKind::Add => Some(rows.len()),
        EditKind::AddSubtask(parent) => Some(
            rows.iter()
                .rposition(|row| row.task.id() == parent || row.task.parent_id() == Some(parent))
                .map_or(rows.len(), |index| index + 1),
        ),
        EditKind::Edit(id) => (!rows.iter().any(|row| row.task.id() == id)).then_some(rows.len()),
        EditKind::Search | EditKind::Filter => None,
    });
    let selected_index = detached_at.or_else(|| {
        rows.iter()
            .position(|row| Some(row.task.id()) == app.selected())
    });
    // Style only the rows the list can show: the window a fresh `ListState` scrolls to, which
    // ends at the selected row once it is past the first screen.
    let height = usize::from(area.height);
    debug_assert!(
        height > 0,
        "the layout keeps list rows at the minimum terminal size"
    );
    let start = selected_index.map_or(0, |index| (index + 1).saturating_sub(height));
    let total = rows.len() + usize::from(detached_at.is_some());
    let items = (start..total.min(start + height))
        .map(|index| {
            let row = match detached_at {
                Some(at) if index == at => None,
                Some(at) if index > at => Some(&rows[index - 1]),
                _ => Some(&rows[index]),
            };
            match (row, editor_view) {
                (Some(row), Some((EditKind::Edit(id), visible, _))) if row.task.id() == id => {
                    editor_item(visible, editor_indent)
                }
                (Some(row), _) => task_item(row, app.today()),
                (None, view) => {
                    editor_item(view.map_or("", |(_, visible, _)| visible), editor_indent)
                }
            }
        })
        .collect::<Vec<_>>();
    let selected_index = selected_index.map(|index| index.saturating_sub(start));

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
                .saturating_add(ROW_PREFIX_WIDTH + editor_indent)
                .saturating_add(cursor_column),
            area.y.saturating_add(row),
        ));
    }
}

fn render_trash_content(frame: &mut Frame<'_>, area: Rect, app: &App, keymap: &Keymap) {
    let deleted = app.tasks().deleted_tasks();
    if deleted.is_empty() {
        let mut text = "Trash is empty".to_owned();
        if let Some(close) = binding_label(keymap, Mode::Trash, BindingId::CloseTrash) {
            text.push_str(&format!("\nPress {close} to return to the list"));
        }
        frame.render_widget(Paragraph::new(text), area);
        return;
    }
    let selected = deleted
        .iter()
        .position(|task| Some(task.id()) == app.selected());
    // A subtask deleted with its parent follows the parent's row here (one sequence, canonical
    // order) and is indented under it; one deleted on its own is not.
    let mut group = None;
    let items = deleted
        .iter()
        .map(|task| {
            let state = if task.completed() { "done" } else { "open" };
            let indented = task.parent_id().is_some()
                && group
                    == task
                        .parent_id()
                        .map(|parent| (parent, task.deletion_sequence()));
            if task.parent_id().is_none() {
                group = Some((task.id(), task.deletion_sequence()));
            }
            let indent = if indented { "  " } else { "" };
            let mut spans = vec![Span::raw(format!("{indent}{}  {state}  ", task.id().get()))];
            // Tombstones are not actionable, so their dates are never overdue or due today.
            let due = metadata::due_token(task.text()).map(|(token, _)| (token, DUE_STYLE));
            spans.extend(text_spans(task.text(), Style::default(), due));
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .highlight_symbol("› ")
        .highlight_style(Style::default().bg(Color::DarkGray));
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_footer(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App, keymap: &Keymap) {
    let mut bindings = keymap
        .bindings_for(app.mode())
        .filter(|binding| binding.footer_priority().is_some())
        .collect::<Vec<_>>();
    bindings.sort_by_key(|binding| binding.footer_priority());
    let hints = bindings
        .into_iter()
        .map(|binding| footer_binding(binding, app.mode()))
        .collect::<Vec<_>>()
        .join(" · ");
    let conflict = if app.editor().is_some_and(|editor| editor.conflicted()) {
        binding_label(keymap, Mode::Insert, BindingId::CancelEdit)
            .map(|cancel| format!("Draft conflict; {cancel} cancel and reopen"))
    } else {
        None
    };
    let label = mode_label(app.mode());
    let notice_width = usize::from(area.width).saturating_sub(text_width(label) + 2);
    let notice = app
        .notice()
        .map(|notice| notice_text(notice, keymap, notice_width));
    let message = if app.needs_sync() || app.storage_unavailable() {
        app.message()
    } else {
        conflict.as_deref().or(app.message()).or(notice.as_deref())
    };
    let detail = match (message, hints.is_empty()) {
        (Some(message), false) => format!("{message} · {hints}"),
        (Some(message), true) => message.to_owned(),
        (None, _) => hints,
    };
    let text = format!("{label}  {detail}");

    frame.render_widget(Paragraph::new(text), area);
}

fn notice_text(notice: &Notice, keymap: &Keymap, width: usize) -> String {
    match notice {
        Notice::ManualOrderRequired => {
            match binding_label(keymap, Mode::Normal, BindingId::ToggleSort) {
                Some(key) => format!("Reorder needs manual order; {key} cycles"),
                None => "Reorder needs manual order".into(),
            }
        }
        Notice::FinishTag(tags) => clip_entries("Finish the tag: ", tags, ", ", width),
        Notice::RestoreParentFirst(parent) => format!("Restore parent {} first", parent.get()),
        Notice::AlreadySubtask => {
            match binding_label(keymap, Mode::Normal, BindingId::OutdentTask) {
                Some(key) => format!("Already a subtask; {key} promotes"),
                None => "Already a subtask".into(),
            }
        }
        Notice::NoTaskAbove => "No task above to nest under".into(),
        Notice::HasSubtasks => "Has subtasks; promote them first".into(),
        Notice::AlreadyTopLevel => "Already top-level".into(),
    }
}

fn render_filter_hint(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let entries = app
        .filter_hint()
        .unwrap_or_default()
        .into_iter()
        .map(|(tag, count)| format!("{tag} {count}"))
        .collect::<Vec<_>>();
    let text = if entries.is_empty() {
        "No tags in this view".to_owned()
    } else {
        clip_entries("", &entries, " · ", usize::from(area.width))
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Gray)),
        area,
    );
}

/// Drops whole entries from the end and appends `…`; if even the first entry does not fit,
/// cuts it to `width - 1` columns plus `…`.
fn clip_entries(prefix: &str, entries: &[String], separator: &str, width: usize) -> String {
    let full = format!("{prefix}{}", entries.join(separator));
    if text_width(&full) <= width {
        return full;
    }
    for kept in (1..entries.len()).rev() {
        let clipped = format!("{prefix}{}{separator}…", entries[..kept].join(separator));
        if text_width(&clipped) <= width {
            return clipped;
        }
    }
    let mut cut = String::new();
    for character in full.chars() {
        if text_width(&cut) + text_width(character.encode_utf8(&mut [0; 4])) >= width {
            break;
        }
        cut.push(character);
    }
    cut.push('…');
    cut
}

fn render_help(frame: &mut Frame<'_>, area: Rect, keymap: &Keymap, app: &App) {
    let trash = app.is_trash_view();
    // The mode name comes first so it survives clipping; the header text follows it, because
    // the popup covers the header. No bottom border, so the footer stays directly below.
    let title = format!(
        "{} · shtodo{}",
        if trash {
            "Keyboard help: Trash"
        } else {
            "Keyboard help"
        },
        header_detail(app)
    );
    let block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let normal = help_lines(keymap, if trash { Mode::Trash } else { Mode::Normal });
    let mut insert_and_help = Vec::new();
    if !trash {
        insert_and_help = help_lines(keymap, Mode::Insert);
        // The search editor shares the Insert keys; listing it here keeps the Normal column short.
        for id in [BindingId::CommitEdit, BindingId::CancelEdit] {
            if let Some(binding) = keymap
                .bindings_for(Mode::Search)
                .find(|binding| binding.id() == id)
            {
                insert_and_help.push(Line::from(format!(
                    "{} {}",
                    binding.labels().collect::<Vec<_>>().join(" / "),
                    if id == BindingId::CommitEdit {
                        "apply search (Insert keys)"
                    } else {
                        search_description(binding)
                    },
                )));
            }
        }
        insert_and_help.push(Line::default());
    }
    let help = help_lines(keymap, Mode::Help);
    let mut separated = insert_and_help.clone();
    separated.extend(help.iter().cloned());
    let mut compact = insert_and_help;
    // Without room for two columns, the blank line before Help is dropped first.
    if !trash {
        compact.pop();
    }
    compact.extend(help);
    let columns = help_column_width(&normal, &separated, inner)
        .map(|width| (width, separated.clone()))
        .or_else(|| help_column_width(&normal, &compact, inner).map(|width| (width, compact)));
    if let Some((normal_width, insert_and_help)) = columns {
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
        all_modes.extend(separated);
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
    let row = |binding: &ResolvedBinding| {
        format!(
            "{} {}",
            binding.labels().collect::<Vec<_>>().join(" / "),
            binding.description()
        )
    };
    // Nest and promote share one row while both are bound.
    let outdent = keymap
        .bindings_for(mode)
        .find(|binding| binding.id() == BindingId::OutdentTask);
    let indent_bound = keymap
        .bindings_for(mode)
        .any(|binding| binding.id() == BindingId::IndentTask);
    lines.extend(keymap.bindings_for(mode).filter_map(|binding| {
        match (binding.id(), outdent) {
            (BindingId::IndentTask, Some(outdent)) => {
                Some(format!("{}, {}", row(binding), row(outdent)))
            }
            (BindingId::OutdentTask, _) if indent_bound => None,
            _ => Some(row(binding)),
        }
        .map(Line::from)
    }));
    lines.extend(keymap.unbound_actions_for(mode).filter_map(|id| {
        id.config_name()
            .map(|name| Line::from(format!("Unbound: {name} (config)")))
    }));
    lines
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "Normal",
        Mode::Insert => "Insert",
        Mode::Search => "Search",
        Mode::Filter => "Filter",
        Mode::Help => "Help",
        Mode::Trash => "Trash",
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

fn task_item(row: &DisplayedRow<'_>, today: Date) -> ListItem<'static> {
    let task = row.task;
    let completed = task.completed();
    let marker = if completed { "✓ " } else { "○ " };
    // A context row is dimmed throughout; a done one keeps its strikethrough.
    let row_style = if row.context {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default()
    };
    let text_style = if completed {
        row_style.add_modifier(Modifier::DIM | Modifier::CROSSED_OUT)
    } else {
        row_style
    };
    let due = metadata::due_token(task.text());
    // A done task is never overdue or due today, so its token is blue and its marker plain.
    let urgent = due
        .filter(|_| !completed)
        .and_then(|(_, date)| urgent_style(date, today));
    let marker_style = row_style.patch(urgent.unwrap_or_default());
    let due_style = text_style.patch(urgent.unwrap_or(DUE_STYLE));

    let mut spans = Vec::new();
    if task.parent_id().is_some() {
        spans.push(Span::raw(" ".repeat(usize::from(SUBTASK_INDENT))));
    }
    spans.push(Span::styled(marker, marker_style));
    if let Some((done, total)) = row.progress {
        spans.push(Span::styled(
            format!("{done}/{total} "),
            row_style.add_modifier(Modifier::DIM),
        ));
    }
    spans.extend(text_spans(
        task.text(),
        text_style,
        due.map(|(token, _)| (token, due_style)),
    ));
    ListItem::new(Line::from(spans))
}

/// Overdue is red and due today yellow, both bold; any other date has no urgent style.
fn urgent_style(due: Date, today: Date) -> Option<Style> {
    let color = match due.cmp(&today) {
        Ordering::Less => OVERDUE_COLOR,
        Ordering::Equal => DUE_TODAY_COLOR,
        Ordering::Greater => return None,
    };
    Some(Style::default().fg(color).add_modifier(Modifier::BOLD))
}

/// Styles the priority marker, tag slices, and the `due` slice in place; the rest, including
/// punctuation stripped from a tag or due token, keeps `base`, so the row text and its width are
/// unchanged. The three kinds never overlap.
fn text_spans(text: &str, base: Style, due: Option<(&str, Style)>) -> Vec<Span<'static>> {
    // Every slice borrows `text`, so its offset comes from the slice address.
    let offset = |slice: &str| {
        let start = slice.as_ptr().addr() - text.as_ptr().addr();
        debug_assert!(start + slice.len() <= text.len() && text.is_char_boundary(start));
        start
    };
    let mut styled = metadata::tags(text)
        .map(|tag| {
            let color = if tag.starts_with('+') {
                PROJECT_COLOR
            } else {
                CONTEXT_COLOR
            };
            (offset(tag), tag, base.fg(color))
        })
        .collect::<Vec<_>>();
    if let Some((token, style)) = due {
        let start = offset(token);
        let index = styled.partition_point(|(tag, _, _)| *tag < start);
        styled.insert(index, (start, token, style));
    }

    let mut spans = Vec::new();
    let mut end = 0;
    if metadata::priority(text).is_some() {
        end = "(A)".len();
        spans.push(Span::styled(
            text[..end].to_owned(),
            base.add_modifier(Modifier::BOLD),
        ));
    }
    for (start, slice, style) in styled {
        if start > end {
            spans.push(Span::styled(text[end..start].to_owned(), base));
        }
        spans.push(Span::styled(slice.to_owned(), style));
        end = start + slice.len();
    }
    if end < text.len() || spans.is_empty() {
        spans.push(Span::styled(text[end..].to_owned(), base));
    }
    spans
}

fn editor_item(text: &str, indent: u16) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::raw(" ".repeat(usize::from(indent))),
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

fn search_description(binding: &ResolvedBinding) -> &'static str {
    match binding.id() {
        BindingId::CommitEdit => "apply search",
        BindingId::CancelEdit => "cancel search",
        _ => binding.description(),
    }
}

fn filter_description(binding: &ResolvedBinding) -> &'static str {
    match binding.id() {
        BindingId::CommitEdit => "apply filter",
        BindingId::CancelEdit => "cancel filter",
        _ => binding.description(),
    }
}

fn footer_binding(binding: &ResolvedBinding, mode: Mode) -> String {
    let description = match mode {
        Mode::Search => search_description(binding),
        Mode::Filter => filter_description(binding),
        _ => binding.description(),
    };
    let description = match description.strip_prefix("show ") {
        Some(description) => description,
        None => description,
    };
    format!("{} {description}", binding.preferred_label())
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Search => "SEARCH",
        Mode::Filter => "FILTER",
        Mode::Help => "HELP",
        Mode::Trash => "TRASH",
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
                    crate::app::Mode::Search | crate::app::Mode::Filter => "insert",
                    crate::app::Mode::Help => "help",
                    crate::app::Mode::Trash => "trash",
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
        assert!(text.contains("Press t to browse trash"));
    }

    #[test]
    fn trash_fallback_should_drive_normal_empty_state_footer_and_help() {
        let keymap =
            Keymap::with_overrides(&[override_for(BindingId::ToggleComplete, &["t"])]).unwrap();
        let mut app = App::new(TaskList::new(ListScope::Global));
        let normal = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 12));

        assert!(normal.contains("Press Ctrl-t to browse trash"));
        assert!(normal.contains("Ctrl-t trash"));
        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 24));
        assert!(help.contains("Ctrl-t show trash"));
        assert!(help.contains("t toggle complete"));
    }

    #[test]
    fn trash_should_render_label_scope_ids_states_and_newest_first() {
        let mut tasks = TaskList::new(ListScope::Global);
        let oldest = tasks.add("same text").unwrap();
        let newest = tasks.add("same text").unwrap();
        tasks.add("live task").unwrap();
        tasks.toggle_complete(newest).unwrap();
        tasks.delete(oldest).unwrap();
        tasks.delete(newest).unwrap();
        let mut app = App::new(tasks);
        app.apply(Action::OpenTrash).unwrap();

        let buffer = render_app(&app, 80, 12);
        assert!(buffer_row(&buffer, 80, 0).contains("TRASH · global · newest first"));
        assert!(buffer_row(&buffer, 80, 0).contains("2 deleted"));
        assert!(buffer_row(&buffer, 80, 1).contains("› 2  done  same text"));
        assert!(buffer_row(&buffer, 80, 2).contains("1  open  same text"));
        assert!(!buffer_text(&buffer).contains("live task"));
        assert_eq!(buffer[(0, 1)].bg, ratatui::style::Color::DarkGray);
    }

    #[test]
    fn empty_trash_should_use_custom_close_hint_and_prioritize_restore_and_return() {
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::CloseTrash, &["b", "esc"]),
            override_for(BindingId::RestoreSelected, &["x"]),
        ])
        .unwrap();
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenTrash).unwrap();
        let buffer = render_app_with_keymap(&app, &keymap, 40, 8);

        assert!(buffer_text(&buffer).contains("Trash is empty"));
        assert!(buffer_text(&buffer).contains("Press b to return to the list"));
        let footer = buffer_row(&buffer, 40, 7);
        assert!(footer.contains("TRASH  x restore task · b close trash"));
        assert!(!buffer_text(&buffer).contains("add task"));
    }

    #[test]
    fn trash_help_should_use_its_keymap_and_exclude_normal_mutation_hints() {
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::RestoreSelected, &["enter", "x"]),
            override_for(BindingId::CloseTrash, &["b", "esc"]),
            override_for(BindingId::TrashMoveDown, &["n", "down"]),
        ])
        .unwrap();
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenTrash).unwrap();
        app.apply(Action::OpenHelp).unwrap();
        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 24));

        for hint in [
            "Keyboard help: Trash",
            "Enter / x restore task",
            "b / Esc close trash",
            "n / Down move down",
            "q / Ctrl-C quit",
            "? / Esc close help",
        ] {
            assert!(text.contains(hint), "missing help hint: {hint}");
        }
        for hint in [
            "add task",
            "edit task",
            "toggle complete",
            "delete task",
            "move task down",
            "restore latest",
        ] {
            assert!(!text.contains(hint), "inactive trash hint: {hint}");
        }
    }

    #[test]
    fn selected_trash_row_should_stay_visible_when_scrolling() {
        let mut tasks = TaskList::new(ListScope::Global);
        for index in 0..20 {
            let id = tasks.add(&format!("deleted {index}")).unwrap();
            tasks.delete(id).unwrap();
        }
        let mut app = App::new(tasks);
        app.apply(Action::OpenTrash).unwrap();
        for _ in 0..19 {
            app.apply(Action::MoveDown).unwrap();
        }
        let text = buffer_text(&render_app(&app, 40, 8));
        assert!(text.contains("› 1  open  deleted 0"));
        assert!(!text.contains("20  open  deleted 19"));
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
    fn deleted_target_should_keep_a_visible_draft_and_configured_conflict_hint() {
        let keymap =
            Keymap::with_overrides(&[override_for(BindingId::CancelEdit, &["ctrl-x"])]).unwrap();
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("draft 東京").unwrap();
        list.add("remaining").unwrap();
        let mut app = App::new(list.clone());
        app.prepare(Action::StartEdit).unwrap();
        app.prepare(Action::InsertChar('é')).unwrap();
        list.delete(id).unwrap();
        app.reconcile(list);

        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 12));
        assert_eq!(app.editor().unwrap().buffer(), "draft 東京é");
        assert!(text.contains("draft") && text.contains('é'), "{text}");
        assert!(text.contains("remaining"));
        assert!(text.contains("INSERT  Draft conflict; Ctrl-x cancel and reopen"));
        app.prepare(Action::InsertChar('!')).unwrap();
        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 12));
        assert_eq!(app.editor().unwrap().buffer(), "draft 東京é!");
        assert!(text.contains("draft") && text.contains("é!"), "{text}");
        assert!(text.contains("Draft conflict; Ctrl-x cancel and reopen"));
        app.set_storage_error(Some("Storage unavailable: invalid snapshot".into()));
        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 12));
        assert!(text.contains("Storage unavailable: invalid snapshot"));
        app.set_storage_error(None);
        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 12));
        assert!(text.contains("Draft conflict; Ctrl-x cancel and reopen"));
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
        assert!(text.contains("t show trash"));
        assert!(text.contains("Tab next view"));
        assert!(text.contains("Shift-Tab previous view"));
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
        assert_eq!(buffer[(2, 2)].symbol(), "○");
        assert_eq!(buffer[(2, 3)].symbol(), "✓");
        assert!(buffer[(4, 3)].modifier.contains(Modifier::DIM));
        assert_eq!(buffer[(0, 2)].bg, Color::DarkGray);
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
    fn list_should_scroll_the_same_window_for_every_selection_and_detached_add() {
        let mut list = TaskList::new(ListScope::Global);
        for index in 0..30 {
            list.add(&format!("(A) task {index} +p")).unwrap();
        }
        let mut app = App::new(list);
        // At 40x10 the list has rows 2 to 8; the selection scrolls into the last row.
        let rows = 2..9_u16;
        let height = rows.len();
        for selected in 0..30_usize {
            let buffer = render_app(&app, 40, 10);
            let first = (selected + 1).saturating_sub(height);
            for (offset, row) in rows.clone().enumerate() {
                let marker = if first + offset == selected {
                    "› "
                } else {
                    "  "
                };
                assert_eq!(
                    buffer_row(&buffer, 40, row).trim_end(),
                    format!("{marker}○ (A) task {} +p", first + offset),
                    "selection {selected}, row {row}"
                );
            }
            app.apply(Action::MoveDown).unwrap();
        }
        app.apply(Action::StartAdd).unwrap();
        type_text(&mut app, "new");
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &app, &Keymap::defaults()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer_row(buffer, 40, 2).trim_end(), "  ○ (A) task 24 +p");
        assert_eq!(buffer_row(buffer, 40, 8).trim_end(), "› ○ new");
        terminal.backend_mut().assert_cursor_position((7, 8));
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

        terminal.backend_mut().assert_cursor_position((12, 2));
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

        assert_eq!(terminal.backend().buffer()[(4, 2)].symbol(), "d");
        assert_eq!(terminal.backend().buffer()[(8, 2)].symbol(), "t");
        terminal.backend_mut().assert_cursor_position((9, 2));
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
            "t show trash",
            "? show help",
            "q / Ctrl-C quit",
            "/ search",
            "Tab next view",
            "Shift-Tab previous view",
            "Esc clear search",
            "Enter apply search",
            "Esc cancel search",
            "A add subtask",
            "> nest, < promote",
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

    fn search_for(app: &mut App, query: &str) {
        app.apply(Action::StartSearch).unwrap();
        for character in query.chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
        app.apply(Action::CommitEdit).unwrap();
    }

    #[test]
    fn search_view_should_render_only_matches_with_global_totals_and_matching_count() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("needle open").unwrap();
        let done = list.add("needle done").unwrap();
        list.add("hidden work").unwrap();
        list.toggle_complete(done).unwrap();
        let mut app = App::new(list);
        search_for(&mut app, "NEEDLE");
        app.apply(Action::CycleView).unwrap();
        let buffer = render_app(&app, 80, 12);
        assert!(buffer_row(&buffer, 80, 0).contains("2 open · 1 done"));
        assert!(buffer_row(&buffer, 80, 1).contains("All │[Open]│ Done  · 1 matches · /NEEDLE"));
        let text = buffer_text(&buffer);
        assert!(text.contains("needle open"));
        assert!(!text.contains("needle done"));
        assert!(!text.contains("hidden work"));
        assert!(buffer[(0, 2)].bg == Color::DarkGray);
    }

    #[test]
    fn no_matches_should_be_distinct_from_empty_storage_and_use_configured_recovery_keys() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("existing").unwrap();
        let mut app = App::new(list);
        search_for(&mut app, "absent");
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::ClearSearch, &["ctrl-g"]),
            override_for(BindingId::CycleView, &["v"]),
        ])
        .unwrap();
        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 12));
        assert!(text.contains("No matching tasks"));
        assert!(!text.contains("No tasks yet"));
        assert!(text.contains("Ctrl-g clear search · v next view"));
        assert!(!text.contains("existing"));
        assert!(text.contains("0 matches"));
    }

    #[test]
    fn search_editor_should_keep_long_unicode_query_cursor_visible_at_minimum_width() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartSearch).unwrap();
        for character in "long query café 界界 tail".chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let keymap = Keymap::defaults();
        terminal.draw(|frame| render(frame, &app, &keymap)).unwrap();
        let cursor = terminal.get_cursor_position().unwrap();
        assert_eq!(cursor.y, 1);
        assert!(cursor.x < 40);
        let buffer = terminal.backend().buffer();
        assert!(buffer_row(buffer, 40, 1).contains("tail"));
        assert!(buffer_row(buffer, 40, 7).contains("SEARCH  Enter apply search"));
        assert!(buffer_text(buffer).contains("No tasks yet"));
        assert!(!buffer_text(buffer).contains("Press i to add"));
        assert!(!buffer_text(buffer).contains("browse trash"));
    }

    #[test]
    fn custom_keys_should_drive_search_footer_and_help_without_stale_defaults() {
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::StartSearch, &["s", "/"]),
            override_for(BindingId::CycleView, &["v"]),
            override_for(BindingId::PreviousView, &["ctrl-v"]),
            override_for(BindingId::CommitEdit, &["ctrl-s"]),
            override_for(BindingId::CancelEdit, &["ctrl-g"]),
        ])
        .unwrap();
        let mut app = App::new(TaskList::new(ListScope::Global));
        let normal = buffer_row(&render_app_with_keymap(&app, &keymap, 200, 12), 200, 11);
        assert!(normal.contains("s search"));
        assert!(normal.contains("v next view"));
        assert!(normal.contains("Ctrl-v previous view"));
        app.apply(Action::StartSearch).unwrap();
        let footer = buffer_row(&render_app_with_keymap(&app, &keymap, 100, 12), 100, 11);
        assert!(footer.contains("Ctrl-s apply search · Ctrl-g cancel search"));
        assert!(!footer.contains("save edit"));
        app.apply(Action::CancelEdit).unwrap();
        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 30));
        assert!(help.contains("s / / search"));
        assert!(help.contains("v next view"));
        assert!(help.contains("Ctrl-v previous view"));
        assert!(!help.contains("Tab next view"));
        assert!(!help.contains("Shift-Tab previous view"));
        assert!(help.contains("Ctrl-s apply search (Insert keys)"));
        assert!(help.contains("Ctrl-g cancel search"));
    }

    #[test]
    fn filtered_navigation_and_rendering_should_highlight_the_same_task_when_scrolling() {
        let mut list = TaskList::new(ListScope::Global);
        for index in 0..20 {
            list.add(&format!(
                "{} task {index}",
                if index % 2 == 0 { "match" } else { "hidden" }
            ))
            .unwrap();
        }
        let mut app = App::new(list);
        search_for(&mut app, "match");
        for _ in 0..9 {
            app.apply(Action::MoveDown).unwrap();
        }
        assert_eq!(app.selected_task().unwrap().text(), "match task 18");
        let buffer = render_app(&app, 40, 8);
        let text = buffer_text(&buffer);
        assert!(text.contains("match task 18"));
        assert!(!text.contains("hidden task"));
        assert!(!text.contains("match task 0"));
        assert!(buffer_row(&buffer, 40, 6).contains("› ○ match task 18"));
    }

    #[test]
    fn help_should_explain_unbound_search_actions_without_advertising_reclaimed_keys() {
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::ToggleComplete, &["f", "tab", "shift-tab"]),
            override_for(BindingId::StartAdd, &["/"]),
            override_for(BindingId::OpenHelp, &["esc"]),
        ])
        .unwrap();
        let mut app = App::new(TaskList::new(ListScope::Global));
        let footer = buffer_row(&render_app_with_keymap(&app, &keymap, 200, 12), 200, 11);
        assert!(footer.contains("/ add task"));
        assert!(footer.contains("Esc help"));
        assert!(footer.contains("f toggle complete"));
        assert!(!footer.contains("/ search"));
        assert!(!footer.contains("next view"));
        assert!(!footer.contains("previous view"));
        assert!(!footer.contains("clear search"));
        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 24));
        for name in [
            "start_search",
            "cycle_view",
            "previous_view",
            "clear_search",
        ] {
            assert!(help.contains(&format!("Unbound: {name} (config)")));
        }
    }

    #[test]
    fn no_matches_should_show_available_recovery_keys_when_clear_or_view_is_unbound() {
        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::ToggleComplete, &["tab"]),
            override_for(BindingId::Delete, &["esc"]),
        ])
        .unwrap();
        let mut list = TaskList::new(ListScope::Global);
        list.add("existing").unwrap();
        let mut app = App::new(list);
        search_for(&mut app, "absent");
        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 12));
        assert!(text.contains("No matching tasks"));
        assert!(text.contains("/ edit search"));
        assert!(!text.contains("Esc clear search"));
        assert!(!text.contains("Tab next view"));
        assert!(text.contains("Shift-Tab previous view"));
    }

    #[test]
    fn tabs_should_highlight_the_active_view_and_help_should_show_both_directions() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        for (selected_x, tab_strip) in [
            (1, "[All]│ Open │ Done "),
            (7, " All │[Open]│ Done "),
            (14, " All │ Open │[Done]"),
        ] {
            let buffer = render_app(&app, 40, 8);
            assert!(buffer_row(&buffer, 40, 1).starts_with(tab_strip));
            for x in [1, 7, 14] {
                let cell = &buffer[(x, 1)];
                assert_eq!(cell.fg == Color::Green, x == selected_x);
                assert_eq!(
                    cell.modifier.contains(Modifier::UNDERLINED),
                    x == selected_x
                );
                assert_eq!(cell.bg == Color::DarkGray, x == selected_x);
            }
            app.apply(Action::CycleView).unwrap();
        }
        app.apply(Action::PreviousView).unwrap();
        let buffer = render_app(&app, 40, 8);
        assert_eq!(buffer[(14, 1)].fg, Color::Green);
        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app(&app, 80, 24));
        assert!(help.contains("Tab next view"));
        assert!(help.contains("Shift-Tab previous view"));
        assert!(help.contains("Enter apply search"));
        assert!(help.contains("Esc cancel search"));
    }

    /// A row as displayed: the blank cell after each wide character is skipped.
    fn display_row(buffer: &Buffer, row: u16) -> String {
        let mut text = String::new();
        let mut column = 0;
        while column < buffer.area.width {
            let symbol = buffer[(column, row)].symbol();
            text.push_str(symbol);
            column += u16::try_from(super::text_width(symbol)).unwrap().max(1);
        }
        text
    }

    fn row_cells(buffer: &Buffer, row: u16, text: &str) -> Vec<Cell> {
        let width = buffer.area.width;
        let line = buffer_row(buffer, width, row);
        let start = line
            .find(text)
            .unwrap_or_else(|| panic!("{text:?} not in {line:?}"));
        let column = line[..start].chars().count() as u16;
        (column..column + text.chars().count() as u16)
            .map(|x| buffer[(x, row)].clone())
            .collect()
    }

    #[test]
    fn rows_should_style_priority_and_tags_in_place_and_leave_stripped_punctuation_plain() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("(A) Ship +shtodo. @laptop x+y").unwrap();
        list.add("plain (B) text").unwrap();
        let app = App::new(list);
        let buffer = render_app(&app, 80, 12);

        assert!(buffer_row(&buffer, 80, 2).contains("○ (A) Ship +shtodo. @laptop x+y"));
        let cells = row_cells(&buffer, 2, "(A) Ship +shtodo. @laptop x+y");
        let bold = |cell: &Cell| cell.modifier.contains(Modifier::BOLD);
        assert!(cells[..3].iter().all(bold));
        assert!(!bold(&cells[3]));
        assert!(
            cells[4..8]
                .iter()
                .all(|cell| !bold(cell) && cell.fg == Color::Reset)
        );
        assert!(cells[9..16].iter().all(|cell| cell.fg == Color::Cyan));
        assert_eq!(cells[16].symbol(), ".");
        assert_eq!(cells[16].fg, Color::Reset);
        assert!(cells[18..25].iter().all(|cell| cell.fg == Color::Magenta));
        assert!(cells[26..].iter().all(|cell| cell.fg == Color::Reset));
        let plain = row_cells(&buffer, 3, "plain (B) text");
        assert!(
            plain
                .iter()
                .all(|cell| !bold(cell) && cell.fg == Color::Reset)
        );
    }

    #[test]
    fn completed_and_trash_rows_should_style_tokens_and_keep_completed_style_on_every_span() {
        let mut list = TaskList::new(ListScope::Global);
        let done = list.add("(C) done +proj @ctx").unwrap();
        list.toggle_complete(done).unwrap();
        let deleted = list.add("(Z) gone +old!").unwrap();
        list.delete(deleted).unwrap();
        let mut app = App::new(list);
        let buffer = render_app(&app, 80, 12);

        let cells = row_cells(&buffer, 2, "(C) done +proj @ctx");
        for cell in &cells {
            assert!(
                cell.modifier
                    .contains(Modifier::DIM | Modifier::CROSSED_OUT),
                "{cell:?}"
            );
        }
        assert!(
            cells[..3]
                .iter()
                .all(|cell| cell.modifier.contains(Modifier::BOLD))
        );
        assert!(cells[9..14].iter().all(|cell| cell.fg == Color::Cyan));
        assert!(cells[15..].iter().all(|cell| cell.fg == Color::Magenta));

        app.apply(Action::OpenTrash).unwrap();
        let buffer = render_app(&app, 80, 12);
        assert!(buffer_row(&buffer, 80, 1).contains("2  open  (Z) gone +old!"));
        let cells = row_cells(&buffer, 1, "(Z) gone +old!");
        assert!(
            cells[..3]
                .iter()
                .all(|cell| cell.modifier.contains(Modifier::BOLD))
        );
        assert!(cells[9..13].iter().all(|cell| cell.fg == Color::Cyan));
        assert_eq!(cells[13].fg, Color::Reset);
    }

    /// The `○ `/`✓ ` marker cells of a list row, after the two-column highlight symbol.
    fn marker_cells(buffer: &Buffer, row: u16) -> [Cell; 2] {
        [buffer[(2, row)].clone(), buffer[(3, row)].clone()]
    }

    fn due_rows() -> App {
        // `App::new` displays 2026-10-21.
        let mut list = TaskList::new(ListScope::Global);
        list.add("(A) late due:2026-10-20. +home").unwrap();
        list.add("now due:2026-10-21").unwrap();
        list.add("later due:2026-10-22 @desk").unwrap();
        let done = list.add("old due:2026-10-01").unwrap();
        list.toggle_complete(done).unwrap();
        list.add("odd due:2026-02-30").unwrap();
        App::new(list)
    }

    #[test]
    fn due_tokens_should_color_rows_and_markers_by_the_displayed_date_at_minimum_size() {
        let app = due_rows();
        let buffer = render_app(&app, 40, 8);
        let bold = |cell: &Cell| cell.modifier.contains(Modifier::BOLD);

        assert!(buffer_row(&buffer, 40, 2).starts_with("› ○ (A) late due:2026-10-20. +home"));
        let marker = marker_cells(&buffer, 2);
        assert_eq!(marker[0].symbol(), "○");
        assert!(
            marker
                .iter()
                .all(|cell| cell.fg == Color::Red && bold(cell))
        );
        let cells = row_cells(&buffer, 2, "(A) late due:2026-10-20. +home");
        assert!(
            cells[..3]
                .iter()
                .all(|cell| bold(cell) && cell.fg == Color::Reset)
        );
        assert!(
            cells[4..8]
                .iter()
                .all(|cell| !bold(cell) && cell.fg == Color::Reset)
        );
        assert!(
            cells[9..23]
                .iter()
                .all(|cell| cell.fg == Color::Red && bold(cell))
        );
        assert_eq!(cells[23].symbol(), ".");
        assert!(cells[23].fg == Color::Reset && !bold(&cells[23]));
        assert!(cells[25..].iter().all(|cell| cell.fg == Color::Cyan));
        assert!(
            buffer[(0, 2)].bg == Color::DarkGray
                && cells.iter().all(|cell| cell.bg == Color::DarkGray),
            "the selection highlight is unchanged"
        );

        let marker = marker_cells(&buffer, 3);
        assert!(
            marker
                .iter()
                .all(|cell| cell.fg == Color::Yellow && bold(cell))
        );
        let cells = row_cells(&buffer, 3, "now due:2026-10-21");
        assert!(cells[..4].iter().all(|cell| cell.fg == Color::Reset));
        assert!(
            cells[4..]
                .iter()
                .all(|cell| cell.fg == Color::Yellow && bold(cell))
        );
        assert_eq!(cells[0].bg, Color::Reset);

        let marker = marker_cells(&buffer, 4);
        assert!(
            marker
                .iter()
                .all(|cell| cell.fg == Color::Reset && !bold(cell))
        );
        let cells = row_cells(&buffer, 4, "later due:2026-10-22 @desk");
        assert!(
            cells[6..20]
                .iter()
                .all(|cell| cell.fg == Color::Blue && !bold(cell))
        );
        assert!(cells[21..].iter().all(|cell| cell.fg == Color::Magenta));

        let marker = marker_cells(&buffer, 5);
        assert_eq!(marker[0].symbol(), "✓");
        assert!(
            marker
                .iter()
                .all(|cell| cell.fg == Color::Reset && !bold(cell))
        );
        let cells = row_cells(&buffer, 5, "old due:2026-10-01");
        assert!(cells.iter().all(|cell| {
            cell.modifier
                .contains(Modifier::DIM | Modifier::CROSSED_OUT)
        }));
        assert!(
            cells[4..]
                .iter()
                .all(|cell| cell.fg == Color::Blue && !bold(cell))
        );
        assert!(
            cells
                .iter()
                .all(|cell| !matches!(cell.fg, Color::Red | Color::Yellow))
        );

        let cells = row_cells(&buffer, 6, "odd due:2026-02-30");
        assert!(
            cells
                .iter()
                .all(|cell| cell.fg == Color::Reset && !bold(cell))
        );
        assert!(
            marker_cells(&buffer, 6)
                .iter()
                .all(|cell| cell.fg == Color::Reset)
        );
    }

    #[test]
    fn a_clipped_due_token_should_leave_the_marker_carrying_the_overdue_state() {
        let mut list = TaskList::new(ListScope::Global);
        list.add(&format!("{} due:2026-10-01", "long ".repeat(10)))
            .unwrap();
        list.add(&format!("{} due:2026-10-21", "wide ".repeat(10)))
            .unwrap();
        let app = App::new(list);
        let buffer = render_app(&app, 40, 8);

        for row in [2, 3] {
            assert!(
                !buffer_row(&buffer, 40, row).contains("due:"),
                "row {row} is clipped"
            );
        }
        let bold = |cell: &Cell| cell.modifier.contains(Modifier::BOLD);
        assert!(
            marker_cells(&buffer, 2)
                .iter()
                .all(|cell| cell.fg == Color::Red && bold(cell))
        );
        assert!(
            marker_cells(&buffer, 3)
                .iter()
                .all(|cell| cell.fg == Color::Yellow && bold(cell))
        );
        assert_eq!(buffer[(0, 2)].bg, Color::DarkGray);
    }

    #[test]
    fn trash_rows_should_style_due_tokens_blue_without_marker_color() {
        let mut list = TaskList::new(ListScope::Global);
        let late = list.add("late due:2026-10-01").unwrap();
        let now = list.add("now due:2026-10-21!").unwrap();
        list.delete(late).unwrap();
        list.delete(now).unwrap();
        let mut app = App::new(list);
        app.apply(Action::OpenTrash).unwrap();
        let buffer = render_app(&app, 40, 8);

        assert!(buffer_row(&buffer, 40, 1).contains("2  open  now due:2026-10-21!"));
        let cells = row_cells(&buffer, 1, "now due:2026-10-21!");
        assert!(cells[4..18].iter().all(|cell| cell.fg == Color::Blue));
        assert_eq!(cells[18].fg, Color::Reset);
        let cells = row_cells(&buffer, 2, "1  open  late due:2026-10-01");
        assert!(cells[..9].iter().all(|cell| cell.fg == Color::Reset));
        assert!(cells[14..].iter().all(|cell| cell.fg == Color::Blue));
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| !matches!(cell.fg, Color::Red | Color::Yellow))
        );
    }

    #[test]
    fn a_new_displayed_date_should_restyle_rows() {
        let mut app = due_rows();
        assert!(!app.set_today(crate::app::test_today()), "same date");
        assert!(app.set_today(crate::metadata::Date::new(2026, 10, 22).unwrap()));
        let buffer = render_app(&app, 40, 8);
        let colors = (2..=4)
            .map(|row| marker_cells(&buffer, row)[0].fg)
            .collect::<Vec<_>>();
        assert_eq!(colors, [Color::Red, Color::Red, Color::Yellow]);
        let cells = row_cells(&buffer, 4, "later due:2026-10-22 @desk");
        assert!(cells[6..20].iter().all(|cell| cell.fg == Color::Yellow));
    }

    #[test]
    fn status_line_should_drop_sort_first_then_truncate_the_filter_and_keep_the_count() {
        use super::status_line;
        use crate::projection::TaskOrder::{Due, Manual, Priority};

        assert_eq!(status_line(3, "", None, Manual, 40), " · 3 matches · /");
        assert_eq!(
            status_line(3, "café", None, Manual, 40),
            " · 3 matches · /café"
        );
        assert_eq!(
            status_line(2, "", Some("+shtodo A-B"), Priority, 60),
            " · 2 matches · f:+shtodo A-B · by priority"
        );
        assert_eq!(
            status_line(2, "x", Some("+shtodo"), Priority, 60),
            " · 2 matches · /x · f:+shtodo · by priority"
        );
        assert_eq!(
            status_line(2, "", None, Priority, 40),
            " · 2 matches · by priority"
        );
        assert_eq!(
            status_line(2, "", Some("+shtodo A-B"), Priority, 30),
            " · 2 matches · f:+shtodo A-B"
        );
        assert_eq!(
            status_line(2, "", Some("+shtodo A-B"), Priority, 21),
            " · 2 matches · f:+sh…"
        );
        assert_eq!(status_line(2, "", None, Priority, 21), " · 2 matches");
        assert_eq!(
            status_line(2, "", Some("+東京 +x"), Manual, 21),
            " · 2 matches · f:+東…"
        );
        assert_eq!(
            status_line(10_000, "", Some("+x A"), Manual, 22),
            " · 10000 matches · f:…"
        );
        assert_eq!(
            status_line(10_000, "", Some("+x A"), Manual, 21),
            " · 10000 matches"
        );
        assert_eq!(
            status_line(10_000, "query", None, Priority, 10),
            " · 10000 matches · /query"
        );

        assert_eq!(status_line(2, "", None, Due, 40), " · 2 matches · by due");
        assert_eq!(
            status_line(2, "", Some("due:today"), Due, 40),
            " · 2 matches · f:due:today · by due"
        );
        assert_eq!(
            status_line(2, "", Some("due:today"), Due, 30),
            " · 2 matches · f:due:today"
        );
        assert_eq!(
            status_line(2, "", Some("+shtodo A-B"), Due, 21),
            " · 2 matches · f:+sh…"
        );
        assert_eq!(status_line(2, "", None, Due, 20), " · 2 matches");
        assert_eq!(
            status_line(2, "x", None, Due, 40),
            " · 2 matches · /x · by due"
        );
    }

    fn prioritized() -> App {
        let mut list = TaskList::new(ListScope::Global);
        for text in ["plain one", "(B) b first", "(A) a open"] {
            list.add(text).unwrap();
        }
        App::new(list)
    }

    #[test]
    fn priority_sort_should_render_sorted_rows_and_a_status_segment_dropped_first_when_narrow() {
        let mut app = prioritized();
        app.apply(Action::ToggleSort).unwrap();
        let buffer = render_app(&app, 80, 12);
        assert!(
            buffer_row(&buffer, 80, 1).contains("[All]│ Open │ Done  · 3 matches · by priority"),
            "{}",
            buffer_row(&buffer, 80, 1)
        );
        assert!(buffer_row(&buffer, 80, 2).contains("○ (A) a open"));
        assert!(buffer_row(&buffer, 80, 3).contains("○ (B) b first"));
        assert!(buffer_row(&buffer, 80, 4).contains("› ○ plain one"));

        let narrow = render_app(&app, 40, 8);
        let status = buffer_row(&narrow, 40, 1);
        assert!(status.contains(" · 3 matches"), "{status}");
        assert!(!status.contains("by priority"));
        assert!(!status.contains('/'));

        app.apply(Action::ToggleSort).unwrap();
        let buffer = render_app(&app, 80, 12);
        let status = buffer_row(&buffer, 80, 1);
        assert!(status.contains("Done  · 3 matches · by due"), "{status}");
        assert!(!status.contains("by priority"));
        assert!(buffer_row(&buffer, 80, 2).contains("○ (A) a open"));
        // ` · by due` is shorter than ` · by priority` and still fits at 40 columns...
        let status = buffer_row(&render_app(&app, 40, 8), 40, 1);
        assert_eq!(status, "[All]│ Open │ Done  · 3 matches · by due");
        // ...until a filter needs the room, when it is dropped before the filter is cut.
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "A-C");
        app.apply(Action::CommitEdit).unwrap();
        let status = buffer_row(&render_app(&app, 40, 8), 40, 1);
        assert_eq!(status, "[All]│ Open │ Done  · 2 matches · f:A-C ");
        assert!(
            buffer_row(&render_app(&app, 80, 12), 80, 1).contains(" · 2 matches · f:A-C · by due")
        );
        app.apply(Action::ClearSearch).unwrap();

        app.apply(Action::ToggleSort).unwrap();
        assert!(buffer_row(&render_app(&app, 80, 12), 80, 1).contains(" · 3 matches · /"));
    }

    #[test]
    fn sorted_reorder_notice_and_help_should_name_the_configured_sort_key() {
        for (keymap, key) in [
            (Keymap::defaults(), "s"),
            (
                Keymap::with_overrides(&[override_for(BindingId::ToggleSort, &["o"])]).unwrap(),
                "o",
            ),
        ] {
            for presses in [1, 2] {
                let mut app = prioritized();
                for _ in 0..presses {
                    app.apply(Action::ToggleSort).unwrap();
                }
                app.apply(Action::MoveTaskUp).unwrap();
                let footer = buffer_row(&render_app_with_keymap(&app, &keymap, 100, 12), 100, 11);
                assert!(
                    footer.contains(&format!("NORMAL  Reorder needs manual order; {key} cycles")),
                    "{footer}"
                );
                assert!(!footer.contains("uses manual order"), "{footer}");
                // The notice is 36 columns; after the mode label, 40 columns keep the key.
                let narrow = buffer_row(&render_app_with_keymap(&app, &keymap, 40, 8), 40, 7);
                assert_eq!(
                    narrow,
                    format!("NORMAL  Reorder needs manual order; {key} cy")
                );
                app.apply(Action::OpenHelp).unwrap();
                let help = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 30));
                assert!(help.contains(&format!("{key} cycle sort order")), "{help}");
                assert!(!help.contains("toggle priority sort"), "{help}");
            }
        }
        let keymap =
            Keymap::with_overrides(&[override_for(BindingId::StartSearch, &["s"])]).unwrap();
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 30));
        assert!(help.contains("Unbound: toggle_sort (config)"));

        // With `toggle_sort` unbound, the notice names no key.
        let mut app = prioritized();
        app.apply(Action::ToggleSort).unwrap();
        app.apply(Action::MoveTaskUp).unwrap();
        let footer = buffer_row(&render_app_with_keymap(&app, &keymap, 100, 12), 100, 11);
        assert!(
            footer.starts_with("NORMAL  Reorder needs manual order · "),
            "{footer}"
        );
    }

    fn type_text(app: &mut App, text: &str) {
        for character in text.chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
    }

    #[test]
    fn filter_editor_should_render_hint_above_footer_and_keep_list_rows_at_minimum_size() {
        let mut list = TaskList::new(ListScope::Global);
        for letter in ['a', 'b', 'c', 'd', 'e', 'f'] {
            list.add(&format!("task {letter} +東京 +東京{letter}"))
                .unwrap();
        }
        let mut app = App::new(list);
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "+東京 +東京 +東");
        let keymap = Keymap::defaults();
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        terminal.draw(|frame| render(frame, &app, &keymap)).unwrap();
        let cursor = terminal.get_cursor_position().unwrap();
        assert_eq!(cursor.y, 1);
        assert!(cursor.x < 40);
        let buffer = terminal.backend().buffer().clone();
        let row = |index| display_row(&buffer, index);
        assert!(row(1).contains(" · 6 matches · f:"), "{}", row(1));
        assert!(row(1).trim_end().ends_with("+東"), "{}", row(1));
        for index in 2..=5 {
            assert!(row(index).contains("task "), "row {index}: {}", row(index));
        }
        assert!(row(6).starts_with("+東京 6 · +東京a 1 · "), "{}", row(6));
        assert!(row(6).trim_end().ends_with(" · …"), "{}", row(6));
        assert!(!row(6).contains("+東京f"));
        assert!(
            row(7).starts_with("FILTER  Enter apply filter"),
            "{}",
            row(7)
        );

        app.apply(Action::CommitEdit).unwrap();
        assert!(app.notice().is_some());
        let buffer = render_app(&app, 40, 8);
        let footer = display_row(&buffer, 7);
        assert!(
            footer.starts_with("FILTER  Finish the tag: +東京, +東京a, "),
            "{footer}"
        );
        assert!(footer.trim_end().ends_with('…'), "{footer}");
        assert!(display_row(&buffer, 6).starts_with("+東京 6 · "));

        app.apply(Action::CancelEdit).unwrap();
        let buffer = render_app(&app, 40, 8);
        for index in 2..=6 {
            assert!(buffer_row(&buffer, 40, index).contains("task "));
        }
        assert!(buffer_row(&buffer, 40, 7).starts_with("NORMAL"));
    }

    #[test]
    fn filter_hint_should_cut_a_single_wide_entry_and_explain_an_empty_view() {
        let mut list = TaskList::new(ListScope::Global);
        list.add(&format!("one +{}", "x".repeat(60))).unwrap();
        list.add("untagged").unwrap();
        let mut app = App::new(list);
        app.apply(Action::EditFilter).unwrap();
        let hint = buffer_row(&render_app(&app, 40, 8), 40, 6);
        assert_eq!(hint, format!("+{}…", "x".repeat(38)));
        type_text(&mut app, "+zzz");
        let hint = buffer_row(&render_app(&app, 40, 8), 40, 6);
        assert!(hint.starts_with("No tags in this view"), "{hint}");
    }

    #[test]
    fn accepted_filter_should_show_status_segment_and_filter_aware_recovery_hints() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("(A) ship +shtodo").unwrap();
        list.add("plain").unwrap();
        let mut app = App::new(list);
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "+shtodo");
        app.apply(Action::CommitEdit).unwrap();
        let status = buffer_row(&render_app(&app, 80, 12), 80, 1);
        assert!(status.contains("Done  · 1 matches · f:+shtodo"), "{status}");
        app.apply(Action::ToggleSort).unwrap();
        let status = buffer_row(&render_app(&app, 80, 12), 80, 1);
        assert!(
            status.contains(" · 1 matches · f:+shtodo · by priority"),
            "{status}"
        );
        let narrow = buffer_row(&render_app(&app, 40, 8), 40, 1);
        assert!(narrow.contains(" · 1 matches · f:+sh…"), "{narrow}");

        app.apply(Action::CycleView).unwrap();
        app.apply(Action::CycleView).unwrap();
        let text = buffer_text(&render_app(&app, 120, 12));
        assert!(text.contains("No matching tasks"));
        assert!(
            text.contains("Esc clear filter · Tab next view · Shift-Tab previous view · / edit search · f edit filter"),
            "{text}"
        );
        app.apply(Action::StartSearch).unwrap();
        type_text(&mut app, "zzz");
        app.apply(Action::CommitEdit).unwrap();
        let text = buffer_text(&render_app(&app, 120, 12));
        assert!(text.contains("Esc clear search · Tab next view"));
        assert!(buffer_row(&render_app(&app, 120, 12), 120, 1).contains("/zzz · f:+shtodo"));

        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::EditFilter, &["ctrl-f"]),
            override_for(BindingId::ClearSearch, &["ctrl-g"]),
        ])
        .unwrap();
        app.apply(Action::ClearSearch).unwrap();
        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 120, 12));
        assert!(text.contains("Ctrl-g clear filter"), "{text}");
        assert!(text.contains("Ctrl-f edit filter"));
    }

    #[test]
    fn a_due_filter_should_show_its_words_in_the_status_and_tags_only_in_the_hint() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("now due:2026-10-21 +x").unwrap();
        list.add("soon due:2026-10-22 @y").unwrap();
        let mut app = App::new(list);
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "due:today");
        let buffer = render_app(&app, 40, 8);
        // Counts follow completed tokens, as for a trailing priority token.
        assert_eq!(display_row(&buffer, 6).trim_end(), "+x 1 · @y 1");
        let status = buffer_row(&render_app(&app, 80, 12), 80, 1);
        assert!(status.contains(" · 1 matches · f:due:today"), "{status}");
        type_text(&mut app, " ");
        let buffer = render_app(&app, 40, 8);
        assert_eq!(display_row(&buffer, 6).trim_end(), "+x 1");
        app.apply(Action::CommitEdit).unwrap();
        let status = buffer_row(&render_app(&app, 80, 12), 80, 1);
        assert!(
            status.contains("Done  · 1 matches · f:due:today"),
            "{status}"
        );
        assert!(app.set_today(crate::metadata::Date::new(2026, 10, 22).unwrap()));
        let buffer = render_app(&app, 80, 12);
        assert!(buffer_row(&buffer, 80, 1).contains(" · 1 matches · f:due:today"));
        assert!(buffer_row(&buffer, 80, 2).contains("soon due:2026-10-22 @y"));
    }

    #[test]
    fn filter_footer_and_help_should_use_the_configured_filter_keys() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app(&app, 100, 30));
        assert!(help.contains("f edit filter"), "{help}");
        app.apply(Action::CloseHelp).unwrap();
        let normal = buffer_row(&render_app(&app, 200, 12), 200, 11);
        assert!(
            normal.contains("s cycle sort order · f edit filter"),
            "{normal}"
        );

        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::CommitEdit, &["ctrl-s"]),
            override_for(BindingId::CancelEdit, &["ctrl-g"]),
        ])
        .unwrap();
        app.apply(Action::EditFilter).unwrap();
        let footer = buffer_row(&render_app_with_keymap(&app, &keymap, 100, 12), 100, 11);
        assert!(
            footer.starts_with("FILTER  Ctrl-s apply filter · Ctrl-g cancel filter"),
            "{footer}"
        );
        app.apply(Action::CancelEdit).unwrap();

        let keymap =
            Keymap::with_overrides(&[override_for(BindingId::ToggleComplete, &["f"])]).unwrap();
        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 30));
        assert!(help.contains("Unbound: edit_filter (config)"));
        assert!(help.contains("f toggle complete"));
    }

    #[test]
    fn trash_should_show_all_tombstones_and_return_to_the_filtered_tab_and_query() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("live unrelated").unwrap();
        let matching = list.add("needle deleted").unwrap();
        let other = list.add("other deleted").unwrap();
        list.toggle_complete(other).unwrap();
        list.delete(matching).unwrap();
        list.delete(other).unwrap();
        let mut app = App::new(list);
        search_for(&mut app, "needle");
        app.apply(Action::PreviousView).unwrap();
        app.apply(Action::OpenTrash).unwrap();
        let buffer = render_app(&app, 80, 12);
        let text = buffer_text(&buffer);
        assert!(text.contains("2 deleted"));
        assert!(text.contains("needle deleted"));
        assert!(text.contains("other deleted"));
        assert!(!text.contains("live unrelated"));
        assert!(!text.contains("/needle"));
        assert!(!text.contains("[Done]"));
        assert!(buffer_row(&buffer, 80, 11).contains("TRASH"));
        app.apply(Action::CloseTrash).unwrap();
        let buffer = render_app(&app, 80, 12);
        assert!(buffer_row(&buffer, 80, 1).contains("[Done] · 0 matches · /needle"));
        assert!(buffer_text(&buffer).contains("No matching tasks"));
    }

    /// 1 [2 done, 3 overdue], 4: the plan's example group, with dates around `test_today`.
    fn subtask_app() -> App {
        let mut list = TaskList::new(ListScope::Global);
        let ship = list.add("Ship the release +shtodo").unwrap();
        let changelog = list.add_subtask(ship, "Write the changelog").unwrap();
        list.add_subtask(ship, "Tag the build due:2026-10-20")
            .unwrap();
        list.add("Pay rent due:2026-11-01").unwrap();
        list.set_completed(changelog, true).unwrap();
        App::new(list)
    }

    #[test]
    fn subtask_rows_should_indent_show_dim_progress_and_keep_the_overdue_marker_at_minimum_size() {
        let app = subtask_app();
        let buffer = render_app(&app, 40, 8);
        assert_eq!(
            display_row(&buffer, 2).trim_end(),
            "› ○ 1/2 Ship the release +shtodo"
        );
        assert_eq!(
            display_row(&buffer, 3).trim_end(),
            "    ✓ Write the changelog"
        );
        assert_eq!(
            display_row(&buffer, 4).trim_end(),
            "    ○ Tag the build due:2026-10-20"
        );
        assert_eq!(
            display_row(&buffer, 5).trim_end(),
            "  ○ Pay rent due:2026-11-01"
        );
        for cell in row_cells(&buffer, 2, "1/2") {
            assert!(cell.modifier.contains(Modifier::DIM), "{cell:?}");
        }
        assert!(
            !row_cells(&buffer, 2, "Ship")[0]
                .modifier
                .contains(Modifier::DIM)
        );
        // The indented subtask's marker still carries the overdue color.
        assert_eq!(buffer[(4, 4)].symbol(), "○");
        assert_eq!(buffer[(4, 4)].fg, Color::Red);
    }

    #[test]
    fn a_context_row_should_be_dimmed_keep_strikethrough_and_stay_out_of_the_match_count() {
        let mut app = subtask_app();
        search_for(&mut app, "build");
        let buffer = render_app(&app, 40, 8);
        assert!(
            buffer_row(&buffer, 40, 1).contains("1 matches"),
            "{}",
            buffer_row(&buffer, 40, 1)
        );
        assert_eq!(
            display_row(&buffer, 2),
            "› ○ 1/2 Ship the release +shtodo        "
        );
        assert_eq!(
            display_row(&buffer, 3),
            "    ○ Tag the build due:2026-10-20      "
        );
        for cell in row_cells(&buffer, 2, "○ 1/2 Ship the release +shtodo") {
            assert!(cell.modifier.contains(Modifier::DIM), "{cell:?}");
        }
        assert!(
            !row_cells(&buffer, 3, "Tag")[0]
                .modifier
                .contains(Modifier::DIM)
        );
        // A done context parent keeps its strikethrough.
        let mut list = app.tasks().clone();
        list.set_completed(crate::task::TaskId::from_shell_integer(1).unwrap(), true)
            .unwrap();
        app.reconcile(list);
        let buffer = render_app(&app, 40, 8);
        for cell in row_cells(&buffer, 2, "Ship the release") {
            assert!(
                cell.modifier
                    .contains(Modifier::DIM | Modifier::CROSSED_OUT),
                "{cell:?}"
            );
        }
    }

    #[test]
    fn editing_a_subtask_should_place_the_draft_and_cursor_two_columns_further_right() {
        let mut app = subtask_app();
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::StartEdit).unwrap();
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &app, &Keymap::defaults()))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(
            display_row(&buffer, 4).starts_with("›   ○ Tag the build"),
            "{}",
            display_row(&buffer, 4)
        );
        // ROW_PREFIX_WIDTH + 2, then the 28 characters of the draft (clipped to the editor width).
        let draft = "Tag the build due:2026-10-20".chars().count() as u16;
        terminal
            .backend_mut()
            .assert_cursor_position((super::ROW_PREFIX_WIDTH + 2 + draft, 4));
    }

    #[test]
    fn add_subtask_editor_should_follow_the_group_indented_with_the_cursor_two_columns_right() {
        let footer = buffer_row(&render_app(&subtask_app(), 40, 8), 40, 7);
        // From the parent and from one of its subtasks, the draft row follows the whole group.
        for moves in [0, 2] {
            let mut app = subtask_app();
            for _ in 0..moves {
                app.apply(Action::MoveDown).unwrap();
            }
            app.apply(Action::AddSubtask).unwrap();
            for character in "Draft".chars() {
                app.apply(Action::InsertChar(character)).unwrap();
            }
            let backend = TestBackend::new(40, 8);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| render(frame, &app, &Keymap::defaults()))
                .unwrap();
            let buffer = terminal.backend().buffer().clone();
            assert_eq!(
                display_row(&buffer, 4).trim_end(),
                "    ○ Tag the build due:2026-10-20"
            );
            assert_eq!(display_row(&buffer, 5).trim_end(), "›   ○ Draft");
            assert_eq!(
                display_row(&buffer, 6).trim_end(),
                "  ○ Pay rent due:2026-11-01"
            );
            terminal
                .backend_mut()
                .assert_cursor_position((super::ROW_PREFIX_WIDTH + 2 + 5, 5));
            app.apply(Action::CancelEdit).unwrap();
            assert_eq!(buffer_row(&render_app(&app, 40, 8), 40, 7), footer);
        }
    }

    #[test]
    fn help_should_list_add_subtask_with_its_configured_key_or_as_unbound() {
        let mut app = subtask_app();
        app.apply(Action::OpenHelp).unwrap();
        let help = buffer_text(&render_app(&app, 100, 30));
        assert!(help.contains("A add subtask"), "{help}");
        let keymap =
            Keymap::with_overrides(&[override_for(BindingId::AddSubtask, &["ctrl-a"])]).unwrap();
        let help = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 30));
        assert!(help.contains("Ctrl-a add subtask"), "{help}");
        let keymap =
            Keymap::with_overrides(&[override_for(BindingId::ToggleComplete, &["A"])]).unwrap();
        let help = buffer_text(&render_app_with_keymap(&app, &keymap, 100, 30));
        assert!(help.contains("Unbound: add_subtask (config)"), "{help}");
        assert!(help.contains("A toggle complete"), "{help}");
    }

    #[test]
    fn help_should_share_one_nest_and_promote_row_and_list_unbound_halves_separately() {
        let mut app = subtask_app();
        app.apply(Action::OpenHelp).unwrap();
        let help = |keymap: &Keymap| buffer_text(&render_app_with_keymap(&app, keymap, 100, 30));
        let text = help(&Keymap::defaults());
        assert!(text.contains("> nest, < promote"), "{text}");
        assert!(!text.contains("Unbound"), "{text}");

        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::IndentTask, &["ctrl-l", "]"]),
            override_for(BindingId::OutdentTask, &["ctrl-h"]),
        ])
        .unwrap();
        assert!(help(&keymap).contains("Ctrl-l / ] nest, Ctrl-h promote"));

        let keymap = Keymap::with_overrides(&[override_for(BindingId::MoveUp, &["<"])]).unwrap();
        let text = help(&keymap);
        assert!(text.contains("> nest"), "{text}");
        assert!(!text.contains("promote"), "{text}");
        assert!(text.contains("Unbound: outdent_task (config)"), "{text}");

        let keymap = Keymap::with_overrides(&[override_for(BindingId::MoveDown, &[">"])]).unwrap();
        let text = help(&keymap);
        assert!(text.contains("< promote"), "{text}");
        assert!(!text.contains("nest"), "{text}");
        assert!(text.contains("Unbound: indent_task (config)"), "{text}");

        let keymap = Keymap::with_overrides(&[
            override_for(BindingId::MoveDown, &[">"]),
            override_for(BindingId::MoveUp, &["<"]),
        ])
        .unwrap();
        let text = help(&keymap);
        assert!(
            !text.contains("nest") && !text.contains("promote"),
            "{text}"
        );
        assert!(text.contains("Unbound: indent_task (config)"), "{text}");
        assert!(text.contains("Unbound: outdent_task (config)"), "{text}");

        // The long-alias keymap still shows every subtask row at 80x24.
        let keymap = Keymap::with_overrides(&[override_for(
            BindingId::OpenHelp,
            &[
                "α", "β", "γ", "δ", "ε", "ζ", "η", "θ", "λ", "μ", "ν", "ξ", "ο", "π", "ρ", "σ",
            ],
        )])
        .unwrap();
        let text = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 24));
        assert!(text.contains("A add subtask"), "{text}");
        assert!(text.contains("> nest, < promote"), "{text}");
        assert!(text.contains("σ show help"), "{text}");
    }

    #[test]
    fn nest_and_promote_notices_should_name_configured_keys() {
        let mut list = TaskList::new(ListScope::Global);
        let ship = list.add("Ship").unwrap();
        list.add_subtask(ship, "Tag").unwrap();
        let mut app = App::new(list);
        let footer = |app: &App, keymap: &Keymap| {
            buffer_row(&render_app_with_keymap(app, keymap, 60, 8), 60, 7)
        };
        let defaults = Keymap::defaults();
        app.apply(Action::IndentTask).unwrap();
        assert!(
            footer(&app, &defaults).starts_with("NORMAL  No task above to nest under"),
            "{}",
            footer(&app, &defaults)
        );
        app.apply(Action::OutdentTask).unwrap();
        assert!(footer(&app, &defaults).starts_with("NORMAL  Already top-level"));
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::IndentTask).unwrap();
        assert!(footer(&app, &defaults).starts_with("NORMAL  Already a subtask; < promotes"));
        let keymap =
            Keymap::with_overrides(&[override_for(BindingId::OutdentTask, &["ctrl-h"])]).unwrap();
        assert!(footer(&app, &keymap).starts_with("NORMAL  Already a subtask; Ctrl-h promotes"));
        let keymap = Keymap::with_overrides(&[override_for(BindingId::MoveUp, &["<"])]).unwrap();
        let text = footer(&app, &keymap);
        assert!(text.starts_with("NORMAL  Already a subtask"), "{text}");
        assert!(!text.contains("promotes"), "{text}");

        let mut list = TaskList::new(ListScope::Global);
        list.add("Pay rent").unwrap();
        let ship = list.add("Ship").unwrap();
        list.add_subtask(ship, "Tag").unwrap();
        let mut app = App::new(list);
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::IndentTask).unwrap();
        assert!(footer(&app, &defaults).starts_with("NORMAL  Has subtasks; promote them first"));
    }

    #[test]
    fn trash_should_indent_co_deleted_subtasks_and_name_the_parent_to_restore_first() {
        let mut list = TaskList::new(ListScope::Global);
        let ship = list.add("Ship").unwrap();
        let alone = list.add_subtask(ship, "deleted alone").unwrap();
        list.add_subtask(ship, "deleted with it").unwrap();
        list.delete(alone).unwrap();
        list.delete(ship).unwrap();
        let mut app = App::new(list);
        app.apply(Action::OpenTrash).unwrap();
        let buffer = render_app(&app, 40, 8);
        assert!(
            display_row(&buffer, 1).starts_with("› 1  open  Ship"),
            "{}",
            display_row(&buffer, 1)
        );
        assert!(display_row(&buffer, 2).starts_with("    3  open  deleted with it"));
        assert!(display_row(&buffer, 3).starts_with("  2  open  deleted alone"));
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::RestoreSelected).unwrap();
        let footer = buffer_row(&render_app(&app, 40, 8), 40, 7);
        assert!(
            footer.starts_with("TRASH  Restore parent 1 first"),
            "{footer}"
        );
    }

    #[test]
    fn help_should_cover_the_header_without_a_bottom_border_and_keep_the_footer() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenHelp).unwrap();
        let buffer = render_app(&app, 80, 24);
        let top = buffer_row(&buffer, 80, 0);
        assert!(top.starts_with("┌Keyboard help · shtodo global"), "{top}");
        assert!(!top.contains("open ·"), "{top}");
        for row in 1..=22 {
            let line = buffer_row(&buffer, 80, row);
            assert!(
                line.starts_with('│') && line.ends_with('│'),
                "{row}: {line}"
            );
        }
        assert!(buffer_row(&buffer, 80, 23).starts_with("HELP  "));
        assert!(!buffer_text(&buffer).contains('└'));

        app.apply(Action::CloseHelp).unwrap();
        app.apply(Action::OpenTrash).unwrap();
        app.apply(Action::OpenHelp).unwrap();
        let top = buffer_row(&render_app(&app, 80, 24), 80, 0);
        assert!(
            top.starts_with("┌Keyboard help: Trash · shtodo TRASH · global · newest first"),
            "{top}"
        );
    }

    #[test]
    fn help_at_minimum_size_should_keep_the_mode_name_in_the_title_and_the_footer_visible() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenHelp).unwrap();
        let buffer = render_app(&app, 40, 8);
        assert!(buffer_row(&buffer, 40, 0).starts_with("┌Keyboard help · shtodo global"));
        for row in 1..=6 {
            assert!(buffer_row(&buffer, 40, row).starts_with('│'), "{row}");
        }
        assert!(buffer_row(&buffer, 40, 7).starts_with("HELP  "));
        app.apply(Action::CloseHelp).unwrap();
        app.apply(Action::OpenTrash).unwrap();
        app.apply(Action::OpenHelp).unwrap();
        let top = buffer_row(&render_app(&app, 40, 8), 40, 0);
        assert!(
            top.starts_with("┌Keyboard help: Trash · shtodo TRASH · "),
            "{top}"
        );
    }
}

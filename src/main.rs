use std::{io, sync::mpsc, thread, time::Duration, usize};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols::DOT,
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Terminal,
};

#[derive(Debug)]
enum InputEvent<I> {
    Input(I),
    Tick,
}

struct SourceTabState {
    sources: Vec<String>,
    sources_list_state: ListState,
}

impl SourceTabState {
    fn new() -> Self {
        Self {
            sources: vec![],
            sources_list_state: ListState::default(),
        }
    }
}
impl SourceTabState {
    fn set_sources(&mut self, sources: Vec<String>) {
        self.sources = sources;
        self.sources_list_state.select(Some(0));
    }
    fn cloned_sources(&self) -> Vec<String> {
        self.sources.clone()
    }
}

struct AppState {
    tab_titles: Vec<String>,
    selected_tab_idx: Option<usize>,
}
impl AppState {
    fn new() -> AppState {
        AppState {
            tab_titles: vec![],
            selected_tab_idx: None,
        }
    }
}
impl AppState {
    fn set_tab_titles(&mut self, tab_titles: Vec<String>) {
        assert!(!tab_titles.is_empty());
        self.tab_titles = tab_titles;
        self.selected_tab_idx = Some(0);
    }
    fn cloned_tab_titles(&self) -> Vec<String> {
        self.tab_titles.clone()
    }
    fn set_selected_tab_idx(&mut self, selected_tab_idx: usize) {
        if selected_tab_idx >= self.tab_titles.len() {
            self.selected_tab_idx = None;
        } else {
            self.selected_tab_idx = Some(selected_tab_idx);
        }
    }
    fn get_selected_tab_idx(&self) -> Option<usize> {
        self.selected_tab_idx
    }
    fn go_previous_tab(&mut self) {
        if let Some(selected_tab_idx) = self.selected_tab_idx {
            if selected_tab_idx > 0 {
                self.selected_tab_idx = Some(selected_tab_idx - 1);
            } else {
                self.selected_tab_idx = (self.tab_titles.len() - 1).into();
            }
        }
    }
    fn go_next_tab(&mut self) {
        if let Some(selected_tab_idx) = self.selected_tab_idx {
            if selected_tab_idx < self.tab_titles.len() - 1 {
                self.selected_tab_idx = Some(selected_tab_idx + 1);
            } else {
                self.selected_tab_idx = Some(0);
            }
        }
    }
}

fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel();

    //Use input event thread to listen key event and send to ui thread
    thread::spawn(move || loop {
        if (event::poll(Duration::from_millis(100)).unwrap()) {
            let ev = event::read().unwrap();
            match ev {
                Event::Key(key) => {
                    tx.send(InputEvent::Input(key)).unwrap();
                }
                _ => {}
            }
        } else {
            tx.send(InputEvent::Tick).unwrap();
        }
    });

    let mut app_state = AppState::new();
    app_state.set_tab_titles(vec![
        "Home".to_string(),
        "Sources".to_string(),
        "Settings".to_string(),
    ]);
    let mut source_tab_state = SourceTabState::new();
    source_tab_state.set_sources(vec![
        "~/music".to_string(),
        "~/google_drive/".to_string(),
        "~/dropbox".to_string(),
    ]);
    loop {
        let input_event = rx.recv().unwrap();
        match input_event {
            InputEvent::Input(key) => match key {
                KeyEvent {
                    code: KeyCode::Char('q'),
                    modifiers: KeyModifiers::CONTROL,
                } => break,
                KeyEvent {
                    code: KeyCode::Char('h'),
                    modifiers: KeyModifiers::NONE,
                } => {
                    app_state.go_previous_tab();
                }
                KeyEvent {
                    code: KeyCode::Char('l'),
                    modifiers: KeyModifiers::NONE,
                } => {
                    app_state.go_next_tab();
                }
                _ => {}
            },
            InputEvent::Tick => {}
        }

        //todo: enable move between boards
        terminal.draw(|f| {
            let boards = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(10), Constraint::Percentage(90)].as_ref())
                .split(f.size());
            let tabs_board = boards[0];
            let main_board = boards[1];

            // draw tabs block and tabs content
            let tab_titles = app_state.cloned_tab_titles();
            let selected_tab_idx = app_state.get_selected_tab_idx().unwrap_or(0);

            let tabs_block = Block::default().borders(Borders::ALL).title("Menu");
            f.render_widget(tabs_block, tabs_board);
            let titles: Vec<Spans> = app_state
                .cloned_tab_titles()
                .iter()
                .cloned()
                .map(Spans::from)
                .collect();
            let tabs_content = Tabs::new(titles)
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(Color::Yellow))
                .select(selected_tab_idx)
                .divider(DOT);
            let tabs_content_board = Layout::default()
                .constraints([Constraint::Percentage(100)])
                .horizontal_margin(5)
                .split(tabs_board)[0];
            let tabs_content_board = Rect::new(
                tabs_content_board.left(),
                tabs_content_board.bottom() / 2,
                tabs_content_board.width,
                tabs_content_board.height - (tabs_content_board.bottom() / 2),
            );
            f.render_widget(tabs_content, tabs_content_board);

            // draw main block and main content corresponde to selected tab
            match selected_tab_idx {
                //Home
                0 => {
                    let main_block = Block::default().borders(Borders::ALL).title("Home");
                    f.render_widget(main_block, main_board);
                }
                //Source
                1 => {
                    let main_boards = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [Constraint::Percentage(98), Constraint::Percentage(2)].as_ref(),
                        )
                        .split(main_board);
                    let main_board = main_boards[0];
                    let helper_board = main_boards[1];

                    //main board
                    let main_block = Block::default().borders(Borders::ALL).title("Sources");
                    let sources = source_tab_state.cloned_sources();
                    let list_items: Vec<ListItem> = sources
                        .iter()
                        .map(|s| ListItem::new(s.clone()).style(Style::default().fg(Color::White)))
                        .collect();
                    let main_content = List::new(list_items).block(main_block).highlight_style(Style::default().fg(Color::Yellow));
                    f.render_stateful_widget(
                        main_content,
                        main_board,
                        &mut source_tab_state.sources_list_state,
                    );

                    //helper board
                    let text = Spans::from(vec![
                        Span::styled("(a) Add new source", Style::default().fg(Color::Magenta)),
                        Span::raw(" ".repeat(10)),
                        Span::styled("(d) Delete source", Style::default().fg(Color::Red)),
                    ]);
                    let helper_content = Paragraph::new(text)
                        .alignment(Alignment::Center)
                        .wrap(Wrap { trim: true });
                    f.render_widget(helper_content, helper_board);
                }
                //Settings
                2 => {
                    let main_block = Block::default().borders(Borders::ALL).title("Settings");
                    f.render_widget(main_block, main_board);
                }
                _ => {}
            }
        })?;
    }
    Ok(())
}

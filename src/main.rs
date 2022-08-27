//todo: add a global player
use serde::{Deserialize, Serialize};
use std::{
    cell::Ref,
    env,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    thread,
    time::Duration,
    usize,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen},
};
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols::DOT,
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Terminal,
};

#[derive(Debug)]
enum InputEvent<I> {
    Input(I),
    Tick,
}

// source json
#[derive(Serialize, Deserialize)]
struct Source {
    sources: Vec<String>,
}
impl Source {
    fn new_empty() -> Self {
        Self { sources: vec![] }
    }
}

// settings tab state
struct SettingsState {}

// source tab state
struct SourceTabState {
    source_db: Source,
    sources_list_state: ListState,
    configuration: Rc<Configuration>,
}
impl SourceTabState {
    fn new(configuration: Rc<Configuration>) -> Self {
        Self {
            source_db: Source::new_empty(),
            sources_list_state: ListState::default(),
            configuration,
        }
    }
}
impl SourceTabState {
    fn load_sources(&mut self) {
        let raw_content = fs::read_to_string(&self.configuration.source_file_path).unwrap();
        let source: Source = serde_json::from_str(&raw_content).unwrap();
        self.source_db = source;
        self.sources_list_state.select(Some(0));
    }
    fn cloned_sources(&self) -> Vec<String> {
        self.source_db.sources.clone()
    }
    fn select_next(&mut self) {
        if self.source_db.sources.is_empty() {
            return;
        }
        let current_idx = self.sources_list_state.selected().unwrap();
        if current_idx + 1 < self.source_db.sources.len() {
            self.sources_list_state.select(Some(current_idx + 1));
        } else {
            self.sources_list_state.select(Some(0));
        }
    }
    fn select_previous(&mut self) {
        if self.source_db.sources.is_empty() {
            return;
        }
        let current_idx = self.sources_list_state.selected().unwrap();
        if current_idx > 0 {
            self.sources_list_state.select(Some(current_idx - 1));
        } else {
            self.sources_list_state
                .select(Some(self.source_db.sources.len() - 1));
        }
    }
    fn add_source(&mut self, source: String) {
        if source.is_empty() {
            return;
        }
        self.source_db.sources.push(source);
        fs::write(
            &self.configuration.source_file_path,
            serde_json::to_string_pretty(&self.source_db).unwrap(),
        )
        .unwrap();
    }
    fn delete_current_selected_source(&mut self) {
        let current_idx = self.sources_list_state.selected().unwrap();
        if current_idx >= self.source_db.sources.len() {
            return;
        }
        self.source_db.sources.remove(current_idx);
        fs::write(
            &self.configuration.source_file_path,
            serde_json::to_string_pretty(&self.source_db).unwrap(),
        )
        .unwrap();
        if !self.source_db.sources.is_empty() {
            self.sources_list_state
                .select(Some(self.source_db.sources.len() - 1));
        } else {
            self.sources_list_state.select(None);
        }
    }
}

enum InputMode {
    Normal,
    Edit,
}
// app global state
struct GlobalState {
    tab_titles: Vec<String>,
    selected_tab_idx: Option<usize>,
    input_mode: InputMode,
    input_stream: Vec<String>,
    configuration: Rc<Configuration>,
}
impl GlobalState {
    fn new(configuration: Rc<Configuration>) -> GlobalState {
        GlobalState {
            tab_titles: vec![],
            selected_tab_idx: None,
            input_mode: InputMode::Normal,
            input_stream: vec![],
            configuration,
        }
    }
}
impl GlobalState {
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
    fn switch_mode_to_normal(&mut self) {
        self.input_mode = InputMode::Normal;
    }
    fn switch_mode_to_edit(&mut self) {
        self.input_mode = InputMode::Edit;
    }
}

//todo: add and delete source
struct Configuration {
    folder_path: PathBuf,
    settting_file_path: PathBuf,
    source_file_path: PathBuf,
}
impl Configuration {
    fn new() -> Self {
        let mut configure = Self {
            folder_path: PathBuf::new(),
            settting_file_path: PathBuf::new(),
            source_file_path: PathBuf::new(),
        };
        configure.folder_path = PathBuf::from(env::var("HOME").unwrap()).join(".songbreeze");
        configure.settting_file_path = configure.folder_path.join("setting.json");
        configure.source_file_path = configure.folder_path.join("source.json");

        // helper function for creating folder or file while asking user
        let create_ff_while_asking = |path: &Path, check_for_file: bool| {
            if !path.exists() {
                let mut input = String::new();
                println!(
                    "Can not find {}, do you want to init it? (y/n)",
                    path.to_str().unwrap()
                );
                match io::stdin().read_line(&mut input) {
                    Ok(_) => {
                        if input.trim().to_lowercase() == "y" {
                            if check_for_file {
                                File::create(path).unwrap();
                            } else {
                                fs::create_dir(path).unwrap();
                            }
                        } else {
                            println!("please init it first");
                            std::process::exit(0);
                        }
                    }
                    Err(e) => println!("failed to read input: {}", e),
                };
            }
        };

        //check folder exists
        let folder_path = Path::new(&configure.folder_path);
        create_ff_while_asking(folder_path, false);

        //check setting file exists and load it
        let settings_file_path = Path::new(&configure.settting_file_path);
        create_ff_while_asking(settings_file_path, true);

        //check source file exists
        let source_file_path = Path::new(&configure.source_file_path);
        create_ff_while_asking(source_file_path, true);
        configure
    }
}

fn main() -> Result<(), io::Error> {
    let configuration = Rc::new(Configuration::new());
    let mut app_state = GlobalState::new(configuration.clone());
    app_state.set_tab_titles(vec![
        "Home".to_string(),
        "Sources".to_string(),
        "Settings".to_string(),
    ]);
    let mut source_tab_state = SourceTabState::new(configuration.clone());
    source_tab_state.load_sources();
    //todo: home tab state

    //main
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

    loop {
        let input_event = rx.recv().unwrap();
        match app_state.input_mode {
            InputMode::Normal => match input_event {
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
                    KeyEvent {
                        code: KeyCode::Char('j'),
                        modifiers: KeyModifiers::NONE,
                    } => match app_state.selected_tab_idx {
                        Some(idx) => match idx {
                            0 => {}
                            1 => {
                                source_tab_state.select_next();
                            }
                            _ => {}
                        },
                        None => {}
                    },
                    KeyEvent {
                        code: KeyCode::Char('k'),
                        modifiers: KeyModifiers::NONE,
                    } => match app_state.selected_tab_idx {
                        Some(idx) => match idx {
                            0 => {}
                            1 => {
                                source_tab_state.select_previous();
                            }
                            _ => {}
                        },
                        None => {}
                    },
                    KeyEvent {
                        code: KeyCode::Char('a'),
                        modifiers: KeyModifiers::NONE,
                    } => match app_state.selected_tab_idx {
                        Some(idx) => match idx {
                            0 => {}
                            1 => {
                                app_state.switch_mode_to_edit();
                                terminal.show_cursor()?;
                            }
                            _ => {}
                        },
                        None => {}
                    },
                    KeyEvent {
                        code: KeyCode::Char('d'),
                        modifiers: KeyModifiers::NONE,
                    } => match app_state.selected_tab_idx {
                        Some(idx) => match idx {
                            0 => {}
                            1 => {
                                source_tab_state.delete_current_selected_source();
                            }
                            _ => {}
                        },
                        None => {}
                    },
                    _ => {}
                },
                InputEvent::Tick => {}
            },
            InputMode::Edit => match input_event {
                InputEvent::Input(key) => match key {
                    KeyEvent {
                        code: KeyCode::Enter,
                        modifiers: KeyModifiers::NONE,
                    } => {
                        // todo: handle input stream before clear
                        match app_state.selected_tab_idx {
                            Some(idx) => match idx {
                                0 => {}
                                1 => {
                                    source_tab_state.add_source(
                                        app_state
                                            .input_stream
                                            .iter()
                                            .map(|s| s.to_string())
                                            .collect::<String>(),
                                    );
                                }
                                _ => {}
                            },
                            None => {}
                        }
                        app_state.input_stream.clear();
                        app_state.switch_mode_to_normal();
                        terminal.hide_cursor()?;
                    }
                    KeyEvent {
                        code: KeyCode::Esc,
                        modifiers: KeyModifiers::NONE,
                    } => {
                        app_state.input_stream.clear();
                        app_state.switch_mode_to_normal();
                        terminal.hide_cursor()?;
                    }
                    KeyEvent {
                        code: KeyCode::Char(c),
                        modifiers: KeyModifiers::NONE,
                    } => {
                        app_state.input_stream.push(c.to_string());
                    }
                    _ => {}
                },
                InputEvent::Tick => {}
            },
        }

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
            let titles: Vec<Spans> = tab_titles.iter().cloned().map(Spans::from).collect();
            let tabs_content = Tabs::new(titles)
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(Color::Yellow))
                .select(selected_tab_idx)
                .divider(DOT);
            let tabs_content_board = Layout::default()
                .constraints([Constraint::Percentage(10)])
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
                    let main_content = List::new(list_items)
                        .block(main_block)
                        .highlight_style(Style::default().fg(Color::Yellow));
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

                    //pop up board
                    if let InputMode::Edit = app_state.input_mode {
                        let pop_up_board = Rect::new(
                            main_board.width / 2 - main_board.width / 3,
                            main_board.height / 2,
                            main_board.width * 2 / 3,
                            main_board.height / 4,
                        );
                        let pop_up_block = Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Yellow));
                        f.render_widget(pop_up_block, pop_up_board);

                        let pop_up_board = Layout::default()
                            .constraints(
                                [Constraint::Percentage(10), Constraint::Percentage(95)].as_ref(),
                            )
                            .split(pop_up_board);
                        let pop_up_title_board = pop_up_board[0];
                        let pop_up_content_board = Layout::default()
                            .constraints([Constraint::Percentage(100)])
                            .margin(2)
                            .split(pop_up_board[1])[0];

                        let pop_up_title = Paragraph::new(Spans::from(vec![Span::styled(
                            "Absolute Path:",
                            Style::default().fg(Color::Yellow),
                        )]))
                        .alignment(Alignment::Center);
                        f.render_widget(pop_up_title, pop_up_title_board);

                        let pop_up_input = Paragraph::new(Spans::from(vec![Span::styled(
                            app_state
                                .input_stream
                                .iter()
                                .map(|s| s.to_string())
                                .collect::<String>(),
                            Style::default().fg(Color::White),
                        )]))
                        .wrap(Wrap { trim: true });
                        let input_stream_len = app_state.input_stream.len() as u16;
                        let scroll_offset_y = if input_stream_len / pop_up_content_board.width > 3 {
                            input_stream_len / pop_up_content_board.width - 3
                        } else {
                            0
                        };
                        let pop_up_input = pop_up_input.scroll((scroll_offset_y, 0));
                        f.set_cursor(
                            pop_up_content_board.left()
                                + input_stream_len % pop_up_content_board.width,
                            pop_up_content_board.top()
                                + input_stream_len / pop_up_content_board.width
                                - scroll_offset_y,
                        );
                        f.render_widget(pop_up_input, pop_up_content_board);
                    }
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
    disable_raw_mode()?;
    Ok(())
}

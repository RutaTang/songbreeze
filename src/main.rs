//todo: add home tabstate and scan audio files from sources folder
//2. middle: songs list
//3. right: song info
//5. scan source
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::Ref,
    collections::HashMap,
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
    symbols::{self, DOT},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, LineGauge, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Terminal,
};

#[derive(Debug)]
enum InputEvent<I> {
    Input(I),
    Tick,
}

enum HomeTabStateFocus {
    Left,
    Mid,
    Right,
}
//home tab state
struct HomeTabState {
    configuration: Rc<Configuration>,
    playlists: Vec<PlayList>,
    playlists_state: ListState,
    songs_list_states: Vec<ListState>, // each liststate coressponding to each playlist liststate
    focus: HomeTabStateFocus,
}
impl HomeTabState {
    fn new(configuration: Rc<Configuration>) -> Self {
        Self {
            configuration,
            playlists: Vec::new(),
            playlists_state: ListState::default(),
            songs_list_states: Vec::new(),
            focus: HomeTabStateFocus::Left,
        }
    }
}
impl HomeTabState {
    fn load_data(&mut self) {
        let data = fs::read_to_string(&self.configuration.playlist_file_path).unwrap();
        let raw_json_data: Value = serde_json::from_str(&data).unwrap_or(json!({
            "playlist_names":[],
            "song_paths":[],
            "playlist_songpaths_relations":{}
        }));

        //load playlist names
        let playlist_names = raw_json_data["playlist_names"].as_array();
        //load songs paths
        let song_paths = raw_json_data["song_paths"].as_array();
        //load playlist/song relations
        let playlist_songpaths_relations: HashMap<String, Vec<String>> =
            serde_json::from_value(raw_json_data["playlist_songpaths_relations"].clone())
                .unwrap_or(HashMap::new());

        //load all data to playlists
        let mut include_default_playlist = false; //check whether include default playlist
        const DEFAULT_PLAYLIST_NAME: &str = "Default";
        for (playlist, song_paths) in playlist_songpaths_relations {
            if playlist == DEFAULT_PLAYLIST_NAME {
                include_default_playlist = true;
            }
            let mut playlist = PlayList {
                name: playlist,
                songs: vec![],
            };
            for song_path in song_paths.iter() {
                let song = Song::new(PathBuf::from(song_path));
                if let Some(song) = song {
                    playlist.songs.push(song);
                }
            }
            self.playlists.push(playlist);
        }
        if !include_default_playlist {
            let playlist = PlayList {
                name: DEFAULT_PLAYLIST_NAME.to_string(),
                songs: vec![],
            };
            self.playlists.push(playlist);
        }
        //sort playlist in a order but guarantee default playlist is the first
        self.playlists.sort_by(|a, b| {
            if a.name == DEFAULT_PLAYLIST_NAME {
                std::cmp::Ordering::Less
            } else if b.name == DEFAULT_PLAYLIST_NAME {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });
        self.playlists_state.select(Some(0));

        //init songs_states
        self.songs_list_states = self
            .playlists
            .iter()
            .map(|_| ListState::default())
            .collect();
    }
    fn select_next_playlist(&mut self) {
        let i = match self.playlists_state.selected() {
            Some(i) => {
                if i >= self.playlists.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.playlists_state.select(Some(i));
    }
    fn select_previous_playlist(&mut self) {
        let i = match self.playlists_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.playlists.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.playlists_state.select(Some(i));
    }
    fn enter_current_playlist_songs_list(&mut self) {
        if let Some(idx) = self.playlists_state.selected() {
            if !self.playlists[idx].songs.is_empty() {
                let current_songs_list_state = self.songs_list_states.get_mut(idx).unwrap();
                self.focus = HomeTabStateFocus::Mid;
                current_songs_list_state.select(Some(0));
            }
        }
    }
    fn back_to_playlists_list(&mut self) {
        self.focus = HomeTabStateFocus::Left;
        if let Some(idx) = self.playlists_state.selected() {
            self.songs_list_states[idx].select(None);
        }
    }
    fn select_next_song(&mut self) {
        if let Some(idx) = self.playlists_state.selected() {
            let current_songs_list_state = self.songs_list_states.get_mut(idx).unwrap();
            let i = match current_songs_list_state.selected() {
                Some(i) => {
                    if i >= self.playlists[idx].songs.len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            current_songs_list_state.select(Some(i));
        }
    }
    fn select_previous_song(&mut self) {
        if let Some(idx) = self.playlists_state.selected() {
            let current_songs_list_state = self.songs_list_states.get_mut(idx).unwrap();
            let i = match current_songs_list_state.selected() {
                Some(i) => {
                    if i == 0 {
                        self.playlists[idx].songs.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            current_songs_list_state.select(Some(i));
        }
    }
}

struct PlayList {
    name: String,
    songs: Vec<Song>,
}

struct Song {
    name: String,
    path: PathBuf,
    size: u16,
    format: String,
}
impl Song {
    fn new(path: PathBuf) -> Option<Self> {
        if path.exists() {
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            let size = path.metadata().unwrap().len() as u16;
            let format = path.extension().unwrap().to_str().unwrap().to_string();
            Some(Self {
                name,
                path,
                size,
                format,
            })
        } else {
            None
        }
    }
}

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
    playlist_file_path: PathBuf,
}
impl Configuration {
    fn new() -> Self {
        let mut configure = Self {
            folder_path: PathBuf::new(),
            settting_file_path: PathBuf::new(),
            source_file_path: PathBuf::new(),
            playlist_file_path: PathBuf::new(),
        };
        configure.folder_path = PathBuf::from(env::var("HOME").unwrap()).join(".songbreeze");
        configure.settting_file_path = configure.folder_path.join("setting.json");
        configure.source_file_path = configure.folder_path.join("source.json");
        configure.playlist_file_path = configure.folder_path.join("playlist.json");

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

        //check playlist file exists
        let playlist_file_path = Path::new(&configure.playlist_file_path);
        create_ff_while_asking(playlist_file_path, true);
        configure
    }
}

fn main() -> Result<(), io::Error> {
    let configuration = Rc::new(Configuration::new());
    //app global state
    let mut app_state = GlobalState::new(configuration.clone());
    app_state.set_tab_titles(vec![
        "Home".to_string(),
        "Sources".to_string(),
        "Settings".to_string(),
    ]);
    //source tab state
    let mut source_tab_state = SourceTabState::new(configuration.clone());
    source_tab_state.load_sources();
    //home tab state
    let mut home_tab_state = HomeTabState::new(configuration.clone());
    home_tab_state.load_data();
    // thread::sleep(Duration::from_secs(3));

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
                        code: KeyCode::Char('i'),
                        modifiers: KeyModifiers::NONE,
                    } => match app_state.selected_tab_idx {
                        Some(idx) => match idx {
                            0 => match home_tab_state.focus {
                                HomeTabStateFocus::Left => {
                                    home_tab_state.enter_current_playlist_songs_list();
                                }
                                HomeTabStateFocus::Mid => {}
                                HomeTabStateFocus::Right => {}
                            },
                            _ => {}
                        },
                        None => {}
                    },
                    KeyEvent {
                        code: KeyCode::Char('b'),
                        modifiers: KeyModifiers::NONE,
                    } => match app_state.selected_tab_idx {
                        Some(idx) => match idx {
                            0 => match home_tab_state.focus {
                                HomeTabStateFocus::Left => {}
                                HomeTabStateFocus::Mid => home_tab_state.back_to_playlists_list(),
                                HomeTabStateFocus::Right => {}
                            },
                            _ => {}
                        },
                        None => {}
                    },
                    KeyEvent {
                        code: KeyCode::Char('j'),
                        modifiers: KeyModifiers::NONE,
                    } => match app_state.selected_tab_idx {
                        Some(idx) => match idx {
                            0 => match home_tab_state.focus {
                                HomeTabStateFocus::Left => {
                                    home_tab_state.select_next_playlist();
                                }
                                HomeTabStateFocus::Mid => {
                                    home_tab_state.select_next_song();
                                }
                                HomeTabStateFocus::Right => {}
                            },
                            //source tab
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
                            0 => match home_tab_state.focus {
                                HomeTabStateFocus::Left => {
                                    home_tab_state.select_previous_playlist();
                                }
                                HomeTabStateFocus::Mid => {
                                    home_tab_state.select_previous_song();
                                }
                                HomeTabStateFocus::Right => {}
                            },
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
                    KeyEvent {
                        code: KeyCode::Backspace,
                        modifiers: KeyModifiers::NONE,
                    } => {
                        app_state.input_stream.pop();
                    }
                    _ => {}
                },
                InputEvent::Tick => {}
            },
        }

        terminal.draw(|f| {
            let boards = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(80),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(f.size());
            let tabs_board = boards[0];
            let main_board = boards[1];
            let player_board = boards[2];

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

                    let main_board = Layout::default()
                        .margin(1)
                        .direction(Direction::Horizontal)
                        .constraints(vec![
                            Constraint::Percentage(20),
                            Constraint::Percentage(60),
                            Constraint::Percentage(20),
                        ])
                        .split(main_board);
                    let main_left_board = main_board[0];
                    let main_mid_board = main_board[1];
                    let main_right_board = main_board[2];

                    //play list
                    let main_left_block = Block::default().borders(Borders::RIGHT);
                    let playlists_list_items: Vec<ListItem> = home_tab_state
                        .playlists
                        .iter()
                        .map(|p| ListItem::new(Spans::from(vec![Span::raw(p.name.clone())])))
                        .collect();
                    let play_list = List::new(playlists_list_items)
                        .block(main_left_block)
                        .highlight_style(Style::default().fg(Color::Yellow));
                    f.render_stateful_widget(
                        play_list,
                        main_left_board,
                        &mut home_tab_state.playlists_state,
                    );
                    //songs list corresponding to the current play list
                    let current_playlist_idx = home_tab_state.playlists_state.selected().unwrap();
                    let main_mid_block = Block::default().borders(Borders::RIGHT);
                    let songs = &home_tab_state.playlists[current_playlist_idx].songs;
                    let songs_list_state =
                        &mut home_tab_state.songs_list_states[current_playlist_idx];
                    if !songs.is_empty() {
                        let song_list_items: Vec<ListItem> = songs
                            .iter()
                            .map(|s| ListItem::new(Spans::from(vec![Span::raw(s.name.clone())])))
                            .collect();
                        let song_list = List::new(song_list_items)
                            .block(main_mid_block)
                            .highlight_style(Style::default().fg(Color::Yellow));
                        f.render_stateful_widget(song_list, main_mid_board, songs_list_state);
                    } else {
                        f.render_widget(main_mid_block, main_mid_board);
                    }
                    //song info
                    let main_right_block = Block::default().borders(Borders::NONE);
                    f.render_widget(main_right_block, main_right_board);
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

            // === draw player board ===
            // draw player block
            let player_block = Block::default().borders(Borders::ALL);
            f.render_widget(player_block, player_board);
            // split player board to progress & content board
            let player_board = Layout::default()
                .direction(Direction::Vertical)
                .vertical_margin(1)
                .horizontal_margin(3)
                .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(player_board);
            let player_progress_board = player_board[0];
            let player_content_board = player_board[1];
            //draw progress
            let player_progress = LineGauge::default()
                .gauge_style(
                    Style::default()
                        .fg(Color::Blue)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .line_set(symbols::line::THICK)
                .ratio(0.4);
            f.render_widget(player_progress, player_progress_board);
            let player_content = Paragraph::new(Spans::from(vec![
                Span::styled("(p) Play", Style::default().fg(Color::White)),
                Span::raw(" ".repeat(5)),
                Span::styled("(<) Previous", Style::default().fg(Color::White)),
                Span::raw(" ".repeat(5)),
                Span::styled("(>) Next", Style::default().fg(Color::White)),
            ]));
            f.render_widget(player_content, player_content_board);
        })?;
    }
    disable_raw_mode()?;
    Ok(())
}

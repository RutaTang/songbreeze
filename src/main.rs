use std::{io, sync::mpsc, thread, time::Duration, usize};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    symbols::DOT,
    text::Spans,
    widgets::{Block, Borders, Paragraph, Tabs},
    Terminal,
};

#[derive(Debug)]
enum InputEvent<I> {
    Input(I),
    Tick,
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

    loop {
        let input_event = rx.recv().unwrap();
        match input_event {
            InputEvent::Input(key) => match key {
                KeyEvent {
                    code: KeyCode::Char('q'),
                    modifiers: KeyModifiers::CONTROL,
                } => break,
                _ => {}
            },
            InputEvent::Tick => {}
        }

        //todo: tabs state
        terminal.draw(|f| {
            let boards = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(50),
                        Constraint::Percentage(40),
                    ]
                    .as_ref(),
                )
                .split(f.size());
            let tabs_board = boards[0];

            // draw tabs block and tabs content
            let tabs_block = Block::default().borders(Borders::ALL).title("Menu");
            f.render_widget(tabs_block, tabs_board);
            let titles: Vec<Spans> = ["Home", "Sources", "Setting"]
                .iter()
                .cloned()
                .map(Spans::from)
                .collect();
            let tabs_content = Tabs::new(titles)
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(Color::Yellow))
                .divider(DOT);
            let tabs_content_board = Layout::default().constraints([Constraint::Percentage(100)]).horizontal_margin(5).split(tabs_board)[0];
            let tabs_content_board = Rect::new(
                tabs_content_board.left(),
                tabs_content_board.bottom() / 2,
                tabs_content_board.width,
                tabs_content_board.height-(tabs_content_board.bottom() / 2),
            );
            f.render_widget(tabs_content, tabs_content_board);
        })?;
    }
    Ok(())
}

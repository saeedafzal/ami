use crossterm::{
    cursor::{position, MoveTo, SetCursorStyle},
    event::{read, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{Color, ResetColor, SetBackgroundColor},
    terminal::{self, Clear, ClearType},
    QueueableCommand,
};
use std::io::{self, Write};

pub enum Mode {
    Normal,
    Command,
    Insert,
}

pub struct Cursor {
    pub normal: (u16, u16),
    pub command: (u16, u16),
    pub insert: (u16, u16),
}

pub struct State {
    pub width: u16,
    pub height: u16,
    pub mode: Mode,
    pub command: String,
    pub cursor_position: Cursor,
    pub buffer: String,
}

fn draw(stdout: &mut io::Stdout, state: &mut State) -> io::Result<()> {
    // Clear the screen for a redraw
    stdout.queue(Clear(ClearType::All))?;

    // Render status bar
    queue!(
        stdout,
        MoveTo(0, state.height - 2),
        SetBackgroundColor(Color::White)
    )?;
    stdout.write_all(" ".repeat(state.width as usize).as_bytes())?;
    stdout.queue(ResetColor)?;

    // Render command text
    stdout.queue(MoveTo(0, state.height - 1))?;
    stdout.write(state.command.as_bytes())?;

    // Render buffer text
    stdout.queue(MoveTo(0, 0))?;
    let buffer: Vec<&str> = state.buffer.split("\n").collect();
    for (i, line) in buffer.iter().enumerate() {
        stdout.write(line.as_bytes())?;
        let index = i + 1;
        stdout.queue(MoveTo(0, index as u16))?;
    }

    // Mode specific and display cursor
    match state.mode {
        Mode::Normal => {
            let (x, y) = state.cursor_position.normal;
            queue!(stdout, SetCursorStyle::SteadyBlock, MoveTo(x, y))?;
        }
        Mode::Command => {
            let (x, y) = state.cursor_position.command;
            queue!(stdout, SetCursorStyle::SteadyBlock, MoveTo(x, y))?;
        }
        Mode::Insert => {
            let (x, y) = state.cursor_position.insert;
            queue!(stdout, SetCursorStyle::SteadyBar, MoveTo(x, y))?;
        }
    };

    // Flush the output
    stdout.flush()?;

    Ok(())
}

fn to_normal_mode(stdout: &mut io::Stdout, state: &mut State) -> io::Result<()> {
    state.command.clear();
    state.mode = Mode::Normal;
    state.cursor_position.command.0 = 1;
    draw(stdout, state)
}

fn main() -> io::Result<()> {
    let mut stdout = io::stdout();

    // Init terminal
    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen)?;

    // Properties

    // Initial state
    let (width, height) = terminal::size()?;
    let mut state = State {
        width,
        height,
        mode: Mode::Normal,
        command: String::new(),
        cursor_position: Cursor {
            normal: position()?,
            command: (1, height - 1),
            insert: position()?,
        },
        buffer: String::new(),
    };

    // Initial draw
    draw(&mut stdout, &mut state)?;

    // Event loop
    loop {
        let event = read()?;

        match event {
            Event::Resize(w, h) => {
                state.width = w;
                state.height = h;
                draw(&mut stdout, &mut state)?;
            }
            Event::Key(event) => {
                let code = event.code;
                if code == KeyCode::Char('z') && event.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }

                match state.mode {
                    Mode::Normal => match code {
                        KeyCode::Char(':') => {
                            state.mode = Mode::Command;
                            state.command = String::from(":");
                            draw(&mut stdout, &mut state)?;
                        }
                        KeyCode::Char('i') => {
                            state.mode = Mode::Insert;
                            state.cursor_position.insert.0 = state.cursor_position.insert.0.saturating_sub(1);
                            draw(&mut stdout, &mut state)?;
                        }
                        KeyCode::Char('a') => {
                            state.mode = Mode::Insert;
                            draw(&mut stdout, &mut state)?;
                        }
                        _ => {}
                    },
                    // TODO: Command seems to draw in every branch, draw at the end instead
                    Mode::Command => match code {
                        KeyCode::Esc => {
                            to_normal_mode(&mut stdout, &mut state)?;
                        }
                        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                            to_normal_mode(&mut stdout, &mut state)?;
                        }
                        KeyCode::Backspace => {
                            state.command.pop();
                            state.cursor_position.command.0 = state.cursor_position.command.0.saturating_sub(1);

                            if state.command.is_empty() {
                                state.mode = Mode::Normal;
                                state.cursor_position.command.0 = 1;
                            }
                            draw(&mut stdout, &mut state)?;
                        }
                        KeyCode::Enter => match state.command.as_str() {
                            ":q" => break,
                            _ => {
                                state.command = String::from("Unknown command.");
                                state.mode = Mode::Normal;
                                state.cursor_position.command.0 = 1;
                                draw(&mut stdout, &mut state)?;
                            }
                        }
                        KeyCode::Char(x) => {
                            state.command.push(x);
                            state.cursor_position.command.0 += 1;
                            draw(&mut stdout, &mut state)?;
                        }
                        _ => {}
                    },
                    Mode::Insert => match code {
                        KeyCode::Esc => {
                            state.cursor_position.normal.0 = state.cursor_position.normal.0.saturating_sub(1);
                            to_normal_mode(&mut stdout, &mut state)?;
                        }
                        KeyCode::Enter => {
                            state.buffer.push_str("\n");
                            state.cursor_position.insert = (0, state.cursor_position.insert.1 + 1);
                            state.cursor_position.normal = (0, state.cursor_position.normal.1 + 1);
                            draw(&mut stdout, &mut state)?;
                        }
                        KeyCode::Char(x) => {
                            state.buffer.push(x);
                            state.cursor_position.insert.0 += 1;
                            state.cursor_position.normal.0 += 1;
                            draw(&mut stdout, &mut state)?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    // Clean up terminal
    terminal::disable_raw_mode()?;
    execute!(stdout, terminal::LeaveAlternateScreen)?;

    Ok(())
}

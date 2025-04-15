use crossterm::{
    cursor::{position, MoveTo, SetCursorStyle},
    event::{poll, read, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{Color, ResetColor, SetBackgroundColor},
    terminal::{self, Clear, ClearType},
    QueueableCommand,
};
use std::io::{self, Write};
use std::time::Duration;

pub enum Mode {
    Normal,
    Command,
}

pub struct Cursor {
    pub normal: (u16, u16),
    pub command: (u16, u16),
}

pub struct State {
    pub width: u16,
    pub height: u16,
    pub mode: Mode,
    pub running: bool,
    pub command: String,
    pub cursor_position: Cursor,
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
    state.cursor_position.command = (0, state.height - 1);
    stdout.write(state.command.as_bytes())?;
    state.cursor_position.command.0 = state.command.len() as u16;

    // Mode specific and display cursor
    match state.mode {
        Mode::Normal => {
            stdout.queue(SetCursorStyle::SteadyBlock)?;

            let (x, y) = state.cursor_position.normal;
            stdout.queue(MoveTo(x, y))?;
        }
        Mode::Command => {
            state.cursor_position.command.0 = state.command.len() as u16;
            let (x, y) = state.cursor_position.command;
            stdout.queue(MoveTo(x, y))?;
        }
    };

    // Flush the output
    stdout.flush()?;

    Ok(())
}

fn command_to_normal(stdout: &mut io::Stdout, state: &mut State) -> io::Result<()> {
    state.command.clear();
    state.mode = Mode::Normal;
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
        running: true,
        command: String::new(),
        cursor_position: Cursor {
            normal: position()?,
            command: (0, height - 1),
        },
    };

    // Initial draw
    draw(&mut stdout, &mut state)?;

    // Event loop
    while state.running {
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
                    state.running = false;
                    break;
                }

                match state.mode {
                    Mode::Normal => match code {
                        KeyCode::Char(':') => {
                            state.mode = Mode::Command;
                            state.command = String::from(":");
                            draw(&mut stdout, &mut state)?;
                        }
                        _ => {}
                    },
                    // TODO: Command seems to draw in every branch, draw at the end instead
                    Mode::Command => match code {
                        // Escapes are weird in terminals... doesn't seem to register so checking
                        // if it's a lone escape key event by polling for any other keys
                        KeyCode::Esc => {
                            if poll(Duration::from_millis(50))? {
                                read()?;
                            } else {
                                command_to_normal(&mut stdout, &mut state)?;
                            }
                        }
                        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                            command_to_normal(&mut stdout, &mut state)?;
                        }
                        KeyCode::Backspace => {
                            state.command.pop();
                            if state.command.is_empty() {
                                state.mode = Mode::Normal;
                            }
                            draw(&mut stdout, &mut state)?;
                        }
                        KeyCode::Enter => match state.command.as_str() {
                            _ => {
                                state.command = String::from("Unknown command.");
                                state.mode = Mode::Normal;
                                draw(&mut stdout, &mut state)?;
                            }
                        }
                        KeyCode::Char(x) => {
                            state.command.push(x);
                            draw(&mut stdout, &mut state)?;
                        }
                        _ => {}
                    },
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

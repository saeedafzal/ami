use crossterm::{
    cursor::{MoveTo, SetCursorStyle},
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    queue,
    style::{Color, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    ExecutableCommand, QueueableCommand,
};
use std::collections::HashMap;
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
    pub running: bool,
    pub width: u16,
    pub height: u16,
    pub mode: Mode,
    pub cursor_pos: Cursor,
    pub status_bar: Vec<String>,
    pub command: String,
    pub buffer: Vec<String>,
}

// Callback
type Action = Box<dyn Fn(&mut io::Stdout, &mut State) -> io::Result<()>>;

// Helper function to create action
fn into_action<F>(f: F) -> Action
where
    F: Fn(&mut io::Stdout, &mut State) -> io::Result<()> + 'static,
{
    Box::new(f)
}

// Global map of actions that runs on all modes
fn global_map() -> HashMap<KeyEvent, Action> {
    let mut m = HashMap::new();

    // Kill editor
    m.insert(
        KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
        into_action(|_, state| {
            state.running = false;
            Ok(())
        }),
    );

    m
}

fn normal_map() -> HashMap<KeyEvent, Action> {
    let mut m = HashMap::new();

    // Go to command mode
    m.insert(
        KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE),
        into_action(|stdout, state| {
            state.mode = Mode::Command;
            state.status_bar[0] = String::from("COMMAND");
            state.command = String::from(":");
            draw(stdout, state)
        }),
    );

    m.insert(
        KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
        into_action(|stdout, state| {
            state.mode = Mode::Insert;
            state.status_bar[0] = String::from("INSERT");
            state.cursor_pos.insert.0 = state.cursor_pos.insert.0.saturating_sub(1);
            draw(stdout, state)
        }),
    );

    m.insert(
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        into_action(|stdout, state| {
            state.mode = Mode::Insert;
            state.status_bar[0] = String::from("INSERT");

            let line = &state.buffer[state.cursor_pos.normal.1 as usize];
            let length = line.len() as u16;
            if state.cursor_pos.normal.0 != length - 1 {
                state.cursor_pos.insert.0 += 1;
            }
            draw(stdout, state)
        }),
    );

    // Navigation
    m.insert(
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        into_action(|stdout, state| {
            state.cursor_pos.normal.0 = state.cursor_pos.normal.0.saturating_sub(1);
            state.cursor_pos.insert.0 = state.cursor_pos.insert.0.saturating_sub(1);
            draw(stdout, state)
        }),
    );

    m.insert(
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        into_action(|stdout, state| {
            let line = &state.buffer[state.cursor_pos.normal.1 as usize];
            let length = line.len() as u16;
            if state.cursor_pos.normal.0 < length - 1 {
                state.cursor_pos.normal.0 += 1;
                state.cursor_pos.insert.0 += 1;
            }
            draw(stdout, state)
        }),
    );

    // TODO: Implement 'j' and 'k' properly
    m.insert(
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        into_action(|stdout, state| {
            let rows = (state.buffer.len() - 1) as u16;

            if state.cursor_pos.normal.1 < rows {
                state.cursor_pos.normal.1 += 1;
            }

            draw(stdout, state)
        }),
    );

    m.insert(
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
        into_action(|stdout, state| {
            if state.cursor_pos.normal.1 > 0 {
                state.cursor_pos.normal.1 -= 1;
            }

            draw(stdout, state)
        }),
    );

    m
}

fn command_to_normal(stdout: &mut io::Stdout, state: &mut State) -> io::Result<()> {
    state.mode = Mode::Normal;
    state.status_bar[0] = String::from("NORMAL");
    state.command.clear();
    state.cursor_pos.command.0 = 1;
    draw(stdout, state)
}

fn command_map() -> HashMap<KeyEvent, Action> {
    let mut m = HashMap::new();

    m.insert(
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        into_action(|stdout, state| {
            state.command.pop();
            state.cursor_pos.command.0 = state.cursor_pos.command.0.saturating_sub(1);

            if state.command.is_empty() {
                state.mode = Mode::Normal;
                state.status_bar[0] = String::from("NORMAL");
                state.cursor_pos.command.0 = 1;
            }

            draw(stdout, state)
        }),
    );

    m.insert(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        into_action(|stdout, state| command_to_normal(stdout, state)),
    );

    m.insert(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        into_action(|stdout, state| command_to_normal(stdout, state)),
    );

    m.insert(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        into_action(|stdout, state| {
            match state.command.as_str() {
                ":q" => state.running = false,
                _ => {
                    state.command = String::from("Unknown command.");
                    state.mode = Mode::Normal;
                    state.status_bar[0] = String::from("NORMAL");
                    state.cursor_pos.command.0 = 1;
                }
            }
            draw(stdout, state)
        }),
    );

    m
}

fn insert_map() -> HashMap<KeyEvent, Action> {
    let mut m = HashMap::new();

    m.insert(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        into_action(|stdout, state| {
            state.cursor_pos.normal.0 = state.cursor_pos.insert.0.saturating_sub(1);
            command_to_normal(stdout, state)
        }),
    );

    m.insert(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        into_action(|stdout, state| {
            let (x, y) = state.cursor_pos.insert;
            let line = &mut state.buffer[y as usize];
            let tail = line.split_off(x as usize);
            state.buffer.insert(y as usize + 1, tail);

            state.cursor_pos.normal = (0, y + 1);
            state.cursor_pos.insert = (0, y + 1);

            draw(stdout, state)
        }),
    );

    m.insert(
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        into_action(|stdout, state| {
            let (x, y) = state.cursor_pos.insert;
            let ys = y as usize;

            if x > 0 {
                let index = (x - 1) as usize;
                if index < state.buffer[ys].len() {
                    state.buffer[ys].remove(index);
                }
                state.cursor_pos.normal.0 -= 1;
                state.cursor_pos.insert.0 -= 1;
            } else if y > 0 {
                let prev_index = ys - 1;
                let line = state.buffer.remove(ys);
                let prev_length = state.buffer[prev_index].len() as u16;
                state.buffer[prev_index].push_str(&line);
                state.cursor_pos.insert = (prev_length, prev_index as u16);
                state.cursor_pos.normal = (prev_length, prev_index as u16);
            }
            draw(stdout, state)
        }),
    );

    m
}

fn draw_status_bar(stdout: &mut io::Stdout, state: &mut State) -> io::Result<()> {
    queue!(
        stdout,
        MoveTo(0, state.height - 2),
        SetBackgroundColor(Color::Rgb {
            r: 29,
            g: 41,
            b: 61
        }),
        SetForegroundColor(Color::White)
    )?;

    // Background
    stdout.write(" ".repeat(state.width as usize).as_bytes())?;

    // Text
    stdout.queue(MoveTo(0, state.height - 2))?;
    let a = String::from(" ") + &state.status_bar[0];
    stdout.write(a.as_bytes())?;

    stdout.queue(ResetColor)?;
    Ok(())
}

fn draw(stdout: &mut io::Stdout, state: &mut State) -> io::Result<()> {
    // Clear the screen for a redraw
    stdout.queue(Clear(ClearType::All))?;

    // Render status bar
    draw_status_bar(stdout, state)?;

    // Render command
    stdout.queue(MoveTo(0, state.height - 1))?;
    stdout.write(state.command.as_bytes())?;

    // Render buffer
    stdout.queue(MoveTo(0, 0))?;
    for (i, line) in state.buffer.iter().enumerate() {
        stdout.write(line.as_bytes())?;
        let index = i + 1;
        stdout.queue(MoveTo(0, index as u16))?;
    }

    // Mode specific
    match state.mode {
        Mode::Normal => {
            let (x, y) = state.cursor_pos.normal;
            queue!(stdout, SetCursorStyle::SteadyBlock, MoveTo(x, y))?;
        }
        Mode::Command => {
            let (x, y) = state.cursor_pos.command;
            stdout.queue(MoveTo(x, y))?;
        }
        Mode::Insert => {
            let (x, y) = state.cursor_pos.insert;
            queue!(stdout, SetCursorStyle::SteadyBar, MoveTo(x, y))?;
        }
    }

    // Flush the output
    stdout.flush()?;

    Ok(())
}

fn main() -> io::Result<()> {
    // Build action maps
    let global_map = global_map();
    let normal_map = normal_map();
    let command_map = command_map();
    let insert_map = insert_map();

    // Var for stdout
    let mut stdout = io::stdout();

    // Init terminal
    stdout.execute(terminal::EnterAlternateScreen)?;
    terminal::enable_raw_mode()?;

    // Init state
    let (width, height) = terminal::size()?;
    let mut state = State {
        running: true,
        width,
        height,
        mode: Mode::Normal,
        cursor_pos: Cursor {
            normal: (0, 0),
            command: (1, height - 1),
            insert: (0, 0),
        },
        status_bar: vec![String::from("NORMAL")],
        command: String::new(),
        buffer: vec![String::new()],
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
                if let Some(action) = global_map.get(&event) {
                    action(&mut stdout, &mut state)?;
                    continue;
                }

                let map = match &state.mode {
                    Mode::Normal => &normal_map,
                    Mode::Command => &command_map,
                    Mode::Insert => &insert_map,
                };

                if let Some(action) = map.get(&event) {
                    action(&mut stdout, &mut state)?;
                    continue;
                }

                // Handle text insert for different modes
                if let (Mode::Command, KeyCode::Char(x)) = (&state.mode, event.code) {
                    state.command.push(x);
                    state.cursor_pos.command.0 += 1;
                    draw(&mut stdout, &mut state)?;
                    continue;
                }

                if let (Mode::Insert, KeyCode::Char(x)) = (&state.mode, event.code) {
                    let (col, row) = state.cursor_pos.insert;
                    let line = &mut state.buffer[row as usize];
                    let insert_index = (col as usize).min(line.len());
                    line.insert(insert_index, x);

                    state.cursor_pos.insert.0 += 1;
                    state.cursor_pos.normal.0 += 1;
                    draw(&mut stdout, &mut state)?;
                }
            }
            _ => {}
        }
    }

    // Clean up terminal
    terminal::disable_raw_mode()?;
    stdout.execute(terminal::LeaveAlternateScreen)?;

    Ok(())
}

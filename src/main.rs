use crossterm::{
    cursor::{MoveTo, RestorePosition, SavePosition},
    terminal::{Clear, ClearType},
    ExecutableCommand, QueueableCommand,
};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use std::io::{stdout, Write};
use std::io::{BufReader, Read};
use vt100::{Cell, Color, Parser};

pub struct VirtualTerminal {
    parser: Parser,
    reader: BufReader<Box<dyn Read + Send>>,
    _child: Box<dyn Child + Send + Sync>,
}

pub fn cell_to_ansi(cell: &Cell) -> String {
    let mut codes = Vec::new();

    // --- Text attributes ---
    if cell.bold() {
        codes.push("1");
    }
    if cell.dim() {
        codes.push("2");
    }
    if cell.italic() {
        codes.push("3");
    }
    if cell.underline() {
        codes.push("4");
    }
    if cell.inverse() {
        codes.push("7");
    }

    // --- Foreground ---
    let fg = cell.fgcolor();
    let fg_color = color_to_ansi_code(&fg, true);
    codes.push(&fg_color);

    // --- Background ---
    let bg = cell.bgcolor();
    let bg_color = color_to_ansi_code(&bg, false);
    codes.push(&bg_color);

    if codes.is_empty() {
        // nothing special, use reset
        "\x1b[0m".to_string()
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

fn color_to_ansi_code(color: &Color, is_foreground: bool) -> String {
    match color {
        Color::Default => {
            if is_foreground {
                "39".to_string()
            } else {
                "49".to_string()
            }
        }
        Color::Idx(idx) => {
            if is_foreground {
                format!("38;5;{}", idx)
            } else {
                format!("48;5;{}", idx)
            }
        }
        Color::Rgb(r, g, b) => {
            if is_foreground {
                format!("38;2;{};{};{}", r, g, b)
            } else {
                format!("48;2;{};{};{}", r, g, b)
            }
        }
    }
}

impl VirtualTerminal {
    pub fn spawn(
        command: CommandBuilder,
        rows: u16,
        cols: u16,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: rows,
            cols: cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let child = pair.slave.spawn_command(command)?;

        let reader = pair.master.try_clone_reader()?;
        let parser = Parser::new(rows, cols, 0);

        Ok(VirtualTerminal {
            parser,
            reader: BufReader::new(reader),
            _child: child,
        })
    }

    pub fn get_used_height(&self) -> u16 {
        let screen = self.parser.screen();
        let (rows, _) = screen.size();

        // Find the last non-empty row (from bottom to top)
        for row in (0..rows).rev() {
            if self.is_row_non_empty(row) {
                // Return row count (1-based) plus some padding
                return (row + 1).min(rows);
            }
        }

        1 // Minimum height if all rows are empty
    }

    fn is_row_non_empty(&self, row: u16) -> bool {
        let screen = self.parser.screen();
        let (_, cols) = screen.size();

        for col in 0..cols {
            if let Some(cell) = screen.cell(row, col) {
                if !cell.contents().is_empty() && cell.contents() != " " {
                    return true;
                }
            }
        }
        false
    }

    pub fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0; 256];
        let mut stdout = stdout();

        match self.reader.read(&mut buf) {
            Ok(0) => return Ok(()), // EOF
            Ok(n) => {
                self.parser.process(&buf[..n]);

                let screen = self.parser.screen();
                let mut frame = String::new();

                let (rows, cols) = screen.size();

                // Calculate dynamic height based on content
                let dynamic_height = self.get_used_height();

                // Save current cursor position
                stdout.execute(SavePosition)?;

                // Move to the bottom area with dynamic height
                let terminal_height = crossterm::terminal::size()?.1;
                let start_row = terminal_height
                    .saturating_sub(dynamic_height)
                    .saturating_sub(1);

                stdout.execute(MoveTo(0, start_row))?;

                // Render only the used portion
                let render_rows = rows.min(dynamic_height);

                for row in 0..render_rows {
                    for col in 0..cols {
                        if let Some(cell) = screen.cell(row, col) {
                            let ansi = cell_to_ansi(cell);
                            frame.push_str(&ansi);
                            if cell.has_contents() {
                                frame.push_str(cell.contents());
                            } else {
                                frame.push(' ');
                            }
                        }
                    }
                    frame.push('\n');
                }

                print!("{}", frame);

                // Restore cursor position
                stdout.execute(RestorePosition)?;
                stdout.flush()?;
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
            }
        }

        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = CommandBuilder::new("/home/florian/dev/square/target/debug/square");
    let mut vt = VirtualTerminal::spawn(cmd, 24, 80)?;

    println!("Running virtual terminal in bottom area. Your normal terminal is preserved above.");
    println!("You can still type commands and scroll normally.");
    println!(
        "The virtual terminal output will appear in a dynamically sized area at the bottom.\n"
    );

    // return Ok(());

    loop {
        vt.render()?;
        // Small delay to prevent excessive CPU usage
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

use clap::Parser as ClapParser;
use crossterm::{
    cursor::{MoveUp, RestorePosition, SavePosition},
    terminal::{Clear, ClearType},
    ExecutableCommand,
};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use std::io::{stdout, Write};
use std::io::{BufReader, Read};
use vt100::{Cell, Color, Parser};

pub struct VirtualTerminal {
    parser: Parser,
    reader: BufReader<Box<dyn Read + Send>>,
    child: Box<dyn Child + Send + Sync>,
    last_render_height: u16,
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
    pub fn spawn(command: CommandBuilder, rows: u16, cols: u16) -> anyhow::Result<Self> {
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
            child: child,
            last_render_height: 0,
        })
    }

    pub fn get_used_height(&self) -> u16 {
        let screen = self.parser.screen();
        let (rows, _) = screen.size();

        for row in (0..rows).rev() {
            if self.is_row_non_empty(row) {
                return (row + 1).min(rows);
            }
        }

        0
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

    pub fn render(&mut self) -> anyhow::Result<()> {
        let mut buf = [0; 256];
        let mut stdout = stdout();

        match self.reader.read(&mut buf) {
            Ok(0) => return Ok(()), // EOF
            Ok(n) => {
                self.parser.process(&buf[..n]);

                let screen = self.parser.screen();
                let (rows, cols) = screen.size();
                let dynamic_height = self.get_used_height();
                let render_rows = rows.min(dynamic_height);

                let mut frame = String::new();
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

                // Move up to the *top of the previous frame*
                if self.last_render_height > 0 {
                    stdout.execute(MoveUp(self.last_render_height))?;
                }

                // Clear everything from here down
                stdout.execute(Clear(ClearType::FromCursorDown))?;

                // Print the new frame
                print!("{}", frame);
                stdout.flush()?;

                // Save new render height
                self.last_render_height = render_rows;
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
            }
        }

        Ok(())
    }

    pub fn is_child_done(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(self.child.try_wait()?.is_some())
    }
}

#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Command and its arguments
    #[arg(required = true, num_args = 1..)]
    cmd: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let command = &args.cmd[0];
    let args = &args.cmd[1..];

    let mut cmd = CommandBuilder::new(command);
    for arg in args {
        cmd.arg(arg);
    }

    let mut vt = VirtualTerminal::spawn(cmd, 24, 80)?;

    while let Ok(false) = vt.is_child_done() {
        std::thread::sleep(std::time::Duration::from_millis(50));
        vt.render()?;
    }

    Ok(())
}

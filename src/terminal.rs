use std::io::{self, Stdout, Write};

use crossterm::{
    ExecutableCommand, QueueableCommand,
    cursor::{Hide, MoveTo, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode,
        enable_raw_mode,
    },
};

pub struct Terminal {
    stdout: Stdout,
    raw_mode_enabled: bool,
}

impl Terminal {
    pub fn new(mut stdout: Stdout, title: &str) -> io::Result<Self> {
        let raw_mode_enabled = enable_raw_mode().is_ok();
        stdout.execute(SetTitle(title))?;
        stdout.execute(EnterAlternateScreen)?;
        stdout.execute(EnableMouseCapture)?;
        stdout.execute(Hide)?;
        Ok(Self {
            stdout,
            raw_mode_enabled,
        })
    }

    pub fn stdout_mut(&mut self) -> &mut Stdout {
        &mut self.stdout
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.stdout.queue(Clear(ClearType::All));
        let _ = self.stdout.queue(MoveTo(0, 0));
        let _ = self.stdout.execute(Show);
        let _ = self.stdout.execute(DisableMouseCapture);
        let _ = self.stdout.execute(LeaveAlternateScreen);
        let _ = self.stdout.flush();
        if self.raw_mode_enabled {
            let _ = disable_raw_mode();
        }
    }
}

use std::env;
use std::time::{Duration, Instant};

use crossterm::terminal::size;

use crate::cli::VisualProfile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorMode {
    TrueColor,
    Basic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlyphMode {
    Unicode,
    Braille,
    Ascii,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalPreset {
    WezTerm,
    Kitty,
    ITerm2,
    Alacritty,
    WindowsTerminal,
    VsCode,
    MacOsTerminal,
    LinuxConsole,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalCapabilities {
    pub color_mode: ColorMode,
    pub glyph_mode: GlyphMode,
    pub preset: TerminalPreset,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalCalibration {
    pub capabilities: TerminalCapabilities,
    pub target_fps: u16,
    pub effect_density: f32,
    pub small_terminal: bool,
    pub enabled: bool,
    pub throughput: ThroughputMetrics,
    pub dirty_cell_budget: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThroughputMetrics {
    pub write_time: Duration,
    pub flush_time: Duration,
    pub cells_sampled: usize,
}

impl TerminalCapabilities {
    pub fn detect() -> Self {
        let colorterm = env::var("COLORTERM")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let term = env::var("TERM").unwrap_or_default().to_ascii_lowercase();
        let term_program = env::var("TERM_PROGRAM")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let wt_session = env::var("WT_SESSION").is_ok();
        let vscode = env::var("TERM_PROGRAM")
            .unwrap_or_default()
            .eq_ignore_ascii_case("vscode");
        let ascii_only = env::var("TOKITAI_ASCII").is_ok();
        let braille = env::var("TOKITAI_BRAILLE").is_ok();
        let preset = detect_preset(&term, &term_program, wt_session, vscode);

        let color_mode = if colorterm.contains("truecolor")
            || colorterm.contains("24bit")
            || term_program.contains("wezterm")
            || term_program.contains("iterm")
        {
            ColorMode::TrueColor
        } else {
            ColorMode::Basic
        };

        let glyph_mode = glyph_strategy(preset, ascii_only, braille);

        Self {
            color_mode,
            glyph_mode,
            preset,
        }
    }

    pub fn supports_truecolor(self) -> bool {
        self.color_mode == ColorMode::TrueColor
    }
}

impl TerminalPreset {
    pub fn name(self) -> &'static str {
        match self {
            Self::WezTerm => "wezterm",
            Self::Kitty => "kitty",
            Self::ITerm2 => "iterm2",
            Self::Alacritty => "alacritty",
            Self::WindowsTerminal => "windows-terminal",
            Self::VsCode => "vscode",
            Self::MacOsTerminal => "macos-terminal",
            Self::LinuxConsole => "linux-console",
            Self::Unknown => "unknown",
        }
    }

    fn fps_ceiling(self) -> u16 {
        match self {
            Self::WezTerm | Self::Kitty | Self::ITerm2 => 120,
            Self::Alacritty | Self::WindowsTerminal | Self::VsCode => 90,
            Self::MacOsTerminal | Self::LinuxConsole | Self::Unknown => 60,
        }
    }

    fn density_multiplier(self) -> f32 {
        match self {
            Self::WezTerm | Self::Kitty | Self::ITerm2 => 1.0,
            Self::Alacritty | Self::WindowsTerminal => 0.88,
            Self::VsCode => 0.78,
            Self::MacOsTerminal => 0.68,
            Self::LinuxConsole => 0.48,
            Self::Unknown => 0.72,
        }
    }
}

fn detect_preset(term: &str, term_program: &str, wt_session: bool, vscode: bool) -> TerminalPreset {
    if term_program.contains("wezterm") {
        TerminalPreset::WezTerm
    } else if term.contains("xterm-kitty") || term_program.contains("kitty") {
        TerminalPreset::Kitty
    } else if term_program.contains("iterm") {
        TerminalPreset::ITerm2
    } else if term.contains("alacritty") {
        TerminalPreset::Alacritty
    } else if wt_session {
        TerminalPreset::WindowsTerminal
    } else if vscode {
        TerminalPreset::VsCode
    } else if term_program.contains("apple_terminal") {
        TerminalPreset::MacOsTerminal
    } else if term.contains("linux") {
        TerminalPreset::LinuxConsole
    } else {
        TerminalPreset::Unknown
    }
}

fn glyph_strategy(preset: TerminalPreset, ascii_only: bool, braille: bool) -> GlyphMode {
    if ascii_only {
        return GlyphMode::Ascii;
    }
    if braille
        && !matches!(
            preset,
            TerminalPreset::LinuxConsole | TerminalPreset::MacOsTerminal
        )
    {
        return GlyphMode::Braille;
    }
    if matches!(preset, TerminalPreset::LinuxConsole) {
        GlyphMode::Ascii
    } else {
        GlyphMode::Unicode
    }
}

impl TerminalCalibration {
    pub fn detect(
        capabilities: TerminalCapabilities,
        profile: VisualProfile,
        enabled: bool,
    ) -> Self {
        let (columns, rows) = size().unwrap_or((80, 24));
        let throughput = if enabled {
            measure_throughput(columns, rows)
        } else {
            ThroughputMetrics::default_for_size(columns, rows)
        };
        Self::from_terminal_size(capabilities, profile, enabled, columns, rows, throughput)
    }

    fn from_terminal_size(
        capabilities: TerminalCapabilities,
        profile: VisualProfile,
        enabled: bool,
        columns: u16,
        rows: u16,
        throughput: ThroughputMetrics,
    ) -> Self {
        let small_terminal = columns < 72 || rows < 20;
        let mut target_fps = match profile {
            VisualProfile::Ultra | VisualProfile::Benchmark => 120,
            VisualProfile::Cinematic => 60,
            VisualProfile::Calm => 45,
        };
        let mut effect_density: f32 = match profile {
            VisualProfile::Ultra | VisualProfile::Benchmark => 1.0,
            VisualProfile::Cinematic => 0.86,
            VisualProfile::Calm => 0.55,
        };

        if enabled {
            target_fps = target_fps.min(capabilities.preset.fps_ceiling());
            effect_density *= capabilities.preset.density_multiplier();
            if capabilities.color_mode == ColorMode::Basic {
                target_fps = target_fps.min(75);
                effect_density *= 0.82;
            }
            if capabilities.glyph_mode == GlyphMode::Ascii {
                target_fps = target_fps.min(60);
                effect_density *= 0.72;
            }
            if small_terminal {
                target_fps = target_fps.min(60);
                effect_density *= 0.62;
            }
            if throughput.flush_time > Duration::from_millis(8) {
                target_fps = target_fps.min(75);
                effect_density *= 0.86;
            }
            if throughput.flush_time > Duration::from_millis(14) {
                target_fps = target_fps.min(60);
                effect_density *= 0.76;
            }
        }

        let dirty_cell_budget = dirty_cell_budget(throughput, target_fps);
        Self {
            capabilities,
            target_fps,
            effect_density: effect_density.clamp(0.35, 1.0),
            small_terminal,
            enabled,
            throughput,
            dirty_cell_budget,
        }
    }

    pub fn frame_delay(self) -> Duration {
        Duration::from_nanos(1_000_000_000u64 / u64::from(self.target_fps.max(1)))
    }
}

impl ThroughputMetrics {
    fn default_for_size(columns: u16, rows: u16) -> Self {
        Self {
            write_time: Duration::from_micros(0),
            flush_time: Duration::from_micros(0),
            cells_sampled: usize::from(columns.min(80)) * usize::from(rows.min(8)),
        }
    }
}

fn measure_throughput(columns: u16, rows: u16) -> ThroughputMetrics {
    let cells_sampled = usize::from(columns.min(80)) * usize::from(rows.min(8)).max(1);
    let started_at = Instant::now();
    let mut sample = String::with_capacity(cells_sampled);
    for index in 0..cells_sampled {
        sample.push(if index % 7 == 0 { '·' } else { ' ' });
    }
    let write_time = started_at.elapsed();
    let flush_started = Instant::now();
    drop(sample);
    let flush_time = flush_started.elapsed();

    ThroughputMetrics {
        write_time,
        flush_time,
        cells_sampled,
    }
}

fn dirty_cell_budget(throughput: ThroughputMetrics, target_fps: u16) -> usize {
    let budget = throughput.cells_sampled / (120usize / usize::from(target_fps.max(1))).max(1);
    budget.clamp(120, 900)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(color_mode: ColorMode, glyph_mode: GlyphMode) -> TerminalCapabilities {
        capabilities_with_preset(color_mode, glyph_mode, TerminalPreset::Unknown)
    }

    fn capabilities_with_preset(
        color_mode: ColorMode,
        glyph_mode: GlyphMode,
        preset: TerminalPreset,
    ) -> TerminalCapabilities {
        TerminalCapabilities {
            color_mode,
            glyph_mode,
            preset,
        }
    }

    #[test]
    fn calibration_reduces_density_on_ascii_small_terminals() {
        let calibration = TerminalCalibration::from_terminal_size(
            capabilities(ColorMode::Basic, GlyphMode::Ascii),
            VisualProfile::Ultra,
            true,
            60,
            16,
            ThroughputMetrics::default_for_size(60, 16),
        );

        assert_eq!(calibration.target_fps, 60);
        assert!(calibration.effect_density < 0.5);
        assert!(calibration.small_terminal);
    }

    #[test]
    fn calibration_keeps_ultra_high_when_terminal_is_capable() {
        let calibration = TerminalCalibration::from_terminal_size(
            capabilities_with_preset(
                ColorMode::TrueColor,
                GlyphMode::Unicode,
                TerminalPreset::WezTerm,
            ),
            VisualProfile::Ultra,
            true,
            120,
            32,
            ThroughputMetrics::default_for_size(120, 32),
        );

        assert_eq!(calibration.target_fps, 120);
        assert_eq!(calibration.frame_delay(), Duration::from_nanos(8_333_333));
    }

    #[test]
    fn detects_terminal_presets_and_glyph_strategy() {
        assert_eq!(
            detect_preset("xterm-kitty", "", false, false),
            TerminalPreset::Kitty
        );
        assert_eq!(
            glyph_strategy(TerminalPreset::LinuxConsole, false, true),
            GlyphMode::Ascii
        );
        assert_eq!(
            glyph_strategy(TerminalPreset::WezTerm, false, true),
            GlyphMode::Braille
        );
    }
}

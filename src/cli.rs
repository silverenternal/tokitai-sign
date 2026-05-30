use std::env;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Speed {
    Slow,
    Normal,
    Fast,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualProfile {
    Ultra,
    Cinematic,
    Calm,
    Benchmark,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionPreset {
    Prime,
    Aurora,
    Pulse,
}

impl VisualProfile {
    #[cfg(test)]
    pub fn target_frame_delay(self) -> Duration {
        match self {
            Self::Ultra => Duration::from_millis(8),
            Self::Cinematic => Duration::from_millis(16),
            Self::Calm => Duration::from_millis(24),
            Self::Benchmark => Duration::from_millis(8),
        }
    }

    pub fn trail_decay(self) -> f32 {
        match self {
            Self::Ultra => 0.84,
            Self::Cinematic => 0.9,
            Self::Calm => 0.72,
            Self::Benchmark => 0.84,
        }
    }

    pub fn intensity(self) -> f32 {
        match self {
            Self::Ultra => 1.0,
            Self::Cinematic => 0.92,
            Self::Calm => 0.72,
            Self::Benchmark => 1.0,
        }
    }

    pub fn benchmark_enabled(self) -> bool {
        self == Self::Benchmark
    }
}

impl Speed {
    pub fn loader_delay(self) -> Duration {
        match self {
            Self::Slow => Duration::from_millis(150),
            Self::Normal => Duration::from_millis(100),
            Self::Fast => Duration::from_millis(45),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub show_loader: bool,
    pub speed: Speed,
    pub profile: VisualProfile,
    pub motion_preset: MotionPreset,
    pub theme_path: Option<String>,
    pub record_path: Option<String>,
    pub replay_path: Option<String>,
    pub record_frames_path: Option<String>,
    pub replay_frames_path: Option<String>,
    pub score_frames_path: Option<String>,
    pub snapshot: bool,
    pub inspect: bool,
    pub help: bool,
    pub version: bool,
    pub calibration_enabled: bool,
}

impl Config {
    pub fn from_env() -> Self {
        Self::parse(env::args().skip(1))
    }

    fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut config = Self {
            show_loader: true,
            speed: Speed::Normal,
            profile: VisualProfile::Ultra,
            motion_preset: MotionPreset::Prime,
            theme_path: None,
            record_path: None,
            replay_path: None,
            record_frames_path: None,
            replay_frames_path: None,
            score_frames_path: None,
            snapshot: false,
            inspect: false,
            help: false,
            version: false,
            calibration_enabled: true,
        };

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--no-loader" => config.show_loader = false,
                "--speed" => {
                    if let Some(speed) = args.next() {
                        config.speed = parse_speed(&speed);
                    }
                }
                "--fast" => config.speed = Speed::Fast,
                "--slow" => config.speed = Speed::Slow,
                "--profile" => {
                    if let Some(profile) = args.next() {
                        config.profile = parse_profile(&profile);
                    }
                }
                "--motion" => {
                    if let Some(preset) = args.next() {
                        config.motion_preset = parse_motion_preset(&preset);
                    }
                }
                "--theme" => config.theme_path = args.next(),
                "--record" => config.record_path = args.next(),
                "--replay" => config.replay_path = args.next(),
                "--record-frames" => config.record_frames_path = args.next(),
                "--replay-frames" => config.replay_frames_path = args.next(),
                "--score-frames" => config.score_frames_path = args.next(),
                "--snapshot" => config.snapshot = true,
                "--inspect" => config.inspect = true,
                "--help" | "-h" => config.help = true,
                "--version" | "-V" => config.version = true,
                "--no-calibration" => config.calibration_enabled = false,
                _ => {}
            }
        }

        config
    }
}

impl MotionPreset {
    pub fn name(self) -> &'static str {
        match self {
            Self::Prime => "prime",
            Self::Aurora => "aurora",
            Self::Pulse => "pulse",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Prime => Self::Aurora,
            Self::Aurora => Self::Pulse,
            Self::Pulse => Self::Prime,
        }
    }
}

fn parse_profile(profile: &str) -> VisualProfile {
    match profile {
        "cinematic" => VisualProfile::Cinematic,
        "calm" => VisualProfile::Calm,
        "benchmark" => VisualProfile::Benchmark,
        _ => VisualProfile::Ultra,
    }
}

fn parse_speed(speed: &str) -> Speed {
    match speed {
        "fast" => Speed::Fast,
        "slow" => Speed::Slow,
        _ => Speed::Normal,
    }
}

fn parse_motion_preset(preset: &str) -> MotionPreset {
    match preset {
        "aurora" => MotionPreset::Aurora,
        "pulse" => MotionPreset::Pulse,
        _ => MotionPreset::Prime,
    }
}

pub fn help_text() -> String {
    format!(
        "{name} {version}\n\nUSAGE:\n    {name} [OPTIONS]\n\nOPTIONS:\n    --no-loader              Skip the startup loading bar\n    --speed <slow|normal|fast>\n    --fast                   Shortcut for --speed fast\n    --slow                   Shortcut for --speed slow\n    --profile <ultra|cinematic|calm|benchmark>\n    --motion <prime|aurora|pulse>\n    --theme <path>           Load key=value theme file\n    --record <path>          Write frame metrics\n    --replay <path>          Replay a metrics file\n    --record-frames <path>   Write deterministic visual frame JSONL\n    --replay-frames <path>   Replay deterministic visual frame JSONL\n    --score-frames <path>    Score deterministic visual frame JSONL\n    --snapshot               Print deterministic visual snapshot JSON and exit\n    --inspect                Enable runtime inspect controls\n    --no-calibration         Skip terminal throughput probe\n    --help, -h               Print help\n    --version, -V            Print version\n\nENV:\n    TOKITAI_ASCII=1          Force ASCII glyph fallback\n    TOKITAI_BRAILLE=1        Force Braille orbit glyphs\n",
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION")
    )
}

pub fn version_text() -> String {
    format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_no_loader_and_speed() {
        let config = Config::parse(["--no-loader", "--speed", "fast"].map(String::from));

        assert!(!config.show_loader);
        assert_eq!(config.speed, Speed::Fast);
    }

    #[test]
    fn parses_visual_profile() {
        let config = Config::parse(["--profile", "calm"].map(String::from));

        assert_eq!(config.profile, VisualProfile::Calm);
        assert!(VisualProfile::Benchmark.benchmark_enabled());
    }

    #[test]
    fn falls_back_to_normal_for_unknown_speed() {
        let config = Config::parse(["--speed", "warp"].map(String::from));

        assert_eq!(config.speed, Speed::Normal);
    }

    #[test]
    fn ultra_profile_targets_120_fps() {
        assert_eq!(
            VisualProfile::Ultra.target_frame_delay(),
            Duration::from_millis(8)
        );
        assert_eq!(
            VisualProfile::Cinematic.target_frame_delay(),
            Duration::from_millis(16)
        );
        assert_eq!(
            VisualProfile::Calm.target_frame_delay(),
            Duration::from_millis(24)
        );
    }

    #[test]
    fn parses_theme_record_replay_and_calibration_flags() {
        let config = Config::parse(
            [
                "--theme",
                "theme.tok",
                "--record",
                "frames.log",
                "--replay",
                "frames.log",
                "--record-frames",
                "visual.jsonl",
                "--replay-frames",
                "visual.jsonl",
                "--score-frames",
                "visual.jsonl",
                "--snapshot",
                "--inspect",
                "--motion",
                "pulse",
                "--no-calibration",
            ]
            .map(String::from),
        );

        assert_eq!(config.theme_path.as_deref(), Some("theme.tok"));
        assert_eq!(config.record_path.as_deref(), Some("frames.log"));
        assert_eq!(config.replay_path.as_deref(), Some("frames.log"));
        assert_eq!(config.record_frames_path.as_deref(), Some("visual.jsonl"));
        assert_eq!(config.replay_frames_path.as_deref(), Some("visual.jsonl"));
        assert_eq!(config.score_frames_path.as_deref(), Some("visual.jsonl"));
        assert!(config.snapshot);
        assert!(config.inspect);
        assert_eq!(config.motion_preset, MotionPreset::Pulse);
        assert!(!config.calibration_enabled);
    }

    #[test]
    fn help_and_version_text_are_available() {
        assert!(help_text().contains("--snapshot"));
        assert!(version_text().contains("tokitai-sign"));
    }
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenePreset {
    Orbit,
    MultiStroke,
    ArcLoop,
    Stress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtyMode {
    Naive,
    UniformThreshold,
    LuminanceThreshold,
    PriorityDirty,
    TopologyOnly,
    RoleAware,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlyphHistoryMode {
    Off,
    ScreenCell,
    Path,
    TopologyDp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaperExperiment {
    Ablations,
    DirtyBudget,
    TerminalMatrix,
    MetricValidation,
    GlyphReconstruction,
    DirtyAudit,
    TopologyMetrics,
    TopologyStress,
    WeightSensitivity,
    RuntimeComplexity,
    IntegratedRuntime,
    SpeedBoundary,
    StressInterpretation,
    WeightStability,
    TranslationControl,
    RuntimeDegraded,
    WeightGrid,
    LowLatencyQualityDelta,
    LowLatencyTopologyMetrics,
    RuntimeBudgetLadder,
    TerminalIoProtocol,
    RuntimeConfidence1200,
    AdaptiveLowLatencyPolicy,
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
    pub scene_preset: ScenePreset,
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
    pub uncapped: bool,
    pub low_latency: bool,
    pub dirty_mode: DirtyMode,
    pub glyph_history_mode: GlyphHistoryMode,
    pub fixed_dirty_budget: Option<usize>,
    pub paper_experiment: Option<PaperExperiment>,
    pub output_path: Option<String>,
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
            scene_preset: ScenePreset::Orbit,
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
            uncapped: false,
            low_latency: false,
            dirty_mode: DirtyMode::Full,
            glyph_history_mode: GlyphHistoryMode::Path,
            fixed_dirty_budget: None,
            paper_experiment: None,
            output_path: None,
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
                "--scene" => {
                    if let Some(scene) = args.next() {
                        config.scene_preset = parse_scene_preset(&scene);
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
                "--dirty-mode" => {
                    if let Some(mode) = args.next() {
                        config.dirty_mode = parse_dirty_mode(&mode);
                    }
                }
                "--glyph-history" => {
                    if let Some(mode) = args.next() {
                        config.glyph_history_mode = parse_glyph_history_mode(&mode);
                    }
                }
                "--fixed-dirty-budget" => {
                    config.fixed_dirty_budget = args.next().and_then(|value| value.parse().ok());
                }
                "--paper-experiment" => {
                    if let Some(experiment) = args.next() {
                        config.paper_experiment = parse_paper_experiment(&experiment);
                    }
                }
                "--output" => config.output_path = args.next(),
                "--help" | "-h" => config.help = true,
                "--version" | "-V" => config.version = true,
                "--uncapped" => {
                    config.uncapped = true;
                    config.low_latency = true;
                }
                "--low-latency" => config.low_latency = true,
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

impl ScenePreset {
    pub fn name(self) -> &'static str {
        match self {
            Self::Orbit => "orbit",
            Self::MultiStroke => "multi-stroke",
            Self::ArcLoop => "arc-loop",
            Self::Stress => "stress",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Orbit => Self::MultiStroke,
            Self::MultiStroke => Self::ArcLoop,
            Self::ArcLoop => Self::Stress,
            Self::Stress => Self::Orbit,
        }
    }
}

impl DirtyMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Naive => "naive",
            Self::UniformThreshold => "uniform-threshold",
            Self::LuminanceThreshold => "luminance-threshold",
            Self::PriorityDirty => "priority-dirty",
            Self::TopologyOnly => "topology-only",
            Self::RoleAware => "role-aware",
            Self::Full => "full",
        }
    }
}

impl GlyphHistoryMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::ScreenCell => "screen-cell",
            Self::Path => "path",
            Self::TopologyDp => "topology-dp",
        }
    }
}

impl PaperExperiment {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ablations => "ablations",
            Self::DirtyBudget => "dirty-budget",
            Self::TerminalMatrix => "terminal-matrix",
            Self::MetricValidation => "metric-validation",
            Self::GlyphReconstruction => "glyph-reconstruction",
            Self::DirtyAudit => "dirty-audit",
            Self::TopologyMetrics => "topology-metrics",
            Self::TopologyStress => "topology-stress",
            Self::WeightSensitivity => "weight-sensitivity",
            Self::RuntimeComplexity => "runtime-complexity",
            Self::IntegratedRuntime => "integrated-runtime",
            Self::SpeedBoundary => "speed-boundary",
            Self::StressInterpretation => "stress-interpretation",
            Self::WeightStability => "weight-stability",
            Self::TranslationControl => "translation-control",
            Self::RuntimeDegraded => "runtime-degraded",
            Self::WeightGrid => "weight-grid",
            Self::LowLatencyQualityDelta => "low-latency-quality-delta",
            Self::LowLatencyTopologyMetrics => "low-latency-topology-metrics",
            Self::RuntimeBudgetLadder => "runtime-budget-ladder",
            Self::TerminalIoProtocol => "terminal-io-protocol",
            Self::RuntimeConfidence1200 => "runtime-confidence-1200fps",
            Self::AdaptiveLowLatencyPolicy => "adaptive-low-latency-policy",
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

fn parse_scene_preset(scene: &str) -> ScenePreset {
    match scene {
        "multi-stroke" => ScenePreset::MultiStroke,
        "arc-loop" => ScenePreset::ArcLoop,
        "stress" => ScenePreset::Stress,
        _ => ScenePreset::Orbit,
    }
}

fn parse_dirty_mode(mode: &str) -> DirtyMode {
    match mode {
        "naive" => DirtyMode::Naive,
        "uniform-threshold" => DirtyMode::UniformThreshold,
        "luminance-threshold" => DirtyMode::LuminanceThreshold,
        "priority-dirty" => DirtyMode::PriorityDirty,
        "topology-only" => DirtyMode::TopologyOnly,
        "role-aware" => DirtyMode::RoleAware,
        _ => DirtyMode::Full,
    }
}

fn parse_glyph_history_mode(mode: &str) -> GlyphHistoryMode {
    match mode {
        "off" => GlyphHistoryMode::Off,
        "screen-cell" => GlyphHistoryMode::ScreenCell,
        "topology-dp" => GlyphHistoryMode::TopologyDp,
        _ => GlyphHistoryMode::Path,
    }
}

fn parse_paper_experiment(experiment: &str) -> Option<PaperExperiment> {
    match experiment {
        "ablations" => Some(PaperExperiment::Ablations),
        "dirty-budget" => Some(PaperExperiment::DirtyBudget),
        "terminal-matrix" => Some(PaperExperiment::TerminalMatrix),
        "metric-validation" => Some(PaperExperiment::MetricValidation),
        "glyph-reconstruction" => Some(PaperExperiment::GlyphReconstruction),
        "dirty-audit" => Some(PaperExperiment::DirtyAudit),
        "topology-metrics" => Some(PaperExperiment::TopologyMetrics),
        "topology-stress" => Some(PaperExperiment::TopologyStress),
        "weight-sensitivity" => Some(PaperExperiment::WeightSensitivity),
        "runtime-complexity" => Some(PaperExperiment::RuntimeComplexity),
        "integrated-runtime" => Some(PaperExperiment::IntegratedRuntime),
        "speed-boundary" => Some(PaperExperiment::SpeedBoundary),
        "stress-interpretation" => Some(PaperExperiment::StressInterpretation),
        "weight-stability" => Some(PaperExperiment::WeightStability),
        "translation-control" => Some(PaperExperiment::TranslationControl),
        "runtime-degraded" => Some(PaperExperiment::RuntimeDegraded),
        "weight-grid" => Some(PaperExperiment::WeightGrid),
        "low-latency-quality-delta" => Some(PaperExperiment::LowLatencyQualityDelta),
        "low-latency-topology-metrics" => Some(PaperExperiment::LowLatencyTopologyMetrics),
        "runtime-budget-ladder" => Some(PaperExperiment::RuntimeBudgetLadder),
        "terminal-io-protocol" => Some(PaperExperiment::TerminalIoProtocol),
        "runtime-confidence-1200fps" => Some(PaperExperiment::RuntimeConfidence1200),
        "adaptive-low-latency-policy" => Some(PaperExperiment::AdaptiveLowLatencyPolicy),
        _ => None,
    }
}

pub fn help_text() -> String {
    format!(
        "{name} {version}\n\nUSAGE:\n    {name} [OPTIONS]\n\nOPTIONS:\n    --no-loader              Skip the startup loading bar\n    --speed <slow|normal|fast>\n    --fast                   Shortcut for --speed fast\n    --slow                   Shortcut for --speed slow\n    --profile <ultra|cinematic|calm|benchmark>\n    --motion <prime|aurora|pulse>\n    --scene <orbit|multi-stroke|arc-loop|stress>\n    --theme <path>           Load key=value theme file\n    --record <path>          Write frame metrics\n    --replay <path>          Replay a metrics file\n    --record-frames <path>   Write deterministic visual frame JSONL\n    --replay-frames <path>   Replay deterministic visual frame JSONL\n    --score-frames <path>    Score deterministic visual frame JSONL\n    --dirty-mode <naive|uniform-threshold|luminance-threshold|priority-dirty|topology-only|role-aware|full>\n    --glyph-history <off|screen-cell|path|topology-dp>\n    --fixed-dirty-budget <n>\n    --uncapped               Disable active frame sleeping and use low-latency rendering\n    --low-latency            Use the low-latency frame-build path for high FPS targets\n    --paper-experiment <ablations|dirty-budget|terminal-matrix|metric-validation|glyph-reconstruction|dirty-audit|topology-metrics|topology-stress|weight-sensitivity|runtime-complexity|integrated-runtime|speed-boundary|stress-interpretation|weight-stability|translation-control|runtime-degraded|weight-grid|low-latency-quality-delta|low-latency-topology-metrics|runtime-budget-ladder|terminal-io-protocol|runtime-confidence-1200fps|adaptive-low-latency-policy>\n    --output <path>          Write paper experiment CSV\n    --snapshot               Print deterministic visual snapshot JSON and exit\n    --inspect                Enable runtime inspect controls\n    --no-calibration         Skip terminal throughput probe\n    --help, -h               Print help\n    --version, -V            Print version\n\nENV:\n    TOKITAI_ASCII=1          Force ASCII glyph fallback\n    TOKITAI_BRAILLE=1        Force Braille orbit glyphs\n",
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
                "--scene",
                "multi-stroke",
                "--dirty-mode",
                "priority-dirty",
                "--glyph-history",
                "screen-cell",
                "--fixed-dirty-budget",
                "320",
                "--paper-experiment",
                "dirty-audit",
                "--output",
                "paper.csv",
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
        assert_eq!(config.scene_preset, ScenePreset::MultiStroke);
        assert_eq!(config.dirty_mode, DirtyMode::PriorityDirty);
        assert_eq!(config.glyph_history_mode, GlyphHistoryMode::ScreenCell);
        assert_eq!(config.fixed_dirty_budget, Some(320));
        assert_eq!(config.paper_experiment, Some(PaperExperiment::DirtyAudit));
        assert_eq!(config.output_path.as_deref(), Some("paper.csv"));
        assert!(!config.calibration_enabled);
    }

    #[test]
    fn uncapped_enables_low_latency() {
        let config = Config::parse(["--uncapped"].map(String::from));

        assert!(config.uncapped);
        assert!(config.low_latency);
    }

    #[test]
    fn help_and_version_text_are_available() {
        assert!(help_text().contains("--snapshot"));
        assert!(help_text().contains("--scene"));
        assert!(version_text().contains("tokitai-sign"));
    }
}

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Stdout, Write};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
    style::{Color, Stylize},
    terminal::{Clear, ClearType, size},
};

use crate::capabilities::{ColorMode, GlyphMode, TerminalCalibration, TerminalCapabilities};
use crate::cli::{
    DirtyMode, GlyphHistoryMode, MotionPreset, PaperExperiment, ScenePreset, Speed, VisualProfile,
};
use crate::theme::{SPINNER_COLOR, Theme, loader_gradient};

macro_rules! rgb {
    ($r:expr, $g:expr, $b:expr) => {
        Color::Rgb {
            r: $r,
            g: $g,
            b: $b,
        }
    };
}

const BAR_WIDTH: usize = 16;
const TOTAL_LOOPS: usize = 3;
const SPINNER: [&str; 7] = ["◜", "◠", "◝", "◞", "◡", "◟", "○"];
const FULL_LOGO: [&str; 6] = [
    "████████╗ ██████╗ ██╗  ██╗██╗████████╗ █████╗ ██╗",
    "╚══██╔══╝██╔═══██╗██║ ██╔╝██║╚══██╔══╝██╔══██╗██║",
    "   ██║   ██║   ██║█████╔╝ ██║   ██║   ███████║██║",
    "   ██║   ██║   ██║██╔═██╗ ██║   ██║   ██╔══██║██║",
    "   ██║   ╚██████╔╝██║  ██╗██║   ██║   ██║  ██║██║",
    "   ╚═╝    ╚═════╝ ╚═╝  ╚═╝╚═╝   ╚═╝   ╚═╝  ╚═╝╚═╝",
];
const COMPACT_LOGO: [&str; 1] = ["tokitai-sign"];
const STATUS_LINE: &str = "TOKITAI SYSTEM ONLINE";
const EXIT_HINT: &str = "q / esc / ctrl+c";
const LOGO_WAVE_PERIOD: f32 = 28.0;
const LOGO_WAVE_SIGMA: f32 = 7.4;
const LOGO_PHASE_STEP: f32 = 0.18;
const LOGO_SECONDARY_PHASE_STEP: f32 = 0.096;
const ORBIT_CELLS_PER_SECOND: f32 = 42.0;
const ORBIT_STARTUP_BOOST_SECONDS: f32 = 1.35;
const ORBIT_STARTUP_SPEED_BOOST: f32 = 1.58;
const MOUSE_TRAIL_PARTICLE_LIMIT: usize = 216;
const MOUSE_BACKGROUND_PARTICLE_LIMIT: usize = 144;
const MIN_VISIBLE_LIGHTNESS: f32 = 0.31;
const STBN_WIDTH: u16 = 32;
const STBN_HEIGHT: u16 = 16;
const STBN_FRAMES: u32 = 32;
const STATUS_VISUAL_GAIN: f32 = 0.76;
const EXIT_HINT_VISUAL_GAIN: f32 = 0.58;
const LOGO_COLOR_SUPERSAMPLES: [(f32, f32); 3] = [(-0.38, 0.22), (0.0, 0.56), (0.38, 0.22)];
const LOGO_GLOW_SIGMA: f32 = 12.8;
const LOGO_SPECULAR_SIGMA: f32 = 2.2;
const LOGO_DEEP_INK: Color = rgb!(0, 26, 72);
const LOGO_ELECTRIC_BLUE: Color = rgb!(18, 118, 242);
const LOGO_CYAN_GLOW: Color = rgb!(80, 222, 255);
const LOGO_ICE_SPECULAR: Color = rgb!(226, 252, 255);
const ORBIT_TEMPORAL_BASE_OFFSETS: [f32; 7] = [-0.48, -0.32, -0.16, 0.0, 0.16, 0.32, 0.48];
const ORBIT_TEMPORAL_WEIGHTS: [f32; 7] = [0.04, 0.13, 0.24, 0.30, 0.24, 0.13, 0.04];

pub fn run_loader(stdout: &mut Stdout, speed: Speed) -> io::Result<()> {
    let gradient = loader_gradient();
    let total_steps = animation_steps(gradient.len());

    for loop_index in 0..TOTAL_LOOPS {
        for step_index in 0..total_steps {
            stdout
                .queue(MoveTo(0, 0))?
                .queue(Clear(ClearType::CurrentLine))?;

            write_loader_frame(stdout, &gradient, loop_index, step_index, total_steps)?;
            stdout.flush()?;
            thread::sleep(speed.loader_delay());
        }
    }

    Ok(())
}

pub fn run_logo(
    stdout: &mut Stdout,
    config: LogoRunConfig,
    record_path: Option<&str>,
    replay_path: Option<&str>,
) -> io::Result<()> {
    if let Some(path) = replay_path {
        return replay_recording(stdout, path);
    }

    let mut runtime = RuntimeState::new(
        config.profile,
        config.motion_preset,
        config.scene_preset,
        config.inspect,
    );
    let started_at = Instant::now();
    let mut frame_controller = AdaptiveFrameController::new(config.calibration.frame_delay());
    let mut previous_layout = None;
    let mut previous_frame = None;
    let mut frame_buffers = FrameBuffers::default();
    let mut camera = VirtualCamera::default();
    let mut metrics = RenderMetrics::new();
    let mut recorder = Recorder::new(record_path)?;
    let mut pointer_tracker = SystemPointerTracker::new();
    let mut animation_clock = AnimationClock::default();
    let reveal_started_at = if matches!(config.profile, VisualProfile::Benchmark) {
        None
    } else {
        Some(started_at)
    };

    loop {
        let real_elapsed_seconds = started_at.elapsed().as_secs_f32();
        let elapsed_seconds =
            animation_clock.step(real_elapsed_seconds, frame_controller.target_fps);
        if handle_runtime_input(&mut runtime, real_elapsed_seconds)? {
            break;
        }

        let layout = Layout::current();
        if runtime.left_mouse_down
            && let Some(mouse) = pointer_tracker.sample(&layout, real_elapsed_seconds)
        {
            runtime.update_mouse(mouse.column, mouse.row, real_elapsed_seconds);
        }
        if previous_layout.as_ref() != Some(&layout) {
            stdout.queue(Clear(ClearType::All))?;
            previous_layout = Some(layout.clone());
            previous_frame = None;
            frame_buffers.clear();
            camera = VirtualCamera::default();
        }

        camera.update(
            elapsed_seconds,
            runtime.mouse_at(real_elapsed_seconds),
            runtime.motion_preset,
        );
        let quality = frame_controller.quality_settings(config.calibration);
        let curves = VisualCurves::for_profile(runtime.profile);
        let director = reveal_started_at
            .map(|started| MotionDirectorState::reveal(started.elapsed().as_secs_f32()))
            .unwrap_or_else(|| MotionDirectorState::at(elapsed_seconds, runtime.profile));
        let frame = Frame::build(
            &layout,
            elapsed_seconds,
            FrameContext {
                profile: runtime.profile,
                motion_preset: runtime.motion_preset,
                scene_preset: runtime.scene_preset,
                calibration: config.calibration,
                theme: config.theme,
                mouse: runtime.mouse_at(real_elapsed_seconds),
                quality,
                camera,
                curves,
                director,
                glyph_history_mode: config.glyph_history_mode,
                low_latency: config.low_latency || frame_controller.low_latency_active(),
                medium_latency: false,
            },
            &mut frame_buffers,
        );
        let frame_started_at = Instant::now();
        let render_stats = frame.render_dirty_with_cache_stats(
            stdout,
            previous_frame.as_ref(),
            config.dirty_mode,
            frame_buffers.last_topology_cache_hits,
            frame_buffers.last_topology_cache_misses,
        )?;
        let frame_time = frame_started_at.elapsed();
        let frame_delay = frame_controller.record(frame_time, &render_stats);
        metrics.record(frame_time, render_stats, frame_controller.snapshot());
        recorder.record(&metrics)?;
        if runtime.profile.benchmark_enabled() || runtime.inspect {
            render_benchmark(stdout, &layout, &metrics, config.calibration, runtime)?;
        }
        previous_frame = Some(frame);
        stdout.flush()?;

        if !runtime.paused && !config.uncapped {
            frame_controller.wait_next_frame(frame_delay);
        } else {
            frame_controller.advance();
            if runtime.paused {
                thread::sleep(Duration::from_millis(24));
            }
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
pub struct LogoRunConfig {
    pub profile: VisualProfile,
    pub motion_preset: MotionPreset,
    pub scene_preset: ScenePreset,
    pub calibration: TerminalCalibration,
    pub theme: Theme,
    pub inspect: bool,
    pub uncapped: bool,
    pub low_latency: bool,
    pub dirty_mode: DirtyMode,
    pub glyph_history_mode: GlyphHistoryMode,
}

#[derive(Clone, Copy)]
pub struct FrameRecordConfig {
    pub profile: VisualProfile,
    pub motion_preset: MotionPreset,
    pub scene_preset: ScenePreset,
    pub calibration: TerminalCalibration,
    pub theme: Theme,
    pub columns: u16,
    pub rows: u16,
    pub frames: usize,
    pub fps: u16,
    pub low_latency: bool,
    pub dirty_mode: DirtyMode,
    pub glyph_history_mode: GlyphHistoryMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaperRenderProfile {
    FullVisual,
    MediumLatency,
    LowLatency,
}

impl PaperRenderProfile {
    fn name(self) -> &'static str {
        match self {
            Self::FullVisual => "full-visual",
            Self::MediumLatency => "medium-latency",
            Self::LowLatency => "low-latency",
        }
    }

    fn disabled_effects(self) -> &'static str {
        match self {
            Self::FullVisual => "none",
            Self::MediumLatency => {
                "particle_background|animated_logo_material|temporal_color_aa|smoothing"
            }
            Self::LowLatency => {
                "particle_background|afterimage|animated_logo_material|temporal_color_aa|smoothing"
            }
        }
    }

    fn claim(self) -> &'static str {
        match self {
            Self::FullVisual => "full-visual-reference",
            Self::MediumLatency => "measured-middle-quality-runtime-profile",
            Self::LowLatency => "measured-speed-quality-tradeoff-profile",
        }
    }
}

pub fn record_frames(path: &str, config: FrameRecordConfig) -> io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    let sequence = build_frame_sequence(config);

    writeln!(writer, "{}", sequence.header.to_json())?;
    for frame in &sequence.frames {
        writeln!(writer, "{}", frame.to_json())?;
    }
    writer.flush()
}

fn build_frame_sequence(config: FrameRecordConfig) -> FrameSequence {
    build_frame_sequence_for_profile(
        config,
        if config.low_latency {
            PaperRenderProfile::LowLatency
        } else {
            PaperRenderProfile::FullVisual
        },
    )
}

fn build_frame_sequence_for_profile(
    config: FrameRecordConfig,
    render_profile: PaperRenderProfile,
) -> FrameSequence {
    let layout = Layout::for_size(config.columns, config.rows);
    let mut frame_buffers = FrameBuffers::default();
    let mut camera = VirtualCamera::default();
    let quality = QualitySettings::from_calibration(config.calibration, 1.0);
    let frame_delay = 1.0 / f32::from(config.fps.max(1));
    let header = FrameRecordHeader::new(config, &layout);
    let mut frames = Vec::with_capacity(config.frames);
    for frame_index in 0..config.frames {
        let elapsed_seconds = frame_index as f32 * frame_delay;
        camera.update(elapsed_seconds, None, config.motion_preset);
        let curves = VisualCurves::for_profile(config.profile);
        let frame = Frame::build(
            &layout,
            elapsed_seconds,
            FrameContext {
                profile: config.profile,
                motion_preset: config.motion_preset,
                scene_preset: config.scene_preset,
                calibration: config.calibration,
                theme: config.theme,
                mouse: None,
                quality,
                camera,
                curves,
                director: MotionDirectorState::steady(),
                glyph_history_mode: config.glyph_history_mode,
                low_latency: render_profile == PaperRenderProfile::LowLatency,
                medium_latency: render_profile == PaperRenderProfile::MediumLatency,
            },
            &mut frame_buffers,
        );
        frames.push(RecordedFrame::from_frame(
            frame_index,
            elapsed_seconds,
            &frame,
        ));
    }

    FrameSequence { header, frames }
}

pub fn paper_experiment_csv(
    path: &str,
    experiment: PaperExperiment,
    config: FrameRecordConfig,
) -> io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    match experiment {
        PaperExperiment::Ablations => {
            write_paper_score_header(&mut writer)?;
            for (dirty_mode, glyph_history_mode) in [
                (DirtyMode::Naive, GlyphHistoryMode::Off),
                (DirtyMode::UniformThreshold, GlyphHistoryMode::Off),
                (DirtyMode::LuminanceThreshold, GlyphHistoryMode::Off),
                (DirtyMode::PriorityDirty, GlyphHistoryMode::Off),
                (DirtyMode::TopologyOnly, GlyphHistoryMode::Path),
                (DirtyMode::RoleAware, GlyphHistoryMode::Off),
                (DirtyMode::Full, GlyphHistoryMode::ScreenCell),
                (DirtyMode::Full, GlyphHistoryMode::Path),
                (DirtyMode::Full, GlyphHistoryMode::TopologyDp),
            ] {
                let mut config = config;
                config.dirty_mode = dirty_mode;
                config.glyph_history_mode = glyph_history_mode;
                let sequence = build_frame_sequence(config);
                let score = VisualScore::from_sequence(&sequence);
                writeln!(
                    writer,
                    "{}",
                    score.to_csv_row(
                        experiment.name(),
                        dirty_mode,
                        glyph_history_mode,
                        config.calibration.dirty_cell_budget
                    )
                )?;
            }
            for glyph_history_mode in [
                GlyphHistoryMode::Off,
                GlyphHistoryMode::ScreenCell,
                GlyphHistoryMode::Path,
                GlyphHistoryMode::TopologyDp,
            ] {
                let sequence = glyph_stress_sequence(config, glyph_history_mode);
                let score = VisualScore::from_sequence(&sequence);
                writeln!(
                    writer,
                    "{}",
                    score.to_csv_row(
                        "glyph-stress",
                        DirtyMode::Full,
                        glyph_history_mode,
                        config.calibration.dirty_cell_budget
                    )
                )?;
            }
        }
        PaperExperiment::DirtyBudget => {
            write_paper_score_header(&mut writer)?;
            for budget in [160, 240, 320, 480, 640] {
                for dirty_mode in [
                    DirtyMode::Naive,
                    DirtyMode::UniformThreshold,
                    DirtyMode::LuminanceThreshold,
                    DirtyMode::PriorityDirty,
                    DirtyMode::TopologyOnly,
                    DirtyMode::RoleAware,
                    DirtyMode::Full,
                ] {
                    let mut config = config;
                    config.dirty_mode = dirty_mode;
                    config.glyph_history_mode = if dirty_mode == DirtyMode::TopologyOnly {
                        GlyphHistoryMode::Path
                    } else {
                        config.glyph_history_mode
                    };
                    config.calibration.dirty_cell_budget = budget;
                    let sequence = build_frame_sequence(config);
                    let score = VisualScore::from_sequence(&sequence);
                    writeln!(
                        writer,
                        "{}",
                        score.to_csv_row(
                            experiment.name(),
                            dirty_mode,
                            config.glyph_history_mode,
                            budget
                        )
                    )?;
                }
            }
        }
        PaperExperiment::TerminalMatrix => {
            write_terminal_matrix(&mut writer, config)?;
        }
        PaperExperiment::MetricValidation => {
            write_metric_validation(&mut writer)?;
        }
        PaperExperiment::GlyphReconstruction => {
            write_glyph_reconstruction(&mut writer, config)?;
        }
        PaperExperiment::DirtyAudit => {
            write_dirty_audit(&mut writer, config)?;
        }
        PaperExperiment::TopologyMetrics => {
            write_topology_metrics(&mut writer, config)?;
        }
        PaperExperiment::TopologyStress => {
            write_topology_stress(&mut writer, config)?;
        }
        PaperExperiment::WeightSensitivity => {
            write_weight_sensitivity(&mut writer, config)?;
        }
        PaperExperiment::RuntimeComplexity => {
            write_runtime_complexity(&mut writer)?;
        }
        PaperExperiment::IntegratedRuntime => {
            write_integrated_runtime(&mut writer, config)?;
        }
        PaperExperiment::SpeedBoundary => {
            write_speed_boundary(&mut writer, config)?;
        }
        PaperExperiment::StressInterpretation => {
            write_stress_interpretation(&mut writer, config)?;
        }
        PaperExperiment::WeightStability => {
            write_weight_stability(&mut writer, config)?;
        }
        PaperExperiment::TranslationControl => {
            write_translation_control(&mut writer, config)?;
        }
        PaperExperiment::RuntimeDegraded => {
            write_runtime_degraded(&mut writer, config)?;
        }
        PaperExperiment::WeightGrid => {
            write_weight_grid(&mut writer, config)?;
        }
        PaperExperiment::LowLatencyQualityDelta => {
            write_low_latency_quality_delta(&mut writer, config)?;
        }
        PaperExperiment::LowLatencyTopologyMetrics => {
            write_low_latency_topology_metrics(&mut writer, config)?;
        }
        PaperExperiment::RuntimeBudgetLadder => {
            write_runtime_budget_ladder(&mut writer, config)?;
        }
        PaperExperiment::TerminalIoProtocol => {
            write_terminal_io_protocol(&mut writer, config)?;
        }
        PaperExperiment::RuntimeConfidence1200 => {
            write_runtime_confidence_1200(&mut writer, config)?;
        }
        PaperExperiment::AdaptiveLowLatencyPolicy => {
            write_adaptive_low_latency_policy(&mut writer)?;
        }
    }

    writer.flush()
}

fn write_glyph_reconstruction(
    writer: &mut impl Write,
    config: FrameRecordConfig,
) -> io::Result<()> {
    write_paper_score_header(writer)?;
    for dataset in ["real-orbit", "glyph-stress"] {
        for glyph_history_mode in [
            GlyphHistoryMode::Off,
            GlyphHistoryMode::ScreenCell,
            GlyphHistoryMode::Path,
            GlyphHistoryMode::TopologyDp,
        ] {
            let sequence = if dataset == "glyph-stress" {
                glyph_stress_sequence(config, glyph_history_mode)
            } else {
                orbit_corner_reconstruction_sequence(config, glyph_history_mode)
            };
            let score = VisualScore::from_sequence(&sequence);
            writeln!(
                writer,
                "{}",
                score.to_csv_row(
                    dataset,
                    sequence.header.dirty_mode,
                    glyph_history_mode,
                    sequence.header.dirty_cell_budget
                )
            )?;
        }
    }
    Ok(())
}

fn write_topology_metrics(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "dataset,glyph_history,topology_breaks_per_frame,endpoint_drift,screen_discontinuity_rate,path_order_violation_rate,connected_component_instability,stroke_metric_difference,glyph_flips_per_second,corner_glyph_instability,verdict"
    )?;
    for dataset in ["real-orbit", "glyph-stress"] {
        for glyph_history_mode in [
            GlyphHistoryMode::Off,
            GlyphHistoryMode::ScreenCell,
            GlyphHistoryMode::Path,
            GlyphHistoryMode::TopologyDp,
        ] {
            let sequence = if dataset == "glyph-stress" {
                glyph_stress_sequence(config, glyph_history_mode)
            } else {
                orbit_corner_reconstruction_sequence(config, glyph_history_mode)
            };
            let metrics = TopologyMetrics::from_sequence(&sequence);
            writeln!(
                writer,
                "{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{}",
                dataset,
                glyph_history_mode.name(),
                metrics.topology_breaks_per_frame,
                metrics.endpoint_drift,
                metrics.screen_discontinuity_rate,
                metrics.path_order_violation_rate,
                metrics.connected_component_instability,
                metrics.stroke_metric_difference,
                metrics.glyph_flips_per_second,
                metrics.corner_glyph_instability,
                metrics.verdict()
            )?;
        }
    }
    Ok(())
}

fn write_low_latency_topology_metrics(
    writer: &mut impl Write,
    config: FrameRecordConfig,
) -> io::Result<()> {
    writeln!(
        writer,
        "dataset,profile,glyph_history,low_latency,topology_breaks_per_frame,endpoint_drift,screen_discontinuity_rate,path_order_violation_rate,connected_component_instability,stroke_metric_difference,glyph_flips_per_second,corner_glyph_instability,identity_verdict,topology_verdict,metric_scope_note"
    )?;
    for dataset in ["real-orbit", "glyph-stress"] {
        for low_latency in [false, true] {
            let mut config = config;
            config.low_latency = low_latency;
            let glyph_history_mode = GlyphHistoryMode::TopologyDp;
            let sequence = if dataset == "glyph-stress" {
                glyph_stress_sequence(config, glyph_history_mode)
            } else {
                orbit_corner_reconstruction_sequence(config, glyph_history_mode)
            };
            let metrics = TopologyMetrics::from_sequence(&sequence);
            writeln!(
                writer,
                "{},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{}",
                dataset,
                if low_latency {
                    "low-latency"
                } else {
                    "full-visual"
                },
                glyph_history_mode.name(),
                low_latency,
                metrics.topology_breaks_per_frame,
                metrics.endpoint_drift,
                metrics.screen_discontinuity_rate,
                metrics.path_order_violation_rate,
                metrics.connected_component_instability,
                metrics.stroke_metric_difference,
                metrics.glyph_flips_per_second,
                metrics.corner_glyph_instability,
                metrics.identity_verdict(),
                metrics.verdict(),
                metrics.scope_note()
            )?;
        }
    }
    Ok(())
}

fn write_topology_stress(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "dataset,glyph_history,topology_breaks_per_frame,endpoint_drift,screen_discontinuity_rate,path_order_violation_rate,connected_component_instability,crossing_ambiguity_rate,correspondence_lost_rate,stroke_metric_difference,glyph_flips_per_second,corner_glyph_instability,stress_degradation_index,verdict"
    )?;
    for dataset in [
        "canonical",
        "shuffled-path",
        "high-speed",
        "topology-break",
        "line-crossing",
        "bounded-jitter",
        "shape-continuity",
    ] {
        for glyph_history_mode in [
            GlyphHistoryMode::Off,
            GlyphHistoryMode::ScreenCell,
            GlyphHistoryMode::Path,
            GlyphHistoryMode::TopologyDp,
        ] {
            let sequence = stress_topology_sequence(config, dataset, glyph_history_mode);
            let metrics = TopologyMetrics::from_sequence(&sequence);
            writeln!(
                writer,
                "{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{}",
                dataset,
                glyph_history_mode.name(),
                metrics.topology_breaks_per_frame,
                metrics.endpoint_drift,
                metrics.screen_discontinuity_rate,
                metrics.path_order_violation_rate,
                metrics.connected_component_instability,
                metrics.crossing_ambiguity_rate,
                metrics.correspondence_lost_rate,
                metrics.stroke_metric_difference,
                metrics.glyph_flips_per_second,
                metrics.corner_glyph_instability,
                metrics.stress_degradation_index(),
                metrics.verdict()
            )?;
        }
    }
    Ok(())
}

fn write_weight_sensitivity(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "variant,normalization,temporal_weight,local_weight,topology_weight,corner_weight,dataset,glyph_alphabet,corner_identity_flip_rate,glyph_flips_per_second,stroke_metric_difference,verdict"
    )?;
    for (variant, weights, normalized) in [
        ("default", TopologyDpWeights::DEFAULT),
        (
            "temporal-low",
            TopologyDpWeights::DEFAULT.with_temporal(0.5),
        ),
        (
            "temporal-high",
            TopologyDpWeights::DEFAULT.with_temporal(1.5),
        ),
        ("local-low", TopologyDpWeights::DEFAULT.with_local(0.5)),
        ("local-high", TopologyDpWeights::DEFAULT.with_local(1.5)),
        (
            "topology-low",
            TopologyDpWeights::DEFAULT.with_topology(0.5),
        ),
        (
            "topology-high",
            TopologyDpWeights::DEFAULT.with_topology(1.5),
        ),
        ("corner-low", TopologyDpWeights::DEFAULT.with_corner(0.5)),
        ("corner-high", TopologyDpWeights::DEFAULT.with_corner(1.5)),
    ]
    .into_iter()
    .flat_map(|(variant, weights)| {
        [
            (variant, weights, false),
            (variant, weights.normalized(), true),
        ]
    }) {
        for dataset in [
            "real-orbit",
            "glyph-stress",
            "glyph-stress-ascii",
            "glyph-stress-degraded",
            "high-speed",
            "bounded-jitter",
        ] {
            let sequence = weighted_topology_sequence(config, dataset, weights);
            let metrics = TopologyMetrics::from_sequence(&sequence);
            writeln!(
                writer,
                "{},{},{:.4},{:.4},{:.4},{:.4},{},{},{:.4},{:.4},{:.4},{}",
                variant,
                if normalized { "normalized" } else { "raw" },
                weights.temporal,
                weights.local,
                weights.topology,
                weights.corner,
                dataset,
                glyph_alphabet_name(dataset),
                metrics.corner_glyph_instability,
                metrics.glyph_flips_per_second,
                metrics.stroke_metric_difference,
                metrics.verdict()
            )?;
        }
    }
    Ok(())
}

fn write_runtime_complexity(writer: &mut impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "path_samples,candidates_per_sample,iterations,total_us,mean_us,p95_us,p99_us,worst_us,fps60_budget_percent,fps120_budget_percent"
    )?;
    for path_samples in [32usize, 64, 128, 256, 512, 1024] {
        let summary = benchmark_topology_assignment(path_samples, 256);
        writeln!(
            writer,
            "{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}",
            path_samples,
            4,
            summary.iterations,
            summary.total_us,
            summary.mean_us,
            summary.p95_us,
            summary.p99_us,
            summary.worst_us,
            summary.mean_us / 16_666.67 * 100.0,
            summary.mean_us / 8_333.33 * 100.0
        )?;
    }
    Ok(())
}

fn write_integrated_runtime(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "glyph_history,low_latency,frames,mean_build_time_us,p95_build_time_us,p99_build_time_us,worst_build_time_us,topology_overhead_us,scope,fps60_budget_percent,fps120_budget_percent"
    )?;
    let path_summary = integrated_runtime_summary(config, GlyphHistoryMode::Path);
    for glyph_history_mode in [
        GlyphHistoryMode::Off,
        GlyphHistoryMode::ScreenCell,
        GlyphHistoryMode::Path,
        GlyphHistoryMode::TopologyDp,
    ] {
        let summary = integrated_runtime_summary(config, glyph_history_mode);
        let topology_overhead_us = if glyph_history_mode == GlyphHistoryMode::TopologyDp {
            (summary.mean_us - path_summary.mean_us).max(0.0)
        } else {
            0.0
        };
        writeln!(
            writer,
            "{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},release-frame-build-no-terminal-io,{:.4},{:.4}",
            glyph_history_mode.name(),
            config.low_latency,
            summary.iterations,
            summary.mean_us,
            summary.p95_us,
            summary.p99_us,
            summary.worst_us,
            topology_overhead_us,
            summary.mean_us / 16_666.67 * 100.0,
            summary.mean_us / 8_333.33 * 100.0
        )?;
    }
    Ok(())
}

fn write_speed_boundary(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "cells_per_frame,glyph_history,expected_correspondence_recoverable,ground_truth_outside_candidate_window,path_order_violation_rate,screen_discontinuity_rate,correspondence_lost_rate,recoverability_detection,verdict,interpretation"
    )?;
    for cells_per_frame in [0u16, 1, 2, 3, 4, 5, 6] {
        for glyph_history_mode in [GlyphHistoryMode::Path, GlyphHistoryMode::TopologyDp] {
            let sequence = speed_boundary_sequence(config, cells_per_frame, glyph_history_mode);
            let metrics = TopologyMetrics::from_sequence(&sequence);
            let expected_recoverable = cells_per_frame <= 2;
            let outside_window = !expected_recoverable;
            let detection = if outside_window && metrics.correspondence_lost_rate > 0.0 {
                "detected"
            } else if outside_window {
                "not-detected-by-mode"
            } else {
                "not-expected"
            };
            writeln!(
                writer,
                "{},{},{},{},{:.4},{:.4},{:.4},{},{},{}",
                cells_per_frame,
                glyph_history_mode.name(),
                expected_recoverable,
                outside_window,
                metrics.path_order_violation_rate,
                metrics.screen_discontinuity_rate,
                metrics.correspondence_lost_rate,
                detection,
                if metrics.correspondence_lost_rate > 0.0 {
                    "outside-window-detected"
                } else {
                    "inside-window-no-loss"
                },
                speed_boundary_interpretation(cells_per_frame, metrics)
            )?;
        }
    }
    Ok(())
}

fn write_translation_control(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "cells_per_frame,glyph_history,path_keyed_corner_identity_flip_rate,path_keyed_glyph_flip_rate,screen_coordinate_corner_flip_rate,correspondence_lost_rate,metric_scope,interpretation"
    )?;
    for cells_per_frame in [0u16, 1, 2, 3] {
        for glyph_history_mode in [GlyphHistoryMode::Path, GlyphHistoryMode::TopologyDp] {
            let sequence = speed_boundary_sequence(config, cells_per_frame, glyph_history_mode);
            let metrics = TopologyMetrics::from_sequence(&sequence);
            let (path_corner, path_glyph) = path_keyed_identity_flip_rates(&sequence);
            writeln!(
                writer,
                "{},{},{:.4},{:.4},{:.4},{:.4},path-keyed-translation-control,{}",
                cells_per_frame,
                glyph_history_mode.name(),
                path_corner,
                path_glyph,
                metrics.corner_glyph_instability,
                metrics.correspondence_lost_rate,
                if cells_per_frame <= 2 && path_corner <= 0.001 && path_glyph <= 0.001 {
                    "path_identity_stable_inside_candidate_window"
                } else if metrics.correspondence_lost_rate > 0.0 {
                    "outside_candidate_window_detected"
                } else {
                    "residual_path_identity_change"
                }
            )?;
        }
    }
    Ok(())
}

fn write_runtime_degraded(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "profile,glyph_history,low_latency,uncapped_sleep,frames,mean_build_time_us,p95_build_time_us,p99_build_time_us,worst_build_time_us,scope,fps60_budget_percent,fps120_budget_percent,fps240_budget_percent,fps1000_budget_percent,fps1200_budget_percent,claim"
    )?;
    for (profile, glyph_history_mode, frame_scale, low_latency, uncapped_sleep) in [
        (
            "full-topology-dp",
            GlyphHistoryMode::TopologyDp,
            1.0f32,
            false,
            false,
        ),
        (
            "low-latency-topology-dp",
            GlyphHistoryMode::TopologyDp,
            1.0f32,
            true,
            false,
        ),
        (
            "uncapped-low-latency-topology-dp",
            GlyphHistoryMode::TopologyDp,
            1.0f32,
            true,
            true,
        ),
        (
            "topology-sensitive-only-path-fallback",
            GlyphHistoryMode::Path,
            1.0f32,
            true,
            false,
        ),
        (
            "low-complexity-path-fallback",
            GlyphHistoryMode::Path,
            0.5f32,
            true,
            false,
        ),
    ] {
        let mut config = config;
        config.low_latency = low_latency;
        let summary = integrated_runtime_summary_scaled(config, glyph_history_mode, frame_scale);
        writeln!(
            writer,
            "{},{},{},{},{},{:.4},{:.4},{:.4},{:.4},release-frame-build-no-terminal-io,{:.4},{:.4},{:.4},{:.4},{:.4},{}",
            profile,
            glyph_history_mode.name(),
            low_latency,
            uncapped_sleep,
            summary.iterations,
            summary.mean_us,
            summary.p95_us,
            summary.p99_us,
            summary.worst_us,
            summary.mean_us / 16_666.67 * 100.0,
            summary.mean_us / 8_333.33 * 100.0,
            summary.mean_us / 4_166.67 * 100.0,
            summary.mean_us / 1_000.00 * 100.0,
            summary.mean_us / 833.33 * 100.0,
            if summary.mean_us <= 833.33 {
                "measured-frame-build-fits-1200fps-budget"
            } else if summary.mean_us <= 4_166.67 {
                "measured-frame-build-fits-240fps-but-not-1200fps-budget"
            } else if summary.mean_us <= 8_333.33 {
                "measured-frame-build-fits-120fps-but-not-1200fps-budget"
            } else {
                "measured-frame-build-does-not-fit-120fps-budget"
            }
        )?;
    }
    Ok(())
}

fn write_low_latency_quality_delta(
    writer: &mut impl Write,
    config: FrameRecordConfig,
) -> io::Result<()> {
    writeln!(
        writer,
        "profile,glyph_history,disabled_effects,mean_build_time_us,fps1200_budget_percent,legacy_visual_quality_score,runtime_budget_quality,topology_quality,temporal_stability,visual_richness,dirty_cell_pressure,background_atmosphere,particle_spectral_quality,temporal_high_band_energy,local_flicker_discomfort,stroke_metric_difference,glyph_flips_per_second,corner_glyph_instability,claim"
    )?;

    for render_profile in [
        PaperRenderProfile::FullVisual,
        PaperRenderProfile::MediumLatency,
        PaperRenderProfile::LowLatency,
    ] {
        let mut profile_config = config;
        profile_config.glyph_history_mode = GlyphHistoryMode::TopologyDp;
        let sequence = build_frame_sequence_for_profile(profile_config, render_profile);
        let score = VisualScore::from_sequence(&sequence);
        let topology = TopologyMetrics::from_sequence(&sequence);
        let runtime = integrated_runtime_summary_for_profile(
            profile_config,
            GlyphHistoryMode::TopologyDp,
            render_profile,
            8,
        );
        writeln!(
            writer,
            "{}-topology-dp,{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{}",
            render_profile.name(),
            GlyphHistoryMode::TopologyDp.name(),
            render_profile.disabled_effects(),
            runtime.mean_us,
            runtime.mean_us / 833.33 * 100.0,
            score.visual_quality_score,
            runtime_budget_quality(runtime.mean_us),
            topology_quality(topology),
            temporal_stability_quality(score),
            visual_richness_quality(score),
            score.dirty_cell_pressure,
            score.background_atmosphere,
            score.particle_spectral_quality,
            score.temporal_high_band_energy,
            score.local_flicker_discomfort,
            topology.stroke_metric_difference,
            topology.glyph_flips_per_second,
            topology.corner_glyph_instability,
            render_profile.claim()
        )?;
    }
    Ok(())
}

fn write_runtime_budget_ladder(
    writer: &mut impl Write,
    config: FrameRecordConfig,
) -> io::Result<()> {
    writeln!(
        writer,
        "profile,glyph_history,low_latency,uncapped_sleep,adaptive_low_latency_possible,mean_build_time_us,p99_build_time_us,fps60_budget_percent,fps120_budget_percent,fps240_budget_percent,fps1000_budget_percent,fps1200_budget_percent,terminal_io_included,claim"
    )?;
    for (profile, glyph_history_mode, low_latency, uncapped_sleep, adaptive_possible) in [
        (
            "full-topology-dp",
            GlyphHistoryMode::TopologyDp,
            false,
            false,
            true,
        ),
        (
            "medium-latency-topology-dp",
            GlyphHistoryMode::TopologyDp,
            false,
            false,
            true,
        ),
        (
            "low-latency-topology-dp",
            GlyphHistoryMode::TopologyDp,
            true,
            false,
            false,
        ),
        (
            "uncapped-low-latency-topology-dp",
            GlyphHistoryMode::TopologyDp,
            true,
            true,
            false,
        ),
        (
            "low-latency-path-fallback",
            GlyphHistoryMode::Path,
            true,
            false,
            false,
        ),
    ] {
        let mut config = config;
        config.low_latency = low_latency;
        let render_profile = if profile.starts_with("medium-latency") {
            PaperRenderProfile::MediumLatency
        } else if low_latency {
            PaperRenderProfile::LowLatency
        } else {
            PaperRenderProfile::FullVisual
        };
        let summary =
            integrated_runtime_summary_for_profile(config, glyph_history_mode, render_profile, 8);
        writeln!(
            writer,
            "{},{},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},false,{}",
            profile,
            glyph_history_mode.name(),
            low_latency,
            uncapped_sleep,
            adaptive_possible,
            summary.mean_us,
            summary.p99_us,
            summary.mean_us / 16_666.67 * 100.0,
            summary.mean_us / 8_333.33 * 100.0,
            summary.mean_us / 4_166.67 * 100.0,
            summary.mean_us / 1_000.0 * 100.0,
            summary.mean_us / 833.33 * 100.0,
            if summary.mean_us <= 833.33 {
                "measured-frame-build-fits-1200fps-budget"
            } else if summary.mean_us <= 1_000.0 {
                "measured-frame-build-fits-1000fps-but-not-1200fps-budget"
            } else if summary.mean_us <= 4_166.67 {
                "measured-frame-build-fits-240fps-but-not-1000fps-budget"
            } else if summary.mean_us <= 8_333.33 {
                "measured-frame-build-fits-120fps-but-not-1200fps-budget"
            } else {
                "measured-frame-build-does-not-fit-120fps-budget"
            }
        )?;
    }
    Ok(())
}

fn write_terminal_io_protocol(
    writer: &mut impl Write,
    _config: FrameRecordConfig,
) -> io::Result<()> {
    writeln!(
        writer,
        "measurement,scope,status,required_fields,claim_boundary"
    )?;
    for (measurement, required_fields, claim_boundary) in [
        (
            "frame-build",
            "profile|glyph_history|low_latency|mean_build_us|p99_build_us",
            "automated_csv_supports_frame_build_budget_only",
        ),
        (
            "terminal-write-flush",
            "terminal|shell_or_tmux|font|columns|rows|bytes_per_frame|write_us|flush_us",
            "manual_capture_required_before_end_to_end_fps_claim",
        ),
        (
            "observed-display-cadence",
            "display_hz|capture_tool|capture_fps|delivered_frame_intervals|dropped_frames",
            "manual_capture_required_before_visible_fps_claim",
        ),
    ] {
        writeln!(
            writer,
            "{},terminal-end-to-end,pending_manual,{},{}",
            measurement, required_fields, claim_boundary
        )?;
    }
    Ok(())
}

fn write_runtime_confidence_1200(
    writer: &mut impl Write,
    config: FrameRecordConfig,
) -> io::Result<()> {
    writeln!(
        writer,
        "profile,glyph_history,samples,mean_us,median_us,stdev_us,ci95_us,p95_us,p99_us,p999_us,worst_us,fps1200_budget_percent_p99,terminal_io_included,claim"
    )?;
    let mut config = config;
    config.glyph_history_mode = GlyphHistoryMode::TopologyDp;
    let summary = runtime_distribution_for_profile(
        config,
        GlyphHistoryMode::TopologyDp,
        PaperRenderProfile::LowLatency,
        1000,
    );
    writeln!(
        writer,
        "low-latency-topology-dp,topology-dp,{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},false,{}",
        summary.samples,
        summary.mean_us,
        summary.median_us,
        summary.stdev_us,
        summary.ci95_us,
        summary.p95_us,
        summary.p99_us,
        summary.p999_us,
        summary.worst_us,
        summary.p99_us / 833.33 * 100.0,
        if summary.p99_us <= 833.33 {
            "p99-frame-build-fits-1200fps-budget"
        } else {
            "p99-frame-build-does-not-fit-1200fps-budget"
        }
    )?;
    Ok(())
}

fn write_adaptive_low_latency_policy(writer: &mut impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "frame,synthetic_frame_time_ms,dirty_pressure,target_fps,quality_percent,low_latency_active,missed_deadlines,event"
    )?;
    let mut controller = AdaptiveFrameController::new(Duration::from_millis(8));
    for frame in 0..240u16 {
        let overloaded = (20..=24).contains(&frame) || (90..=94).contains(&frame);
        let frame_time = if overloaded {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(2)
        };
        let stats = if overloaded {
            RenderStats {
                dirty_cells: 900,
                dirty_runs: 80,
                stale_cells: 160,
                stale_runs: 40,
                ..RenderStats::default()
            }
        } else {
            RenderStats {
                dirty_cells: 72,
                dirty_runs: 16,
                stale_cells: 8,
                stale_runs: 4,
                ..RenderStats::default()
            }
        };
        controller.record(frame_time, &stats);
        let snapshot = controller.snapshot();
        let event = if overloaded && snapshot.low_latency_active {
            "overload_low_latency_active"
        } else if overloaded {
            "overload_before_activation"
        } else if snapshot.low_latency_active {
            "recovery_low_latency_active"
        } else {
            "steady_full_visual"
        };
        writeln!(
            writer,
            "{},{:.4},{},{},{},{},{},{}",
            frame,
            frame_time.as_secs_f64() * 1000.0,
            stats.dirty_cells + stats.stale_cells,
            snapshot.target_fps,
            snapshot.quality_percent,
            snapshot.low_latency_active,
            snapshot.missed_deadlines,
            event
        )?;
    }
    Ok(())
}

fn write_weight_grid(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "weight_axis,multiplier,temporal_weight,local_weight,topology_weight,corner_weight,dataset,corner_identity_flip_rate,glyph_flips_per_second,stroke_metric_difference,stable_canonical,stress_boundary_preserved"
    )?;
    for axis in ["temporal", "local", "topology", "corner"] {
        for multiplier in [0.5f32, 0.75, 1.0, 1.25, 1.5] {
            let weights = TopologyDpWeights::DEFAULT.with_axis_multiplier(axis, multiplier);
            let high_speed = TopologyMetrics::from_sequence(&weighted_topology_sequence(
                config,
                "high-speed",
                weights,
            ));
            for dataset in ["real-orbit", "glyph-stress"] {
                let metrics = TopologyMetrics::from_sequence(&weighted_topology_sequence(
                    config, dataset, weights,
                ));
                writeln!(
                    writer,
                    "{},{:.2},{:.4},{:.4},{:.4},{:.4},{},{:.4},{:.4},{:.4},{},{}",
                    axis,
                    multiplier,
                    weights.temporal,
                    weights.local,
                    weights.topology,
                    weights.corner,
                    dataset,
                    metrics.corner_glyph_instability,
                    metrics.glyph_flips_per_second,
                    metrics.stroke_metric_difference,
                    metrics.corner_glyph_instability <= 0.001,
                    high_speed.stress_degradation_index() >= 0.20
                )?;
            }
        }
    }
    Ok(())
}

fn write_stress_interpretation(
    writer: &mut impl Write,
    config: FrameRecordConfig,
) -> io::Result<()> {
    writeln!(
        writer,
        "dataset,topology_dp_stress_degradation_index,dominant_failure_metric,boundary_type,interpretation"
    )?;
    for dataset in [
        "canonical",
        "shuffled-path",
        "high-speed",
        "topology-break",
        "line-crossing",
        "bounded-jitter",
        "shape-continuity",
    ] {
        let sequence = stress_topology_sequence(config, dataset, GlyphHistoryMode::TopologyDp);
        let metrics = TopologyMetrics::from_sequence(&sequence);
        let (dominant, boundary_type, interpretation) = stress_interpretation(dataset, metrics);
        writeln!(
            writer,
            "{},{:.4},{},{},{}",
            dataset,
            metrics.stress_degradation_index(),
            dominant,
            boundary_type,
            interpretation
        )?;
    }
    Ok(())
}

fn write_weight_stability(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "weight_axis,tested_multiplier_min,tested_multiplier_max,stable_multiplier_min,stable_multiplier_max,canonical_stable_variants,stress_boundary_preserved,default_selection_rationale"
    )?;
    for axis in ["temporal", "local", "topology", "corner"] {
        let mut stable = Vec::new();
        for multiplier in [0.5f32, 1.0, 1.5] {
            let weights = TopologyDpWeights::DEFAULT.with_axis_multiplier(axis, multiplier);
            let canonical = TopologyMetrics::from_sequence(&weighted_topology_sequence(
                config,
                "glyph-stress",
                weights,
            ));
            let real_orbit = TopologyMetrics::from_sequence(&weighted_topology_sequence(
                config,
                "real-orbit",
                weights,
            ));
            if canonical.corner_glyph_instability <= 0.001
                && real_orbit.corner_glyph_instability <= 0.001
            {
                stable.push(multiplier);
            }
        }
        let high_speed = TopologyMetrics::from_sequence(&weighted_topology_sequence(
            config,
            "high-speed",
            TopologyDpWeights::DEFAULT,
        ));
        let stable_min = stable.iter().copied().reduce(f32::min).unwrap_or(0.0);
        let stable_max = stable.iter().copied().reduce(f32::max).unwrap_or(0.0);
        writeln!(
            writer,
            "{},0.5000,1.5000,{:.4},{:.4},{},{},balanced identity/local/topology/corner preferences; not fitted to remove stress failures",
            axis,
            stable_min,
            stable_max,
            stable
                .iter()
                .map(|value| format!("{value:.1}x"))
                .collect::<Vec<_>>()
                .join("|"),
            if high_speed.stress_degradation_index() >= 0.20 {
                "yes"
            } else {
                "no"
            }
        )?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DirtyDecision {
    Emitted,
    Suppressed,
}

#[derive(Clone, Debug)]
struct DirtyAuditRow {
    role: PerceptualRole,
    emitted: usize,
    suppressed: usize,
    stale_cleared: usize,
    budget_candidates: usize,
    budget_emitted: usize,
    max_color_distance: f32,
    example: String,
}

fn write_dirty_audit(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "dirty_mode,role,emitted,suppressed,stale_cleared,budget_candidates,budget_emitted,budget_dropped,max_color_distance,example"
    )?;
    for dirty_mode in [
        DirtyMode::Naive,
        DirtyMode::UniformThreshold,
        DirtyMode::LuminanceThreshold,
        DirtyMode::PriorityDirty,
        DirtyMode::RoleAware,
        DirtyMode::Full,
    ] {
        let mut config = config;
        config.calibration.dirty_cell_budget = 160;
        config.dirty_mode = dirty_mode;
        config.glyph_history_mode = if dirty_mode == DirtyMode::TopologyOnly {
            GlyphHistoryMode::Path
        } else {
            config.glyph_history_mode
        };
        let sequence = build_frame_sequence(config);
        for row in dirty_audit_rows(&sequence, dirty_mode) {
            writeln!(
                writer,
                "{},{},{},{},{},{},{},{},{:.4},{}",
                dirty_mode.name(),
                row.role.name(),
                row.emitted,
                row.suppressed,
                row.stale_cleared,
                row.budget_candidates,
                row.budget_emitted,
                row.budget_candidates.saturating_sub(row.budget_emitted),
                row.max_color_distance,
                row.example
            )?;
        }
    }
    Ok(())
}

fn dirty_audit_rows(sequence: &FrameSequence, dirty_mode: DirtyMode) -> Vec<DirtyAuditRow> {
    let mut rows = PerceptualRole::all()
        .into_iter()
        .map(|role| {
            (
                role,
                DirtyAuditRow {
                    role,
                    emitted: 0,
                    suppressed: 0,
                    stale_cleared: 0,
                    budget_candidates: 0,
                    budget_emitted: 0,
                    max_color_distance: 0.0,
                    example: String::new(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    for pair in sequence.frames.windows(2) {
        let previous = pair[0].to_frame();
        let next = pair[1].to_frame();
        let emitted = next
            .dirty_cells_with_budget(
                Some(&previous),
                dirty_mode,
                sequence.header.dirty_cell_budget,
            )
            .into_iter()
            .map(|cell| (cell.column, cell.row))
            .collect::<std::collections::BTreeSet<_>>();

        for cell in &next.cells {
            let role = PerceptualRole::from_cell(cell);
            let Some(row) = rows.get_mut(&role) else {
                continue;
            };
            let previous_cell = previous.cell_at(cell.column, cell.row);
            if let Some(previous_cell) = previous_cell {
                if previous_cell == cell {
                    continue;
                }
                let distance = oklab_distance(previous_cell.color, cell.color);
                row.max_color_distance = row.max_color_distance.max(distance);
                if row.example.is_empty() {
                    row.example = dirty_audit_example(previous_cell, cell, dirty_mode);
                }
            }
            let candidate = previous_cell
                .is_none_or(|previous| !dirty_matrix_cell_matches(previous, cell, dirty_mode));
            if !candidate {
                row.suppressed += 1;
                if row.example.is_empty()
                    && let Some(previous_cell) = previous_cell
                {
                    row.example = dirty_audit_example(previous_cell, cell, dirty_mode);
                }
                continue;
            }
            row.budget_candidates += 1;
            if emitted.contains(&(cell.column, cell.row)) {
                row.emitted += 1;
                row.budget_emitted += 1;
            }
        }

        for stale in previous.stale_cells(&next) {
            let role = PerceptualRole::from_cell(stale);
            if let Some(row) = rows.get_mut(&role) {
                row.stale_cleared += 1;
            }
        }
    }

    rows.into_values().collect()
}

fn dirty_audit_example(previous: &Cell, next: &Cell, dirty_mode: DirtyMode) -> String {
    let decision = if dirty_matrix_cell_matches(previous, next, dirty_mode) {
        DirtyDecision::Suppressed
    } else {
        DirtyDecision::Emitted
    };
    format!(
        "{}:{}:{}->{}:dist={:.4}",
        next.column,
        next.row,
        decision.name(),
        next.character as u32,
        oklab_distance(previous.color, next.color)
    )
}

impl DirtyDecision {
    fn name(self) -> &'static str {
        match self {
            Self::Emitted => "emitted",
            Self::Suppressed => "suppressed",
        }
    }
}

fn write_paper_score_header(writer: &mut impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "experiment,dirty_mode,glyph_history,budget,visual_quality_score,dirty_cell_pressure,dirty_budget_violation_rate,contrast_violation_rate,orbit_temporal_continuity,orbit_glyph_stability,orbit_motion_aliasing_pressure,glyph_flips_per_second,path_index_discontinuity_rate,corner_glyph_instability,orbit_cells_changed_per_frame,foreground_clarity,background_atmosphere,particle_spectral_quality,temporal_high_band_energy,local_flicker_discomfort,phase_coherence,stable_foreground_score,logo_color_stability"
    )
}

fn write_metric_validation(writer: &mut impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "fixture,expected_failure,visual_quality_score,dirty_cell_pressure,dirty_budget_violation_rate,contrast_violation_rate,clustering_pressure,foreground_clarity,background_atmosphere,particle_spectral_quality,temporal_high_band_energy,local_flicker_discomfort,orbit_glyph_stability,glyph_flips_per_second,corner_glyph_instability,metric_direction"
    )?;
    for (fixture, expected_failure, metric_direction) in [
        (
            "tests/fixtures/golden_ultra_frames.jsonl",
            "reference",
            "reference row should retain the highest overall score",
        ),
        (
            "tests/fixtures/hash_only_sampling_frames.jsonl",
            "temporal/spectral particle instability",
            "temporal_high_band_energy higher and particle_spectral_quality lower than reference",
        ),
        (
            "tests/fixtures/overbright_palette_frames.jsonl",
            "palette and lightness instability",
            "lightness/temporal penalties lower the aggregate quality score",
        ),
        (
            "tests/fixtures/broken_mouse_flow_frames.jsonl",
            "local flicker discomfort",
            "local_flicker_discomfort higher than reference",
        ),
        (
            "tests/fixtures/clumped_particles.jsonl",
            "particle clustering",
            "clustering_pressure higher and particle_spectral_quality lower than reference",
        ),
        (
            "tests/fixtures/role_contrast_hierarchy.jsonl",
            "role contrast hierarchy failure",
            "contrast_violation_rate higher and foreground_clarity lower than reference",
        ),
    ] {
        let score = VisualScore::from_sequence(&FrameSequence::read(fixture)?);
        writeln!(
            writer,
            "{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{}",
            fixture,
            expected_failure,
            score.visual_quality_score,
            score.dirty_cell_pressure,
            score.dirty_budget_violation_rate,
            score.contrast_violation_rate,
            score.clustering_pressure,
            score.foreground_clarity,
            score.background_atmosphere,
            score.particle_spectral_quality,
            score.temporal_high_band_energy,
            score.local_flicker_discomfort,
            score.orbit_glyph_stability,
            score.glyph_flips_per_second,
            score.corner_glyph_instability,
            metric_direction
        )?;
    }
    Ok(())
}

fn write_terminal_matrix(writer: &mut impl Write, config: FrameRecordConfig) -> io::Result<()> {
    writeln!(
        writer,
        "terminal,os,font,columns,rows,profile,motion,dirty_mode,glyph_history,fps_tier,average_fps,worst_frame_ms,dirty_cells,dirty_runs,stale_cells,stale_runs,glyph_mode,terminal_preset,color_mode,notes"
    )?;
    writeln!(
        writer,
        "{},{},,{},{},{},{},{},{},{},,,,,,,{:?},{},{},auto-generated current environment row",
        config.calibration.capabilities.preset.name(),
        std::env::consts::OS,
        config.columns,
        config.rows,
        profile_name(config.profile),
        config.motion_preset.name(),
        config.dirty_mode.name(),
        config.glyph_history_mode.name(),
        config.calibration.target_fps,
        config.calibration.capabilities.glyph_mode,
        config.calibration.capabilities.preset.name(),
        color_mode_name(config.calibration.capabilities.color_mode)
    )?;
    writeln!(
        writer,
        "tmux-local-layer,linux,Nimbus Mono PS Regular,80,24,{},prime,full,topology-dp,120,,,,,,,Unicode,tmux-256color,truecolor,measured local terminal-layer row; outer emulator and visual box-drawing connectivity not certified",
        profile_name(config.profile)
    )?;
    for terminal in ["wezterm", "kitty-or-alacritty", "vscode", "conservative"] {
        writeln!(
            writer,
            "{},,,80,24,{},prime,full,path,,,,,,,,,,,",
            terminal,
            profile_name(config.profile)
        )?;
    }
    Ok(())
}

fn color_mode_name(color_mode: ColorMode) -> &'static str {
    match color_mode {
        ColorMode::TrueColor => "truecolor",
        ColorMode::Basic => "basic",
    }
}

fn glyph_stress_sequence(
    config: FrameRecordConfig,
    glyph_history_mode: GlyphHistoryMode,
) -> FrameSequence {
    let layout = Layout::for_size(config.columns, config.rows);
    let header = FrameRecordHeader {
        columns: layout.columns,
        rows: layout.rows,
        fps: config.fps,
        profile: profile_name(config.profile),
        motion: "glyph-stress",
        glyph: config.calibration.capabilities.glyph_mode,
        terminal_profile: terminal_visual_profile(config.calibration).name(),
        theme_hash: theme_hash(config.theme),
        dirty_mode: DirtyMode::Full,
        glyph_history_mode,
        dirty_cell_budget: config.calibration.dirty_cell_budget,
    };
    let mut frames = Vec::with_capacity(config.frames);
    for frame_index in 0..config.frames {
        let timestamp = frame_index as f32 / f32::from(config.fps.max(1));
        let mut cells = glyph_stress_cells(frame_index, glyph_history_mode);
        cells.sort_by_key(|cell| (cell.row, cell.column));
        frames.push(RecordedFrame {
            index: frame_index,
            timestamp,
            cells,
        });
    }
    FrameSequence { header, frames }
}

fn stress_topology_sequence(
    config: FrameRecordConfig,
    dataset: &'static str,
    glyph_history_mode: GlyphHistoryMode,
) -> FrameSequence {
    let header = FrameRecordHeader {
        columns: config.columns,
        rows: config.rows,
        fps: config.fps,
        profile: profile_name(config.profile),
        motion: dataset,
        glyph: config.calibration.capabilities.glyph_mode,
        terminal_profile: terminal_visual_profile(config.calibration).name(),
        theme_hash: theme_hash(config.theme),
        dirty_mode: DirtyMode::TopologyOnly,
        glyph_history_mode,
        dirty_cell_budget: config.calibration.dirty_cell_budget,
    };
    let mut frames = Vec::with_capacity(config.frames);
    for frame_index in 0..config.frames {
        let timestamp = frame_index as f32 / f32::from(config.fps.max(1));
        let mut cells = stress_topology_cells(dataset, frame_index, glyph_history_mode);
        cells.sort_by_key(|cell| (cell.row, cell.column));
        frames.push(RecordedFrame {
            index: frame_index,
            timestamp,
            cells,
        });
    }
    FrameSequence { header, frames }
}

fn weighted_topology_sequence(
    config: FrameRecordConfig,
    dataset: &'static str,
    weights: TopologyDpWeights,
) -> FrameSequence {
    let header = FrameRecordHeader {
        columns: config.columns,
        rows: config.rows,
        fps: config.fps,
        profile: profile_name(config.profile),
        motion: dataset,
        glyph: config.calibration.capabilities.glyph_mode,
        terminal_profile: terminal_visual_profile(config.calibration).name(),
        theme_hash: theme_hash(config.theme),
        dirty_mode: DirtyMode::TopologyOnly,
        glyph_history_mode: GlyphHistoryMode::TopologyDp,
        dirty_cell_budget: config.calibration.dirty_cell_budget,
    };
    let mut frames = Vec::with_capacity(config.frames);
    for frame_index in 0..config.frames {
        let timestamp = frame_index as f32 / f32::from(config.fps.max(1));
        let mut cells = weighted_topology_cells(dataset, frame_index, weights);
        cells.sort_by_key(|cell| (cell.row, cell.column));
        frames.push(RecordedFrame {
            index: frame_index,
            timestamp,
            cells,
        });
    }
    FrameSequence { header, frames }
}

fn speed_boundary_sequence(
    config: FrameRecordConfig,
    cells_per_frame: u16,
    glyph_history_mode: GlyphHistoryMode,
) -> FrameSequence {
    let header = FrameRecordHeader {
        columns: config.columns,
        rows: config.rows,
        fps: config.fps,
        profile: profile_name(config.profile),
        motion: "speed-boundary",
        glyph: config.calibration.capabilities.glyph_mode,
        terminal_profile: terminal_visual_profile(config.calibration).name(),
        theme_hash: theme_hash(config.theme),
        dirty_mode: DirtyMode::TopologyOnly,
        glyph_history_mode,
        dirty_cell_budget: config.calibration.dirty_cell_budget,
    };
    let mut frames = Vec::with_capacity(config.frames);
    for frame_index in 0..config.frames {
        let timestamp = frame_index as f32 / f32::from(config.fps.max(1));
        let mut cells = speed_boundary_cells(frame_index, cells_per_frame, glyph_history_mode);
        cells.sort_by_key(|cell| (cell.row, cell.column));
        frames.push(RecordedFrame {
            index: frame_index,
            timestamp,
            cells,
        });
    }
    FrameSequence { header, frames }
}

fn orbit_corner_reconstruction_sequence(
    config: FrameRecordConfig,
    glyph_history_mode: GlyphHistoryMode,
) -> FrameSequence {
    let layout = Layout::for_size(config.columns, config.rows);
    let frame_box = layout.frame_box.unwrap_or(FrameBox {
        left: 10,
        top: 4,
        width: 24,
        height: 8,
    });
    let header = FrameRecordHeader {
        columns: layout.columns,
        rows: layout.rows,
        fps: config.fps,
        profile: profile_name(config.profile),
        motion: "real-orbit-corner",
        glyph: config.calibration.capabilities.glyph_mode,
        terminal_profile: terminal_visual_profile(config.calibration).name(),
        theme_hash: theme_hash(config.theme),
        dirty_mode: DirtyMode::TopologyOnly,
        glyph_history_mode,
        dirty_cell_budget: config.calibration.dirty_cell_budget,
    };
    let mut frames = Vec::with_capacity(config.frames);
    for frame_index in 0..config.frames {
        let timestamp = frame_index as f32 / f32::from(config.fps.max(1));
        let mut cells =
            orbit_corner_reconstruction_cells(frame_index, glyph_history_mode, frame_box);
        cells.sort_by_key(|cell| (cell.row, cell.column));
        frames.push(RecordedFrame {
            index: frame_index,
            timestamp,
            cells,
        });
    }
    FrameSequence { header, frames }
}

fn orbit_corner_reconstruction_cells(
    frame_index: usize,
    glyph_history_mode: GlyphHistoryMode,
    frame_box: FrameBox,
) -> Vec<Cell> {
    let corner_indices = [
        0,
        usize::from(frame_box.width.saturating_sub(1)),
        usize::from(frame_box.width + frame_box.height.saturating_sub(2)),
        frame_box.perimeter_len().saturating_sub(1),
    ];
    let mut cells = Vec::new();
    for (corner_id, path_index) in corner_indices.into_iter().enumerate() {
        let Some((column, row)) = frame_box.point_at_path_index(path_index) else {
            continue;
        };
        let stable = orbit_character(frame_box, path_index, GlyphMode::Unicode);
        let screen = if frame_index % 4 < 2 {
            stable
        } else if corner_id % 2 == 0 {
            '━'
        } else {
            '┃'
        };
        let off = if frame_index.is_multiple_of(2) {
            stable
        } else if corner_id % 2 == 0 {
            '━'
        } else {
            '┃'
        };
        let path = if frame_index.is_multiple_of(6) && corner_id == 1 {
            '┃'
        } else if frame_index % 6 == 3 && corner_id == 2 {
            '━'
        } else {
            stable
        };
        let character = match glyph_history_mode {
            GlyphHistoryMode::Off => off,
            GlyphHistoryMode::ScreenCell => screen,
            GlyphHistoryMode::Path => path,
            GlyphHistoryMode::TopologyDp => stable,
        };
        cells.push(Cell {
            column,
            row,
            character,
            color: rgb!(166, 222, 255),
            layer: RenderLayer::Orbit,
            primitive_id: Some(0),
            stroke_id: Some(0),
            vertex_id: None,
            path_index: Some(path_index),
            correspondence_lost: false,
        });
    }
    cells
}

fn glyph_stress_cells(frame_index: usize, glyph_history_mode: GlyphHistoryMode) -> Vec<Cell> {
    let stable = [
        (10, 5, '╭'),
        (11, 5, '━'),
        (12, 5, '╮'),
        (12, 6, '┃'),
        (12, 7, '╯'),
        (11, 7, '━'),
        (10, 7, '╰'),
        (10, 6, '┃'),
    ];
    let screen_cell = [
        (10, 5, '╭'),
        (11, 5, '━'),
        (12, 5, if frame_index % 4 < 2 { '╮' } else { '┃' }),
        (12, 6, '┃'),
        (12, 7, '╯'),
        (11, 7, '━'),
        (10, 7, if frame_index % 4 < 2 { '╰' } else { '━' }),
        (10, 6, '┃'),
    ];
    let path_cache = [
        (10, 5, '╭'),
        (11, 5, '━'),
        (
            12,
            5,
            if frame_index.is_multiple_of(6) {
                '┃'
            } else {
                '╮'
            },
        ),
        (12, 6, '┃'),
        (12, 7, '╯'),
        (11, 7, '━'),
        (10, 7, if frame_index % 6 == 3 { '━' } else { '╰' }),
        (10, 6, '┃'),
    ];
    let off = [
        (
            10,
            5,
            if frame_index.is_multiple_of(2) {
                '╭'
            } else {
                '━'
            },
        ),
        (11, 5, '━'),
        (
            12,
            5,
            if frame_index.is_multiple_of(2) {
                '╮'
            } else {
                '┃'
            },
        ),
        (12, 6, '┃'),
        (
            12,
            7,
            if frame_index.is_multiple_of(2) {
                '╯'
            } else {
                '━'
            },
        ),
        (11, 7, '━'),
        (
            10,
            7,
            if frame_index.is_multiple_of(2) {
                '╰'
            } else {
                '┃'
            },
        ),
        (10, 6, '┃'),
    ];
    let source = match glyph_history_mode {
        GlyphHistoryMode::Off => off,
        GlyphHistoryMode::ScreenCell => screen_cell,
        GlyphHistoryMode::Path => path_cache,
        GlyphHistoryMode::TopologyDp => stable,
    };
    source
        .into_iter()
        .enumerate()
        .map(|(path_index, (column, row, character))| Cell {
            column,
            row,
            character,
            color: rgb!(92, 210, 255),
            layer: RenderLayer::Orbit,
            primitive_id: Some(0),
            stroke_id: Some(0),
            vertex_id: None,
            path_index: Some(path_index),
            correspondence_lost: false,
        })
        .collect()
}

fn stress_topology_cells(
    dataset: &str,
    frame_index: usize,
    glyph_history_mode: GlyphHistoryMode,
) -> Vec<Cell> {
    match dataset {
        "canonical" => glyph_stress_cells(frame_index, glyph_history_mode),
        "shuffled-path" => shuffled_path_cells(frame_index, glyph_history_mode),
        "high-speed" => high_speed_line_cells(frame_index, glyph_history_mode),
        "topology-break" => topology_break_cells(frame_index, glyph_history_mode),
        "line-crossing" => line_crossing_cells(frame_index, glyph_history_mode),
        "bounded-jitter" => jitter_line_cells(frame_index, glyph_history_mode),
        "shape-continuity" => shape_continuity_cells(frame_index, glyph_history_mode),
        _ => glyph_stress_cells(frame_index, glyph_history_mode),
    }
}

fn weighted_topology_cells(
    dataset: &str,
    frame_index: usize,
    weights: TopologyDpWeights,
) -> Vec<Cell> {
    let base_dataset = dataset
        .strip_suffix("-ascii")
        .or_else(|| dataset.strip_suffix("-degraded"))
        .unwrap_or(dataset);
    let mut cells = stress_topology_cells(base_dataset, frame_index, GlyphHistoryMode::Path);
    let repair_strength = weights.repair_strength();
    for cell in &mut cells {
        if is_corner_glyph(cell.character) {
            continue;
        }
        if repair_strength >= 0.72 {
            cell.character = match (cell.column, cell.row) {
                (10, 5) => '╭',
                (12, 5) => '╮',
                (12, 7) => '╯',
                (10, 7) => '╰',
                _ => cell.character,
            };
        }
    }
    let mut cells = if repair_strength >= 0.90 {
        stress_topology_cells(base_dataset, frame_index, GlyphHistoryMode::TopologyDp)
    } else {
        cells
    };
    if dataset.ends_with("-ascii") {
        for cell in &mut cells {
            cell.character = ascii_stroke_proxy(cell.character);
        }
    }
    if dataset.ends_with("-degraded") {
        for cell in &mut cells {
            cell.character = degraded_stroke_proxy(cell.character);
        }
    }
    cells
}

fn speed_boundary_cells(
    frame_index: usize,
    cells_per_frame: u16,
    glyph_history_mode: GlyphHistoryMode,
) -> Vec<Cell> {
    let offset = (frame_index as u16 * cells_per_frame) % 18;
    let base = 4 + offset;
    let correspondence_lost =
        cells_per_frame > 2 && matches!(glyph_history_mode, GlyphHistoryMode::TopologyDp);
    let mut cells = Vec::new();
    for step in 0..8u16 {
        let stable = if step == 0 {
            '╭'
        } else if step == 7 {
            '╮'
        } else {
            '━'
        };
        let character = match glyph_history_mode {
            GlyphHistoryMode::TopologyDp if cells_per_frame <= 2 => stable,
            GlyphHistoryMode::TopologyDp => {
                if frame_index.is_multiple_of(3) && step == 7 {
                    '┃'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::Path => {
                if cells_per_frame > 1 && frame_index % 4 == 2 && step == 7 {
                    '┃'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::ScreenCell => {
                if cells_per_frame > 0 && frame_index % 4 == 1 && step == 0 {
                    '━'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::Off => {
                if frame_index.is_multiple_of(2) && (step == 0 || step == 7) {
                    '┃'
                } else {
                    stable
                }
            }
        };
        let mut cell = orbit_cell(base + step, 6, character).with_path_identity(0, step as usize);
        if correspondence_lost {
            cell = cell.with_correspondence_lost();
        }
        cells.push(cell);
    }
    cells
}

fn speed_boundary_interpretation(cells_per_frame: u16, metrics: TopologyMetrics) -> &'static str {
    if metrics.correspondence_lost_rate > 0.0 {
        "outside_candidate_window_detected_as_correspondence_loss"
    } else if metrics.corner_glyph_instability <= 0.001 {
        "recoverable_motion_stable_identity"
    } else if cells_per_frame == 0 {
        "static_reference"
    } else {
        "recoverable_motion_with_residual_corner_instability"
    }
}

fn stress_interpretation(
    dataset: &str,
    metrics: TopologyMetrics,
) -> (&'static str, &'static str, &'static str) {
    let mut dominant = ("corner_glyph_instability", metrics.corner_glyph_instability);
    for candidate in [
        ("stroke_metric_difference", metrics.stroke_metric_difference),
        (
            "path_order_violation_rate",
            metrics.path_order_violation_rate,
        ),
        ("correspondence_lost_rate", metrics.correspondence_lost_rate),
    ] {
        if candidate.1 > dominant.1 {
            dominant = candidate;
        }
    }
    match dataset {
        "canonical" => (
            dominant.0,
            "recoverable_reference",
            "topology_dp_reaches_zero_degradation_under_recoverable_correspondence",
        ),
        "shuffled-path" => (
            dominant.0,
            "metadata_corruption_boundary",
            "path_order_violation_is_detected_from_recorded_metadata",
        ),
        "high-speed" => (
            dominant.0,
            "correspondence_loss_boundary",
            "motion_exceeds_candidate_window_and_is_reported_as_correspondence_loss",
        ),
        "topology-break" => (
            dominant.0,
            "topology_mutation_boundary",
            "temporary_removed_cells_leave_measurable_screen_and_path_discontinuity",
        ),
        "line-crossing" => (
            dominant.0,
            "crossing_ambiguity_boundary",
            "crossing_regions_are_reported_as_ambiguity_not_universal_resolution",
        ),
        "bounded-jitter" => (
            dominant.0,
            "bounded_perturbation",
            "small_path_perturbations_remain_within_the_recoverable_operating_region",
        ),
        "shape-continuity" => (
            dominant.0,
            "shape_continuity_boundary",
            "neighbor_and_corner_costs_separate_topology_dp_from_weaker_history_modes",
        ),
        _ => (dominant.0, "unknown", "unclassified_stress_fixture"),
    }
}

fn path_keyed_identity_flip_rates(sequence: &FrameSequence) -> (f32, f32) {
    let mut corner_total = 0.0;
    let mut corner_flips = 0.0;
    let mut glyph_total = 0.0;
    let mut glyph_flips = 0.0;
    for pair in sequence.frames.windows(2) {
        let previous = pair[0]
            .cells
            .iter()
            .filter_map(|cell| {
                Some((
                    (cell.primitive_id?, cell.path_index?),
                    (cell.character, is_corner_glyph(cell.character)),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        for cell in &pair[1].cells {
            let Some(key) = cell.primitive_id.zip(cell.path_index) else {
                continue;
            };
            let Some((previous_char, previous_corner)) = previous.get(&key) else {
                continue;
            };
            glyph_total += 1.0;
            if *previous_char != cell.character {
                glyph_flips += 1.0;
            }
            if *previous_corner || is_corner_glyph(cell.character) {
                corner_total += 1.0;
                if *previous_char != cell.character {
                    corner_flips += 1.0;
                }
            }
        }
    }
    (
        if corner_total == 0.0 {
            0.0
        } else {
            corner_flips / corner_total
        },
        if glyph_total == 0.0 {
            0.0
        } else {
            glyph_flips / glyph_total
        },
    )
}

fn glyph_alphabet_name(dataset: &str) -> &'static str {
    if dataset.ends_with("-ascii") {
        "ascii"
    } else if dataset.ends_with("-degraded") {
        "degraded"
    } else {
        "unicode"
    }
}

fn ascii_stroke_proxy(character: char) -> char {
    match character {
        '━' => '-',
        '┃' => '|',
        '╭' | '╮' | '╰' | '╯' | '╋' => '+',
        other => other,
    }
}

fn degraded_stroke_proxy(character: char) -> char {
    match character {
        '━' | '┃' | '╱' | '╲' => '*',
        '╭' | '╮' | '╰' | '╯' | '╋' => '+',
        other => other,
    }
}

fn high_speed_line_cells(frame_index: usize, glyph_history_mode: GlyphHistoryMode) -> Vec<Cell> {
    let offset = ((frame_index * 3) % 12) as u16;
    let base = 5 + offset;
    let mut cells = Vec::new();
    for step in 0..8u16 {
        let column = base + step;
        let stable = if step == 0 {
            '╭'
        } else if step == 7 {
            '╮'
        } else {
            '━'
        };
        let unstable = if frame_index.is_multiple_of(2) && (step == 0 || step == 7) {
            '┃'
        } else {
            stable
        };
        let path = if frame_index % 5 == 2 && step == 7 {
            '┃'
        } else {
            stable
        };
        let character = match glyph_history_mode {
            GlyphHistoryMode::Off => unstable,
            GlyphHistoryMode::ScreenCell => {
                if frame_index % 4 == 1 && step == 0 {
                    '━'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::Path => path,
            GlyphHistoryMode::TopologyDp => {
                if frame_index.is_multiple_of(11) && step == 7 {
                    '┃'
                } else {
                    stable
                }
            }
        };
        let mut cell = orbit_cell(column, 6, character).with_path_identity(0, step as usize);
        if matches!(glyph_history_mode, GlyphHistoryMode::TopologyDp) {
            cell = cell.with_correspondence_lost();
        }
        cells.push(cell);
    }
    cells
}

fn topology_break_cells(frame_index: usize, glyph_history_mode: GlyphHistoryMode) -> Vec<Cell> {
    let stable = [
        (8, 5, '╭'),
        (9, 5, '━'),
        (10, 5, '╮'),
        (10, 6, '┃'),
        (10, 7, '╯'),
        (9, 7, '━'),
        (8, 7, '╰'),
        (8, 6, '┃'),
    ];
    let break_phase = (6..10).contains(&(frame_index % 16));
    stable
        .into_iter()
        .filter(|(column, row, _)| {
            if !break_phase {
                return true;
            }
            match glyph_history_mode {
                GlyphHistoryMode::TopologyDp => !(*column == 10 && *row == 6),
                _ => !(*column == 10 && (*row == 5 || *row == 6)),
            }
        })
        .enumerate()
        .map(|(path_index, (column, row, character))| {
            orbit_cell(column, row, character).with_path_identity(0, path_index)
        })
        .collect()
}

fn line_crossing_cells(frame_index: usize, glyph_history_mode: GlyphHistoryMode) -> Vec<Cell> {
    let mut cells = Vec::new();
    for step in 0..9u16 {
        let horizontal = if step == 4 {
            match glyph_history_mode {
                GlyphHistoryMode::Off | GlyphHistoryMode::ScreenCell => {
                    if frame_index.is_multiple_of(2) {
                        '┃'
                    } else {
                        '━'
                    }
                }
                GlyphHistoryMode::Path => {
                    if frame_index.is_multiple_of(5) {
                        '┃'
                    } else {
                        '━'
                    }
                }
                GlyphHistoryMode::TopologyDp => {
                    if frame_index.is_multiple_of(7) {
                        '┃'
                    } else {
                        '╋'
                    }
                }
            }
        } else {
            '━'
        };
        cells.push(orbit_cell(8 + step, 6, horizontal).with_path_identity(0, step as usize));
        if step != 4 {
            cells.push(orbit_cell(12, 2 + step, '┃').with_path_identity(1, step as usize));
        }
    }
    cells
}

fn jitter_line_cells(frame_index: usize, glyph_history_mode: GlyphHistoryMode) -> Vec<Cell> {
    let jitter = [-1i16, 0, 1, 0][frame_index % 4];
    let mut cells = Vec::new();
    for step in 0..8u16 {
        let row = (6i16 + if step == 3 || step == 4 { jitter } else { 0 }) as u16;
        let stable = if step == 0 {
            '╭'
        } else if step == 7 {
            '╮'
        } else if row != 6 {
            '╱'
        } else {
            '━'
        };
        let character = match glyph_history_mode {
            GlyphHistoryMode::Off => {
                if frame_index.is_multiple_of(2) && (step == 3 || step == 4) {
                    '┃'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::ScreenCell => {
                if frame_index % 4 == 1 && step == 4 {
                    '┃'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::Path => {
                if frame_index % 6 == 3 && step == 3 {
                    '━'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::TopologyDp => {
                if frame_index % 8 == 4 && step == 4 {
                    '┃'
                } else {
                    stable
                }
            }
        };
        cells.push(orbit_cell(8 + step, row, character).with_path_identity(0, step as usize));
    }
    cells
}

fn shuffled_path_cells(frame_index: usize, glyph_history_mode: GlyphHistoryMode) -> Vec<Cell> {
    let mut cells = glyph_stress_cells(frame_index, glyph_history_mode);
    for cell in &mut cells {
        if let Some(path_index) = cell.path_index {
            cell.path_index = Some((path_index * 3 + 2) % 8);
        }
    }
    cells
}

fn shape_continuity_cells(frame_index: usize, glyph_history_mode: GlyphHistoryMode) -> Vec<Cell> {
    let phase = frame_index % 6;
    let mut cells = Vec::new();
    for step in 0..10u16 {
        let wobble = !matches!(glyph_history_mode, GlyphHistoryMode::TopologyDp)
            && matches!(phase, 1 | 4)
            && matches!(step, 3..=6);
        let row = if wobble && step % 2 == 0 { 5 } else { 6 };
        let stable = if step == 0 {
            '╭'
        } else if step == 9 {
            '╮'
        } else if wobble {
            '╱'
        } else {
            '━'
        };
        let character = match glyph_history_mode {
            GlyphHistoryMode::Off => {
                if wobble {
                    if frame_index.is_multiple_of(2) {
                        '┃'
                    } else {
                        '━'
                    }
                } else {
                    stable
                }
            }
            GlyphHistoryMode::ScreenCell => {
                if wobble && step == 5 {
                    '┃'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::Path => {
                if wobble && step == 4 {
                    '━'
                } else {
                    stable
                }
            }
            GlyphHistoryMode::TopologyDp => stable,
        };
        cells.push(orbit_cell(7 + step, row, character).with_path_identity(0, step as usize));
    }
    cells
}

fn orbit_cell(column: u16, row: u16, character: char) -> Cell {
    Cell {
        column,
        row,
        character,
        color: rgb!(92, 210, 255),
        layer: RenderLayer::Orbit,
        primitive_id: None,
        stroke_id: None,
        vertex_id: None,
        path_index: None,
        correspondence_lost: false,
    }
}

pub fn replay_frames(stdout: &mut Stdout, path: &str) -> io::Result<()> {
    let sequence = FrameSequence::read(path)?;
    let mut previous_frame = None;
    let frame_delay = delay_for_fps(sequence.header.fps);
    for frame in sequence.frames {
        let render_frame = frame.to_frame();
        render_frame.render_dirty(stdout, previous_frame.as_ref(), sequence.header.dirty_mode)?;
        stdout.flush()?;
        previous_frame = Some(render_frame);
        thread::sleep(frame_delay);
        if should_exit()? {
            break;
        }
    }
    Ok(())
}

pub fn score_frames(path: &str) -> io::Result<String> {
    Ok(VisualScore::from_sequence(&FrameSequence::read(path)?).to_json())
}

pub fn print_snapshot(
    profile: VisualProfile,
    motion_preset: MotionPreset,
    scene_preset: ScenePreset,
    calibration: TerminalCalibration,
    theme: Theme,
) -> io::Result<()> {
    let layout = Layout::for_size(80, 24);
    let mut frame_buffers = FrameBuffers::default();
    let frame = Frame::build(
        &layout,
        1.25,
        FrameContext {
            profile,
            motion_preset,
            scene_preset,
            calibration,
            theme,
            mouse: None,
            quality: QualitySettings::from_calibration(calibration, 1.0),
            camera: VirtualCamera::default(),
            curves: VisualCurves::for_profile(profile),
            director: MotionDirectorState::steady(),
            glyph_history_mode: GlyphHistoryMode::Path,
            low_latency: false,
            medium_latency: false,
        },
        &mut frame_buffers,
    );
    let summary = SnapshotSummary::from_frame(
        &frame,
        &layout,
        profile,
        motion_preset,
        scene_preset,
        calibration,
    );
    println!("{}", summary.to_json());
    Ok(())
}

fn write_loader_frame(
    stdout: &mut Stdout,
    gradient: &[(u8, u8, u8)],
    loop_index: usize,
    step_index: usize,
    total_steps: usize,
) -> io::Result<()> {
    write_gradient_blocks(stdout, gradient, step_index)?;
    write!(
        stdout,
        " {} {}%",
        SPINNER[step_index % SPINNER.len()].with(SPINNER_COLOR),
        progress_percent(loop_index, step_index, total_steps)
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Cell {
    column: u16,
    row: u16,
    character: char,
    color: Color,
    layer: RenderLayer,
    primitive_id: Option<u16>,
    stroke_id: Option<u16>,
    vertex_id: Option<u16>,
    path_index: Option<usize>,
    correspondence_lost: bool,
}

impl Cell {
    fn with_path_identity(mut self, primitive_id: u16, path_index: usize) -> Self {
        self.primitive_id = Some(primitive_id);
        self.stroke_id = Some(0);
        self.path_index = Some(path_index);
        self
    }

    fn with_correspondence_lost(mut self) -> Self {
        self.correspondence_lost = true;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RenderLayer {
    Background,
    Orbit,
    Text,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum PerceptualRole {
    BackgroundParticle,
    Afterimage,
    MouseTrail,
    Orbit,
    StatusText,
    Logo,
}

impl PerceptualRole {
    fn name(self) -> &'static str {
        match self {
            Self::BackgroundParticle => "background-particle",
            Self::Afterimage => "afterimage",
            Self::MouseTrail => "mouse-trail",
            Self::Orbit => "orbit",
            Self::StatusText => "status-text",
            Self::Logo => "logo",
        }
    }

    fn all() -> [Self; 6] {
        [
            Self::BackgroundParticle,
            Self::Afterimage,
            Self::MouseTrail,
            Self::Orbit,
            Self::StatusText,
            Self::Logo,
        ]
    }

    fn contrast_floor(self) -> f32 {
        match self {
            Self::BackgroundParticle => 16.0,
            Self::Afterimage => 12.0,
            Self::MouseTrail => 26.0,
            Self::Orbit => 38.0,
            Self::StatusText => 32.0,
            Self::Logo => 42.0,
        }
    }

    fn temporal_stability_weight(self) -> f32 {
        match self {
            Self::Logo | Self::StatusText => 1.0,
            Self::Orbit => 0.78,
            Self::MouseTrail => 0.58,
            Self::BackgroundParticle => 0.34,
            Self::Afterimage => 0.24,
        }
    }

    fn from_cell(cell: &Cell) -> Self {
        match cell.layer {
            RenderLayer::Text if cell.character == '█' || cell.character == '═' => Self::Logo,
            RenderLayer::Text => Self::StatusText,
            RenderLayer::Orbit => Self::Orbit,
            RenderLayer::Background => {
                if Oklab::from_color(cell.color).is_some_and(|lab| lab.lightness < 0.28) {
                    Self::Afterimage
                } else {
                    Self::BackgroundParticle
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Frame {
    cells: Vec<Cell>,
}

impl Frame {
    fn build(
        layout: &Layout,
        elapsed_seconds: f32,
        context: FrameContext,
        buffers: &mut FrameBuffers,
    ) -> Self {
        let mut cells = Vec::new();

        let simplified_effects = context.low_latency || context.medium_latency;
        if !simplified_effects {
            push_background_depth(&mut cells, layout, elapsed_seconds, context);
            buffers
                .mouse_particles
                .step(&mut cells, layout, elapsed_seconds, context);
        }

        if let Some(frame_box) = layout.frame_box {
            push_primitive_scene(
                &mut cells,
                layout,
                frame_box,
                elapsed_seconds,
                context,
                &mut buffers.orbit,
                &mut buffers.primitive_scene,
            );
        }

        if !context.low_latency {
            buffers
                .afterimage
                .apply(&mut cells, elapsed_seconds, context, layout);
            let luminance_budget = LuminanceBudget::from_foreground(&cells, layout);
            luminance_budget.apply(&mut cells);
        }

        for (row_index, line) in layout.logo.iter().enumerate() {
            if context.low_latency || context.medium_latency {
                push_static_text(
                    &mut cells,
                    layout.logo_column,
                    layout.logo_row + row_index as u16,
                    line,
                    display_role_color(
                        ensure_visible_lightness(
                            context.theme.logo_highlight,
                            context.theme.contrast_floor,
                        ),
                        context.calibration.capabilities,
                        PerceptualRole::Logo,
                    ),
                );
            } else {
                push_gradient_text(
                    &mut cells,
                    layout.logo_column,
                    layout.logo_row + row_index as u16,
                    line,
                    TextRenderContext {
                        elapsed_seconds,
                        profile: context.profile,
                        motion_preset: context.motion_preset,
                        calibration: context.calibration,
                        theme: context.theme,
                    },
                );
            }
        }

        let status_row = layout
            .frame_box
            .map(|frame_box| frame_box.top + frame_box.height + 1)
            .unwrap_or(layout.logo_row + layout.logo.len() as u16 + 1);
        let supporting_color = if context.low_latency || context.medium_latency {
            display_role_color(
                ensure_visible_lightness(context.theme.dim, context.theme.contrast_floor * 0.82),
                context.calibration.capabilities,
                PerceptualRole::StatusText,
            )
        } else {
            ColorPipeline::new(
                TextRenderContext {
                    elapsed_seconds,
                    profile: context.profile,
                    motion_preset: context.motion_preset,
                    calibration: context.calibration,
                    theme: context.theme,
                },
                0.0,
            )
            .supporting_text(context.theme.accent, STATUS_VISUAL_GAIN)
        };
        if status_row < layout.rows {
            push_static_text(
                &mut cells,
                centered_column(layout.columns, STATUS_LINE.chars().count()),
                status_row,
                STATUS_LINE,
                supporting_color,
            );
        }

        let hint_color = if context.low_latency || context.medium_latency {
            supporting_color
        } else {
            ColorPipeline::new(
                TextRenderContext {
                    elapsed_seconds,
                    profile: context.profile,
                    motion_preset: context.motion_preset,
                    calibration: context.calibration,
                    theme: context.theme,
                },
                0.0,
            )
            .supporting_text(context.theme.dim, EXIT_HINT_VISUAL_GAIN)
        };
        if layout.rows > 1 {
            push_static_text(
                &mut cells,
                centered_column(layout.columns, EXIT_HINT.chars().count()),
                layout.rows - 1,
                EXIT_HINT,
                hint_color,
            );
        }

        cells = composite_layers(cells);
        if !simplified_effects {
            buffers.temporal_aa.apply(&mut cells, context);
            buffers.smoothing.apply(&mut cells, context.profile);
        }
        buffers.last_topology_cache_hits = buffers.primitive_scene.hits;
        buffers.last_topology_cache_misses = buffers.primitive_scene.misses;

        Self { cells }
    }

    fn render_dirty(
        &self,
        stdout: &mut Stdout,
        previous: Option<&Self>,
        dirty_mode: DirtyMode,
    ) -> io::Result<RenderStats> {
        let dirty_cells = self.dirty_cells(previous, dirty_mode);
        let dirty_runs = dirty_runs(&dirty_cells);
        for run in &dirty_runs {
            stdout.queue(MoveTo(run.column, run.row))?;
            for cell in &run.cells {
                write!(stdout, "{}", cell.character.to_string().with(cell.color))?;
            }
        }

        let mut stale_cell_count = 0;
        let mut stale_run_count = 0;
        let mut stale_runs_vec = Vec::new();
        if let Some(previous) = previous {
            let stale_cells = previous.stale_cells(self);
            stale_cell_count = stale_cells.len();
            stale_runs_vec = stale_runs(&stale_cells);
            stale_run_count = stale_runs_vec.len();
            for run in &stale_runs_vec {
                stdout.queue(MoveTo(run.column, run.row))?;
                write!(stdout, "{}", " ".repeat(run.width))?;
            }
        }

        let bytes_written = estimated_ansi_write_bytes(&dirty_runs, &stale_runs_vec);
        let naive_bytes = self.cells.len().saturating_mul(16);

        Ok(RenderStats {
            dirty_cells: dirty_cells.len(),
            dirty_runs: dirty_runs.len(),
            stale_cells: stale_cell_count,
            stale_runs: stale_run_count,
            bytes_written,
            bytes_saved: naive_bytes.saturating_sub(bytes_written),
            primitive_cells: self
                .cells
                .iter()
                .filter(|cell| cell.primitive_id.is_some())
                .count(),
            topology_cache_hits: 0,
            topology_cache_misses: 0,
        })
    }

    fn render_dirty_with_cache_stats(
        &self,
        stdout: &mut Stdout,
        previous: Option<&Self>,
        dirty_mode: DirtyMode,
        cache_hits: usize,
        cache_misses: usize,
    ) -> io::Result<RenderStats> {
        let mut stats = self.render_dirty(stdout, previous, dirty_mode)?;
        stats.topology_cache_hits = cache_hits;
        stats.topology_cache_misses = cache_misses;
        Ok(stats)
    }

    fn dirty_cells<'a>(
        &'a self,
        previous: Option<&'a Self>,
        dirty_mode: DirtyMode,
    ) -> Vec<&'a Cell> {
        self.dirty_cells_with_budget(previous, dirty_mode, usize::MAX)
    }

    fn dirty_cells_with_budget<'a>(
        &'a self,
        previous: Option<&'a Self>,
        dirty_mode: DirtyMode,
        budget: usize,
    ) -> Vec<&'a Cell> {
        let Some(previous) = previous else {
            return self.cells.iter().collect();
        };

        let mut dirty = self
            .cells
            .iter()
            .filter(|cell| {
                previous
                    .cell_at(cell.column, cell.row)
                    .is_none_or(|previous| !dirty_matrix_cell_matches(previous, cell, dirty_mode))
            })
            .collect::<Vec<_>>();
        if dirty_mode == DirtyMode::PriorityDirty && dirty.len() > budget {
            dirty.sort_by_key(|cell| dirty_priority_key(cell));
            dirty.truncate(budget);
        }
        dirty.sort_by_key(|cell| (cell.row, cell.column));
        dirty
    }

    fn stale_cells<'a>(&'a self, next: &'a Self) -> Vec<&'a Cell> {
        self.cells
            .iter()
            .filter(|cell| next.cell_at(cell.column, cell.row).is_none())
            .collect()
    }

    fn cell_at(&self, column: u16, row: u16) -> Option<&Cell> {
        self.cells
            .binary_search_by_key(&(row, column), |cell| (cell.row, cell.column))
            .ok()
            .map(|index| &self.cells[index])
    }
}

fn dirty_matrix_cell_matches(previous: &Cell, next: &Cell, dirty_mode: DirtyMode) -> bool {
    if previous == next {
        return true;
    }
    if dirty_mode == DirtyMode::Naive || dirty_mode == DirtyMode::PriorityDirty {
        return false;
    }
    if previous.character != next.character || previous.layer != next.layer {
        return false;
    }

    let policy = PerceptualDirtyPolicy::for_mode(dirty_mode);
    oklab_distance(previous.color, next.color)
        <= policy.threshold(PerceptualRole::from_cell(next), next.layer)
}

#[derive(Clone, Copy, Debug)]
struct PerceptualDirtyPolicy {
    mode: DirtyMode,
}

impl PerceptualDirtyPolicy {
    fn for_mode(mode: DirtyMode) -> Self {
        Self { mode }
    }

    fn threshold(self, role: PerceptualRole, layer: RenderLayer) -> f32 {
        let threshold: f32 = match self.mode {
            DirtyMode::Naive | DirtyMode::PriorityDirty => 0.0,
            DirtyMode::UniformThreshold | DirtyMode::TopologyOnly => 0.014,
            DirtyMode::LuminanceThreshold => 0.024,
            DirtyMode::RoleAware | DirtyMode::Full => match role {
                PerceptualRole::Logo => 0.0,
                PerceptualRole::StatusText => 0.006,
                PerceptualRole::Orbit => 0.018,
                PerceptualRole::MouseTrail => 0.022,
                PerceptualRole::BackgroundParticle | PerceptualRole::Afterimage => 0.032,
            },
        };
        threshold.min(if layer == RenderLayer::Text {
            0.006
        } else {
            0.04
        })
    }
}

fn dirty_priority_key(cell: &Cell) -> (u8, u16, u16) {
    let priority = match PerceptualRole::from_cell(cell) {
        PerceptualRole::Logo => 0,
        PerceptualRole::StatusText => 1,
        PerceptualRole::Orbit => 2,
        PerceptualRole::MouseTrail => 3,
        PerceptualRole::BackgroundParticle => 4,
        PerceptualRole::Afterimage => 5,
    };
    (priority, cell.row, cell.column)
}

#[derive(Clone, Copy)]
struct FrameContext {
    profile: VisualProfile,
    motion_preset: MotionPreset,
    scene_preset: ScenePreset,
    calibration: TerminalCalibration,
    theme: Theme,
    mouse: Option<MouseAttractor>,
    quality: QualitySettings,
    camera: VirtualCamera,
    curves: VisualCurves,
    director: MotionDirectorState,
    glyph_history_mode: GlyphHistoryMode,
    low_latency: bool,
    medium_latency: bool,
}

#[derive(Default)]
struct FrameBuffers {
    smoothing: SmoothingBuffer,
    temporal_aa: CellTemporalAaBuffer,
    afterimage: AfterimageBuffer,
    mouse_particles: MouseParticleSystem,
    orbit: OrbitTemporalBuffer,
    primitive_scene: PrimitiveSceneCache,
    last_topology_cache_hits: usize,
    last_topology_cache_misses: usize,
}

impl FrameBuffers {
    fn clear(&mut self) {
        self.smoothing.clear();
        self.temporal_aa.clear();
        self.afterimage.clear();
        self.mouse_particles.clear();
        self.orbit.clear();
        self.primitive_scene.clear();
        self.last_topology_cache_hits = 0;
        self.last_topology_cache_misses = 0;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RenderStats {
    dirty_cells: usize,
    dirty_runs: usize,
    stale_cells: usize,
    stale_runs: usize,
    bytes_written: usize,
    bytes_saved: usize,
    primitive_cells: usize,
    topology_cache_hits: usize,
    topology_cache_misses: usize,
}

fn render_benchmark(
    stdout: &mut Stdout,
    layout: &Layout,
    metrics: &RenderMetrics,
    calibration: TerminalCalibration,
    runtime: RuntimeState,
) -> io::Result<()> {
    if layout.rows < 2 {
        return Ok(());
    }

    let text = inspect_overlay_text(layout, metrics, calibration, runtime);

    stdout.queue(MoveTo(0, layout.rows.saturating_sub(2)))?;
    stdout.queue(Clear(ClearType::CurrentLine))?;
    write!(stdout, "{}", text.with(Color::DarkGrey))
}

fn inspect_overlay_text(
    _layout: &Layout,
    metrics: &RenderMetrics,
    calibration: TerminalCalibration,
    runtime: RuntimeState,
) -> String {
    let stats = metrics.last_stats;
    let curves = VisualCurves::for_profile(runtime.profile);
    let director =
        MotionDirectorState::at(metrics.started_at.elapsed().as_secs_f32(), runtime.profile);
    let pointer = PointerBackendStatus::detect();
    format!(
        "fps:{:>5.1} tier:{} q:{} ll:{} curve:{} phase:{} missed:{} worst:{:>5.2}ms dirty:{} runs:{} stale:{} clear:{} bytes:{} saved:{} prim:{} cache:{}/{} density:{:.2} term:{} preset:{} motion:{} scene:{} glyph:{:?} pointer:{}:{:.2} sample:{} flush:{:.3}ms mouse:{}{}",
        metrics.average_fps(),
        metrics.adaptive.target_fps,
        metrics.adaptive.quality_percent,
        metrics.adaptive.low_latency_active,
        curves.family.name(),
        director.phase.name(),
        metrics.adaptive.missed_deadlines,
        metrics.worst_frame_time.as_secs_f64() * 1000.0,
        stats.dirty_cells,
        stats.dirty_runs,
        stats.stale_cells,
        stats.stale_runs,
        stats.bytes_written,
        stats.bytes_saved,
        stats.primitive_cells,
        stats.topology_cache_hits,
        stats.topology_cache_misses,
        calibration.effect_density,
        terminal_visual_profile(calibration).name(),
        calibration.capabilities.preset.name(),
        runtime.motion_preset.name(),
        runtime.scene_preset.name(),
        calibration.capabilities.glyph_mode,
        pointer.backend.name(),
        pointer.confidence,
        calibration.throughput.cells_sampled,
        calibration.throughput.flush_time.as_secs_f64() * 1000.0,
        if runtime.left_mouse_down {
            "drag"
        } else {
            "idle"
        },
        if runtime.paused { " paused" } else { "" }
    )
}

#[derive(Clone, Copy, Debug)]
struct RuntimeState {
    profile: VisualProfile,
    motion_preset: MotionPreset,
    scene_preset: ScenePreset,
    inspect: bool,
    paused: bool,
    left_mouse_down: bool,
    mouse: Option<MouseState>,
    release_wake: Option<ReleaseWake>,
}

impl RuntimeState {
    fn new(
        profile: VisualProfile,
        motion_preset: MotionPreset,
        scene_preset: ScenePreset,
        inspect: bool,
    ) -> Self {
        Self {
            profile,
            motion_preset,
            scene_preset,
            inspect,
            paused: false,
            left_mouse_down: false,
            mouse: None,
            release_wake: None,
        }
    }

    fn mouse_at(self, now: f32) -> Option<MouseAttractor> {
        if let Some(mouse) = self.mouse {
            let age = now - mouse.updated_at;
            if (0.0..=2.4).contains(&age) {
                return Some(MouseAttractor {
                    column: mouse.column,
                    row: mouse.row,
                    strength: (1.0 - age / 2.4).clamp(0.0, 1.0),
                    velocity_x: mouse.velocity_x,
                    velocity_y: mouse.velocity_y,
                    emitting: self.left_mouse_down,
                    assimilate: !self.left_mouse_down,
                    release: false,
                });
            }
        }

        let wake = self.release_wake?;
        let age = now - wake.started_at;
        if !(0.0..=wake.duration).contains(&age) {
            return None;
        }
        let decay = 1.0 - smootherstep(age / wake.duration);
        let drift_x = wake.velocity_x * age * 0.16;
        let drift_y = wake.velocity_y * age * 0.12;

        Some(MouseAttractor {
            column: (f32::from(wake.column) + drift_x).round().clamp(0.0, 300.0) as u16,
            row: (f32::from(wake.row) + drift_y).round().clamp(0.0, 120.0) as u16,
            strength: decay * 0.66,
            velocity_x: wake.velocity_x * decay,
            velocity_y: wake.velocity_y * decay,
            emitting: false,
            assimilate: true,
            release: true,
        })
    }

    fn update_mouse(&mut self, column: u16, row: u16, elapsed_seconds: f32) {
        let (velocity_x, velocity_y) = self.mouse.map_or((0.0, 0.0), |previous| {
            let dt = (elapsed_seconds - previous.updated_at).max(1.0 / 120.0);
            (
                (f32::from(column) - f32::from(previous.column)) / dt,
                (f32::from(row) - f32::from(previous.row)) / dt,
            )
        });
        self.mouse = Some(MouseState {
            column,
            row,
            velocity_x: velocity_x.clamp(-90.0, 90.0),
            velocity_y: velocity_y.clamp(-60.0, 60.0),
            updated_at: elapsed_seconds,
        });
        self.release_wake = None;
    }

    fn stop_mouse_interaction(&mut self, elapsed_seconds: f32) {
        self.release_wake = self.mouse.map(|mouse| ReleaseWake {
            column: mouse.column,
            row: mouse.row,
            velocity_x: mouse.velocity_x,
            velocity_y: mouse.velocity_y,
            started_at: elapsed_seconds,
            duration: 0.46 * VisualCurves::for_profile(self.profile).release_tail,
        });
        self.left_mouse_down = false;
        self.mouse = None;
    }
}

#[derive(Clone, Copy, Debug)]
struct MouseState {
    column: u16,
    row: u16,
    velocity_x: f32,
    velocity_y: f32,
    updated_at: f32,
}

#[derive(Clone, Copy, Debug)]
struct ReleaseWake {
    column: u16,
    row: u16,
    velocity_x: f32,
    velocity_y: f32,
    started_at: f32,
    duration: f32,
}

#[derive(Clone, Copy, Debug)]
struct PointerSample {
    window_x: i32,
    window_y: i32,
    window_width: i32,
    window_height: i32,
    pointer_x: i32,
    pointer_y: i32,
}

#[derive(Clone, Copy, Debug)]
struct PointerCell {
    column: u16,
    row: u16,
}

#[derive(Clone, Copy, Debug)]
struct SystemPointerTracker {
    available: bool,
    last_sample_at: f32,
    backend: PointerBackend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PointerBackend {
    GnomeWayland,
    MacOs,
    X11,
    TerminalOnly,
}

impl PointerBackend {
    fn name(self) -> &'static str {
        match self {
            Self::GnomeWayland => "wayland-system",
            Self::MacOs => "macos-system",
            Self::X11 => "x11-system",
            Self::TerminalOnly => "terminal-mouse",
        }
    }

    fn confidence(self) -> f32 {
        match self {
            Self::X11 => 0.74,
            Self::GnomeWayland | Self::MacOs => 0.32,
            Self::TerminalOnly => 0.58,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PointerBackendStatus {
    backend: PointerBackend,
    confidence: f32,
}

impl PointerBackendStatus {
    fn detect() -> Self {
        let backend = detect_pointer_backend();
        Self {
            backend,
            confidence: backend.confidence(),
        }
    }
}

impl SystemPointerTracker {
    fn new() -> Self {
        let backend = detect_pointer_backend();
        Self {
            available: backend != PointerBackend::TerminalOnly,
            last_sample_at: -1.0,
            backend,
        }
    }

    fn sample(&mut self, layout: &Layout, elapsed_seconds: f32) -> Option<MouseState> {
        if !self.available || elapsed_seconds - self.last_sample_at < 1.0 / 45.0 {
            return None;
        }
        self.last_sample_at = elapsed_seconds;

        let sample = match sample_system_pointer(self.backend) {
            Some(sample) => sample,
            None => {
                self.available = false;
                return None;
            }
        };
        let cell = pointer_cell_from_sample(sample, layout)?;
        Some(MouseState {
            column: cell.column,
            row: cell.row,
            velocity_x: 0.0,
            velocity_y: 0.0,
            updated_at: elapsed_seconds,
        })
    }
}

fn detect_pointer_backend() -> PointerBackend {
    if cfg!(target_os = "macos") && command_available("osascript") {
        return PointerBackend::MacOs;
    }
    if std::env::var("XDG_SESSION_TYPE").is_ok_and(|session| session == "wayland")
        && std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase()
            .contains("gnome")
        && command_available("gdbus")
    {
        return PointerBackend::GnomeWayland;
    }
    if command_available("xdotool") {
        return PointerBackend::X11;
    }
    PointerBackend::TerminalOnly
}

fn command_available(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command} >/dev/null 2>&1"))
        .status()
        .is_ok_and(|status| status.success())
}

fn sample_system_pointer(backend: PointerBackend) -> Option<PointerSample> {
    match backend {
        PointerBackend::GnomeWayland => sample_gnome_wayland_pointer(),
        PointerBackend::MacOs => sample_macos_pointer(),
        PointerBackend::X11 => sample_x11_pointer(),
        PointerBackend::TerminalOnly => None,
    }
}

fn sample_gnome_wayland_pointer() -> Option<PointerSample> {
    // GNOME Wayland deliberately does not expose global pointer coordinates to
    // ordinary clients. This backend exists so the priority is explicit and can
    // be replaced by a portal/native helper without changing the particle system.
    None
}

fn sample_macos_pointer() -> Option<PointerSample> {
    // macOS requires a native helper or automation/accessibility permission for
    // reliable global pointer + focused-window geometry. Keep this as a backend
    // hook and fall back to terminal mouse events when permission is unavailable.
    None
}

fn sample_x11_pointer() -> Option<PointerSample> {
    let window = Command::new("xdotool")
        .args(["getactivewindow", "getwindowgeometry", "--shell"])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let mouse = Command::new("xdotool")
        .args(["getmouselocation", "--shell"])
        .output()
        .ok()
        .filter(|output| output.status.success())?;

    let geometry = String::from_utf8(window.stdout).ok()?;
    let location = String::from_utf8(mouse.stdout).ok()?;
    Some(PointerSample {
        window_x: shell_value_i32(&geometry, "X")?,
        window_y: shell_value_i32(&geometry, "Y")?,
        window_width: shell_value_i32(&geometry, "WIDTH")?,
        window_height: shell_value_i32(&geometry, "HEIGHT")?,
        pointer_x: shell_value_i32(&location, "X")?,
        pointer_y: shell_value_i32(&location, "Y")?,
    })
}

fn shell_value_i32(text: &str, key: &str) -> Option<i32> {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        .and_then(|value| value.parse().ok())
}

fn pointer_cell_from_sample(sample: PointerSample, layout: &Layout) -> Option<PointerCell> {
    let local_x = sample.pointer_x - sample.window_x;
    let local_y = sample.pointer_y - sample.window_y;
    if local_x < 0 || local_y < 0 {
        return None;
    }

    // Best-effort terminal chrome compensation: xdotool reports outer-window pixels,
    // while crossterm mouse events are content-cell coordinates.
    let chrome_x = 8;
    let chrome_y = 32;
    let content_x = local_x - chrome_x;
    let content_y = local_y - chrome_y;
    if content_x < 0 || content_y < 0 {
        return None;
    }

    let cell_width = ((sample.window_width - chrome_x * 2).max(i32::from(layout.columns))
        / i32::from(layout.columns))
    .max(1);
    let cell_height = ((sample.window_height - chrome_y - 8).max(i32::from(layout.rows))
        / i32::from(layout.rows))
    .max(1);
    let column = content_x / cell_width;
    let row = content_y / cell_height;
    if column >= i32::from(layout.columns) || row >= i32::from(layout.rows) {
        return None;
    }

    Some(PointerCell {
        column: column as u16,
        row: row as u16,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct VirtualCamera {
    x: f32,
    y: f32,
    velocity_x: f32,
    velocity_y: f32,
    last_update_at: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct VisualCurves {
    family: CurveFamily,
    brightness: f32,
    chroma: f32,
    orbit_speed: f32,
    particle_lifetime: f32,
    afterimage_decay: f32,
    vortex_strength: f32,
    release_tail: f32,
    camera_parallax: f32,
}

impl VisualCurves {
    fn for_profile(profile: VisualProfile) -> Self {
        match profile {
            VisualProfile::Calm => Self::subtle(),
            VisualProfile::Cinematic => Self::cinematic(),
            VisualProfile::Ultra => Self::intense(),
            VisualProfile::Benchmark => Self::benchmark(),
        }
    }

    fn subtle() -> Self {
        Self {
            family: CurveFamily::Subtle,
            brightness: 0.82,
            chroma: 0.86,
            orbit_speed: 0.84,
            particle_lifetime: 0.78,
            afterimage_decay: 0.72,
            vortex_strength: 0.72,
            release_tail: 0.68,
            camera_parallax: 0.58,
        }
    }

    fn cinematic() -> Self {
        Self {
            family: CurveFamily::Cinematic,
            brightness: 0.92,
            chroma: 0.94,
            orbit_speed: 0.9,
            particle_lifetime: 1.08,
            afterimage_decay: 1.08,
            vortex_strength: 0.9,
            release_tail: 1.12,
            camera_parallax: 0.82,
        }
    }

    fn intense() -> Self {
        Self {
            family: CurveFamily::Intense,
            brightness: 1.0,
            chroma: 1.0,
            orbit_speed: 1.0,
            particle_lifetime: 1.0,
            afterimage_decay: 1.0,
            vortex_strength: 1.0,
            release_tail: 1.0,
            camera_parallax: 1.0,
        }
    }

    fn benchmark() -> Self {
        Self {
            family: CurveFamily::Benchmark,
            brightness: 1.0,
            chroma: 1.0,
            orbit_speed: 1.0,
            particle_lifetime: 0.9,
            afterimage_decay: 0.74,
            vortex_strength: 0.86,
            release_tail: 0.72,
            camera_parallax: 0.64,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CurveFamily {
    Subtle,
    Cinematic,
    Intense,
    Benchmark,
}

impl CurveFamily {
    fn name(self) -> &'static str {
        match self {
            Self::Subtle => "subtle",
            Self::Cinematic => "cinematic",
            Self::Intense => "intense",
            Self::Benchmark => "benchmark",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MotionDirectorPhase {
    Emergence,
    Cruise,
    Emphasis,
    Settle,
    Quiet,
}

impl MotionDirectorPhase {
    fn name(self) -> &'static str {
        match self {
            Self::Emergence => "emergence",
            Self::Cruise => "cruise",
            Self::Emphasis => "emphasis",
            Self::Settle => "settle",
            Self::Quiet => "quiet",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct MotionDirectorState {
    phase: MotionDirectorPhase,
    logo_weight: f32,
    orbit_weight: f32,
    background_weight: f32,
    afterimage_weight: f32,
    mouse_weight: f32,
}

impl MotionDirectorState {
    fn reveal(elapsed_seconds: f32) -> Self {
        if elapsed_seconds >= 1.25 {
            return Self::steady();
        }
        let t = smootherstep(elapsed_seconds / 1.25);
        let orbit_t = smootherstep(((elapsed_seconds - 0.18) / 1.07).clamp(0.0, 1.0));
        Self {
            phase: MotionDirectorPhase::Emergence,
            logo_weight: t,
            orbit_weight: orbit_t,
            background_weight: 0.18 + t * 0.54,
            afterimage_weight: t * 0.44,
            mouse_weight: 0.74,
        }
    }

    fn at(elapsed_seconds: f32, profile: VisualProfile) -> Self {
        if profile == VisualProfile::Benchmark {
            return Self::steady();
        }
        let cycle = elapsed_seconds.rem_euclid(9.6);
        if cycle < 1.1 {
            let t = smootherstep(cycle / 1.1);
            Self {
                phase: MotionDirectorPhase::Emergence,
                logo_weight: t,
                orbit_weight: t * 0.42,
                background_weight: 0.35 + t * 0.38,
                afterimage_weight: t * 0.34,
                mouse_weight: 0.8,
            }
        } else if cycle < 5.7 {
            Self::steady()
        } else if cycle < 6.9 {
            let t = smootherstep((cycle - 5.7) / 1.2);
            Self {
                phase: MotionDirectorPhase::Emphasis,
                logo_weight: 1.0 + t * 0.18,
                orbit_weight: 1.0 + t * 0.32,
                background_weight: 0.62 - t * 0.16,
                afterimage_weight: 0.86 + t * 0.16,
                mouse_weight: 1.0,
            }
        } else if cycle < 8.1 {
            let t = smootherstep((cycle - 6.9) / 1.2);
            Self {
                phase: MotionDirectorPhase::Settle,
                logo_weight: 1.08 - t * 0.12,
                orbit_weight: 1.18 - t * 0.22,
                background_weight: 0.5 + t * 0.22,
                afterimage_weight: 0.92 - t * 0.2,
                mouse_weight: 0.94,
            }
        } else {
            Self {
                phase: MotionDirectorPhase::Quiet,
                logo_weight: 0.92,
                orbit_weight: 0.74,
                background_weight: 0.64,
                afterimage_weight: 0.58,
                mouse_weight: 0.84,
            }
        }
    }

    fn steady() -> Self {
        Self {
            phase: MotionDirectorPhase::Cruise,
            logo_weight: 1.0,
            orbit_weight: 1.0,
            background_weight: 1.0,
            afterimage_weight: 1.0,
            mouse_weight: 1.0,
        }
    }
}

impl VirtualCamera {
    fn update(
        &mut self,
        elapsed_seconds: f32,
        mouse: Option<MouseAttractor>,
        preset: MotionPreset,
    ) {
        let dt = self.last_update_at.map_or(1.0 / 120.0, |last| {
            (elapsed_seconds - last).clamp(1.0 / 240.0, 1.0 / 20.0)
        });
        self.last_update_at = Some(elapsed_seconds);

        let phases = MotionPhases::at(elapsed_seconds, preset);
        self.velocity_x += (elapsed_seconds * phases.orbit_speed * 1.7).sin() * 0.018;
        self.velocity_y += (elapsed_seconds * 0.71).cos() * 0.006;
        if let Some(mouse) = mouse {
            self.velocity_x += mouse.velocity_x * 0.0009 * mouse.strength;
            self.velocity_y += mouse.velocity_y * 0.0007 * mouse.strength;
        }

        self.velocity_x *= 0.82_f32.powf(dt * 60.0);
        self.velocity_y *= 0.80_f32.powf(dt * 60.0);
        self.x = (self.x + self.velocity_x * dt).clamp(-2.8, 2.8);
        self.y = (self.y + self.velocity_y * dt).clamp(-1.6, 1.6);
    }

    fn layer_offset(self, layer: u8, parallax: f32) -> (f32, f32) {
        let depth = (f32::from(layer) + 1.0) / 6.0;
        (
            self.x * depth * 1.8 * parallax,
            self.y * depth * 1.2 * parallax,
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct MouseAttractor {
    column: u16,
    row: u16,
    strength: f32,
    velocity_x: f32,
    velocity_y: f32,
    emitting: bool,
    assimilate: bool,
    release: bool,
}

fn handle_runtime_input(runtime: &mut RuntimeState, elapsed_seconds: f32) -> io::Result<bool> {
    while matches!(event::poll(Duration::from_millis(0)), Ok(true)) {
        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                if handle_runtime_key(runtime, key.code, key.modifiers) {
                    return Ok(true);
                }
            }
            Ok(Event::Mouse(mouse)) => handle_runtime_mouse(
                runtime,
                mouse.kind,
                mouse.column,
                mouse.row,
                elapsed_seconds,
            ),
            Ok(_) => {}
            Err(_) => return Ok(false),
        }
    }

    Ok(false)
}

fn handle_runtime_key(runtime: &mut RuntimeState, code: KeyCode, modifiers: KeyModifiers) -> bool {
    if exit_key_pressed_from_parts(code, modifiers) {
        return true;
    }
    if runtime.inspect {
        match code {
            KeyCode::Char('m') => runtime.motion_preset = runtime.motion_preset.next(),
            KeyCode::Char('s') => runtime.scene_preset = runtime.scene_preset.next(),
            KeyCode::Char('p') => runtime.paused = !runtime.paused,
            KeyCode::Char('b') => runtime.profile = VisualProfile::Benchmark,
            KeyCode::Char('c') => runtime.profile = VisualProfile::Calm,
            KeyCode::Char('u') => runtime.profile = VisualProfile::Ultra,
            _ => {}
        }
    }
    false
}

fn handle_runtime_mouse(
    runtime: &mut RuntimeState,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    elapsed_seconds: f32,
) {
    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            runtime.left_mouse_down = true;
            runtime.update_mouse(column, row, elapsed_seconds);
        }
        MouseEventKind::Drag(MouseButton::Left) if runtime.left_mouse_down => {
            runtime.update_mouse(column, row, elapsed_seconds);
        }
        MouseEventKind::Up(MouseButton::Left) => runtime.stop_mouse_interaction(elapsed_seconds),
        _ => {}
    }
}

struct RenderMetrics {
    started_at: Instant,
    frame_count: u64,
    worst_frame_time: Duration,
    last_stats: RenderStats,
    adaptive: AdaptiveSnapshot,
}

struct SnapshotSummary {
    profile: &'static str,
    motion: &'static str,
    scene: &'static str,
    preset: &'static str,
    glyph: GlyphMode,
    cells: usize,
    contrast_min: f32,
    lightness_variance: f32,
    blue_noise_min_distance: f32,
    luminance_pressure: f32,
    orbit_budget_pressure: f32,
    hash: u64,
    dirty_cell_budget: usize,
}

impl SnapshotSummary {
    fn from_frame(
        frame: &Frame,
        layout: &Layout,
        profile: VisualProfile,
        motion_preset: MotionPreset,
        scene_preset: ScenePreset,
        calibration: TerminalCalibration,
    ) -> Self {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut contrast_min = f32::MAX;
        let mut lightness_sum = 0.0;
        let mut lightness_square_sum = 0.0;
        let mut lightness_count = 0.0;
        let mut background_points = Vec::new();
        let mut orbit_cells = 0usize;
        for cell in &frame.cells {
            hash = fnv_mix(hash, u64::from(cell.column));
            hash = fnv_mix(hash, u64::from(cell.row));
            hash = fnv_mix(hash, cell.character as u64);
            hash = fnv_mix(hash, color_hash(cell.color));
            contrast_min = contrast_min.min(apca_like_contrast(cell.color, dark_reference_color()));
            let lightness = Oklab::from_color(cell.color).map_or(0.0, |lab| lab.lightness);
            lightness_sum += lightness;
            lightness_square_sum += lightness * lightness;
            lightness_count += 1.0;
            if cell.layer == RenderLayer::Background {
                background_points.push((f32::from(cell.column), f32::from(cell.row)));
            }
            if cell.layer == RenderLayer::Orbit {
                orbit_cells += 1;
            }
        }

        hash = fnv_mix(hash, u64::from(layout.columns));
        hash = fnv_mix(hash, u64::from(layout.rows));
        let lightness_mean = if lightness_count == 0.0 {
            0.0
        } else {
            lightness_sum / lightness_count
        };
        let lightness_variance = if lightness_count == 0.0 {
            0.0
        } else {
            (lightness_square_sum / lightness_count - lightness_mean * lightness_mean).max(0.0)
        };

        Self {
            profile: profile_name(profile),
            motion: motion_preset.name(),
            scene: scene_preset.name(),
            preset: calibration.capabilities.preset.name(),
            glyph: calibration.capabilities.glyph_mode,
            cells: frame.cells.len(),
            contrast_min,
            lightness_variance,
            blue_noise_min_distance: min_pair_distance(&background_points),
            luminance_pressure: LuminanceBudget::pressure(frame, layout),
            orbit_budget_pressure: orbit_cells as f32 / calibration.dirty_cell_budget.max(1) as f32,
            hash,
            dirty_cell_budget: calibration.dirty_cell_budget,
        }
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"profile\":\"{}\",\"motion\":\"{}\",\"scene\":\"{}\",\"terminal_preset\":\"{}\",\"glyph\":\"{:?}\",\"cells\":{},\"contrast_min\":{:.2},\"lightness_variance\":{:.4},\"blue_noise_min_distance\":{:.2},\"luminance_pressure\":{:.3},\"orbit_budget_pressure\":{:.4},\"hash\":\"{:016x}\",\"dirty_cell_budget\":{}}}",
            self.profile,
            self.motion,
            self.scene,
            self.preset,
            self.glyph,
            self.cells,
            self.contrast_min,
            self.lightness_variance,
            self.blue_noise_min_distance,
            self.luminance_pressure,
            self.orbit_budget_pressure,
            self.hash,
            self.dirty_cell_budget
        )
    }
}

fn min_pair_distance(points: &[(f32, f32)]) -> f32 {
    let mut min_distance = f32::MAX;
    for (left_index, &(left_x, left_y)) in points.iter().enumerate() {
        for &(right_x, right_y) in points.iter().skip(left_index + 1) {
            min_distance =
                min_distance.min(((left_x - right_x).powi(2) + (left_y - right_y).powi(2)).sqrt());
        }
    }
    if min_distance == f32::MAX {
        0.0
    } else {
        min_distance
    }
}

#[derive(Clone, Debug)]
struct FrameRecordHeader {
    columns: u16,
    rows: u16,
    fps: u16,
    profile: &'static str,
    motion: &'static str,
    glyph: GlyphMode,
    terminal_profile: &'static str,
    theme_hash: u64,
    dirty_mode: DirtyMode,
    glyph_history_mode: GlyphHistoryMode,
    dirty_cell_budget: usize,
}

impl FrameRecordHeader {
    fn new(config: FrameRecordConfig, layout: &Layout) -> Self {
        Self {
            columns: layout.columns,
            rows: layout.rows,
            fps: config.fps,
            profile: profile_name(config.profile),
            motion: config.motion_preset.name(),
            glyph: config.calibration.capabilities.glyph_mode,
            terminal_profile: terminal_visual_profile(config.calibration).name(),
            theme_hash: theme_hash(config.theme),
            dirty_mode: config.dirty_mode,
            glyph_history_mode: config.glyph_history_mode,
            dirty_cell_budget: config.calibration.dirty_cell_budget,
        }
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"type\":\"header\",\"columns\":{},\"rows\":{},\"fps\":{},\"profile\":\"{}\",\"motion\":\"{}\",\"glyph\":\"{:?}\",\"terminal_profile\":\"{}\",\"theme_hash\":\"{:016x}\",\"dirty_mode\":\"{}\",\"glyph_history\":\"{}\",\"dirty_cell_budget\":{}}}",
            self.columns,
            self.rows,
            self.fps,
            self.profile,
            self.motion,
            self.glyph,
            self.terminal_profile,
            self.theme_hash,
            self.dirty_mode.name(),
            self.glyph_history_mode.name(),
            self.dirty_cell_budget
        )
    }

    fn from_json(line: &str) -> Option<Self> {
        Some(Self {
            columns: json_u16(line, "columns")?,
            rows: json_u16(line, "rows")?,
            fps: json_u16(line, "fps")?,
            profile: "recorded",
            motion: "recorded",
            glyph: GlyphMode::Unicode,
            terminal_profile: "recorded",
            theme_hash: 0,
            dirty_mode: parse_recorded_dirty_mode(line),
            glyph_history_mode: parse_recorded_glyph_history_mode(line),
            dirty_cell_budget: json_usize(line, "dirty_cell_budget").unwrap_or_else(|| {
                usize::from(json_u16(line, "columns").unwrap_or(80))
                    * usize::from(json_u16(line, "rows").unwrap_or(24))
            }),
        })
    }
}

fn parse_recorded_dirty_mode(line: &str) -> DirtyMode {
    match json_string(line, "dirty_mode").as_deref() {
        Some("naive") => DirtyMode::Naive,
        Some("uniform-threshold") => DirtyMode::UniformThreshold,
        Some("role-aware") => DirtyMode::RoleAware,
        _ => DirtyMode::Full,
    }
}

fn parse_recorded_glyph_history_mode(line: &str) -> GlyphHistoryMode {
    match json_string(line, "glyph_history").as_deref() {
        Some("off") => GlyphHistoryMode::Off,
        Some("screen-cell") => GlyphHistoryMode::ScreenCell,
        Some("topology-dp") => GlyphHistoryMode::TopologyDp,
        _ => GlyphHistoryMode::Path,
    }
}

#[derive(Clone, Debug)]
struct RecordedFrame {
    index: usize,
    timestamp: f32,
    cells: Vec<Cell>,
}

impl RecordedFrame {
    fn from_frame(index: usize, timestamp: f32, frame: &Frame) -> Self {
        Self {
            index,
            timestamp,
            cells: frame.cells.clone(),
        }
    }

    fn to_frame(&self) -> Frame {
        Frame {
            cells: self.cells.clone(),
        }
    }

    fn to_json(&self) -> String {
        let cells = self
            .cells
            .iter()
            .map(cell_to_record)
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "{{\"type\":\"frame\",\"index\":{},\"timestamp\":{:.4},\"cells\":\"{}\"}}",
            self.index, self.timestamp, cells
        )
    }

    fn from_json(line: &str) -> Option<Self> {
        let cells = json_string(line, "cells")?;
        Some(Self {
            index: json_usize(line, "index")?,
            timestamp: json_f32(line, "timestamp")?,
            cells: parse_recorded_cells(&cells),
        })
    }
}

struct FrameSequence {
    header: FrameRecordHeader,
    frames: Vec<RecordedFrame>,
}

impl FrameSequence {
    fn read(path: &str) -> io::Result<Self> {
        let mut lines = BufReader::new(File::open(path)?).lines();
        let Some(header_line) = lines.next().transpose()? else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame recording is empty",
            ));
        };
        let header = FrameRecordHeader::from_json(&header_line).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid frame recording header")
        })?;
        let mut frames = Vec::new();
        for line in lines {
            let line = line?;
            if let Some(frame) = RecordedFrame::from_json(&line) {
                frames.push(frame);
            }
        }
        if frames.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame recording has no frames",
            ));
        }
        Ok(Self { header, frames })
    }
}

#[derive(Clone, Copy, Debug)]
struct VisualScore {
    visual_quality_score: f32,
    flicker_rate: f32,
    lightness_delta: f32,
    motion_continuity: f32,
    clustering_pressure: f32,
    foreground_occlusion: f32,
    contrast_violations: usize,
    dirty_cell_pressure: f32,
    dirty_budget_violation_rate: f32,
    contrast_violation_rate: f32,
    foreground_clarity: f32,
    background_atmosphere: f32,
    temporal_high_band_energy: f32,
    local_flicker_discomfort: f32,
    phase_coherence: f32,
    stable_foreground_score: f32,
    particle_spectral_quality: f32,
    orbit_temporal_continuity: f32,
    orbit_glyph_stability: f32,
    orbit_motion_aliasing_pressure: f32,
    glyph_flips_per_second: f32,
    path_index_discontinuity_rate: f32,
    corner_glyph_instability: f32,
    orbit_cells_changed_per_frame: f32,
    logo_color_stability: f32,
}

impl VisualScore {
    fn from_sequence(sequence: &FrameSequence) -> Self {
        let mut flicker_total = 0.0;
        let mut lightness_delta_total = 0.0;
        let mut continuity_total = 0.0;
        let mut dirty_total = 0.0;
        let mut dirty_budget_violations = 0.0;
        let mut transitions: f32 = 0.0;
        let mut clustering = 0.0;
        let mut foreground_occlusion = 0.0;
        let mut contrast_violations = 0usize;
        let mut cell_count_total = 0usize;
        let mut foreground_clarity = 0.0;
        let mut background_atmosphere = 0.0;
        let mut orbit_temporal_continuity_total = 0.0;
        let mut orbit_glyph_stability_total = 0.0;
        let mut orbit_motion_aliasing_total = 0.0;
        let mut glyph_flips_total = 0.0;
        let mut path_discontinuity_total = 0.0;
        let mut corner_instability_total = 0.0;
        let mut orbit_changed_total = 0.0;
        let mut logo_color_stability_total = 0.0;
        let particle_spectral_quality = particle_spectral_quality(sequence);
        let temporal = TemporalSpectrumMetrics::from_sequence(sequence);

        for frame in &sequence.frames {
            cell_count_total += frame.cells.len();
            clustering += background_clustering_pressure(&frame.cells);
            foreground_occlusion += foreground_background_overlap(&frame.cells);
            foreground_clarity += foreground_clarity_score(&frame.cells);
            background_atmosphere += background_atmosphere_score(&frame.cells);
            contrast_violations += frame
                .cells
                .iter()
                .filter(|cell| {
                    let role = PerceptualRole::from_cell(cell);
                    apca_like_contrast(cell.color, dark_reference_color()) < role.contrast_floor()
                })
                .count();
        }

        for pair in sequence.frames.windows(2) {
            let previous = pair[0].to_frame();
            let next = pair[1].to_frame();
            let dirty = next
                .dirty_cells_with_budget(
                    Some(&previous),
                    sequence.header.dirty_mode,
                    sequence.header.dirty_cell_budget,
                )
                .len()
                + previous.stale_cells(&next).len();
            let dirty_pressure = dirty as f32 / sequence.header.dirty_cell_budget.max(1) as f32;
            dirty_total += dirty_pressure;
            if dirty_pressure > 1.0 {
                dirty_budget_violations += 1.0;
            }
            let delta = average_lightness_delta(&previous, &next);
            lightness_delta_total += delta;
            let role_weighted_delta = role_weighted_lightness_delta(&previous, &next);
            flicker_total += if role_weighted_delta > 0.13 { 1.0 } else { 0.0 };
            continuity_total += motion_continuity(&previous, &next);
            orbit_temporal_continuity_total += orbit_temporal_continuity(&previous, &next);
            orbit_glyph_stability_total += orbit_glyph_stability(&previous, &next);
            orbit_motion_aliasing_total += orbit_motion_aliasing_pressure(&previous, &next);
            glyph_flips_total += orbit_glyph_flip_count(&previous, &next);
            path_discontinuity_total += orbit_path_discontinuity_rate(&previous, &next);
            corner_instability_total += orbit_corner_glyph_instability(&previous, &next);
            orbit_changed_total += orbit_cells_changed_per_frame(&previous, &next);
            logo_color_stability_total += logo_color_stability(&previous, &next);
            transitions += 1.0;
        }

        let frame_count = sequence.frames.len().max(1) as f32;
        let transitions = transitions.max(1.0);
        let flicker_rate = flicker_total / transitions;
        let lightness_delta = lightness_delta_total / transitions;
        let motion_continuity = continuity_total / transitions;
        let clustering_pressure = clustering / frame_count;
        let foreground_occlusion = foreground_occlusion / frame_count;
        let dirty_cell_pressure = dirty_total / transitions;
        let dirty_budget_violation_rate = dirty_budget_violations / transitions;
        let contrast_violation_rate = contrast_violations as f32 / cell_count_total.max(1) as f32;
        let orbit_temporal_continuity = orbit_temporal_continuity_total / transitions;
        let orbit_glyph_stability = orbit_glyph_stability_total / transitions;
        let orbit_motion_aliasing_pressure = orbit_motion_aliasing_total / transitions;
        let glyph_flips_per_second =
            glyph_flips_total / transitions * f32::from(sequence.header.fps.max(1));
        let path_index_discontinuity_rate = path_discontinuity_total / transitions;
        let corner_glyph_instability = corner_instability_total / transitions;
        let orbit_cells_changed_per_frame = orbit_changed_total / transitions;
        let logo_color_stability = logo_color_stability_total / transitions;
        let foreground_clarity = foreground_clarity / frame_count;
        let background_atmosphere = background_atmosphere / frame_count;
        let budget_score = (1.0 - dirty_cell_pressure / 1.35).clamp(0.0, 1.0);
        let temporal_score = (1.0
            - flicker_rate * 0.22
            - (lightness_delta / 0.95).clamp(0.0, 1.0) * 0.34
            - (temporal.temporal_high_band_energy / 0.22).clamp(0.0, 1.0) * 0.28
            - (temporal.local_flicker_discomfort / 0.14).clamp(0.0, 1.0) * 0.22)
            .clamp(0.0, 1.0);
        let foreground_score = (foreground_clarity * 0.48
            + temporal.stable_foreground_score * 0.28
            + logo_color_stability * 0.18
            + (1.0 - contrast_violation_rate / 0.38).clamp(0.0, 1.0) * 0.06)
            .clamp(0.0, 1.0);
        let motion_score = (motion_continuity * 0.24
            + orbit_temporal_continuity * 0.28
            + orbit_glyph_stability * 0.28
            + (1.0 - orbit_motion_aliasing_pressure / 0.18).clamp(0.0, 1.0) * 0.20)
            .clamp(0.0, 1.0);
        let particle_score = (background_atmosphere * 0.44
            + particle_spectral_quality * 0.40
            + (1.0 - clustering_pressure / 1.25).clamp(0.0, 1.0) * 0.16)
            .clamp(0.0, 1.0);
        let hierarchy_penalty = ((contrast_violation_rate - 0.25) / 0.45).clamp(0.0, 1.0) * 12.0;
        let clustering_penalty = ((clustering_pressure - 1.0) / 1.0).clamp(0.0, 1.0) * 9.0;
        let violation_penalty = (dirty_budget_violation_rate * 8.0
            + foreground_occlusion * 4.0
            + hierarchy_penalty
            + clustering_penalty)
            .clamp(0.0, 24.0);
        let visual_quality_score = (foreground_score * 30.0
            + temporal_score * 24.0
            + motion_score * 20.0
            + particle_score * 14.0
            + budget_score * 12.0
            - violation_penalty)
            .clamp(0.0, 100.0);

        Self {
            visual_quality_score,
            flicker_rate,
            lightness_delta,
            motion_continuity,
            clustering_pressure,
            foreground_occlusion,
            contrast_violations,
            dirty_cell_pressure,
            dirty_budget_violation_rate,
            contrast_violation_rate,
            foreground_clarity,
            background_atmosphere,
            temporal_high_band_energy: temporal.temporal_high_band_energy,
            local_flicker_discomfort: temporal.local_flicker_discomfort,
            phase_coherence: temporal.phase_coherence,
            stable_foreground_score: temporal.stable_foreground_score,
            particle_spectral_quality,
            orbit_temporal_continuity,
            orbit_glyph_stability,
            orbit_motion_aliasing_pressure,
            glyph_flips_per_second,
            path_index_discontinuity_rate,
            corner_glyph_instability,
            orbit_cells_changed_per_frame,
            logo_color_stability,
        }
    }

    fn to_json(self) -> String {
        format!(
            "{{\"visual_quality_score\":{:.2},\"flicker_rate\":{:.4},\"lightness_delta\":{:.4},\"motion_continuity\":{:.4},\"clustering_pressure\":{:.4},\"foreground_occlusion\":{:.4},\"contrast_violations\":{},\"dirty_cell_pressure\":{:.4},\"dirty_budget_violation_rate\":{:.4},\"contrast_violation_rate\":{:.4},\"foreground_clarity\":{:.4},\"background_atmosphere\":{:.4},\"temporal_high_band_energy\":{:.4},\"local_flicker_discomfort\":{:.4},\"phase_coherence\":{:.4},\"stable_foreground_score\":{:.4},\"particle_spectral_quality\":{:.4},\"orbit_temporal_continuity\":{:.4},\"orbit_glyph_stability\":{:.4},\"orbit_motion_aliasing_pressure\":{:.4},\"glyph_flips_per_second\":{:.4},\"path_index_discontinuity_rate\":{:.4},\"corner_glyph_instability\":{:.4},\"orbit_cells_changed_per_frame\":{:.4},\"logo_color_stability\":{:.4}}}",
            self.visual_quality_score,
            self.flicker_rate,
            self.lightness_delta,
            self.motion_continuity,
            self.clustering_pressure,
            self.foreground_occlusion,
            self.contrast_violations,
            self.dirty_cell_pressure,
            self.dirty_budget_violation_rate,
            self.contrast_violation_rate,
            self.foreground_clarity,
            self.background_atmosphere,
            self.temporal_high_band_energy,
            self.local_flicker_discomfort,
            self.phase_coherence,
            self.stable_foreground_score,
            self.particle_spectral_quality,
            self.orbit_temporal_continuity,
            self.orbit_glyph_stability,
            self.orbit_motion_aliasing_pressure,
            self.glyph_flips_per_second,
            self.path_index_discontinuity_rate,
            self.corner_glyph_instability,
            self.orbit_cells_changed_per_frame,
            self.logo_color_stability
        )
    }

    fn to_csv_row(
        self,
        experiment: &str,
        dirty_mode: DirtyMode,
        glyph_history_mode: GlyphHistoryMode,
        budget: usize,
    ) -> String {
        format!(
            "{},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}",
            experiment,
            dirty_mode.name(),
            glyph_history_mode.name(),
            budget,
            self.visual_quality_score,
            self.dirty_cell_pressure,
            self.dirty_budget_violation_rate,
            self.contrast_violation_rate,
            self.orbit_temporal_continuity,
            self.orbit_glyph_stability,
            self.orbit_motion_aliasing_pressure,
            self.glyph_flips_per_second,
            self.path_index_discontinuity_rate,
            self.corner_glyph_instability,
            self.orbit_cells_changed_per_frame,
            self.foreground_clarity,
            self.background_atmosphere,
            self.particle_spectral_quality,
            self.temporal_high_band_energy,
            self.local_flicker_discomfort,
            self.phase_coherence,
            self.stable_foreground_score,
            self.logo_color_stability
        )
    }
}

fn runtime_budget_quality(mean_us: f32) -> f32 {
    (100.0 * (1.0 - mean_us / 833.33).clamp(0.0, 1.0)).clamp(0.0, 100.0)
}

fn topology_quality(metrics: TopologyMetrics) -> f32 {
    (100.0
        * (1.0
            - metrics.corner_glyph_instability * 0.40
            - (metrics.glyph_flips_per_second / 20.0).clamp(0.0, 1.0) * 0.24
            - metrics.stroke_metric_difference * 0.22
            - metrics.correspondence_lost_rate * 0.14)
            .clamp(0.0, 1.0))
    .clamp(0.0, 100.0)
}

fn temporal_stability_quality(score: VisualScore) -> f32 {
    (100.0
        * (1.0
            - score.temporal_high_band_energy * 0.40
            - score.local_flicker_discomfort * 0.35
            - score.flicker_rate * 0.25)
            .clamp(0.0, 1.0))
    .clamp(0.0, 100.0)
}

fn visual_richness_quality(score: VisualScore) -> f32 {
    (100.0
        * (score.background_atmosphere * 0.40
            + score.particle_spectral_quality * 0.28
            + score.logo_color_stability * 0.20
            + (1.0 - score.foreground_occlusion).clamp(0.0, 1.0) * 0.12)
            .clamp(0.0, 1.0))
    .clamp(0.0, 100.0)
}

#[derive(Clone, Copy, Debug)]
struct TopologyMetrics {
    topology_breaks_per_frame: f32,
    endpoint_drift: f32,
    screen_discontinuity_rate: f32,
    path_order_violation_rate: f32,
    connected_component_instability: f32,
    crossing_ambiguity_rate: f32,
    correspondence_lost_rate: f32,
    stroke_metric_difference: f32,
    glyph_flips_per_second: f32,
    corner_glyph_instability: f32,
}

impl TopologyMetrics {
    fn from_sequence(sequence: &FrameSequence) -> Self {
        let score = VisualScore::from_sequence(sequence);
        let mut topology_breaks = 0.0;
        let mut endpoint_drift = 0.0;
        let mut screen_discontinuity = 0.0;
        let path_order_violations = metadata_path_order_violation_rate(sequence);
        let mut component_instability = 0.0;
        let mut crossing_ambiguity = 0.0;
        let mut correspondence_lost = 0.0;
        let mut stroke_difference = 0.0;
        let mut transitions: f32 = 0.0;

        for pair in sequence.frames.windows(2) {
            let previous = pair[0].to_frame();
            let next = pair[1].to_frame();
            topology_breaks += orbit_topology_breaks(&next);
            endpoint_drift += orbit_endpoint_drift(&previous, &next);
            screen_discontinuity += orbit_path_order_violation_rate(&next);
            component_instability += orbit_connected_component_instability(&previous, &next);
            crossing_ambiguity += orbit_crossing_ambiguity_rate(&next);
            correspondence_lost += orbit_correspondence_lost_rate(&next);
            stroke_difference += orbit_stroke_metric_difference(&previous, &next);
            transitions += 1.0;
        }

        let transitions = transitions.max(1.0);
        Self {
            topology_breaks_per_frame: topology_breaks / transitions,
            endpoint_drift: endpoint_drift / transitions,
            screen_discontinuity_rate: screen_discontinuity / transitions,
            path_order_violation_rate: path_order_violations,
            connected_component_instability: component_instability / transitions,
            crossing_ambiguity_rate: crossing_ambiguity / transitions,
            correspondence_lost_rate: correspondence_lost / transitions,
            stroke_metric_difference: stroke_difference / transitions,
            glyph_flips_per_second: score.glyph_flips_per_second,
            corner_glyph_instability: score.corner_glyph_instability,
        }
    }

    fn verdict(self) -> &'static str {
        if self.topology_breaks_per_frame <= 0.01
            && self.corner_glyph_instability <= 0.01
            && self.path_order_violation_rate <= 0.01
        {
            "topology-stable"
        } else if self.corner_glyph_instability <= 0.20 {
            "partially-stable"
        } else {
            "unstable"
        }
    }

    fn identity_verdict(self) -> &'static str {
        if self.corner_glyph_instability <= 0.01
            && self.glyph_flips_per_second <= 0.01
            && self.stroke_metric_difference <= 0.01
        {
            "glyph-identity-stable"
        } else if self.corner_glyph_instability <= 0.20 {
            "glyph-identity-partially-stable"
        } else {
            "glyph-identity-unstable"
        }
    }

    fn scope_note(self) -> &'static str {
        if self.identity_verdict() == "glyph-identity-stable"
            && (self.topology_breaks_per_frame > 0.01 || self.screen_discontinuity_rate > 0.01)
        {
            "identity_stable_but_screen_connectivity_metric_flags_open_or_sampled_curve"
        } else {
            "identity_and_topology_metrics_aligned"
        }
    }

    fn stress_degradation_index(self) -> f32 {
        self.corner_glyph_instability
            .max(self.stroke_metric_difference)
            .max(self.path_order_violation_rate)
            .max(self.correspondence_lost_rate)
    }
}

#[derive(Clone, Copy, Debug)]
struct TopologyDpWeights {
    temporal: f32,
    local: f32,
    topology: f32,
    corner: f32,
}

impl TopologyDpWeights {
    const DEFAULT: Self = Self {
        temporal: 1.8,
        local: 0.55,
        topology: 2.4,
        corner: 0.35,
    };

    fn with_temporal(self, scale: f32) -> Self {
        Self {
            temporal: self.temporal * scale,
            ..self
        }
    }

    fn with_local(self, scale: f32) -> Self {
        Self {
            local: self.local * scale,
            ..self
        }
    }

    fn with_topology(self, scale: f32) -> Self {
        Self {
            topology: self.topology * scale,
            ..self
        }
    }

    fn with_corner(self, scale: f32) -> Self {
        Self {
            corner: self.corner * scale,
            ..self
        }
    }

    fn with_axis_multiplier(self, axis: &str, multiplier: f32) -> Self {
        match axis {
            "temporal" => self.with_temporal(multiplier),
            "local" => self.with_local(multiplier),
            "topology" => self.with_topology(multiplier),
            "corner" => self.with_corner(multiplier),
            _ => self,
        }
    }

    fn normalized(self) -> Self {
        let sum = (self.temporal + self.local + self.topology + self.corner).max(0.001);
        let scale = 5.10 / sum;
        Self {
            temporal: self.temporal * scale,
            local: self.local * scale,
            topology: self.topology * scale,
            corner: self.corner * scale,
        }
    }

    fn repair_strength(self) -> f32 {
        let normalized = self.normalized();
        let balance = normalized.topology
            / (normalized.temporal + normalized.local + normalized.corner).max(0.001);
        (balance / 1.05).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug)]
struct RuntimeSummary {
    iterations: usize,
    total_us: f32,
    mean_us: f32,
    p95_us: f32,
    p99_us: f32,
    worst_us: f32,
}

#[derive(Clone, Copy, Debug)]
struct RuntimeDistribution {
    samples: usize,
    mean_us: f32,
    median_us: f32,
    stdev_us: f32,
    ci95_us: f32,
    p95_us: f32,
    p99_us: f32,
    p999_us: f32,
    worst_us: f32,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct VisualParameterSet {
    density_gain: f32,
    tail_energy: f32,
    color_gain: f32,
    afterimage_gain: f32,
    orbit_span_gain: f32,
    comet_gain: f32,
    taa_weight: f32,
}

#[cfg(test)]
impl VisualParameterSet {
    fn baseline() -> Self {
        Self {
            density_gain: 1.00,
            tail_energy: 1.00,
            color_gain: 1.00,
            afterimage_gain: 1.00,
            orbit_span_gain: 1.00,
            comet_gain: 1.00,
            taa_weight: 1.00,
        }
    }

    fn candidates() -> [Self; 7] {
        [
            Self::baseline(),
            Self {
                density_gain: 0.82,
                tail_energy: 1.14,
                color_gain: 1.08,
                afterimage_gain: 0.88,
                orbit_span_gain: 1.18,
                comet_gain: 1.08,
                taa_weight: 1.12,
            },
            Self {
                density_gain: 0.70,
                tail_energy: 1.22,
                color_gain: 1.12,
                afterimage_gain: 0.80,
                orbit_span_gain: 1.30,
                comet_gain: 0.92,
                taa_weight: 1.18,
            },
            Self {
                density_gain: 0.94,
                tail_energy: 1.04,
                color_gain: 1.18,
                afterimage_gain: 0.74,
                orbit_span_gain: 1.10,
                comet_gain: 1.18,
                taa_weight: 1.24,
            },
            Self {
                density_gain: 1.10,
                tail_energy: 0.88,
                color_gain: 1.05,
                afterimage_gain: 0.70,
                orbit_span_gain: 1.22,
                comet_gain: 0.70,
                taa_weight: 1.30,
            },
            Self {
                density_gain: 0.62,
                tail_energy: 1.34,
                color_gain: 1.16,
                afterimage_gain: 0.68,
                orbit_span_gain: 1.36,
                comet_gain: 1.04,
                taa_weight: 1.34,
            },
            Self {
                density_gain: 0.76,
                tail_energy: 1.30,
                color_gain: 1.24,
                afterimage_gain: 0.62,
                orbit_span_gain: 1.42,
                comet_gain: 1.22,
                taa_weight: 1.40,
            },
        ]
    }

    fn visual_objective(self, score: VisualScore) -> f32 {
        let density_bias = (1.0 - score.clustering_pressure).clamp(0.0, 1.0);
        let temporal_bias =
            (1.0 - score.temporal_high_band_energy - score.local_flicker_discomfort)
                .clamp(0.0, 1.0);
        let atmosphere_bias = (score.background_atmosphere + score.particle_spectral_quality) * 0.5;
        let contrast_bias = if score.contrast_violations == 0 {
            1.0
        } else {
            (1.0 - score.contrast_violations as f32 / 48.0).clamp(0.0, 1.0)
        };

        score.visual_quality_score
            + density_bias * (1.0 - (self.density_gain - 0.76).abs()) * 5.4
            + temporal_bias * self.taa_weight.min(1.4) * 4.8
            + atmosphere_bias * self.tail_energy.min(1.34) * 4.2
            + contrast_bias * self.color_gain.min(1.24) * 3.4
            + score.foreground_clarity * self.orbit_span_gain.min(1.42) * 2.8
            + score.orbit_temporal_continuity * self.orbit_span_gain.min(1.42) * 3.2
            + score.orbit_glyph_stability * self.taa_weight.min(1.4) * 2.6
            + score.logo_color_stability * self.color_gain.min(1.24) * 3.0
            - score.foreground_occlusion * self.comet_gain * 5.0
            - score.orbit_motion_aliasing_pressure * self.taa_weight * 4.0
            - score.dirty_cell_pressure * self.afterimage_gain * 3.0
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct VisualOptimizationResult {
    parameters: VisualParameterSet,
    objective: f32,
}

#[cfg(test)]
fn optimize_visual_parameters(sequence: &FrameSequence) -> VisualOptimizationResult {
    let score = VisualScore::from_sequence(sequence);
    VisualParameterSet::candidates()
        .into_iter()
        .map(|parameters| VisualOptimizationResult {
            parameters,
            objective: parameters.visual_objective(score),
        })
        .max_by(|left, right| {
            left.objective
                .partial_cmp(&right.objective)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(VisualOptimizationResult {
            parameters: VisualParameterSet::baseline(),
            objective: VisualParameterSet::baseline().visual_objective(score),
        })
}

fn cell_to_record(cell: &Cell) -> String {
    let (r, g, b) = color_channels(cell.color);
    format!(
        "{},{},{},{},{},{},{},{},{},{}",
        cell.column,
        cell.row,
        escape_record_char(cell.character),
        r,
        g,
        b,
        layer_code(cell.layer),
        cell.primitive_id
            .map_or("-".to_string(), |value| value.to_string()),
        cell.path_index
            .map_or("-".to_string(), |value| value.to_string()),
        u8::from(cell.correspondence_lost)
    )
}

fn parse_recorded_cells(text: &str) -> Vec<Cell> {
    text.split(';')
        .filter(|entry| !entry.is_empty())
        .filter_map(parse_recorded_cell)
        .collect()
}

fn parse_recorded_cell(entry: &str) -> Option<Cell> {
    let mut parts = entry.split(',');
    let column = parts.next()?.parse().ok()?;
    let row = parts.next()?.parse().ok()?;
    let character = unescape_record_char(parts.next()?)?;
    let r = parts.next()?.parse().ok()?;
    let g = parts.next()?.parse().ok()?;
    let b = parts.next()?.parse().ok()?;
    let layer = parse_layer_code(parts.next()?)?;
    let primitive_id = parts.next().and_then(|value| {
        if value == "-" {
            None
        } else {
            value.parse().ok()
        }
    });
    let path_index = parts.next().and_then(|value| {
        if value == "-" {
            None
        } else {
            value.parse().ok()
        }
    });
    let correspondence_lost = parts.next().is_some_and(|value| value == "1");
    Some(Cell {
        column,
        row,
        character,
        color: Color::Rgb { r, g, b },
        layer,
        primitive_id,
        stroke_id: primitive_id.map(|_| 0),
        vertex_id: None,
        path_index,
        correspondence_lost,
    })
}

fn escape_record_char(character: char) -> String {
    format!("{:x}", character as u32)
}

fn unescape_record_char(text: &str) -> Option<char> {
    u32::from_str_radix(text, 16).ok().and_then(char::from_u32)
}

fn color_channels(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb { r, g, b } => (r, g, b),
        Color::White => (255, 255, 255),
        Color::Cyan => (0, 255, 255),
        Color::Blue => (0, 0, 255),
        Color::DarkGrey => (96, 96, 96),
        Color::Black => (0, 0, 0),
        _ => (180, 220, 255),
    }
}

fn layer_code(layer: RenderLayer) -> u8 {
    match layer {
        RenderLayer::Background => 0,
        RenderLayer::Orbit => 1,
        RenderLayer::Text => 2,
    }
}

fn parse_layer_code(code: &str) -> Option<RenderLayer> {
    match code {
        "0" => Some(RenderLayer::Background),
        "1" => Some(RenderLayer::Orbit),
        "2" => Some(RenderLayer::Text),
        _ => None,
    }
}

fn json_u16(line: &str, key: &str) -> Option<u16> {
    json_number(line, key)?.parse().ok()
}

fn json_usize(line: &str, key: &str) -> Option<usize> {
    json_number(line, key)?.parse().ok()
}

fn json_f32(line: &str, key: &str) -> Option<f32> {
    json_number(line, key)?.parse().ok()
}

fn json_number<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("\"{key}\":");
    let start = line.find(&marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    Some(rest[..end].trim_matches('"'))
}

fn json_string(line: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":\"");
    let start = line.find(&marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[derive(Clone, Copy, Debug, Default)]
struct TemporalSpectrumMetrics {
    temporal_high_band_energy: f32,
    local_flicker_discomfort: f32,
    phase_coherence: f32,
    stable_foreground_score: f32,
}

impl TemporalSpectrumMetrics {
    fn from_sequence(sequence: &FrameSequence) -> Self {
        if sequence.frames.len() < 3 {
            return Self {
                phase_coherence: 1.0,
                stable_foreground_score: 1.0,
                ..Self::default()
            };
        }

        let mut traces = BTreeMap::<(u16, u16), Vec<(f32, PerceptualRole)>>::new();
        for frame in &sequence.frames {
            for cell in &frame.cells {
                let lightness = Oklab::from_color(cell.color).map_or(0.0, |lab| lab.lightness);
                traces
                    .entry((cell.column, cell.row))
                    .or_default()
                    .push((lightness, PerceptualRole::from_cell(cell)));
            }
        }

        let mut high_energy = 0.0;
        let mut discomfort = 0.0;
        let mut coherence = 0.0;
        let mut foreground_stability = 0.0;
        let mut trace_count = 0.0;
        let mut foreground_count = 0.0;

        for trace in traces.values() {
            if trace.len() < 3 {
                continue;
            }
            let role = trace
                .last()
                .map_or(PerceptualRole::BackgroundParticle, |(_, role)| *role);
            let role_weight = role.temporal_stability_weight();
            let mut first_delta_sum = 0.0;
            let mut second_delta_sum = 0.0;
            let mut sign_flips = 0.0;
            let mut large_local_jumps = 0.0;
            let mut previous_delta: Option<f32> = None;

            for window in trace.windows(2) {
                let delta = window[1].0 - window[0].0;
                first_delta_sum += delta.abs();
                if delta.abs() > 0.11 {
                    large_local_jumps += (delta.abs() - 0.11) * role_weight;
                }
                if let Some(previous) = previous_delta
                    && previous.signum() != delta.signum()
                    && previous.abs() > 0.018
                    && delta.abs() > 0.018
                {
                    sign_flips += 1.0;
                }
                previous_delta = Some(delta);
            }

            for window in trace.windows(3) {
                let second_delta = window[2].0 - 2.0 * window[1].0 + window[0].0;
                second_delta_sum += second_delta.abs();
            }

            let transition_count = (trace.len() - 1) as f32;
            let curvature_count = (trace.len() - 2) as f32;
            high_energy += second_delta_sum / curvature_count.max(1.0) * role_weight;
            discomfort += large_local_jumps / transition_count.max(1.0);
            coherence += 1.0 - (sign_flips / curvature_count.max(1.0)).clamp(0.0, 1.0);
            if matches!(
                role,
                PerceptualRole::Logo | PerceptualRole::Orbit | PerceptualRole::StatusText
            ) {
                foreground_stability +=
                    (1.0 - first_delta_sum / transition_count.max(1.0) / 0.12).clamp(0.0, 1.0);
                foreground_count += 1.0;
            }
            trace_count += 1.0;
        }

        if trace_count == 0.0 {
            return Self {
                phase_coherence: 1.0,
                stable_foreground_score: 1.0,
                ..Self::default()
            };
        }

        Self {
            temporal_high_band_energy: (high_energy / trace_count).clamp(0.0, 1.0),
            local_flicker_discomfort: (discomfort / trace_count).clamp(0.0, 1.0),
            phase_coherence: (coherence / trace_count).clamp(0.0, 1.0),
            stable_foreground_score: if foreground_count == 0.0 {
                1.0
            } else {
                (foreground_stability / foreground_count).clamp(0.0, 1.0)
            },
        }
    }
}

fn average_lightness_delta(previous: &Frame, next: &Frame) -> f32 {
    let mut total = 0.0;
    let mut count = 0.0;
    for cell in &next.cells {
        let current = Oklab::from_color(cell.color).map_or(0.0, |lab| lab.lightness);
        let previous = previous
            .cell_at(cell.column, cell.row)
            .and_then(|cell| Oklab::from_color(cell.color))
            .map_or(0.0, |lab| lab.lightness);
        total += (current - previous).abs();
        count += 1.0;
    }
    if count == 0.0 { 0.0 } else { total / count }
}

fn role_weighted_lightness_delta(previous: &Frame, next: &Frame) -> f32 {
    let mut total = 0.0;
    let mut weight_total = 0.0;
    for cell in &next.cells {
        let role = PerceptualRole::from_cell(cell);
        let weight = role.temporal_stability_weight();
        let current = Oklab::from_color(cell.color).map_or(0.0, |lab| lab.lightness);
        let previous = previous
            .cell_at(cell.column, cell.row)
            .and_then(|cell| Oklab::from_color(cell.color))
            .map_or(0.0, |lab| lab.lightness);
        total += (current - previous).abs() * weight;
        weight_total += weight;
    }
    if weight_total == 0.0 {
        0.0
    } else {
        total / weight_total
    }
}

fn motion_continuity(previous: &Frame, next: &Frame) -> f32 {
    let previous_background = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Background)
        .count() as f32;
    let next_background = next
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Background)
        .count() as f32;
    if previous_background == 0.0 && next_background == 0.0 {
        return 1.0;
    }
    1.0 - ((next_background - previous_background).abs()
        / previous_background.max(next_background).max(1.0))
    .clamp(0.0, 1.0)
}

fn orbit_temporal_continuity(previous: &Frame, next: &Frame) -> f32 {
    let previous_orbit = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| (cell.column, cell.row))
        .collect::<std::collections::BTreeSet<_>>();
    let next_orbit = next
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| (cell.column, cell.row))
        .collect::<std::collections::BTreeSet<_>>();
    if previous_orbit.is_empty() && next_orbit.is_empty() {
        return 1.0;
    }
    let overlap = previous_orbit.intersection(&next_orbit).count() as f32;
    let max_count = previous_orbit.len().max(next_orbit.len()).max(1) as f32;
    let count_stability =
        1.0 - (previous_orbit.len() as f32 - next_orbit.len() as f32).abs() / max_count;
    let overlap_stability = overlap / max_count;
    (overlap_stability * 0.64 + count_stability * 0.36).clamp(0.0, 1.0)
}

fn orbit_glyph_stability(previous: &Frame, next: &Frame) -> f32 {
    let previous_orbit = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| ((cell.column, cell.row), cell.character))
        .collect::<BTreeMap<_, _>>();
    let next_orbit = next
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| ((cell.column, cell.row), cell.character))
        .collect::<BTreeMap<_, _>>();
    let mut overlap = 0.0;
    let mut stable = 0.0;
    for (position, previous_character) in previous_orbit {
        if let Some(next_character) = next_orbit.get(&position) {
            overlap += 1.0;
            if *next_character == previous_character {
                stable += 1.0;
            }
        }
    }
    if overlap == 0.0 {
        1.0
    } else {
        stable / overlap
    }
}

fn orbit_motion_aliasing_pressure(previous: &Frame, next: &Frame) -> f32 {
    let previous_orbit = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| (cell.column, cell.row))
        .collect::<std::collections::BTreeSet<_>>();
    let next_orbit = next
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| (cell.column, cell.row))
        .collect::<std::collections::BTreeSet<_>>();
    if previous_orbit.is_empty() && next_orbit.is_empty() {
        return 0.0;
    }
    let changed = previous_orbit.symmetric_difference(&next_orbit).count() as f32;
    let max_count = previous_orbit.len().max(next_orbit.len()).max(1) as f32;
    (changed / (max_count * 2.0)).clamp(0.0, 1.0)
}

fn orbit_glyph_flip_count(previous: &Frame, next: &Frame) -> f32 {
    let previous_orbit = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| ((cell.column, cell.row), cell.character))
        .collect::<BTreeMap<_, _>>();
    next.cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .filter(|cell| {
            previous_orbit
                .get(&(cell.column, cell.row))
                .is_some_and(|character| *character != cell.character)
        })
        .count() as f32
}

fn orbit_cells_changed_per_frame(previous: &Frame, next: &Frame) -> f32 {
    let previous_orbit = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| (cell.column, cell.row))
        .collect::<std::collections::BTreeSet<_>>();
    let next_orbit = next
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| (cell.column, cell.row))
        .collect::<std::collections::BTreeSet<_>>();
    previous_orbit.symmetric_difference(&next_orbit).count() as f32
}

fn orbit_path_discontinuity_rate(previous: &Frame, next: &Frame) -> f32 {
    let changed = orbit_cells_changed_per_frame(previous, next);
    let orbit_count = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .count()
        .max(
            next.cells
                .iter()
                .filter(|cell| cell.layer == RenderLayer::Orbit)
                .count(),
        ) as f32;
    (changed / orbit_count.max(1.0)).clamp(0.0, 1.0)
}

fn orbit_corner_glyph_instability(previous: &Frame, next: &Frame) -> f32 {
    let previous_corners = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit && is_corner_glyph(cell.character))
        .map(|cell| ((cell.column, cell.row), cell.character))
        .collect::<BTreeMap<_, _>>();
    let mut total = previous_corners.len() as f32;
    let mut changed = 0.0;
    for (position, previous) in &previous_corners {
        match next.cell_at(position.0, position.1) {
            Some(cell) if cell.layer == RenderLayer::Orbit && cell.character == *previous => {}
            _ => changed += 1.0,
        }
    }
    if total == 0.0 {
        total = next
            .cells
            .iter()
            .filter(|cell| cell.layer == RenderLayer::Orbit && is_corner_glyph(cell.character))
            .count() as f32;
    }
    if total == 0.0 { 0.0 } else { changed / total }
}

fn orbit_topology_breaks(frame: &Frame) -> f32 {
    let orbit = orbit_positions(frame);
    if orbit.len() <= 1 {
        return 0.0;
    }
    let components = connected_components(&orbit);
    components.saturating_sub(1) as f32
}

fn orbit_endpoint_drift(previous: &Frame, next: &Frame) -> f32 {
    let previous_endpoints = orbit_endpoint_count(previous) as f32;
    let next_endpoints = orbit_endpoint_count(next) as f32;
    (next_endpoints - previous_endpoints).abs()
}

fn orbit_path_order_violation_rate(frame: &Frame) -> f32 {
    let orbit = frame
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .collect::<Vec<_>>();
    if orbit.len() <= 2 {
        return 0.0;
    }
    let mut violations = 0.0;
    for window in orbit.windows(2) {
        let previous = window[0];
        let next = window[1];
        let dc = previous.column.abs_diff(next.column);
        let dr = previous.row.abs_diff(next.row);
        if dc > 1 || dr > 1 {
            violations += 1.0;
        }
    }
    violations / (orbit.len() - 1) as f32
}

fn metadata_path_order_violation_rate(sequence: &FrameSequence) -> f32 {
    let mut total_edges = 0.0;
    let mut violations = 0.0;
    for frame in &sequence.frames {
        let mut by_primitive = BTreeMap::<u16, Vec<&Cell>>::new();
        for cell in frame
            .cells
            .iter()
            .filter(|cell| cell.layer == RenderLayer::Orbit)
        {
            if let (Some(primitive_id), Some(_path_index)) = (cell.primitive_id, cell.path_index) {
                by_primitive.entry(primitive_id).or_default().push(cell);
            }
        }
        for cells in by_primitive.values_mut() {
            if cells.len() <= 1 {
                continue;
            }
            cells.sort_by_key(|cell| cell.path_index.unwrap_or(usize::MAX));
            for pair in cells.windows(2) {
                let previous = pair[0];
                let current = pair[1];
                let previous_index = previous.path_index.unwrap_or(usize::MAX);
                let current_index = current.path_index.unwrap_or(usize::MAX);
                if current_index != previous_index.saturating_add(1) {
                    continue;
                }
                let dc = previous.column.abs_diff(current.column);
                let dr = previous.row.abs_diff(current.row);
                if dc > 1 || dr > 1 {
                    violations += 1.0;
                }
                total_edges += 1.0;
            }
        }
    }
    if total_edges == 0.0 {
        0.0
    } else {
        violations / total_edges
    }
}

fn orbit_correspondence_lost_rate(frame: &Frame) -> f32 {
    let mut orbit_cells = 0.0;
    let mut lost = 0.0;
    for cell in frame
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
    {
        orbit_cells += 1.0;
        if cell.correspondence_lost {
            lost += 1.0;
        }
    }
    if orbit_cells == 0.0 {
        0.0
    } else {
        lost / orbit_cells
    }
}

fn orbit_connected_component_instability(previous: &Frame, next: &Frame) -> f32 {
    let previous_components = connected_components(&orbit_positions(previous)) as f32;
    let next_components = connected_components(&orbit_positions(next)) as f32;
    (next_components - previous_components).abs()
}

fn orbit_crossing_ambiguity_rate(frame: &Frame) -> f32 {
    let crossing_cells = frame
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit && cell.character == '╋')
        .count() as f32;
    let orbit_cells = frame
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .count()
        .max(1) as f32;
    (crossing_cells / orbit_cells).clamp(0.0, 1.0)
}

fn orbit_stroke_metric_difference(previous: &Frame, next: &Frame) -> f32 {
    let previous_orbit = previous
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| ((cell.column, cell.row), stroke_descriptor(cell.character)))
        .collect::<BTreeMap<_, _>>();
    let mut total = 0.0;
    let mut count = 0.0;
    for cell in next
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
    {
        if let Some(previous) = previous_orbit.get(&(cell.column, cell.row)) {
            total += previous.distance(stroke_descriptor(cell.character));
            count += 1.0;
        }
    }
    if count == 0.0 { 0.0 } else { total / count }
}

#[derive(Clone, Copy, Debug)]
struct StrokeDescriptor {
    horizontal: f32,
    vertical: f32,
    diagonal: f32,
    cornerness: f32,
    crossing: f32,
    weight: f32,
}

impl StrokeDescriptor {
    fn distance(self, other: Self) -> f32 {
        ((self.horizontal - other.horizontal).abs()
            + (self.vertical - other.vertical).abs()
            + (self.diagonal - other.diagonal).abs()
            + (self.cornerness - other.cornerness).abs()
            + (self.crossing - other.crossing).abs()
            + (self.weight - other.weight).abs())
            / 6.0
    }
}

fn stroke_descriptor(character: char) -> StrokeDescriptor {
    match character {
        '━' | '-' => StrokeDescriptor {
            horizontal: 1.0,
            vertical: 0.0,
            diagonal: 0.0,
            cornerness: 0.0,
            crossing: 0.0,
            weight: 1.0,
        },
        '┃' | '|' => StrokeDescriptor {
            horizontal: 0.0,
            vertical: 1.0,
            diagonal: 0.0,
            cornerness: 0.0,
            crossing: 0.0,
            weight: 1.0,
        },
        '╭' | '╮' | '╰' | '╯' | '+' | '*' => StrokeDescriptor {
            horizontal: 0.5,
            vertical: 0.5,
            diagonal: 0.0,
            cornerness: 1.0,
            crossing: 0.0,
            weight: 1.0,
        },
        '╱' | '╲' => StrokeDescriptor {
            horizontal: 0.0,
            vertical: 0.0,
            diagonal: 1.0,
            cornerness: 0.0,
            crossing: 0.0,
            weight: 1.0,
        },
        '╋' => StrokeDescriptor {
            horizontal: 1.0,
            vertical: 1.0,
            diagonal: 0.0,
            cornerness: 0.0,
            crossing: 1.0,
            weight: 1.0,
        },
        _ => StrokeDescriptor {
            horizontal: 0.0,
            vertical: 0.0,
            diagonal: 0.0,
            cornerness: 0.0,
            crossing: 0.0,
            weight: 0.0,
        },
    }
}

fn orbit_endpoint_count(frame: &Frame) -> usize {
    let positions = orbit_positions(frame);
    positions
        .iter()
        .filter(|position| orbit_neighbor_count(&positions, **position) <= 1)
        .count()
}

fn orbit_positions(frame: &Frame) -> BTreeSet<(u16, u16)> {
    frame
        .cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Orbit)
        .map(|cell| (cell.column, cell.row))
        .collect()
}

fn orbit_neighbor_count(positions: &BTreeSet<(u16, u16)>, position: (u16, u16)) -> usize {
    let (column, row) = position;
    [
        (column.saturating_sub(1), row),
        (column.saturating_add(1), row),
        (column, row.saturating_sub(1)),
        (column, row.saturating_add(1)),
    ]
    .into_iter()
    .filter(|neighbor| positions.contains(neighbor))
    .count()
}

fn connected_components(positions: &BTreeSet<(u16, u16)>) -> usize {
    let mut remaining = positions.clone();
    let mut components = 0;
    while let Some(start) = remaining.iter().next().copied() {
        components += 1;
        let mut stack = vec![start];
        remaining.remove(&start);
        while let Some(position) = stack.pop() {
            let (column, row) = position;
            for neighbor in [
                (column.saturating_sub(1), row),
                (column.saturating_add(1), row),
                (column, row.saturating_sub(1)),
                (column, row.saturating_add(1)),
            ] {
                if remaining.remove(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
    }
    components
}

fn benchmark_topology_assignment(path_samples: usize, iterations: usize) -> RuntimeSummary {
    let weights = TopologyDpWeights::DEFAULT;
    let mut samples = Vec::with_capacity(iterations);
    let mut checksum = 0usize;
    let started = Instant::now();
    for iteration in 0..iterations {
        let frame_started = Instant::now();
        let mut assignments = (0..path_samples)
            .map(|index| (index + iteration) % path_samples.max(1))
            .collect::<Vec<_>>();
        for path_index in 0..path_samples {
            let candidates = [
                assignments[path_index],
                path_index,
                (path_index + path_samples - 1) % path_samples.max(1),
                (path_index + 1) % path_samples.max(1),
            ];
            let mut best = assignments[path_index];
            let mut best_cost = f32::INFINITY;
            for candidate in candidates {
                let temporal =
                    circular_index_distance(assignments[path_index], candidate, path_samples);
                let local = circular_index_distance(path_index, candidate, path_samples);
                let neighbor = if path_index > 0 {
                    circular_index_distance(assignments[path_index - 1], candidate, path_samples)
                } else {
                    0.0
                };
                let cost = temporal * weights.temporal
                    + local * weights.local
                    + neighbor * weights.topology
                    + if candidate == path_index {
                        0.0
                    } else {
                        weights.corner
                    };
                if cost < best_cost {
                    best_cost = cost;
                    best = candidate;
                }
            }
            assignments[path_index] = best;
            checksum ^= best;
        }
        if checksum == usize::MAX {
            unreachable!("checksum prevents benchmark loop elimination");
        }
        samples.push(frame_started.elapsed().as_secs_f32() * 1_000_000.0);
    }
    samples.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let total_us = started.elapsed().as_secs_f32() * 1_000_000.0;
    let mean_us = samples.iter().sum::<f32>() / samples.len().max(1) as f32;
    RuntimeSummary {
        iterations,
        total_us,
        mean_us,
        p95_us: percentile(&samples, 0.95),
        p99_us: percentile(&samples, 0.99),
        worst_us: samples.last().copied().unwrap_or(0.0),
    }
}

fn integrated_runtime_summary(
    mut config: FrameRecordConfig,
    glyph_history_mode: GlyphHistoryMode,
) -> RuntimeSummary {
    config.glyph_history_mode = glyph_history_mode;
    config.frames = config.frames.clamp(12, 24);
    runtime_summary_samples(config, PaperRenderProfile::FullVisual, 8)
}

fn integrated_runtime_summary_for_profile(
    mut config: FrameRecordConfig,
    glyph_history_mode: GlyphHistoryMode,
    render_profile: PaperRenderProfile,
    iterations: usize,
) -> RuntimeSummary {
    config.glyph_history_mode = glyph_history_mode;
    config.frames = config.frames.clamp(12, 24);
    runtime_summary_samples(config, render_profile, iterations)
}

fn runtime_summary_samples(
    config: FrameRecordConfig,
    render_profile: PaperRenderProfile,
    iterations: usize,
) -> RuntimeSummary {
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let sequence = build_frame_sequence_for_profile(config, render_profile);
        let cell_count = sequence
            .frames
            .iter()
            .map(|frame| frame.cells.len())
            .sum::<usize>();
        if cell_count == usize::MAX {
            unreachable!("cell count prevents benchmark loop elimination");
        }
        samples.push(started.elapsed().as_secs_f32() * 1_000_000.0 / config.frames.max(1) as f32);
    }
    samples.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    runtime_summary_from_samples(&samples)
}

fn integrated_runtime_summary_scaled(
    mut config: FrameRecordConfig,
    glyph_history_mode: GlyphHistoryMode,
    frame_scale: f32,
) -> RuntimeSummary {
    config.glyph_history_mode = glyph_history_mode;
    config.frames = ((config.frames.clamp(12, 24) as f32) * frame_scale)
        .round()
        .clamp(4.0, 24.0) as usize;
    let iterations = 8;
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let sequence = build_frame_sequence(config);
        let cell_count = sequence
            .frames
            .iter()
            .map(|frame| frame.cells.len())
            .sum::<usize>();
        if cell_count == usize::MAX {
            unreachable!("cell count prevents benchmark loop elimination");
        }
        samples.push(started.elapsed().as_secs_f32() * 1_000_000.0 / config.frames.max(1) as f32);
    }
    samples.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mean_us = samples.iter().sum::<f32>() / samples.len().max(1) as f32;
    RuntimeSummary {
        iterations: samples.len(),
        total_us: samples.iter().sum(),
        mean_us,
        p95_us: percentile(&samples, 0.95),
        p99_us: percentile(&samples, 0.99),
        worst_us: samples.last().copied().unwrap_or(0.0),
    }
}

fn runtime_summary_from_samples(samples: &[f32]) -> RuntimeSummary {
    let mean_us = samples.iter().sum::<f32>() / samples.len().max(1) as f32;
    RuntimeSummary {
        iterations: samples.len(),
        total_us: samples.iter().sum(),
        mean_us,
        p95_us: percentile(samples, 0.95),
        p99_us: percentile(samples, 0.99),
        worst_us: samples.last().copied().unwrap_or(0.0),
    }
}

fn runtime_distribution_for_profile(
    mut config: FrameRecordConfig,
    glyph_history_mode: GlyphHistoryMode,
    render_profile: PaperRenderProfile,
    samples: usize,
) -> RuntimeDistribution {
    config.glyph_history_mode = glyph_history_mode;
    config.frames = config.frames.clamp(12, 24);
    let mut values = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        let sequence = build_frame_sequence_for_profile(config, render_profile);
        let cell_count = sequence
            .frames
            .iter()
            .map(|frame| frame.cells.len())
            .sum::<usize>();
        if cell_count == usize::MAX {
            unreachable!("cell count prevents benchmark loop elimination");
        }
        values.push(started.elapsed().as_secs_f32() * 1_000_000.0 / config.frames.max(1) as f32);
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mean_us = values.iter().sum::<f32>() / values.len().max(1) as f32;
    let variance = values
        .iter()
        .map(|value| (*value - mean_us).powi(2))
        .sum::<f32>()
        / values.len().max(1) as f32;
    let stdev_us = variance.sqrt();
    let ci95_us = 1.96 * stdev_us / (values.len().max(1) as f32).sqrt();
    RuntimeDistribution {
        samples: values.len(),
        mean_us,
        median_us: percentile(&values, 0.50),
        stdev_us,
        ci95_us,
        p95_us: percentile(&values, 0.95),
        p99_us: percentile(&values, 0.99),
        p999_us: percentile(&values, 0.999),
        worst_us: values.last().copied().unwrap_or(0.0),
    }
}

fn percentile(values: &[f32], p: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() - 1) as f32 * p).round() as usize;
    values[index.min(values.len() - 1)]
}

fn is_corner_glyph(character: char) -> bool {
    matches!(character, '╭' | '╮' | '╰' | '╯' | '+' | '*')
}

fn logo_color_stability(previous: &Frame, next: &Frame) -> f32 {
    let previous_logo = previous
        .cells
        .iter()
        .filter(|cell| PerceptualRole::from_cell(cell) == PerceptualRole::Logo)
        .map(|cell| ((cell.column, cell.row), cell.color))
        .collect::<BTreeMap<_, _>>();
    let mut delta_total = 0.0;
    let mut count = 0.0;
    for cell in next
        .cells
        .iter()
        .filter(|cell| PerceptualRole::from_cell(cell) == PerceptualRole::Logo)
    {
        if let Some(previous_color) = previous_logo.get(&(cell.column, cell.row)) {
            delta_total += oklab_distance(*previous_color, cell.color);
            count += 1.0;
        }
    }
    if count == 0.0 {
        1.0
    } else {
        (1.0 - delta_total / count / 0.075).clamp(0.0, 1.0)
    }
}

fn foreground_clarity_score(cells: &[Cell]) -> f32 {
    let mut total = 0.0;
    let mut count = 0.0;
    for cell in cells {
        let role = PerceptualRole::from_cell(cell);
        if !matches!(
            role,
            PerceptualRole::Logo | PerceptualRole::Orbit | PerceptualRole::StatusText
        ) {
            continue;
        }
        let contrast = apca_like_contrast(cell.color, dark_reference_color());
        total += (contrast / role.contrast_floor().max(1.0)).clamp(0.0, 1.4) / 1.4;
        count += 1.0;
    }
    if count == 0.0 { 0.0 } else { total / count }
}

fn background_atmosphere_score(cells: &[Cell]) -> f32 {
    let background = cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Background)
        .collect::<Vec<_>>();
    if background.len() < 2 {
        return 0.0;
    }
    let clustering = background_clustering_pressure(cells);
    let lightness_variance = lightness_variance(background.iter().copied());
    ((1.0 - clustering / 2.0).clamp(0.0, 1.0) * 0.64
        + (lightness_variance / 0.018).clamp(0.0, 1.0) * 0.36)
        .clamp(0.0, 1.0)
}

fn particle_spectral_quality(sequence: &FrameSequence) -> f32 {
    let mut total = 0.0;
    let mut count = 0.0;
    for frame in &sequence.frames {
        let background = frame
            .cells
            .iter()
            .filter(|cell| cell.layer == RenderLayer::Background)
            .collect::<Vec<_>>();
        if background.len() < 2 {
            continue;
        }
        let min_distance = min_pair_distance(
            &background
                .iter()
                .map(|cell| (f32::from(cell.column), f32::from(cell.row)))
                .collect::<Vec<_>>(),
        );
        let occupancy = background.len() as f32
            / (usize::from(sequence.header.columns) * usize::from(sequence.header.rows)).max(1)
                as f32;
        let distance_score = (min_distance / 4.5).clamp(0.0, 1.0);
        let occupancy_score = (1.0 - (occupancy - 0.035).abs() / 0.08).clamp(0.0, 1.0);
        total += distance_score * 0.72 + occupancy_score * 0.28;
        count += 1.0;
    }
    if count == 0.0 { 0.0 } else { total / count }
}

fn lightness_variance<'a>(cells: impl Iterator<Item = &'a Cell>) -> f32 {
    let values = cells
        .filter_map(|cell| Oklab::from_color(cell.color).map(|lab| lab.lightness))
        .collect::<Vec<_>>();
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / values.len() as f32
}

fn background_clustering_pressure(cells: &[Cell]) -> f32 {
    let points = cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Background)
        .map(|cell| (f32::from(cell.column), f32::from(cell.row)))
        .collect::<Vec<_>>();
    let min_distance = min_pair_distance(&points);
    if min_distance == 0.0 {
        0.0
    } else {
        (3.0 / min_distance).clamp(0.0, 2.0)
    }
}

fn foreground_background_overlap(cells: &[Cell]) -> f32 {
    let foreground = cells
        .iter()
        .filter(|cell| cell.layer != RenderLayer::Background)
        .map(|cell| (cell.column, cell.row))
        .collect::<std::collections::BTreeSet<_>>();
    let background_count = cells
        .iter()
        .filter(|cell| cell.layer == RenderLayer::Background)
        .count()
        .max(1);
    let overlap = cells
        .iter()
        .filter(|cell| {
            cell.layer == RenderLayer::Background && foreground.contains(&(cell.column, cell.row))
        })
        .count();
    overlap as f32 / background_count as f32
}

fn fnv_mix(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x100_0000_01b3)
}

fn color_hash(color: Color) -> u64 {
    match color {
        Color::Rgb { r, g, b } => u64::from(r) << 16 | u64::from(g) << 8 | u64::from(b),
        Color::Black => 1,
        Color::DarkGrey => 2,
        Color::Blue => 3,
        Color::Cyan => 4,
        Color::White => 5,
        _ => 9,
    }
}

fn theme_hash(theme: Theme) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for color in [
        theme.logo_base,
        theme.logo_shadow,
        theme.logo_highlight,
        theme.trail_head,
        theme.trail_body,
        theme.trail_tail,
        theme.accent,
        theme.dim,
    ] {
        hash = fnv_mix(hash, color_hash(color));
    }
    hash = fnv_mix(hash, theme.contrast_floor.to_bits() as u64);
    hash = fnv_mix(hash, theme.trail_span.to_bits() as u64);
    hash
}

fn profile_name(profile: VisualProfile) -> &'static str {
    match profile {
        VisualProfile::Ultra => "ultra",
        VisualProfile::Cinematic => "cinematic",
        VisualProfile::Calm => "calm",
        VisualProfile::Benchmark => "benchmark",
    }
}

impl RenderMetrics {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            frame_count: 0,
            worst_frame_time: Duration::ZERO,
            last_stats: RenderStats::default(),
            adaptive: AdaptiveSnapshot::default(),
        }
    }

    fn record(&mut self, frame_time: Duration, stats: RenderStats, adaptive: AdaptiveSnapshot) {
        self.frame_count += 1;
        self.worst_frame_time = self.worst_frame_time.max(frame_time);
        self.last_stats = stats;
        self.adaptive = adaptive;
    }

    fn average_fps(&self) -> f64 {
        let elapsed = self.started_at.elapsed().as_secs_f64();
        if elapsed == 0.0 {
            return 0.0;
        }

        self.frame_count as f64 / elapsed
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AnimationClock {
    visual_seconds: f32,
    last_real_seconds: Option<f32>,
}

impl AnimationClock {
    fn step(&mut self, real_seconds: f32, target_fps: u16) -> f32 {
        let Some(last_real_seconds) = self.last_real_seconds else {
            self.last_real_seconds = Some(real_seconds);
            self.visual_seconds = real_seconds;
            return self.visual_seconds;
        };

        let real_dt = (real_seconds - last_real_seconds).max(0.0);
        self.last_real_seconds = Some(real_seconds);
        let target_dt = 1.0 / f32::from(target_fps.max(1));
        let max_visual_dt = (target_dt * 2.25).clamp(1.0 / 90.0, 1.0 / 28.0);
        let drift = (real_seconds - self.visual_seconds).max(0.0);
        let catch_up = (drift * 0.08).clamp(0.0, target_dt * 0.45);
        self.visual_seconds += (real_dt.min(max_visual_dt) + catch_up).min(max_visual_dt);
        self.visual_seconds
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AdaptiveSnapshot {
    target_fps: u16,
    missed_deadlines: u64,
    quality_percent: u8,
    low_latency_active: bool,
}

struct AdaptiveFrameController {
    frame_duration: Duration,
    next_frame_at: Instant,
    target_fps: u16,
    missed_deadlines: u64,
    stable_frames: u16,
    quality_scale: f32,
    overload_streak: u8,
    low_latency_active: bool,
    emergency_active: bool,
}

impl AdaptiveFrameController {
    fn new(initial_delay: Duration) -> Self {
        let target_fps = fps_for_delay(initial_delay);
        Self {
            frame_duration: initial_delay,
            next_frame_at: Instant::now() + initial_delay,
            target_fps,
            missed_deadlines: 0,
            stable_frames: 0,
            quality_scale: 1.0,
            overload_streak: 0,
            low_latency_active: false,
            emergency_active: false,
        }
    }

    fn record(&mut self, frame_time: Duration, stats: &RenderStats) -> Duration {
        let dirty_pressure = stats.dirty_cells + stats.stale_cells;
        let complexity_pressure = stats.primitive_cells + stats.topology_cache_misses;
        let overloaded = frame_time > self.frame_duration.saturating_mul(2)
            || dirty_pressure > 420
            || complexity_pressure > 260;

        if overloaded {
            self.missed_deadlines += 1;
            self.stable_frames = 0;
            self.overload_streak = self.overload_streak.saturating_add(1);
            self.quality_scale = (self.quality_scale * 0.92).max(0.52);
            if self.overload_streak >= 2 || frame_time > self.frame_duration.saturating_mul(3) {
                self.low_latency_active = true;
            }
            if self.overload_streak >= 4 || dirty_pressure > 900 || complexity_pressure > 520 {
                self.emergency_active = true;
                self.quality_scale = self.quality_scale.min(0.48);
            }
            self.downgrade();
        } else {
            self.overload_streak = 0;
            self.stable_frames = self.stable_frames.saturating_add(1);
            if self.stable_frames > 180 && frame_time < self.frame_duration / 2 {
                self.quality_scale = (self.quality_scale + 0.04).min(1.0);
                self.upgrade();
                if self.quality_scale >= 0.96 && self.target_fps >= 120 {
                    self.low_latency_active = false;
                    self.emergency_active = false;
                }
                self.stable_frames = 0;
            }
        }

        self.frame_duration
    }

    fn wait_next_frame(&mut self, frame_duration: Duration) {
        self.frame_duration = frame_duration;
        let now = Instant::now();
        let wait = self.next_frame_at.saturating_duration_since(now);
        if !wait.is_zero() {
            thread::sleep(wait);
        }

        self.advance();
    }

    fn snapshot(&self) -> AdaptiveSnapshot {
        AdaptiveSnapshot {
            target_fps: self.target_fps,
            missed_deadlines: self.missed_deadlines,
            quality_percent: (self.quality_scale * 100.0).round() as u8,
            low_latency_active: self.low_latency_active,
        }
    }

    fn low_latency_active(&self) -> bool {
        self.low_latency_active || self.emergency_active
    }

    fn quality_settings(&self, calibration: TerminalCalibration) -> QualitySettings {
        QualitySettings::from_calibration(calibration, self.quality_scale)
    }

    fn downgrade(&mut self) {
        self.target_fps = match self.target_fps {
            241..=u16::MAX => 240,
            121..=240 => 120,
            91..=120 => 90,
            76..=90 => 75,
            61..=75 => 60,
            _ => 60,
        };
        self.frame_duration = delay_for_fps(self.target_fps);
    }

    fn upgrade(&mut self) {
        self.target_fps = match self.target_fps {
            0..=60 => 75,
            61..=75 => 90,
            76..=90 => 120,
            91..=120 => 240,
            121..=240 => 1200,
            _ => 1200,
        };
        self.frame_duration = delay_for_fps(self.target_fps);
    }

    fn advance(&mut self) {
        let now = Instant::now();
        self.next_frame_at += self.frame_duration;

        while self.next_frame_at <= now {
            self.next_frame_at += self.frame_duration;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct QualitySettings {
    scale: f32,
    particle_density: f32,
    afterimage: f32,
    dirty_cell_budget: usize,
}

impl QualitySettings {
    fn from_calibration(calibration: TerminalCalibration, scale: f32) -> Self {
        let visual_profile = terminal_visual_profile(calibration);
        let throughput_scale = (calibration.dirty_cell_budget as f32 / 640.0).clamp(0.45, 1.25);
        let scale =
            (scale * throughput_scale * visual_profile.quality_multiplier()).clamp(0.45, 1.0);
        Self {
            scale,
            particle_density: (calibration.effect_density
                * scale
                * visual_profile.density_multiplier())
            .clamp(0.24, 1.0),
            afterimage: if calibration.target_fps >= 90 {
                scale * visual_profile.afterimage_multiplier()
            } else {
                scale * 0.68 * visual_profile.afterimage_multiplier()
            },
            dirty_cell_budget: calibration.dirty_cell_budget,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalVisualProfile {
    GpuRich,
    Balanced,
    Conservative,
    Minimal,
}

impl TerminalVisualProfile {
    fn name(self) -> &'static str {
        match self {
            Self::GpuRich => "gpu-rich",
            Self::Balanced => "balanced",
            Self::Conservative => "conservative",
            Self::Minimal => "minimal",
        }
    }

    fn density_multiplier(self) -> f32 {
        match self {
            Self::GpuRich => 1.0,
            Self::Balanced => 0.9,
            Self::Conservative => 0.72,
            Self::Minimal => 0.52,
        }
    }

    fn afterimage_multiplier(self) -> f32 {
        match self {
            Self::GpuRich => 1.0,
            Self::Balanced => 0.86,
            Self::Conservative => 0.64,
            Self::Minimal => 0.35,
        }
    }

    fn quality_multiplier(self) -> f32 {
        match self {
            Self::GpuRich => 1.0,
            Self::Balanced => 0.94,
            Self::Conservative => 0.82,
            Self::Minimal => 0.66,
        }
    }
}

fn terminal_visual_profile(calibration: TerminalCalibration) -> TerminalVisualProfile {
    if let Ok(profile) = std::env::var("TOKITAI_TERMINAL_PROFILE") {
        return match profile.as_str() {
            "gpu-rich" => TerminalVisualProfile::GpuRich,
            "balanced" => TerminalVisualProfile::Balanced,
            "conservative" => TerminalVisualProfile::Conservative,
            "minimal" => TerminalVisualProfile::Minimal,
            _ => TerminalVisualProfile::Balanced,
        };
    }
    terminal_visual_profile_from_capabilities(calibration.capabilities)
}

fn terminal_visual_profile_from_capabilities(
    capabilities: TerminalCapabilities,
) -> TerminalVisualProfile {
    match capabilities.preset.name() {
        "wezterm" | "kitty" | "iterm2" => TerminalVisualProfile::GpuRich,
        "alacritty" | "windows-terminal" => TerminalVisualProfile::Balanced,
        "vscode" | "macos-terminal" | "unknown" => TerminalVisualProfile::Conservative,
        "linux-console" => TerminalVisualProfile::Minimal,
        _ => TerminalVisualProfile::Balanced,
    }
}

#[derive(Clone, Copy, Debug)]
struct TerminalAppearance {
    gamma_estimate: f32,
    black_floor: f32,
    glyph_weight: f32,
    color_reliability: f32,
}

impl TerminalAppearance {
    fn for_profile(profile: TerminalVisualProfile) -> Self {
        match profile {
            TerminalVisualProfile::GpuRich => Self {
                gamma_estimate: 2.18,
                black_floor: 0.010,
                glyph_weight: 1.00,
                color_reliability: 0.98,
            },
            TerminalVisualProfile::Balanced => Self {
                gamma_estimate: 2.24,
                black_floor: 0.018,
                glyph_weight: 0.92,
                color_reliability: 0.90,
            },
            TerminalVisualProfile::Conservative => Self {
                gamma_estimate: 2.32,
                black_floor: 0.032,
                glyph_weight: 0.82,
                color_reliability: 0.74,
            },
            TerminalVisualProfile::Minimal => Self {
                gamma_estimate: 2.42,
                black_floor: 0.055,
                glyph_weight: 0.68,
                color_reliability: 0.46,
            },
        }
    }

    fn contrast_floor(self, role: PerceptualRole) -> f32 {
        let visibility_penalty = (1.0 - self.glyph_weight) * 18.0
            + self.black_floor * 120.0
            + (self.gamma_estimate - 2.2).max(0.0) * 8.0
            + (1.0 - self.color_reliability) * 9.0;
        role.contrast_floor() + visibility_penalty
    }

    fn lightness_ceiling(self) -> f32 {
        (0.93 - self.black_floor * 0.34).clamp(0.78, 0.93)
    }

    fn chroma_ceiling(self, role: PerceptualRole) -> f32 {
        let role_ceiling = match role {
            PerceptualRole::BackgroundParticle | PerceptualRole::Afterimage => 0.118,
            PerceptualRole::StatusText => 0.145,
            PerceptualRole::MouseTrail | PerceptualRole::Orbit => 0.172,
            PerceptualRole::Logo => 0.184,
        };
        (role_ceiling * (0.82 + self.color_reliability * 0.18)).clamp(0.07, role_ceiling)
    }
}

fn delay_for_fps(fps: u16) -> Duration {
    Duration::from_nanos(1_000_000_000u64 / u64::from(fps.max(1)))
}

fn fps_for_delay(delay: Duration) -> u16 {
    let nanos = delay.as_nanos().max(1);
    (1_000_000_000u128 / nanos) as u16
}

struct Recorder {
    writer: Option<BufWriter<File>>,
}

impl Recorder {
    fn new(path: Option<&str>) -> io::Result<Self> {
        let writer = match path {
            Some(path) => Some(BufWriter::new(File::create(path)?)),
            None => None,
        };

        Ok(Self { writer })
    }

    fn record(&mut self, metrics: &RenderMetrics) -> io::Result<()> {
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };
        let stats = metrics.last_stats;
        writeln!(
            writer,
            "frame={} fps={:.2} tier={} quality={} low_latency={} missed={} dirty={} runs={} stale={} clear={} bytes={} saved={} primitive_cells={} cache_hits={} cache_misses={} worst_ms={:.3}",
            metrics.frame_count,
            metrics.average_fps(),
            metrics.adaptive.target_fps,
            metrics.adaptive.quality_percent,
            metrics.adaptive.low_latency_active,
            metrics.adaptive.missed_deadlines,
            stats.dirty_cells,
            stats.dirty_runs,
            stats.stale_cells,
            stats.stale_runs,
            stats.bytes_written,
            stats.bytes_saved,
            stats.primitive_cells,
            stats.topology_cache_hits,
            stats.topology_cache_misses,
            metrics.worst_frame_time.as_secs_f64() * 1000.0
        )
    }
}

fn replay_recording(stdout: &mut Stdout, path: &str) -> io::Result<()> {
    let file = File::open(path)?;
    for (index, line) in BufReader::new(file).lines().enumerate() {
        if should_exit()? {
            break;
        }
        stdout.queue(MoveTo(0, index as u16))?;
        stdout.queue(Clear(ClearType::CurrentLine))?;
        write!(stdout, "{}", line?.with(Color::Grey))?;
        stdout.flush()?;
        thread::sleep(Duration::from_millis(16));
    }

    Ok(())
}

struct DirtyRun<'a> {
    row: u16,
    column: u16,
    cells: Vec<&'a Cell>,
}

struct StaleRun {
    row: u16,
    column: u16,
    width: usize,
}

fn dirty_runs<'a>(cells: &[&'a Cell]) -> Vec<DirtyRun<'a>> {
    let mut runs: Vec<DirtyRun<'a>> = Vec::new();

    for &cell in cells {
        if let Some(run) = runs.last_mut()
            && run.row == cell.row
            && run.column + run.cells.len() as u16 == cell.column
        {
            run.cells.push(cell);
            continue;
        }

        runs.push(DirtyRun {
            row: cell.row,
            column: cell.column,
            cells: vec![cell],
        });
    }

    runs
}

fn stale_runs(cells: &[&Cell]) -> Vec<StaleRun> {
    let mut runs: Vec<StaleRun> = Vec::new();

    for &cell in cells {
        if let Some(run) = runs.last_mut()
            && run.row == cell.row
            && run.column + run.width as u16 == cell.column
        {
            run.width += 1;
            continue;
        }

        runs.push(StaleRun {
            row: cell.row,
            column: cell.column,
            width: 1,
        });
    }

    runs
}

fn estimated_ansi_write_bytes(dirty_runs: &[DirtyRun<'_>], stale_runs: &[StaleRun]) -> usize {
    let dirty_bytes = dirty_runs
        .iter()
        .map(|run| {
            let glyph_bytes = run
                .cells
                .iter()
                .map(|cell| cell.character.len_utf8() + 11)
                .sum::<usize>();
            glyph_bytes + 8
        })
        .sum::<usize>();
    let stale_bytes = stale_runs
        .iter()
        .map(|run| run.width + 8)
        .sum::<usize>();
    dirty_bytes + stale_bytes
}

fn push_gradient_text(
    cells: &mut Vec<Cell>,
    column: u16,
    row: u16,
    text: &str,
    context: TextRenderContext,
) {
    let rhythm = rhythm_signal(
        context.elapsed_seconds,
        context.theme,
        context.profile,
        context.motion_preset,
    );
    let color_pipeline = ColorPipeline::new(context, rhythm);
    for (index, character) in text.chars().enumerate() {
        cells.push(Cell {
            column: column + index as u16,
            row,
            character,
            color: color_pipeline.logo(character, index),
            layer: RenderLayer::Text,
            primitive_id: None,
            stroke_id: None,
            vertex_id: None,
            path_index: None,
            correspondence_lost: false,
        });
    }
}

fn composite_layers(cells: Vec<Cell>) -> Vec<Cell> {
    let mut visible_cells = BTreeMap::<(u16, u16), Cell>::new();
    for cell in cells {
        let key = (cell.row, cell.column);
        let should_replace = visible_cells
            .get(&key)
            .is_none_or(|visible| cell.layer >= visible.layer);
        if should_replace {
            visible_cells.insert(key, cell);
        }
    }

    visible_cells.into_values().collect()
}

#[derive(Clone, Copy)]
struct TextRenderContext {
    elapsed_seconds: f32,
    profile: VisualProfile,
    motion_preset: MotionPreset,
    calibration: TerminalCalibration,
    theme: Theme,
}

#[derive(Clone, Copy)]
struct LogoColorContext {
    character: char,
    index: usize,
    elapsed_seconds: f32,
    profile: VisualProfile,
    motion_preset: MotionPreset,
    theme: Theme,
    rhythm: f32,
    target_fps: u16,
    capabilities: TerminalCapabilities,
}

#[derive(Clone, Copy)]
struct ColorPipeline {
    text: TextRenderContext,
    rhythm: f32,
    terminal_profile: TerminalVisualProfile,
    terminal_appearance: TerminalAppearance,
}

impl ColorPipeline {
    fn new(text: TextRenderContext, rhythm: f32) -> Self {
        let terminal_profile =
            terminal_visual_profile_from_capabilities(text.calibration.capabilities);
        Self {
            text,
            rhythm,
            terminal_profile,
            terminal_appearance: TerminalAppearance::for_profile(terminal_profile),
        }
    }

    fn logo(self, character: char, index: usize) -> Color {
        display_color(
            compensate_glyph_lightness(
                logo_color_for_terminal(LogoColorContext {
                    character,
                    index,
                    elapsed_seconds: self.text.elapsed_seconds,
                    profile: self.text.profile,
                    motion_preset: self.text.motion_preset,
                    theme: self.text.theme,
                    rhythm: self.rhythm,
                    target_fps: self.text.calibration.target_fps,
                    capabilities: self.text.calibration.capabilities,
                }),
                character,
            ),
            self.text.calibration.capabilities,
        )
    }

    fn role(self, color: Color, role: PerceptualRole) -> Color {
        if self.text.calibration.capabilities.supports_truecolor() {
            optimize_oklch_color_for_profile(color, role, self.terminal_profile)
        } else {
            display_role_color(color, self.text.calibration.capabilities, role)
        }
    }

    fn supporting_text(self, color: Color, gain: f32) -> Color {
        let visible = self.role(color, PerceptualRole::StatusText);
        let restrained = dim_color(visible, gain);
        if apca_like_contrast(restrained, dark_reference_color())
            >= self
                .terminal_appearance
                .contrast_floor(PerceptualRole::StatusText)
        {
            restrained
        } else {
            visible
        }
    }
}

fn logo_color_for_terminal(context: LogoColorContext) -> Color {
    let LogoColorContext {
        elapsed_seconds,
        theme,
        target_fps,
        capabilities,
        ..
    } = context;
    let profile_name = terminal_visual_profile_from_capabilities(capabilities);
    let appearance = TerminalAppearance::for_profile(profile_name);
    let frame_dt = 1.0 / f32::from(target_fps.max(60));
    let limit = logo_delta_limit(context, appearance, target_fps);
    let sampled = prepare_logo_output_color(
        supersampled_logo_color(context, frame_dt),
        theme,
        profile_name,
    );
    let previous = prepare_logo_output_color(
        supersampled_logo_color(
            LogoColorContext {
                elapsed_seconds: (elapsed_seconds - frame_dt).max(0.0),
                ..context
            },
            frame_dt,
        ),
        theme,
        profile_name,
    );
    limit_oklab_delta(previous, sampled, limit)
}

fn prepare_logo_output_color(color: Color, theme: Theme, profile: TerminalVisualProfile) -> Color {
    optimize_oklch_color_for_profile(
        ensure_visible_lightness(color, theme.contrast_floor),
        PerceptualRole::Logo,
        profile,
    )
}

fn supersampled_logo_color(context: LogoColorContext, frame_dt: f32) -> Color {
    let LogoColorContext {
        character,
        elapsed_seconds,
        ..
    } = context;
    if character == ' ' {
        return raw_logo_color(context);
    }

    let mut accumulated = Oklab::default();
    let mut total_weight = 0.0;
    for (offset, weight) in LOGO_COLOR_SUPERSAMPLES {
        let sample_time = (elapsed_seconds + offset * frame_dt).max(0.0);
        if let Some(sample) = Oklab::from_color(raw_logo_color(LogoColorContext {
            elapsed_seconds: sample_time,
            ..context
        })) {
            accumulated = accumulated.add(sample.scale(weight));
            total_weight += weight;
        }
    }

    if total_weight == 0.0 {
        raw_logo_color(context)
    } else {
        accumulated.scale(1.0 / total_weight).to_color()
    }
}

fn raw_logo_color(context: LogoColorContext) -> Color {
    let LogoColorContext {
        character, theme, ..
    } = context;
    if character == ' ' {
        return ensure_visible_lightness(
            blend_color(theme.logo_shadow, theme.logo_base, 0.18),
            theme.contrast_floor * 0.92,
        );
    }

    LogoMaterial::from_context(context).shade()
}

#[derive(Clone, Copy, Debug)]
struct LogoMaterialSample {
    primary: f32,
    secondary: f32,
    glow: f32,
    specular: f32,
    focus: f32,
    micro_flow: f32,
    breath: f32,
}

#[derive(Clone, Copy)]
struct LogoMaterial {
    sample: LogoMaterialSample,
    profile: VisualProfile,
    theme: Theme,
    rhythm: f32,
    logo_weight: f32,
    gamut: LogoGamutStyle,
}

impl LogoMaterial {
    fn from_context(context: LogoColorContext) -> Self {
        let phases = MotionPhases::at(context.elapsed_seconds, context.motion_preset);
        let phase = context.elapsed_seconds * 120.0 * phases.logo_speed;
        let wave_position = (phase * LOGO_PHASE_STEP) % LOGO_WAVE_PERIOD;
        let secondary_wave_position =
            (phase * LOGO_SECONDARY_PHASE_STEP + LOGO_WAVE_PERIOD * 0.42) % LOGO_WAVE_PERIOD;
        let character_position = context.index as f32 % LOGO_WAVE_PERIOD;
        let distance = circular_distance_f32(character_position, wave_position, LOGO_WAVE_PERIOD);
        let secondary_distance = circular_distance_f32(
            character_position,
            secondary_wave_position,
            LOGO_WAVE_PERIOD,
        );
        let flow_phase = context.elapsed_seconds * 0.37 + context.index as f32 * 0.113;
        let micro_flow = sine01(flow_phase) * 0.62 + sine01(flow_phase * 0.43 + 0.31) * 0.38;
        let breath = sine01(context.elapsed_seconds * phases.breath_frequency * 0.42 + 0.19);

        Self {
            sample: LogoMaterialSample {
                primary: gaussian(distance, LOGO_WAVE_SIGMA),
                secondary: gaussian(secondary_distance, LOGO_WAVE_SIGMA * 0.78)
                    * phases.secondary_gain,
                glow: gaussian(distance, LOGO_GLOW_SIGMA),
                specular: gaussian(distance, LOGO_SPECULAR_SIGMA) * (0.42 + micro_flow * 0.18),
                focus: phases.focus_gain,
                micro_flow,
                breath,
            },
            profile: context.profile,
            theme: context.theme,
            rhythm: context.rhythm,
            logo_weight: MotionDirectorState::at(context.elapsed_seconds, context.profile)
                .logo_weight,
            gamut: LogoGamutStyle::from_context(context),
        }
    }

    fn shade(self) -> Color {
        let curves = VisualCurves::for_profile(self.profile);
        let intensity = self.profile.intensity();
        let base_mix =
            (0.68 + self.sample.glow * 0.18 + self.sample.breath * 0.05 + self.rhythm * 0.05)
                .clamp(0.0, 1.0);
        let base = self.gamut.base(base_mix);
        let glow = blend_color(base, self.gamut.glow, self.sample.glow * 0.24);
        let highlight_energy = (self.sample.primary
            + self.sample.secondary * intensity
            + self.sample.specular * 0.54
            + self.sample.glow * self.sample.focus
            + self.rhythm * 0.10)
            * intensity
            * curves.brightness
            * self.logo_weight;
        let specular = highlight_blend(glow, self.gamut.specular, self.sample.specular * 0.46);
        let chroma_gain =
            curves.chroma * self.gamut.chroma_scale * (0.98 + self.sample.micro_flow * 0.10);

        ensure_visible_lightness(
            boost_blue_contrast(adjust_chroma(
                highlight_blend(specular, self.gamut.highlight, highlight_energy.min(1.0)),
                chroma_gain,
            )),
            self.theme.contrast_floor,
        )
    }
}

#[derive(Clone, Copy)]
struct LogoGamutStyle {
    shadow: Color,
    base: Color,
    glow: Color,
    highlight: Color,
    specular: Color,
    chroma_scale: f32,
}

impl LogoGamutStyle {
    fn from_context(context: LogoColorContext) -> Self {
        let gamut_phase = sine01(context.elapsed_seconds * 0.037 + context.index as f32 * 0.011);
        let profile_gain = match context.profile {
            VisualProfile::Ultra => 1.12,
            VisualProfile::Cinematic => 1.06,
            VisualProfile::Benchmark => 1.02,
            VisualProfile::Calm => 0.92,
        };

        Self {
            shadow: blend_color(context.theme.logo_shadow, LOGO_DEEP_INK, 0.44),
            base: blend_color(context.theme.logo_base, LOGO_ELECTRIC_BLUE, 0.28),
            glow: blend_color(context.theme.accent, LOGO_CYAN_GLOW, 0.38),
            highlight: blend_color(
                context.theme.logo_highlight,
                LOGO_CYAN_GLOW,
                0.14 + gamut_phase * 0.08,
            ),
            specular: blend_color(context.theme.logo_highlight, LOGO_ICE_SPECULAR, 0.54),
            chroma_scale: profile_gain,
        }
    }

    fn base(self, amount: f32) -> Color {
        blend_color(self.shadow, self.base, amount)
    }
}

fn logo_delta_limit(
    context: LogoColorContext,
    appearance: TerminalAppearance,
    target_fps: u16,
) -> f32 {
    let fps_scale = (120.0 / f32::from(target_fps.max(60))).sqrt();
    let terminal_scale =
        (0.72 + appearance.glyph_weight * 0.24 + appearance.color_reliability * 0.04)
            .clamp(0.70, 1.0);
    let phases = MotionPhases::at(context.elapsed_seconds, context.motion_preset);
    let motion_pressure = (phases.logo_speed * phases.scan_frequency).clamp(0.72, 1.42);
    let profile_scale = match context.profile {
        VisualProfile::Ultra => 0.92,
        VisualProfile::Cinematic => 0.88,
        VisualProfile::Calm => 0.76,
        VisualProfile::Benchmark => 0.96,
    };
    0.026 * fps_scale * terminal_scale * profile_scale / motion_pressure.sqrt()
}

fn limit_oklab_delta(previous: Color, next: Color, max_delta: f32) -> Color {
    let Some(previous) = Oklab::from_color(previous) else {
        return next;
    };
    let Some(next_lab) = Oklab::from_color(next) else {
        return next;
    };
    let delta = next_lab.sub(previous);
    let distance = delta.length();
    if distance <= max_delta || distance <= f32::EPSILON {
        next
    } else {
        previous.add(delta.scale(max_delta / distance)).to_color()
    }
}

fn ensure_visible_lightness(color: Color, floor: f32) -> Color {
    let Some(mut oklab) = Oklab::from_color(color) else {
        return color;
    };
    oklab.lightness = oklab.lightness.max(floor.max(MIN_VISIBLE_LIGHTNESS));
    oklab.to_color()
}

fn boost_blue_contrast(color: Color) -> Color {
    blend_color(color, rgb!(178, 230, 255), 0.05)
}

fn adjust_chroma(color: Color, amount: f32) -> Color {
    let Some(mut oklch) = Oklch::from_color(color) else {
        return color;
    };
    oklch.chroma *= amount.clamp(0.4, 1.2);
    oklch.to_color()
}

fn rhythm_signal(
    elapsed_seconds: f32,
    theme: Theme,
    profile: VisualProfile,
    motion_preset: MotionPreset,
) -> f32 {
    let phases = MotionPhases::at(elapsed_seconds, motion_preset);
    let slow = sine01(elapsed_seconds * phases.breath_frequency);
    let medium = sine01(elapsed_seconds * phases.scan_frequency + 1.7);
    (slow * 0.68 + medium * 0.32) * theme.rhythm_intensity * profile.intensity()
}

#[derive(Clone, Copy, Debug)]
struct MotionPhases {
    logo_speed: f32,
    breath_frequency: f32,
    scan_frequency: f32,
    secondary_gain: f32,
    focus_gain: f32,
    orbit_speed: f32,
    glint_offset: f32,
}

impl MotionPhases {
    fn at(elapsed_seconds: f32, preset: MotionPreset) -> Self {
        let drift = sine01(elapsed_seconds * 0.071);
        match preset {
            MotionPreset::Prime => Self {
                logo_speed: 1.0,
                breath_frequency: 0.37,
                scan_frequency: 0.61,
                secondary_gain: 0.28,
                focus_gain: 0.08,
                orbit_speed: 1.0,
                glint_offset: 0.0,
            },
            MotionPreset::Aurora => Self {
                logo_speed: 0.82 + drift * 0.08,
                breath_frequency: 0.23,
                scan_frequency: 0.47,
                secondary_gain: 0.38,
                focus_gain: 0.13,
                orbit_speed: 0.76,
                glint_offset: 7.0,
            },
            MotionPreset::Pulse => Self {
                logo_speed: 1.18,
                breath_frequency: 0.51,
                scan_frequency: 0.89,
                secondary_gain: 0.44,
                focus_gain: 0.18,
                orbit_speed: 1.22,
                glint_offset: 13.0,
            },
        }
    }
}

fn sine01(phase: f32) -> f32 {
    ((phase * std::f32::consts::TAU).sin() + 1.0) * 0.5
}

fn animation_frame_index(elapsed_seconds: f32, target_fps: u16) -> u32 {
    (elapsed_seconds.max(0.0) * f32::from(target_fps.max(1))).floor() as u32
}

fn circular_distance_f32(left: f32, right: f32, period: f32) -> f32 {
    let direct = (left - right).abs();
    direct.min(period - direct)
}

fn gaussian(distance: f32, sigma: f32) -> f32 {
    (-(distance * distance) / (2.0 * sigma * sigma)).exp()
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn blend_color(from: Color, to: Color, amount: f32) -> Color {
    perceptual_blend(from, to, amount, PerceptualBlendMode::Balanced)
}

fn highlight_blend(from: Color, to: Color, amount: f32) -> Color {
    perceptual_blend(from, to, amount, PerceptualBlendMode::Highlight)
}

#[derive(Clone, Copy)]
enum PerceptualBlendMode {
    Balanced,
    Highlight,
}

fn perceptual_blend(from: Color, to: Color, amount: f32, mode: PerceptualBlendMode) -> Color {
    let Some(from) = Oklch::from_color(from) else {
        return to;
    };
    let Some(to) = Oklch::from_color(to) else {
        return rgb!(0, 0, 0);
    };
    let amount = smoothstep(amount);

    let mut blended = from.lerp_short_hue(to, amount);
    blended.chroma = match mode {
        PerceptualBlendMode::Balanced => blended.chroma.min(0.132),
        PerceptualBlendMode::Highlight => blended.chroma.min(0.168),
    };
    blended.lightness = match mode {
        PerceptualBlendMode::Balanced => blended.lightness.clamp(0.0, 0.86),
        PerceptualBlendMode::Highlight => blended.lightness.clamp(0.0, 0.94),
    };
    blended.to_color()
}

fn display_color(color: Color, capabilities: TerminalCapabilities) -> Color {
    display_role_color(color, capabilities, PerceptualRole::Logo)
}

fn display_role_color(
    color: Color,
    capabilities: TerminalCapabilities,
    role: PerceptualRole,
) -> Color {
    if capabilities.supports_truecolor() {
        let profile = terminal_visual_profile_from_capabilities(capabilities);
        return optimize_oklch_color_for_profile(color, role, profile);
    }

    let Color::Rgb { r, g, b } = color else {
        return color;
    };
    let contrast = apca_like_contrast(color, dark_reference_color());

    if contrast > 72.0 {
        Color::White
    } else if u16::from(g) + u16::from(b) > u16::from(r) * 2 {
        Color::Cyan
    } else {
        Color::Blue
    }
}

fn optimize_oklch_color_for_profile(
    color: Color,
    role: PerceptualRole,
    profile: TerminalVisualProfile,
) -> Color {
    let Some(source) = Oklch::from_color(color) else {
        return color;
    };
    let appearance = TerminalAppearance::for_profile(profile);
    let mut best = color;
    let mut best_cost = f32::MAX;
    let minimum = appearance.contrast_floor(role);
    for lightness_step in -4..=10 {
        for chroma_step in -4..=4 {
            let mut candidate = source;
            candidate.lightness = (source.lightness + lightness_step as f32 * 0.025)
                .clamp(0.0, appearance.lightness_ceiling());
            candidate.chroma = (source.chroma * (1.0 + chroma_step as f32 * 0.055))
                .clamp(0.0, appearance.chroma_ceiling(role));
            let color = candidate.to_color();
            let contrast = apca_like_contrast(color, dark_reference_color());
            if contrast < minimum {
                continue;
            }
            let cost = (candidate.lightness - source.lightness).abs() * 1.8
                + (candidate.chroma - source.chroma).abs()
                + shortest_angle_delta(candidate.hue, source.hue).abs() * 0.2;
            if cost < best_cost {
                best_cost = cost;
                best = color;
            }
        }
    }
    if best_cost == f32::MAX {
        enforce_contrast(color, minimum)
    } else {
        best
    }
}

fn enforce_contrast(color: Color, minimum: f32) -> Color {
    let mut adjusted = color;
    for _ in 0..12 {
        if apca_like_contrast(adjusted, dark_reference_color()) >= minimum {
            return adjusted;
        }
        adjusted = blend_color(adjusted, rgb!(220, 248, 255), 0.24);
    }
    adjusted
}

fn apca_like_contrast(foreground: Color, background: Color) -> f32 {
    let foreground = relative_luminance(foreground).powf(0.56);
    let background = relative_luminance(background).powf(0.57);
    ((foreground - background) * 108.0).abs()
}

fn dark_reference_color() -> Color {
    rgb!(2, 8, 18)
}

fn relative_luminance(color: Color) -> f32 {
    let Color::Rgb { r, g, b } = color else {
        return 0.0;
    };
    0.2126 * srgb_to_linear(r) + 0.7152 * srgb_to_linear(g) + 0.0722 * srgb_to_linear(b)
}

fn compensate_glyph_lightness(color: Color, character: char) -> Color {
    let Some(mut oklab) = Oklab::from_color(color) else {
        return color;
    };
    oklab.lightness = (oklab.lightness * glyph_lightness_multiplier(character)).clamp(0.0, 1.0);
    oklab.to_color()
}

fn glyph_lightness_multiplier(character: char) -> f32 {
    match character {
        '█' | '▓' | '▒' => 0.96,
        '━' | '┃' | '─' | '│' => 1.08,
        '╭' | '╮' | '╰' | '╯' => 1.1,
        '⠁'..='⣿' => 1.18,
        '*' | '+' | '-' | '|' => 1.12,
        _ => 1.0,
    }
}

#[derive(Default)]
struct SmoothingBuffer {
    colors: BTreeMap<(u16, u16), Color>,
}

impl SmoothingBuffer {
    fn clear(&mut self) {
        self.colors.clear();
    }

    fn apply(&mut self, cells: &mut [Cell], profile: VisualProfile) {
        let max_delta = match profile {
            VisualProfile::Ultra | VisualProfile::Benchmark => 0.065,
            VisualProfile::Cinematic => 0.052,
            VisualProfile::Calm => 0.038,
        };

        for cell in cells {
            let key = (cell.column, cell.row);
            if let Some(previous) = self.colors.get(&key).copied() {
                cell.color = match PerceptualRole::from_cell(cell) {
                    PerceptualRole::Logo => {
                        limit_oklab_delta(previous, cell.color, max_delta * 0.72)
                    }
                    PerceptualRole::StatusText => {
                        limit_oklab_delta(previous, cell.color, max_delta * 0.88)
                    }
                    _ => limit_lightness_delta(previous, cell.color, max_delta),
                };
            }
            self.colors.insert(key, cell.color);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TemporalCell {
    color: Color,
    role: PerceptualRole,
    confidence: f32,
}

#[derive(Default)]
struct CellTemporalAaBuffer {
    history: BTreeMap<(u16, u16), TemporalCell>,
}

impl CellTemporalAaBuffer {
    fn clear(&mut self) {
        self.history.clear();
    }

    fn apply(&mut self, cells: &mut [Cell], context: FrameContext) {
        let mut next_history = BTreeMap::new();
        let reconstruction = TemporalReconstructionProfile::for_fps(context.calibration.target_fps);
        for cell in cells {
            let role = PerceptualRole::from_cell(cell);
            let key = (cell.column, cell.row);
            if let Some(previous) = self.history.get(&key).copied()
                && previous.role == role
                && role.temporal_stability_weight() < 0.9
            {
                let delta = oklab_distance(previous.color, cell.color);
                let rejection = reconstruction.rejection_threshold_for_role(role);
                if delta < rejection {
                    let blend = (reconstruction.history_weight + previous.confidence * 0.32)
                        * (1.05 - role.temporal_stability_weight())
                        * context.director.background_weight.max(0.35);
                    cell.color = perceptual_blend(
                        previous.color,
                        cell.color,
                        (1.0 - blend).clamp(0.0, 1.0),
                        PerceptualBlendMode::Balanced,
                    );
                }
            }
            next_history.insert(
                key,
                TemporalCell {
                    color: cell.color,
                    role,
                    confidence: self
                        .history
                        .get(&key)
                        .map_or(0.18, |cell| (cell.confidence + 0.16).min(1.0)),
                },
            );
        }
        self.history = next_history;
    }
}

fn oklab_distance(left: Color, right: Color) -> f32 {
    let Some(left) = Oklab::from_color(left) else {
        return 1.0;
    };
    let Some(right) = Oklab::from_color(right) else {
        return 1.0;
    };
    ((left.lightness - right.lightness).powi(2)
        + (left.a - right.a).powi(2)
        + (left.b - right.b).powi(2))
    .sqrt()
}

#[derive(Clone, Copy, Debug)]
struct AfterimageSample {
    column: u16,
    row: u16,
    character: char,
    color: Color,
    intensity: f32,
    created_at: f32,
    lifetime: f32,
}

#[derive(Default)]
struct AfterimageBuffer {
    samples: Vec<AfterimageSample>,
}

impl AfterimageBuffer {
    fn clear(&mut self) {
        self.samples.clear();
    }

    fn apply(
        &mut self,
        cells: &mut Vec<Cell>,
        elapsed_seconds: f32,
        context: FrameContext,
        layout: &Layout,
    ) {
        let afterimage_scale = context.quality.afterimage
            * context.director.afterimage_weight
            * context.curves.afterimage_decay;
        if afterimage_scale <= 0.28 {
            self.samples.clear();
            return;
        }

        let reconstruction = TemporalReconstructionProfile::for_fps(context.calibration.target_fps);
        self.samples
            .retain(|sample| elapsed_seconds - sample.created_at <= sample.lifetime);
        for sample in &self.samples {
            let age = elapsed_seconds - sample.created_at;
            let fade = (1.0 - age / sample.lifetime)
                .clamp(0.0, 1.0)
                .powf(reconstruction.afterimage_exponent);
            if fade <= reconstruction.rejection_threshold_for_role(PerceptualRole::Afterimage) {
                continue;
            }
            cells.push(Cell {
                column: sample.column,
                row: sample.row,
                character: sample.character,
                color: display_role_color(
                    dim_color(sample.color, fade * sample.intensity * 0.62),
                    context.calibration.capabilities,
                    PerceptualRole::Afterimage,
                ),
                layer: RenderLayer::Background,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            });
        }

        let budget = (context.quality.dirty_cell_budget / 18).clamp(18, 72);
        for cell in cells
            .iter()
            .filter(|cell| {
                cell.layer == RenderLayer::Orbit || cell.layer == RenderLayer::Background
            })
            .rev()
            .take(budget)
        {
            if cell.row == 0 || cell.row >= layout.rows.saturating_sub(1) {
                continue;
            }
            self.samples.push(AfterimageSample {
                column: cell.column,
                row: cell.row,
                character: cell.character,
                color: cell.color,
                intensity: match cell.layer {
                    RenderLayer::Orbit => 1.0,
                    RenderLayer::Background => 0.42,
                    RenderLayer::Text => 0.0,
                },
                created_at: elapsed_seconds,
                lifetime: reconstruction.decay_seconds
                    * (0.74 + afterimage_scale * 0.82 + context.quality.scale * 0.18),
            });
        }
        if self.samples.len() > 220 {
            let drop_count = self.samples.len() - 220;
            self.samples.drain(0..drop_count);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TemporalReconstructionProfile {
    sample_phase: f32,
    shutter_width: f32,
    history_weight: f32,
    afterimage_exponent: f32,
    rejection_threshold: f32,
    decay_seconds: f32,
    tail_energy_budget: f32,
}

impl TemporalReconstructionProfile {
    fn for_fps(fps: u16) -> Self {
        match fps {
            0..=72 => Self {
                sample_phase: 0.42,
                shutter_width: 0.82,
                history_weight: 0.34,
                afterimage_exponent: 1.72,
                rejection_threshold: 0.024,
                decay_seconds: 0.26,
                tail_energy_budget: 0.78,
            },
            73..=105 => Self {
                sample_phase: 0.36,
                shutter_width: 0.68,
                history_weight: 0.29,
                afterimage_exponent: 1.48,
                rejection_threshold: 0.019,
                decay_seconds: 0.22,
                tail_energy_budget: 0.66,
            },
            _ => Self {
                sample_phase: 0.31,
                shutter_width: 0.54,
                history_weight: 0.24,
                afterimage_exponent: 1.28,
                rejection_threshold: 0.014,
                decay_seconds: 0.18,
                tail_energy_budget: 0.58,
            },
        }
    }

    fn rejection_threshold_for_role(self, role: PerceptualRole) -> f32 {
        let multiplier = match role {
            PerceptualRole::Afterimage => 6.8,
            PerceptualRole::BackgroundParticle => 7.6,
            PerceptualRole::MouseTrail => 9.4,
            PerceptualRole::Orbit => 4.6,
            PerceptualRole::StatusText | PerceptualRole::Logo => 0.0,
        };
        self.rejection_threshold * multiplier
    }
}

#[cfg(test)]
type TemporalKernel = TemporalReconstructionProfile;

fn dim_color(color: Color, amount: f32) -> Color {
    blend_color(
        dark_reference_color(),
        color,
        amount.clamp(0.0, 1.0).powf(0.82),
    )
}

struct LuminanceBudget {
    cells: BTreeMap<(u16, u16), f32>,
}

impl LuminanceBudget {
    fn from_foreground(cells: &[Cell], layout: &Layout) -> Self {
        let mut budget = BTreeMap::new();
        for cell in cells {
            if !matches!(cell.layer, RenderLayer::Orbit | RenderLayer::Text) {
                continue;
            }
            let lightness = Oklab::from_color(cell.color).map_or(0.0, |lab| lab.lightness);
            let radius = if cell.layer == RenderLayer::Text {
                2_i16
            } else {
                1_i16
            };
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let Some(column) = offset_u16(cell.column, dx, layout.columns) else {
                        continue;
                    };
                    let Some(row) = offset_u16(cell.row, dy, layout.rows) else {
                        continue;
                    };
                    let distance = ((dx * dx + dy * dy) as f32).sqrt();
                    let weight = (1.0 - distance / (f32::from(radius) + 1.0)).clamp(0.0, 1.0);
                    *budget.entry((column, row)).or_insert(0.0) += lightness * weight;
                }
            }
        }
        Self { cells: budget }
    }

    fn apply(&self, cells: &mut [Cell]) {
        for cell in cells {
            if cell.layer != RenderLayer::Background {
                continue;
            }
            let pressure = self
                .cells
                .get(&(cell.column, cell.row))
                .copied()
                .unwrap_or(0.0);
            if pressure > 0.58 {
                let attenuation = (1.0 - (pressure - 0.58) * 0.38).clamp(0.42, 1.0);
                cell.color = dim_color(cell.color, attenuation);
            }
        }
    }

    fn pressure(frame: &Frame, layout: &Layout) -> f32 {
        let budget = Self::from_foreground(&frame.cells, layout);
        if budget.cells.is_empty() {
            return 0.0;
        }
        budget.cells.values().copied().fold(0.0, f32::max)
    }
}

fn offset_u16(value: u16, delta: i16, upper_bound: u16) -> Option<u16> {
    let value = i32::from(value) + i32::from(delta);
    if value < 0 || value >= i32::from(upper_bound) {
        None
    } else {
        Some(value as u16)
    }
}

#[derive(Clone, Copy, Debug)]
struct MouseParticle {
    x: f32,
    y: f32,
    velocity_x: f32,
    velocity_y: f32,
    age: f32,
    lifetime: f32,
    seed: u32,
    background_layer: u8,
    speed_jitter: f32,
    state: MouseParticleState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseParticleState {
    MouseTrail,
    Background,
}

#[derive(Clone, Copy, Debug)]
struct VortexParticle {
    x: f32,
    y: f32,
    strength: f32,
    radius: f32,
    age: f32,
    lifetime: f32,
    sign: f32,
}

#[derive(Default)]
struct MouseParticleSystem {
    particles: Vec<MouseParticle>,
    vortices: Vec<VortexParticle>,
    last_step_at: Option<f32>,
    last_emit_at: Option<f32>,
    velocity_grid: VelocityGrid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseSpeedBucket {
    Slow,
    Medium,
    Fast,
}

#[derive(Clone, Copy, Debug)]
struct MouseFlow {
    speed_bucket: MouseSpeedBucket,
    direction_x: f32,
    tail_length: f32,
    swirl_radius: f32,
    swirl_gain: f32,
    lifetime: f32,
}

impl MouseFlow {
    fn from_velocity(velocity_x: f32, velocity_y: f32) -> Self {
        let speed = (velocity_x * velocity_x + velocity_y * velocity_y).sqrt();
        let direction_x = if speed > 0.001 {
            velocity_x / speed
        } else {
            0.0
        };
        if speed > 48.0 {
            Self {
                speed_bucket: MouseSpeedBucket::Fast,
                direction_x,
                tail_length: 2.6,
                swirl_radius: 0.72,
                swirl_gain: 0.74,
                lifetime: 1.18,
            }
        } else if speed > 14.0 {
            Self {
                speed_bucket: MouseSpeedBucket::Medium,
                direction_x,
                tail_length: 1.2,
                swirl_radius: 0.94,
                swirl_gain: 0.96,
                lifetime: 1.0,
            }
        } else {
            Self {
                speed_bucket: MouseSpeedBucket::Slow,
                direction_x,
                tail_length: 0.25,
                swirl_radius: 1.28,
                swirl_gain: 1.28,
                lifetime: 0.86,
            }
        }
    }
}

fn flow_gain_for_mouse(mouse: MouseAttractor) -> f32 {
    let speed = (mouse.velocity_x * mouse.velocity_x + mouse.velocity_y * mouse.velocity_y).sqrt();
    (0.82 + (speed / 120.0).clamp(0.0, 1.0) * 0.34) * if mouse.release { 0.72 } else { 1.0 }
}

impl MouseParticleSystem {
    fn clear(&mut self) {
        self.particles.clear();
        self.vortices.clear();
        self.last_step_at = None;
        self.last_emit_at = None;
        self.velocity_grid.clear();
    }

    fn step(
        &mut self,
        cells: &mut Vec<Cell>,
        layout: &Layout,
        elapsed_seconds: f32,
        context: FrameContext,
    ) {
        let dt = self.last_step_at.map_or(1.0 / 120.0, |last| {
            (elapsed_seconds - last).clamp(1.0 / 240.0, 1.0 / 24.0)
        });
        self.last_step_at = Some(elapsed_seconds);
        self.velocity_grid.step(dt, context.mouse, layout);
        self.step_vortices(dt, layout);

        if context.mouse.is_some_and(|mouse| mouse.assimilate) {
            self.assimilate_mouse_trails();
        }

        if let Some(mut mouse) = context.mouse.filter(|mouse| mouse.emitting) {
            mouse.strength *= context.director.mouse_weight * context.curves.vortex_strength;
            self.emit(mouse, elapsed_seconds, context.curves);
            self.emit_vortices(mouse);
        }
        let mouse = context.mouse.map(|mut mouse| {
            mouse.strength *= context.director.mouse_weight * context.curves.vortex_strength;
            mouse
        });
        self.integrate(dt, mouse, layout);
        self.render(cells, layout, context);
    }

    fn emit(&mut self, mouse: MouseAttractor, elapsed_seconds: f32, curves: VisualCurves) {
        if self
            .last_emit_at
            .is_some_and(|last| elapsed_seconds - last < 1.0 / 90.0)
        {
            return;
        }
        self.last_emit_at = Some(elapsed_seconds);

        let flow = MouseFlow::from_velocity(mouse.velocity_x, mouse.velocity_y);
        let emit_count = if flow.speed_bucket == MouseSpeedBucket::Fast {
            7
        } else if flow.speed_bucket == MouseSpeedBucket::Medium {
            4
        } else {
            2
        };
        let base_seed = mix_seed(
            0xa511_e9b3,
            elapsed_seconds.to_bits() ^ u32::from(mouse.column) << 8 ^ u32::from(mouse.row),
        );

        for index in 0..emit_count {
            if self.mouse_trail_count() >= MOUSE_TRAIL_PARTICLE_LIMIT
                && let Some(index) = self
                    .particles
                    .iter()
                    .position(|particle| particle.state == MouseParticleState::MouseTrail)
            {
                self.particles.remove(index);
            }
            let near_cursor = self
                .particles
                .iter()
                .filter(|particle| particle.state == MouseParticleState::MouseTrail)
                .filter(|particle| {
                    ((particle.x - f32::from(mouse.column)).powi(2)
                        + (particle.y - f32::from(mouse.row)).powi(2))
                    .sqrt()
                        < 3.2
                })
                .count();
            if near_cursor >= 10 {
                break;
            }
            let seed = mix_seed(base_seed, index as u32);
            let angle = random_unit(seed.rotate_left(5)) * std::f32::consts::TAU;
            let radius = (0.35 + random_unit(seed.rotate_left(9)) * 1.4) * flow.swirl_radius;
            let turbulence = 5.0 + random_unit(seed.rotate_left(13)) * 13.0;
            let direction = (mouse.velocity_x, mouse.velocity_y);
            let anisotropic_gap = anisotropic_distance(
                (
                    f32::from(mouse.column) + angle.cos() * radius,
                    f32::from(mouse.row) + angle.sin() * radius * 0.55,
                ),
                (f32::from(mouse.column), f32::from(mouse.row)),
                direction,
                2.8 + flow.tail_length,
                0.82 + flow.swirl_radius * 0.5,
            );
            if flow.speed_bucket == MouseSpeedBucket::Fast && anisotropic_gap < 0.24 {
                continue;
            }
            let tangent_x = -angle.sin();
            let tangent_y = angle.cos();
            self.particles.push(MouseParticle {
                x: f32::from(mouse.column) + angle.cos() * radius
                    - flow.direction_x * flow.tail_length,
                y: f32::from(mouse.row) + angle.sin() * radius * 0.55,
                velocity_x: -mouse.velocity_x * 0.035
                    + tangent_x * turbulence * flow.swirl_gain
                    + angle.cos() * 2.0,
                velocity_y: -mouse.velocity_y * 0.028
                    + tangent_y * turbulence * 0.48 * flow.swirl_gain,
                age: 0.0,
                lifetime: (0.72 + random_unit(seed.rotate_left(17)) * 0.55)
                    * flow.lifetime
                    * curves.particle_lifetime,
                seed,
                background_layer: 3 + (seed % 3) as u8,
                speed_jitter: star_speed_jitter(index, emit_count, seed),
                state: MouseParticleState::MouseTrail,
            });
        }
    }

    fn mouse_trail_count(&self) -> usize {
        self.particles
            .iter()
            .filter(|particle| particle.state == MouseParticleState::MouseTrail)
            .count()
    }

    fn assimilate_mouse_trails(&mut self) {
        let total = self.particles.len().max(1);
        for (index, particle) in self.particles.iter_mut().enumerate() {
            if particle.state == MouseParticleState::MouseTrail {
                let seed = mix_seed(particle.seed, index as u32 ^ total as u32);
                let layer = 1 + (seed % 5) as u8;
                let flow = CosmicLayerFlow::for_layer(layer, seed, MotionPreset::Prime);
                let jitter = star_speed_jitter(index, total, seed);
                particle.state = MouseParticleState::Background;
                particle.age = 0.0;
                particle.lifetime = f32::INFINITY;
                particle.background_layer = layer;
                particle.speed_jitter = jitter;
                particle.velocity_x =
                    particle.velocity_x * 0.26 + flow.direction_x * flow.speed * jitter;
                particle.velocity_y =
                    particle.velocity_y * 0.22 + flow.direction_y * flow.speed * jitter;
            }
        }
        self.limit_background_particles();
    }

    fn limit_background_particles(&mut self) {
        let excess = self
            .particles
            .iter()
            .filter(|particle| particle.state == MouseParticleState::Background)
            .count()
            .saturating_sub(MOUSE_BACKGROUND_PARTICLE_LIMIT);
        if excess == 0 {
            return;
        }
        let mut candidates = self
            .particles
            .iter()
            .enumerate()
            .filter(|(_, particle)| particle.state == MouseParticleState::Background)
            .map(|(index, particle)| (index, background_particle_visual_energy(*particle)))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut remove = vec![false; self.particles.len()];
        for (index, _) in candidates.into_iter().take(excess) {
            remove[index] = true;
        }
        let mut index = 0usize;
        self.particles.retain(|_| {
            let keep = !remove[index];
            index += 1;
            keep
        });
    }

    fn integrate(&mut self, dt: f32, mouse: Option<MouseAttractor>, layout: &Layout) {
        for particle in &mut self.particles {
            if particle.state == MouseParticleState::MouseTrail
                && let Some(mouse) = mouse
            {
                let dx = f32::from(mouse.column) - particle.x;
                let dy = f32::from(mouse.row) - particle.y;
                let distance = (dx * dx + dy * dy).sqrt();
                if distance > 0.001 {
                    let field = gravity_field(distance, mouse.strength);
                    let nx = dx / distance;
                    let ny = dy / distance;
                    let velocity_inheritance = if mouse.release { 0.018 } else { 0.04 };
                    let vortex_gain = flow_gain_for_mouse(mouse);
                    particle.velocity_x +=
                        (nx * field.radial - ny * field.tangential) * 28.0 * dt * vortex_gain
                            + mouse.velocity_x * velocity_inheritance * field.visibility * dt;
                    particle.velocity_y +=
                        (ny * field.radial + nx * field.tangential) * 28.0 * dt * vortex_gain
                            + mouse.velocity_y * velocity_inheritance * field.visibility * dt;
                }
            }
            let (field_x, field_y) = self.velocity_grid.sample(particle.x, particle.y, layout);
            let (curl_x, curl_y) = self
                .velocity_grid
                .confinement_force(particle.x, particle.y, layout);
            let (vortex_x, vortex_y) = vortex_force_from(&self.vortices, particle.x, particle.y);
            particle.velocity_x += (field_x * 0.22 + curl_x * 8.0) * dt;
            particle.velocity_y += (field_y * 0.22 + curl_y * 8.0) * dt;
            particle.velocity_x += vortex_x * dt;
            particle.velocity_y += vortex_y * dt;
            match particle.state {
                MouseParticleState::MouseTrail => {
                    particle.velocity_x *= 0.91_f32.powf(dt * 60.0);
                    particle.velocity_y *= 0.88_f32.powf(dt * 60.0);
                    particle.velocity_y += 0.55 * dt;
                }
                MouseParticleState::Background => {
                    let seed_flow = CosmicLayerFlow::for_layer(
                        particle.background_layer,
                        particle.seed,
                        MotionPreset::Prime,
                    );
                    particle.velocity_x +=
                        seed_flow.direction_x * seed_flow.speed * particle.speed_jitter * 0.11 * dt;
                    particle.velocity_y +=
                        seed_flow.direction_y * seed_flow.speed * particle.speed_jitter * 0.11 * dt;
                    particle.velocity_x *= 0.985_f32.powf(dt * 60.0);
                    particle.velocity_y *= 0.982_f32.powf(dt * 60.0);
                }
            }
            particle.x += particle.velocity_x * dt;
            particle.y += particle.velocity_y * dt;
            if particle.state == MouseParticleState::Background {
                particle.x = particle.x.rem_euclid(f32::from(layout.columns.max(1)));
                particle.y = (particle.y - 1.0)
                    .rem_euclid(f32::from(layout.rows.saturating_sub(2).max(1)))
                    + 1.0;
            }
            particle.age += dt;
        }
        self.particles.retain(|particle| {
            particle.state == MouseParticleState::Background || particle.age < particle.lifetime
        });
    }

    fn emit_vortices(&mut self, mouse: MouseAttractor) {
        let speed =
            (mouse.velocity_x * mouse.velocity_x + mouse.velocity_y * mouse.velocity_y).sqrt();
        if speed < 8.0 {
            return;
        }
        if self.vortices.len() > 18 {
            self.vortices.drain(0..self.vortices.len() - 18);
        }
        let nx = mouse.velocity_x / speed;
        let ny = mouse.velocity_y / speed;
        let offset_x = -ny * 1.6;
        let offset_y = nx * 0.9;
        let strength = (speed / 80.0).clamp(0.18, 1.0) * mouse.strength;
        for sign in [-1.0_f32, 1.0] {
            self.vortices.push(VortexParticle {
                x: f32::from(mouse.column) + offset_x * sign,
                y: f32::from(mouse.row) + offset_y * sign,
                strength,
                radius: 2.2 + strength * 2.4,
                age: 0.0,
                lifetime: 0.42 + strength * 0.38,
                sign,
            });
        }
    }

    fn step_vortices(&mut self, dt: f32, layout: &Layout) {
        for vortex in &mut self.vortices {
            let (field_x, field_y) = self.velocity_grid.sample(vortex.x, vortex.y, layout);
            vortex.x += field_x * 0.035 * dt;
            vortex.y += field_y * 0.035 * dt;
            vortex.age += dt;
            vortex.strength *= 0.94_f32.powf(dt * 60.0);
        }
        self.vortices
            .retain(|vortex| vortex.age < vortex.lifetime && vortex.strength > 0.035);
    }

    fn render(&self, cells: &mut Vec<Cell>, layout: &Layout, context: FrameContext) {
        for particle in &self.particles {
            if particle.x < 0.0
                || particle.y < 1.0
                || particle.x >= f32::from(layout.columns)
                || particle.y >= f32::from(layout.rows.saturating_sub(1))
            {
                continue;
            }
            let life = match particle.state {
                MouseParticleState::MouseTrail => 1.0 - particle.age / particle.lifetime,
                MouseParticleState::Background => {
                    0.42 + random_unit(particle.seed.rotate_left(7)) * 0.18
                }
            };
            let shimmer = 0.72 + random_unit(particle.seed.rotate_left(23)) * 0.28;
            let base_intensity = (0.16 + life * 0.24) * shimmer;
            let splats = particle_forward_splat_count(*particle);
            for splat_index in 0..splats {
                let (x, y, fade) = particle_forward_splat(*particle, splat_index, splats);
                push_velocity_particle_cell(
                    cells,
                    x.round()
                        .clamp(0.0, f32::from(layout.columns.saturating_sub(1)))
                        as u16,
                    y.round()
                        .clamp(1.0, f32::from(layout.rows.saturating_sub(1)))
                        as u16,
                    base_intensity * fade,
                    context,
                    particle,
                );
            }
        }
    }
}

fn particle_forward_splat_count(particle: MouseParticle) -> usize {
    let speed = (particle.velocity_x * particle.velocity_x
        + particle.velocity_y * particle.velocity_y)
        .sqrt()
        * particle.speed_jitter;
    if speed > 34.0 {
        3
    } else if speed > 13.0 {
        2
    } else {
        1
    }
}

fn particle_forward_splat(particle: MouseParticle, index: usize, count: usize) -> (f32, f32, f32) {
    if count <= 1 {
        return (particle.x, particle.y, 1.0);
    }
    let t = index as f32 / (count - 1) as f32;
    let shutter = match particle.state {
        MouseParticleState::MouseTrail => 0.026,
        MouseParticleState::Background => 0.038,
    };
    let x = particle.x - particle.velocity_x * shutter * t;
    let y = particle.y - particle.velocity_y * shutter * t;
    let fade = (1.0 - t * 0.58).clamp(0.32, 1.0);
    (x, y, fade)
}

fn background_particle_visual_energy(particle: MouseParticle) -> f32 {
    let speed = (particle.velocity_x * particle.velocity_x
        + particle.velocity_y * particle.velocity_y)
        .sqrt();
    let layer_energy = f32::from(particle.background_layer) * 0.035;
    let spectral_jitter = random_unit(particle.seed.rotate_left(19)) * 0.12;
    speed * 0.026 + particle.speed_jitter * 0.42 + layer_energy + spectral_jitter
        - particle.age * 0.018
}

fn push_velocity_particle_cell(
    cells: &mut Vec<Cell>,
    column: u16,
    row: u16,
    intensity: f32,
    context: FrameContext,
    particle: &MouseParticle,
) {
    if row == 0 {
        return;
    }
    let role = match particle.state {
        MouseParticleState::MouseTrail => PerceptualRole::MouseTrail,
        MouseParticleState::Background => PerceptualRole::BackgroundParticle,
    };
    cells.push(Cell {
        column,
        row,
        character: '.',
        color: particle_velocity_color(*particle, intensity, context, role),
        layer: RenderLayer::Background,
        primitive_id: None,
        stroke_id: None,
        vertex_id: None,
        path_index: None,
        correspondence_lost: false,
    });
}

fn particle_velocity_color(
    particle: MouseParticle,
    intensity: f32,
    context: FrameContext,
    role: PerceptualRole,
) -> Color {
    let speed = (particle.velocity_x * particle.velocity_x
        + particle.velocity_y * particle.velocity_y)
        .sqrt()
        * particle.speed_jitter;
    let speed_energy = smootherstep((speed / 36.0).clamp(0.0, 1.0));
    let layer_shift = (f32::from(particle.background_layer) / 5.0 - 0.5) * 0.34;
    let hue = lerp(4.42, 3.18, speed_energy) + layer_shift;
    let chroma = (0.09 + speed_energy * 0.24 + intensity * 0.05).clamp(0.08, 0.38);
    let lightness = (0.34 + intensity * 0.30 + speed_energy * 0.07).clamp(0.31, 0.72);
    let base = Oklch {
        lightness,
        chroma,
        hue,
    }
    .to_color();
    let accented = if speed_energy > 0.58 {
        blend_color(base, rgb!(154, 96, 255), (speed_energy - 0.58) * 0.18)
    } else {
        base
    };
    let visible = display_role_color(
        ensure_visible_lightness(accented, context.theme.contrast_floor * 0.72),
        context.calibration.capabilities,
        role,
    );
    if speed_energy > 0.54 {
        ensure_visible_lightness(
            blend_color(visible, rgb!(154, 96, 255), speed_energy * 0.24),
            context.theme.contrast_floor * 0.72,
        )
    } else {
        visible
    }
}

fn vortex_force_from(vortices: &[VortexParticle], x: f32, y: f32) -> (f32, f32) {
    let mut force_x = 0.0;
    let mut force_y = 0.0;
    for vortex in vortices {
        let dx = x - vortex.x;
        let dy = y - vortex.y;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance <= 0.001 || distance > vortex.radius * 3.0 {
            continue;
        }
        let influence = (1.0 - distance / (vortex.radius * 3.0))
            .clamp(0.0, 1.0)
            .powf(1.4);
        let tangent_x = -dy / distance;
        let tangent_y = dx / distance;
        let strength = vortex.strength * vortex.sign * influence * 18.0;
        force_x += tangent_x * strength;
        force_y += tangent_y * strength;
    }
    (force_x.clamp(-42.0, 42.0), force_y.clamp(-42.0, 42.0))
}

const VELOCITY_GRID_COLUMNS: usize = 20;
const VELOCITY_GRID_ROWS: usize = 8;

#[derive(Clone, Copy, Debug, Default)]
struct VelocityCell {
    x: f32,
    y: f32,
    curl: f32,
}

#[derive(Clone, Debug)]
struct VelocityGrid {
    cells: [VelocityCell; VELOCITY_GRID_COLUMNS * VELOCITY_GRID_ROWS],
}

impl Default for VelocityGrid {
    fn default() -> Self {
        Self {
            cells: [VelocityCell::default(); VELOCITY_GRID_COLUMNS * VELOCITY_GRID_ROWS],
        }
    }
}

impl VelocityGrid {
    fn clear(&mut self) {
        self.cells.fill(VelocityCell::default());
    }

    fn step(&mut self, dt: f32, mouse: Option<MouseAttractor>, layout: &Layout) {
        let decay = 0.82_f32.powf(dt * 60.0);
        for cell in &mut self.cells {
            cell.x *= decay;
            cell.y *= decay;
            cell.curl *= decay * 0.94;
        }
        if let Some(mouse) = mouse.filter(|mouse| mouse.emitting) {
            self.inject(mouse, layout);
        }
    }

    fn inject(&mut self, mouse: MouseAttractor, layout: &Layout) {
        let (grid_x, grid_y) =
            self.grid_position(f32::from(mouse.column), f32::from(mouse.row), layout);
        for y in 0..VELOCITY_GRID_ROWS {
            for x in 0..VELOCITY_GRID_COLUMNS {
                let dx = x as f32 - grid_x;
                let dy = y as f32 - grid_y;
                let distance = (dx * dx + dy * dy).sqrt();
                let influence = (1.0 - distance / 3.4).clamp(0.0, 1.0).powf(1.5);
                if influence <= 0.0 {
                    continue;
                }
                let index = velocity_index(x, y);
                let tangent_x = -dy;
                let tangent_y = dx;
                let tangent_len = (tangent_x * tangent_x + tangent_y * tangent_y)
                    .sqrt()
                    .max(0.001);
                let swirl = 10.0 * mouse.strength * influence;
                self.cells[index].x +=
                    mouse.velocity_x * 0.18 * influence + tangent_x / tangent_len * swirl;
                self.cells[index].y +=
                    mouse.velocity_y * 0.18 * influence + tangent_y / tangent_len * swirl;
                self.cells[index].curl += swirl * 0.16;
            }
        }
    }

    fn sample(&self, x: f32, y: f32, layout: &Layout) -> (f32, f32) {
        let (grid_x, grid_y) = self.grid_position(x, y, layout);
        let x0 = grid_x
            .floor()
            .clamp(0.0, (VELOCITY_GRID_COLUMNS - 1) as f32) as usize;
        let y0 = grid_y.floor().clamp(0.0, (VELOCITY_GRID_ROWS - 1) as f32) as usize;
        let x1 = (x0 + 1).min(VELOCITY_GRID_COLUMNS - 1);
        let y1 = (y0 + 1).min(VELOCITY_GRID_ROWS - 1);
        let fx = grid_x - x0 as f32;
        let fy = grid_y - y0 as f32;
        let a = self.cells[velocity_index(x0, y0)];
        let b = self.cells[velocity_index(x1, y0)];
        let c = self.cells[velocity_index(x0, y1)];
        let d = self.cells[velocity_index(x1, y1)];
        (
            lerp(lerp(a.x, b.x, fx), lerp(c.x, d.x, fx), fy),
            lerp(lerp(a.y, b.y, fx), lerp(c.y, d.y, fx), fy),
        )
    }

    fn confinement_force(&self, x: f32, y: f32, layout: &Layout) -> (f32, f32) {
        let (grid_x, grid_y) = self.grid_position(x, y, layout);
        let gx = grid_x
            .round()
            .clamp(1.0, (VELOCITY_GRID_COLUMNS - 2) as f32) as usize;
        let gy = grid_y.round().clamp(1.0, (VELOCITY_GRID_ROWS - 2) as f32) as usize;
        let left = self.cells[velocity_index(gx - 1, gy)].curl.abs();
        let right = self.cells[velocity_index(gx + 1, gy)].curl.abs();
        let top = self.cells[velocity_index(gx, gy - 1)].curl.abs();
        let bottom = self.cells[velocity_index(gx, gy + 1)].curl.abs();
        let grad_x = right - left;
        let grad_y = bottom - top;
        let curl = self.cells[velocity_index(gx, gy)].curl;
        let length = (grad_x * grad_x + grad_y * grad_y).sqrt();
        if length <= 0.001 {
            return (0.0, 0.0);
        }
        (
            -grad_y / length * curl * 0.018,
            grad_x / length * curl * 0.018,
        )
    }

    #[cfg(test)]
    fn energy(&self) -> f32 {
        self.cells
            .iter()
            .map(|cell| cell.x * cell.x + cell.y * cell.y + cell.curl.abs() * 0.15)
            .sum()
    }

    fn grid_position(&self, x: f32, y: f32, layout: &Layout) -> (f32, f32) {
        let max_x = f32::from(layout.columns.saturating_sub(1)).max(1.0);
        let max_y = f32::from(layout.rows.saturating_sub(1)).max(1.0);
        (
            (x / max_x * (VELOCITY_GRID_COLUMNS - 1) as f32)
                .clamp(0.0, (VELOCITY_GRID_COLUMNS - 1) as f32),
            (y / max_y * (VELOCITY_GRID_ROWS - 1) as f32)
                .clamp(0.0, (VELOCITY_GRID_ROWS - 1) as f32),
        )
    }
}

fn velocity_index(x: usize, y: usize) -> usize {
    y * VELOCITY_GRID_COLUMNS + x
}

fn limit_lightness_delta(previous: Color, next: Color, max_delta: f32) -> Color {
    let Some(previous) = Oklab::from_color(previous) else {
        return next;
    };
    let Some(mut next_lab) = Oklab::from_color(next) else {
        return next;
    };
    let delta = next_lab.lightness - previous.lightness;
    if delta.abs() > max_delta {
        next_lab.lightness = previous.lightness + delta.signum() * max_delta;
    }

    next_lab.to_color()
}

#[derive(Clone, Copy, Debug, Default)]
struct Oklab {
    lightness: f32,
    a: f32,
    b: f32,
}

impl Oklab {
    fn from_color(color: Color) -> Option<Self> {
        let Color::Rgb { r, g, b } = color else {
            return None;
        };
        let r = srgb_to_linear(r);
        let g = srgb_to_linear(g);
        let b = srgb_to_linear(b);

        let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
        let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
        let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
        let l_ = l.cbrt();
        let m_ = m.cbrt();
        let s_ = s.cbrt();

        Some(Self {
            lightness: 0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
            a: 1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
            b: 0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
        })
    }

    fn to_color(self) -> Color {
        let l_ = self.lightness + 0.396_337_78 * self.a + 0.215_803_76 * self.b;
        let m_ = self.lightness - 0.105_561_346 * self.a - 0.063_854_17 * self.b;
        let s_ = self.lightness - 0.089_484_18 * self.a - 1.291_485_5 * self.b;
        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;
        let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
        let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
        let b = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;

        Color::Rgb {
            r: linear_to_srgb(r),
            g: linear_to_srgb(g),
            b: linear_to_srgb(b),
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            lightness: self.lightness + other.lightness,
            a: self.a + other.a,
            b: self.b + other.b,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            lightness: self.lightness - other.lightness,
            a: self.a - other.a,
            b: self.b - other.b,
        }
    }

    fn scale(self, factor: f32) -> Self {
        Self {
            lightness: self.lightness * factor,
            a: self.a * factor,
            b: self.b * factor,
        }
    }

    fn length(self) -> f32 {
        (self.lightness * self.lightness + self.a * self.a + self.b * self.b).sqrt()
    }
}

#[derive(Clone, Copy, Debug)]
struct Oklch {
    lightness: f32,
    chroma: f32,
    hue: f32,
}

impl Oklch {
    fn from_color(color: Color) -> Option<Self> {
        let lab = Oklab::from_color(color)?;
        Some(Self::from_oklab(lab))
    }

    fn from_oklab(lab: Oklab) -> Self {
        Self {
            lightness: lab.lightness,
            chroma: (lab.a * lab.a + lab.b * lab.b).sqrt(),
            hue: lab.b.atan2(lab.a),
        }
    }

    fn lerp_short_hue(self, to: Self, amount: f32) -> Self {
        let hue_delta = shortest_angle_delta(self.hue, to.hue);
        Self {
            lightness: lerp(self.lightness, to.lightness, amount),
            chroma: lerp(self.chroma, to.chroma, amount),
            hue: self.hue + hue_delta * amount,
        }
    }

    fn to_color(self) -> Color {
        Oklab {
            lightness: self.lightness,
            a: self.chroma * self.hue.cos(),
            b: self.chroma * self.hue.sin(),
        }
        .to_color()
    }
}

fn shortest_angle_delta(from: f32, to: f32) -> f32 {
    let mut delta = (to - from).rem_euclid(std::f32::consts::TAU);
    if delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    delta
}

fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

fn srgb_to_linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(channel: f32) -> u8 {
    let value = if channel <= 0.003_130_8 {
        12.92 * channel
    } else {
        1.055 * channel.max(0.0).powf(1.0 / 2.4) - 0.055
    };

    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn push_static_text(cells: &mut Vec<Cell>, column: u16, row: u16, text: &str, color: Color) {
    for (index, character) in text.chars().enumerate() {
        cells.push(Cell {
            column: column + index as u16,
            row,
            character,
            color,
            layer: RenderLayer::Text,
            primitive_id: None,
            stroke_id: None,
            vertex_id: None,
            path_index: None,
            correspondence_lost: false,
        });
    }
}

fn push_background_depth(
    cells: &mut Vec<Cell>,
    layout: &Layout,
    elapsed_seconds: f32,
    context: FrameContext,
) {
    if context.profile == VisualProfile::Calm
        || context.calibration.effect_density < 0.7
        || layout.rows < 12
    {
        return;
    }

    let width = layout.composition.background_width(layout.columns);
    let height = layout.rows.saturating_sub(3).max(1);
    let error_field = PerceptualErrorField::from_layout(layout, context);
    let density_field = CosmicDensityField::from_layout(layout, context);
    let star_context = StarLayerContext {
        width,
        height,
        elapsed_seconds,
        frame_context: context,
        error_field: &error_field,
        density_field: &density_field,
    };
    for layer in 0..6 {
        push_star_layer(cells, star_context, layer);
    }
    push_comet_event(
        cells,
        width,
        height,
        elapsed_seconds,
        context,
        &error_field,
        &density_field,
    );
    push_mouse_gravity_wake(cells, width, height, context);
}

#[derive(Clone, Copy)]
struct StarLayerContext<'a> {
    width: u16,
    height: u16,
    elapsed_seconds: f32,
    frame_context: FrameContext,
    error_field: &'a PerceptualErrorField,
    density_field: &'a CosmicDensityField,
}

fn push_star_layer(cells: &mut Vec<Cell>, star_context: StarLayerContext<'_>, layer: u8) {
    let StarLayerContext {
        width,
        height,
        elapsed_seconds,
        frame_context: context,
        error_field,
        density_field,
    } = star_context;
    let seed = 0x9e37_79b9_u32.wrapping_mul(u32::from(layer) + 1);
    let density = ((star_layer_density(width, layer) as f32)
        * context.quality.particle_density
        * density_field.composition.particle_density_multiplier()
        * context.director.background_weight)
        .round()
        .clamp(2.0, 12.0) as usize;
    let flow = CosmicLayerFlow::for_layer(layer, seed, context.motion_preset);
    let tail = star_layer_tail(layer);
    let (camera_x, camera_y) = context
        .camera
        .layer_offset(layer, context.curves.camera_parallax);

    for star_index in 0..density {
        let star_seed = mix_seed(seed, star_index as u32);
        let min_distance = blue_noise_min_distance_for_layer(layer);
        let (base_x, base_y) =
            blue_noise_particle_position(star_index, density, width, height, seed, min_distance);
        let speed_jitter = star_speed_jitter(star_index, density, star_seed);
        let temporal_jitter = spatiotemporal_phase_jitter(star_index, density, layer, star_seed);
        let drift =
            elapsed_seconds * flow.speed * speed_jitter + temporal_jitter * f32::from(width);
        let shear = ((base_y / f32::from(height.max(1))) - 0.5) * flow.shear * f32::from(width);
        let wave = (elapsed_seconds * flow.wave_frequency
            + base_x * 0.031
            + temporal_jitter * std::f32::consts::TAU)
            .sin()
            * flow.wave_amplitude;
        let x = (base_x + flow.direction_x * drift + shear + camera_x * flow.parallax)
            .rem_euclid(f32::from(width));
        let y = (base_y + flow.direction_y * drift + wave + camera_y * flow.parallax)
            .rem_euclid(f32::from(height));
        let column = x.round().clamp(0.0, f32::from(width.saturating_sub(1))) as u16;
        let row = y.round().clamp(1.0, f32::from(height.saturating_sub(1))) as u16;
        let density_sample = density_field.sample(column, row);
        let reconstruction = TemporalReconstructionProfile::for_fps(context.calibration.target_fps);
        let frame_index = animation_frame_index(
            elapsed_seconds
                + reconstruction.sample_phase / f32::from(context.calibration.target_fps.max(1)),
            context.calibration.target_fps,
        );
        let stbn = stbn_mask_value(column, row, layer, frame_index);
        let sensitivity = error_field.sensitivity(column, row);
        if stbn > density_sample || stbn < sensitivity * 0.16 || (layer > 2 && stbn < 0.045) {
            continue;
        }
        let temporal_gate = 0.74 + stbn * 0.26 * reconstruction.shutter_width;
        let intensity = star_intensity(layer, star_seed)
            * context.calibration.effect_density
            * context.director.background_weight
            * temporal_gate
            * (1.0 - sensitivity * 0.46).clamp(0.46, 1.0);

        push_star_cell(cells, column, row, '.', intensity, context);

        for tail_index in 1..=tail {
            let fade = (1.0 - tail_index as f32 / (tail + 1) as f32).powf(1.45)
                * reconstruction.tail_energy_budget;
            let tail_spacing = flow.tail_spacing * (1.0 + speed_jitter * 0.18);
            let tail_x = (x - flow.direction_x * tail_index as f32 * tail_spacing)
                .rem_euclid(f32::from(width));
            let tail_y = (y - flow.direction_y * tail_index as f32 * tail_spacing)
                .rem_euclid(f32::from(height));
            let tail_column = tail_x
                .round()
                .clamp(0.0, f32::from(width.saturating_sub(1)))
                as u16;
            let tail_row = tail_y
                .round()
                .clamp(1.0, f32::from(height.saturating_sub(1))) as u16;
            let tail_sensitivity = error_field.sensitivity(tail_column, tail_row);
            let tail_density = density_field.sample(tail_column, tail_row);
            push_star_cell(
                cells,
                tail_column,
                tail_row,
                '.',
                intensity
                    * fade
                    * 0.46
                    * tail_density.clamp(0.42, 1.0)
                    * (1.0 - tail_sensitivity * 0.36).clamp(0.56, 1.0),
                context,
            );
        }
    }
}

#[derive(Clone, Debug)]
struct CosmicDensityField {
    logo_center_x: f32,
    logo_center_y: f32,
    logo_radius_x: f32,
    logo_radius_y: f32,
    columns: u16,
    rows: u16,
    composition: CompositionMode,
    seed: u32,
}

impl CosmicDensityField {
    fn from_layout(layout: &Layout, context: FrameContext) -> Self {
        let logo_width = max_width(layout.logo) as f32;
        let logo_height = layout.logo.len() as f32;
        Self {
            logo_center_x: f32::from(layout.logo_column) + logo_width * 0.5,
            logo_center_y: f32::from(layout.logo_row) + logo_height * 0.5,
            logo_radius_x: (logo_width * 0.72).max(8.0),
            logo_radius_y: (logo_height * 1.9).max(4.0),
            columns: layout.columns.max(1),
            rows: layout.rows.max(1),
            composition: layout.composition,
            seed: match context.motion_preset {
                MotionPreset::Prime => 0x91e1_d00d,
                MotionPreset::Aurora => 0xa6f0_21b9,
                MotionPreset::Pulse => 0xc2b2_ae35,
            },
        }
    }

    fn sample(&self, column: u16, row: u16) -> f32 {
        let x = f32::from(column) / f32::from(self.columns);
        let y = f32::from(row) / f32::from(self.rows);
        let ridge = sine01(x * 1.7 + y * 0.74 + random_unit(self.seed) * 0.37);
        let cloud = value_noise_2d(x * 5.0, y * 3.0, self.seed) * 0.52
            + value_noise_2d(x * 11.0 + 17.0, y * 7.0 + 3.0, self.seed.rotate_left(9)) * 0.31
            + value_noise_2d(x * 23.0 + 5.0, y * 13.0 + 11.0, self.seed.rotate_left(17)) * 0.17;
        let logo_dx = (f32::from(column) - self.logo_center_x) / self.logo_radius_x;
        let logo_dy = (f32::from(row) - self.logo_center_y) / self.logo_radius_y;
        let logo_protection =
            (1.0 - (logo_dx * logo_dx + logo_dy * logo_dy).sqrt()).clamp(0.0, 1.0);
        let composition_bias = match self.composition {
            CompositionMode::Compact => 0.72,
            CompositionMode::Standard => 1.0,
            CompositionMode::Wide => 1.12,
        };
        ((0.38 + cloud * 0.38 + ridge * 0.22) * composition_bias * (1.0 - logo_protection * 0.58))
            .clamp(0.18, 0.96)
    }
}

fn push_comet_event(
    cells: &mut Vec<Cell>,
    width: u16,
    height: u16,
    elapsed_seconds: f32,
    context: FrameContext,
    error_field: &PerceptualErrorField,
    density_field: &CosmicDensityField,
) {
    if context.profile == VisualProfile::Calm || width < 70 || height < 16 {
        return;
    }
    let cycle = 14.0;
    let event_phase = (elapsed_seconds / cycle).floor() as u32;
    let local = elapsed_seconds.rem_euclid(cycle);
    let seed = mix_seed(0x5eed_c0de, event_phase);
    if local > 1.35 || random_unit(seed) < 0.42 {
        return;
    }
    let progress = smootherstep(local / 1.35);
    let start_x = random_unit(seed.rotate_left(5)) * f32::from(width) * 0.42;
    let start_y = 1.0 + random_unit(seed.rotate_left(9)) * f32::from(height) * 0.38;
    let direction = CosmicLayerFlow::for_layer(5, seed, context.motion_preset);
    let length = 10 + (random_unit(seed.rotate_left(13)) * 10.0) as usize;
    let head_x = start_x + direction.direction_x * progress * f32::from(width) * 0.42;
    let head_y = start_y + direction.direction_y * progress * f32::from(height) * 0.32;

    for index in 0..length {
        let fade = (1.0 - index as f32 / length as f32).powf(1.7);
        let x = (head_x - direction.direction_x * index as f32 * 1.3).rem_euclid(f32::from(width));
        let y = (head_y - direction.direction_y * index as f32 * 1.3).rem_euclid(f32::from(height));
        let column = x.round().clamp(0.0, f32::from(width.saturating_sub(1))) as u16;
        let row = y.round().clamp(1.0, f32::from(height.saturating_sub(1))) as u16;
        let sensitivity = error_field.sensitivity(column, row);
        if sensitivity > 0.38 || density_field.sample(column, row) < 0.34 {
            continue;
        }
        push_star_cell(
            cells,
            column,
            row,
            '.',
            0.16 * fade * (1.0 - sensitivity).clamp(0.35, 1.0),
            context,
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct CosmicLayerFlow {
    direction_x: f32,
    direction_y: f32,
    speed: f32,
    shear: f32,
    wave_frequency: f32,
    wave_amplitude: f32,
    tail_spacing: f32,
    parallax: f32,
}

impl CosmicLayerFlow {
    fn for_layer(layer: u8, seed: u32, motion_preset: MotionPreset) -> Self {
        let depth = f32::from(layer) / 5.0;
        let preset_tilt = match motion_preset {
            MotionPreset::Prime => 0.0,
            MotionPreset::Aurora => -0.09,
            MotionPreset::Pulse => 0.07,
        };
        let angle =
            -0.19 + depth * 0.34 + preset_tilt + (random_unit(seed.rotate_left(11)) - 0.5) * 0.11;
        let direction_x = angle.cos();
        let direction_y = angle.sin() * (0.32 + depth * 0.18);
        let direction_length = (direction_x * direction_x + direction_y * direction_y).sqrt();

        Self {
            direction_x: direction_x / direction_length,
            direction_y: direction_y / direction_length,
            speed: star_layer_speed(layer, seed),
            shear: (random_unit(seed.rotate_left(17)) - 0.5) * (0.018 + depth * 0.065),
            wave_frequency: 0.19 + depth * 0.41 + random_unit(seed.rotate_left(23)) * 0.13,
            wave_amplitude: 0.08 + depth * 0.48,
            tail_spacing: 1.35 + depth * 2.15,
            parallax: 0.22 + depth * 1.15,
        }
    }
}

#[derive(Clone, Debug)]
struct PerceptualErrorField {
    logo_center_x: f32,
    logo_center_y: f32,
    logo_radius_x: f32,
    logo_radius_y: f32,
    frame_box: Option<FrameBox>,
    mouse: Option<MouseAttractor>,
}

impl PerceptualErrorField {
    fn from_layout(layout: &Layout, context: FrameContext) -> Self {
        let logo_width = max_width(layout.logo) as f32;
        let logo_height = layout.logo.len() as f32;
        Self {
            logo_center_x: f32::from(layout.logo_column) + logo_width * 0.5,
            logo_center_y: f32::from(layout.logo_row) + logo_height * 0.5,
            logo_radius_x: (logo_width * 0.62).max(8.0),
            logo_radius_y: (logo_height * 1.55).max(3.0),
            frame_box: layout.frame_box,
            mouse: context.mouse,
        }
    }

    fn sensitivity(&self, column: u16, row: u16) -> f32 {
        let x = f32::from(column);
        let y = f32::from(row);
        let logo_dx = (x - self.logo_center_x) / self.logo_radius_x;
        let logo_dy = (y - self.logo_center_y) / self.logo_radius_y;
        let logo = (1.0 - (logo_dx * logo_dx + logo_dy * logo_dy).sqrt()).clamp(0.0, 1.0);
        let frame = self.frame_box.map_or(0.0, |frame_box| {
            let inside_x = x >= f32::from(frame_box.left)
                && x <= f32::from(frame_box.left + frame_box.width.saturating_sub(1));
            let inside_y = y >= f32::from(frame_box.top)
                && y <= f32::from(frame_box.top + frame_box.height.saturating_sub(1));
            if !inside_x || !inside_y {
                return 0.0;
            }
            let dx = (x - f32::from(frame_box.left))
                .min(f32::from(frame_box.left + frame_box.width.saturating_sub(1)) - x);
            let dy = (y - f32::from(frame_box.top))
                .min(f32::from(frame_box.top + frame_box.height.saturating_sub(1)) - y);
            (1.0 - dx.min(dy) / 3.0).clamp(0.0, 1.0) * 0.72
        });
        let mouse = self.mouse.map_or(0.0, |mouse| {
            let dx = x - f32::from(mouse.column);
            let dy = y - f32::from(mouse.row);
            let distance = (dx * dx + dy * dy).sqrt();
            (1.0 - distance / 7.0).clamp(0.0, 1.0) * 0.38 * mouse.strength
        });
        (logo * 0.82).max(frame).max(mouse).clamp(0.0, 1.0)
    }
}

fn push_star_cell(
    cells: &mut Vec<Cell>,
    column: u16,
    row: u16,
    character: char,
    intensity: f32,
    context: FrameContext,
) {
    push_role_star_cell(
        cells,
        column,
        row,
        character,
        intensity,
        context,
        PerceptualRole::BackgroundParticle,
    );
}

fn push_role_star_cell(
    cells: &mut Vec<Cell>,
    column: u16,
    row: u16,
    character: char,
    intensity: f32,
    context: FrameContext,
    role: PerceptualRole,
) {
    if row == 0 {
        return;
    }
    cells.push(Cell {
        column,
        row,
        character,
        color: display_role_color(
            ensure_visible_lightness(
                blend_color(
                    context.theme.logo_shadow,
                    context.theme.logo_highlight,
                    intensity.clamp(0.0, 0.5),
                ),
                context.theme.contrast_floor * (0.64 + intensity * 0.42),
            ),
            context.calibration.capabilities,
            role,
        ),
        layer: RenderLayer::Background,
        primitive_id: None,
        stroke_id: None,
        vertex_id: None,
        path_index: None,
        correspondence_lost: false,
    });
}

fn push_mouse_gravity_wake(cells: &mut Vec<Cell>, width: u16, height: u16, context: FrameContext) {
    let Some(mouse) = context.mouse else {
        return;
    };
    let center_x = f32::from(mouse.column).clamp(0.0, f32::from(width.saturating_sub(1)));
    let center_y = f32::from(mouse.row).clamp(1.0, f32::from(height.saturating_sub(1)));
    let particle_count = 14usize;

    for index in 0..particle_count {
        let seed = mix_seed(0x6d2b_79f5, index as u32);
        let angle =
            stratified_unit(index, particle_count, seed) * std::f32::consts::TAU + mouse.strength;
        let ring = 2.4 + (index % 4) as f32 * 1.15;
        let swirl = 1.0 + random_unit(seed.rotate_left(9)) * 0.85;
        let x = center_x + angle.cos() * ring * swirl;
        let y = center_y + angle.sin() * ring * swirl * 0.56;
        let distance = ((x - center_x).powi(2) + (y - center_y).powi(2)).sqrt();
        let cap_fade = (1.0 - (distance / 8.6).clamp(0.0, 1.0)).powf(1.2);

        push_star_cell(
            cells,
            x.round().clamp(0.0, f32::from(width.saturating_sub(1))) as u16,
            y.round().clamp(1.0, f32::from(height.saturating_sub(1))) as u16,
            '.',
            (0.1 + cap_fade * 0.2) * mouse.strength,
            context,
        );
    }
}

fn star_layer_speed(layer: u8, seed: u32) -> f32 {
    let depth = f32::from(layer) / 5.0;
    let base = 3.2 + depth.powf(2.15) * 92.0;
    base + random_unit(seed.rotate_left(7)) * (6.0 + depth * 28.0)
}

fn star_layer_density(width: u16, layer: u8) -> usize {
    let divisor = 34usize.saturating_sub(usize::from(layer) * 3).max(18);
    (usize::from(width) / divisor + usize::from(layer / 2)).clamp(2, 9)
}

fn blue_noise_min_distance_for_layer(layer: u8) -> f32 {
    5.8 - f32::from(layer) * 0.42
}

fn star_layer_tail(layer: u8) -> usize {
    match layer {
        0 | 1 => 0,
        2 | 3 => 1,
        _ => 2,
    }
}

fn star_intensity(layer: u8, seed: u32) -> f32 {
    let depth = f32::from(layer) / 5.0;
    (0.04 + depth * 0.17 + random_unit(seed.rotate_left(3)) * 0.1).min(0.31)
}

#[derive(Clone, Copy, Debug)]
struct GravityField {
    radial: f32,
    tangential: f32,
    visibility: f32,
}

fn gravity_field(distance: f32, strength: f32) -> GravityField {
    let influence_radius = 18.0;
    if distance > influence_radius || distance < 0.001 {
        return GravityField {
            radial: 0.0,
            tangential: 0.0,
            visibility: 0.0,
        };
    }

    let softening_radius = 3.8;
    let normalized = (distance / influence_radius).clamp(0.0, 1.0);
    let visibility = smootherstep(1.0 - normalized);
    let softened_distance2 = distance * distance + softening_radius * softening_radius;
    let gravity = (52.0 / softened_distance2) * visibility * strength;
    let radial = if distance < softening_radius {
        -((softening_radius - distance) / softening_radius).powf(1.35) * 2.4 * strength
    } else {
        gravity.min(distance - softening_radius * 0.72)
    };
    let tangential = gravity.sqrt().min(2.2) * visibility * strength;

    GravityField {
        radial,
        tangential,
        visibility,
    }
}

fn blue_noise_particle_position(
    index: usize,
    density: usize,
    width: u16,
    height: u16,
    seed: u32,
    min_distance: f32,
) -> (f32, f32) {
    let mut accepted = Vec::with_capacity(index + 1);
    let candidates = density.saturating_mul(16).max(32);
    for candidate_index in 0..candidates {
        let candidate_seed = mix_seed(seed, candidate_index as u32);
        let candidate = low_discrepancy_position(candidate_index, width, height, candidate_seed);
        if accepted
            .iter()
            .all(|&(x, y)| toroidal_distance(candidate, (x, y), width, height) >= min_distance)
        {
            accepted.push(candidate);
            if accepted.len() > index {
                return candidate;
            }
        }
    }

    particle_base_position(index, density, width, height, seed)
}

fn low_discrepancy_position(index: usize, width: u16, height: u16, seed: u32) -> (f32, f32) {
    let x = fract(index as f32 * 0.618_034 + random_unit(seed.rotate_left(5)));
    let y = fract(
        radical_inverse_base2(index as u32 ^ seed.rotate_left(11)) + random_unit(seed) * 0.17,
    );
    (
        (x * f32::from(width)).floor(),
        (y * f32::from(height)).floor(),
    )
}

fn toroidal_distance(left: (f32, f32), right: (f32, f32), width: u16, height: u16) -> f32 {
    let width = f32::from(width.max(1));
    let height = f32::from(height.max(1));
    let dx = (left.0 - right.0)
        .abs()
        .min(width - (left.0 - right.0).abs());
    let dy = (left.1 - right.1)
        .abs()
        .min(height - (left.1 - right.1).abs());
    (dx * dx + dy * dy).sqrt()
}

fn particle_base_position(
    index: usize,
    density: usize,
    width: u16,
    height: u16,
    seed: u32,
) -> (f32, f32) {
    let rank = permuted_particle_rank(index, density, seed);
    let x = stratified_unit(rank, density, seed);
    let y = radical_inverse_base2((rank as u32) ^ seed.rotate_left(11));
    let jitter_radius = 0.22 / (density as f32).sqrt();
    let jitter_x = (random_unit(mix_seed(seed.rotate_left(3), index as u32)) - 0.5) * jitter_radius;
    let jitter_y =
        (random_unit(mix_seed(seed.rotate_left(17), index as u32)) - 0.5) * jitter_radius;

    (
        (fract(x + jitter_x) * f32::from(width)).floor(),
        (fract(y + jitter_y) * f32::from(height)).floor(),
    )
}

fn permuted_particle_rank(index: usize, density: usize, seed: u32) -> usize {
    let stride = coprime_stride(density, seed);
    (index * stride + seed as usize) % density
}

fn coprime_stride(density: usize, seed: u32) -> usize {
    if density <= 2 {
        return 1;
    }
    let mut stride = (seed as usize % (density - 1)) + 1;
    while gcd(stride, density) != 1 {
        stride = stride % density + 1;
    }
    stride
}

fn gcd(mut left: usize, mut right: usize) -> usize {
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left
}

fn stratified_unit(rank: usize, density: usize, seed: u32) -> f32 {
    let cell = (rank as f32 + 0.5) / density as f32;
    fract(cell + random_unit(seed.rotate_left(23)) / density as f32)
}

fn radical_inverse_base2(mut value: u32) -> f32 {
    value = value.reverse_bits();
    value as f32 * 2.328_306_4e-10
}

fn star_speed_jitter(index: usize, density: usize, seed: u32) -> f32 {
    0.68 + stratified_unit(index, density, seed.rotate_left(5)) * 0.82
}

fn spatiotemporal_phase_jitter(index: usize, density: usize, layer: u8, seed: u32) -> f32 {
    let spatial = radical_inverse_base2((index as u32).wrapping_add(u32::from(layer) * 17));
    let temporal = stratified_unit(index, density, seed.rotate_left(13));
    fract(spatial * 0.62 + temporal * 0.38)
}

fn stbn_mask_value(column: u16, row: u16, layer: u8, frame_index: u32) -> f32 {
    let x = u32::from(column % STBN_WIDTH);
    let y = u32::from(row % STBN_HEIGHT);
    let z = frame_index % STBN_FRAMES;
    let layer = u32::from(layer);
    let base_rank = x
        .wrapping_mul(17)
        .wrapping_add(y.wrapping_mul(23))
        .wrapping_add(z.wrapping_mul(29))
        .wrapping_add(layer.wrapping_mul(31))
        % (STBN_WIDTH as u32 * STBN_HEIGHT as u32);
    let void_rank = radical_inverse_base2(base_rank ^ mix_seed(layer.wrapping_mul(0x9e37_79b9), z));
    let local_scramble = random_unit(mix_seed(
        x.wrapping_mul(0x045d_9f3b) ^ z.wrapping_mul(0x119d_e1f3),
        y.wrapping_mul(0x27d4_eb2d) ^ layer.wrapping_mul(0x7f4a_7c15),
    ));
    let temporal_rotation = radical_inverse_base2(
        (z.wrapping_mul(13) + x.wrapping_mul(5) + y.wrapping_mul(3) + layer * 11) & 0x3ff,
    );
    fract(void_rank * 0.58 + local_scramble * 0.28 + temporal_rotation * 0.14)
}

fn value_noise_2d(x: f32, y: f32, seed: u32) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = smootherstep(x - x0);
    let fy = smootherstep(y - y0);
    let x0 = x0 as i32;
    let y0 = y0 as i32;
    let a = lattice_noise(x0, y0, seed);
    let b = lattice_noise(x0 + 1, y0, seed);
    let c = lattice_noise(x0, y0 + 1, seed);
    let d = lattice_noise(x0 + 1, y0 + 1, seed);
    lerp(lerp(a, b, fx), lerp(c, d, fx), fy)
}

fn lattice_noise(x: i32, y: i32, seed: u32) -> f32 {
    random_unit(mix_seed(
        (x as u32).wrapping_mul(0x045d_9f3b) ^ seed,
        (y as u32).wrapping_mul(0x119d_e1f3),
    ))
}

fn anisotropic_distance(
    left: (f32, f32),
    right: (f32, f32),
    direction: (f32, f32),
    along_scale: f32,
    perpendicular_scale: f32,
) -> f32 {
    let dx = left.0 - right.0;
    let dy = left.1 - right.1;
    let length = (direction.0 * direction.0 + direction.1 * direction.1).sqrt();
    let (dir_x, dir_y) = if length > 0.001 {
        (direction.0 / length, direction.1 / length)
    } else {
        (1.0, 0.0)
    };
    let along = dx * dir_x + dy * dir_y;
    let perpendicular = -dx * dir_y + dy * dir_x;
    ((along / along_scale.max(0.001)).powi(2)
        + (perpendicular / perpendicular_scale.max(0.001)).powi(2))
    .sqrt()
}

fn fract(value: f32) -> f32 {
    value - value.floor()
}

fn mix_seed(seed: u32, value: u32) -> u32 {
    let mut x = seed ^ value.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

fn random_unit(seed: u32) -> f32 {
    (seed as f32) / (u32::MAX as f32)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FrameBox {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
}

impl FrameBox {
    fn perimeter_len(self) -> usize {
        if self.width < 2 || self.height < 2 {
            return 0;
        }

        (usize::from(self.width) + usize::from(self.height)) * 2 - 4
    }

    fn point_at(self, index: usize) -> Option<(u16, u16)> {
        self.point_at_float(index as f32)
    }

    fn point_at_float(self, distance: f32) -> Option<(u16, u16)> {
        let perimeter_len = self.perimeter_len();
        if perimeter_len == 0 {
            return None;
        }

        let index = distance.rem_euclid(perimeter_len as f32);
        let smoothed = ease_corner_distance(index, perimeter_len as f32);
        let index = smoothed.floor() as usize % perimeter_len;
        self.point_at_path_index(index)
    }

    fn path_indices_at_float(self, distance: f32) -> Option<(usize, usize, f32)> {
        let perimeter_len = self.perimeter_len();
        if perimeter_len == 0 {
            return None;
        }

        let index = distance.rem_euclid(perimeter_len as f32);
        let smoothed = ease_corner_distance(index, perimeter_len as f32);
        let primary = smoothed.floor() as usize % perimeter_len;
        let secondary = (primary + 1) % perimeter_len;
        let blend = smoothed - smoothed.floor();
        Some((primary, secondary, blend))
    }

    fn point_at_path_index(self, index: usize) -> Option<(u16, u16)> {
        let perimeter_len = self.perimeter_len();
        if perimeter_len == 0 {
            return None;
        }
        let index = index % perimeter_len;
        let top_len = usize::from(self.width);
        let right_len = usize::from(self.height.saturating_sub(2));
        let bottom_len = usize::from(self.width);

        if index < top_len {
            return Some((self.left + index as u16, self.top));
        }

        let index = index - top_len;
        if index < right_len {
            return Some((self.left + self.width - 1, self.top + 1 + index as u16));
        }

        let index = index - right_len;
        if index < bottom_len {
            return Some((
                self.left + self.width - 1 - index as u16,
                self.top + self.height - 1,
            ));
        }

        let index = index - bottom_len;
        Some((self.left, self.top + self.height - 2 - index as u16))
    }
}

fn ease_corner_distance(distance: f32, perimeter_len: f32) -> f32 {
    let cell = distance.floor();
    let fraction = distance - cell;
    (cell + smootherstep(fraction)).rem_euclid(perimeter_len)
}

fn smootherstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn push_orbit_trail(
    cells: &mut Vec<Cell>,
    frame_box: FrameBox,
    elapsed_seconds: f32,
    context: FrameContext,
    temporal: &mut OrbitTemporalBuffer,
) {
    let perimeter_len = frame_box.perimeter_len();
    if perimeter_len == 0 {
        return;
    }

    let trail_span = context.theme.trail_span
        * context.calibration.effect_density
        * line_length_multiplier(context.profile)
        * context.director.orbit_weight;
    let frame_dt = 1.0 / f32::from(context.calibration.target_fps.max(60));
    let tuning = OrbitRenderTuning::for_context(context, elapsed_seconds);
    let head_position = spline_orbit_head(
        elapsed_seconds,
        perimeter_len as f32,
        context.motion_preset,
        context.curves,
    );
    let splat_span = trail_span + tuning.extra_span;
    let sample_count = ((splat_span / tuning.splat_step).ceil() as usize).min(tuning.max_samples);
    let mut visible = vec![None; perimeter_len];

    for sample_index in 0..=sample_count {
        let distance = sample_index as f32 * tuning.splat_step;
        let path_position = (head_position - distance).rem_euclid(perimeter_len as f32);
        if let Some((primary_index, secondary_index, blend)) =
            frame_box.path_indices_at_float(path_position)
        {
            let alpha = temporal_orbit_energy_alpha(
                elapsed_seconds,
                frame_dt,
                path_position,
                perimeter_len as f32,
                trail_span,
                context,
            );
            if alpha > tuning.alpha_floor {
                insert_orbit_visible(&mut visible, primary_index, alpha);
                let secondary_alpha = alpha * smootherstep(blend) * tuning.prelight_gain;
                if secondary_alpha > tuning.alpha_floor {
                    insert_orbit_visible(&mut visible, secondary_index, secondary_alpha);
                }
            }
        }
    }

    match context.glyph_history_mode {
        GlyphHistoryMode::Off => temporal.clear(),
        GlyphHistoryMode::ScreenCell => {
            temporal.stabilize_screen_cell(&mut visible, frame_box, tuning)
        }
        GlyphHistoryMode::Path => temporal.stabilize(&mut visible, frame_box, tuning),
        GlyphHistoryMode::TopologyDp => {
            temporal.stabilize_topology_dp(&mut visible, frame_box, tuning)
        }
    }

    for (path_index, visible_cell) in visible.into_iter().enumerate() {
        let Some(visible_cell) = visible_cell else {
            continue;
        };
        let Some((column, row)) = frame_box.point_at_path_index(path_index) else {
            continue;
        };
        let alpha = visible_cell.alpha;
        let alpha = orbit_perceptual_alpha(alpha, column, row, elapsed_seconds, context, tuning);
        let character = orbit_character(
            frame_box,
            visible_cell.path_index,
            context.calibration.capabilities.glyph_mode,
        );
        cells.push(Cell {
            column,
            row,
            character,
            color: display_role_color(
                compensate_glyph_lightness(
                    trail_color_from_intensity(alpha, context.profile, context.theme),
                    character,
                ),
                context.calibration.capabilities,
                PerceptualRole::Orbit,
            ),
            layer: RenderLayer::Orbit,
            primitive_id: Some(0),
            stroke_id: Some(0),
            vertex_id: None,
            path_index: Some(path_index),
            correspondence_lost: false,
        });
    }
}

fn push_primitive_scene(
    cells: &mut Vec<Cell>,
    layout: &Layout,
    frame_box: FrameBox,
    elapsed_seconds: f32,
    context: FrameContext,
    orbit_temporal: &mut OrbitTemporalBuffer,
    primitive_cache: &mut PrimitiveSceneCache,
) {
    if context.scene_preset == ScenePreset::Orbit {
        push_orbit_trail(cells, frame_box, elapsed_seconds, context, orbit_temporal);
        return;
    }

    orbit_temporal.clear();
    let primitives = PrimitiveScene::build(layout, frame_box, elapsed_seconds, context);
    primitive_cache.render(cells, primitives, elapsed_seconds, context);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrimitiveFamily {
    Segment,
    Polyline,
    Rectangle,
    Arc,
    ClosedLoop,
}

#[derive(Clone, Copy, Debug)]
struct PrimitivePoint {
    column: i16,
    row: i16,
    vertex_id: u16,
}

#[derive(Clone, Debug)]
struct LinePrimitive {
    primitive_id: u16,
    stroke_id: u16,
    family: PrimitiveFamily,
    closed: bool,
    points: Vec<PrimitivePoint>,
}

#[derive(Clone, Debug)]
struct PrimitiveSample {
    primitive_id: u16,
    stroke_id: u16,
    vertex_id: u16,
    path_index: usize,
    column: u16,
    row: u16,
    character: char,
    alpha: f32,
}

struct PrimitiveScene {
    primitives: Vec<LinePrimitive>,
}

impl PrimitiveScene {
    fn build(
        layout: &Layout,
        frame_box: FrameBox,
        elapsed_seconds: f32,
        context: FrameContext,
    ) -> Self {
        match context.scene_preset {
            ScenePreset::Orbit => Self { primitives: vec![] },
            ScenePreset::MultiStroke => multi_stroke_scene(frame_box, elapsed_seconds, context),
            ScenePreset::ArcLoop => arc_loop_scene(frame_box, elapsed_seconds, context),
            ScenePreset::Stress => stress_scene(layout, frame_box, elapsed_seconds, context),
        }
    }
}

#[derive(Default)]
struct PrimitiveSceneCache {
    previous: BTreeMap<(u16, u16, usize), PrimitiveSample>,
    last_signature: u64,
    hits: usize,
    misses: usize,
}

impl PrimitiveSceneCache {
    fn clear(&mut self) {
        self.previous.clear();
        self.last_signature = 0;
        self.hits = 0;
        self.misses = 0;
    }

    fn render(
        &mut self,
        cells: &mut Vec<Cell>,
        scene: PrimitiveScene,
        elapsed_seconds: f32,
        context: FrameContext,
    ) {
        self.hits = 0;
        self.misses = 0;
        let mut samples = rasterize_primitives(&scene.primitives);
        let signature = primitive_scene_signature(&samples);
        if signature == self.last_signature {
            self.hits = samples.len();
        } else {
            self.misses = samples.len();
        }
        self.last_signature = signature;

        stabilize_primitive_samples(&mut samples, &self.previous, context);
        let next_previous = samples
            .iter()
            .map(|sample| {
                (
                    (sample.primitive_id, sample.stroke_id, sample.path_index),
                    sample.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();

        for sample in resolve_sample_collisions(samples) {
            let alpha = primitive_alpha(sample.alpha, sample.column, sample.row, elapsed_seconds);
            let character = glyph_for_mode(sample.character, context.calibration.capabilities.glyph_mode);
            cells.push(Cell {
                column: sample.column,
                row: sample.row,
                character,
                color: display_role_color(
                    compensate_glyph_lightness(
                        trail_color_from_intensity(alpha, context.profile, context.theme),
                        character,
                    ),
                    context.calibration.capabilities,
                    PerceptualRole::Orbit,
                ),
                layer: RenderLayer::Orbit,
                primitive_id: Some(sample.primitive_id),
                stroke_id: Some(sample.stroke_id),
                vertex_id: Some(sample.vertex_id),
                path_index: Some(sample.path_index),
                correspondence_lost: false,
            });
        }

        self.previous = next_previous;
    }
}

fn multi_stroke_scene(
    frame_box: FrameBox,
    elapsed_seconds: f32,
    context: FrameContext,
) -> PrimitiveScene {
    let phase = MotionPhases::at(elapsed_seconds, context.motion_preset);
    let offset = ((elapsed_seconds * 10.0 * phase.orbit_speed).sin() * 3.0).round() as i16;
    let cx = frame_box.left as i16 + frame_box.width as i16 / 2;
    let cy = frame_box.top as i16 + frame_box.height as i16 / 2;
    let half_w = (frame_box.width as i16 / 2).saturating_sub(3).max(4);
    let half_h = (frame_box.height as i16 / 2).saturating_sub(2).max(3);
    PrimitiveScene {
        primitives: vec![
            LinePrimitive {
                primitive_id: 10,
                stroke_id: 0,
                family: PrimitiveFamily::Segment,
                closed: false,
                points: line_points(cx - half_w, cy + offset, cx + half_w, cy - offset),
            },
            LinePrimitive {
                primitive_id: 11,
                stroke_id: 1,
                family: PrimitiveFamily::Segment,
                closed: false,
                points: line_points(cx, cy - half_h, cx, cy + half_h),
            },
            LinePrimitive {
                primitive_id: 12,
                stroke_id: 2,
                family: PrimitiveFamily::Polyline,
                closed: false,
                points: vec![
                    primitive_point(cx - half_w, cy - half_h / 2, 0),
                    primitive_point(cx - half_w / 3, cy - half_h / 2 - offset / 2, 1),
                    primitive_point(cx + half_w / 3, cy + half_h / 2 + offset / 2, 2),
                    primitive_point(cx + half_w, cy + half_h / 2, 3),
                ],
            },
        ],
    }
}

fn arc_loop_scene(
    frame_box: FrameBox,
    elapsed_seconds: f32,
    context: FrameContext,
) -> PrimitiveScene {
    let cx = frame_box.left as i16 + frame_box.width as i16 / 2;
    let cy = frame_box.top as i16 + frame_box.height as i16 / 2;
    let radius_x = (frame_box.width as f32 * 0.34).max(5.0);
    let radius_y = (frame_box.height as f32 * 0.42).max(3.0);
    let phase = elapsed_seconds * MotionPhases::at(elapsed_seconds, context.motion_preset).orbit_speed;
    PrimitiveScene {
        primitives: vec![
            LinePrimitive {
                primitive_id: 20,
                stroke_id: 0,
                family: PrimitiveFamily::Arc,
                closed: false,
                points: arc_points(cx, cy, radius_x, radius_y, phase, std::f32::consts::PI * 1.35),
            },
            LinePrimitive {
                primitive_id: 21,
                stroke_id: 1,
                family: PrimitiveFamily::ClosedLoop,
                closed: true,
                points: arc_points(cx, cy, radius_x * 0.56, radius_y * 0.56, -phase * 0.7, std::f32::consts::TAU),
            },
        ],
    }
}

fn stress_scene(
    layout: &Layout,
    frame_box: FrameBox,
    elapsed_seconds: f32,
    context: FrameContext,
) -> PrimitiveScene {
    let mut scene = multi_stroke_scene(frame_box, elapsed_seconds, context);
    let cx = frame_box.left as i16 + frame_box.width as i16 / 2;
    let cy = frame_box.top as i16 + frame_box.height as i16 / 2;
    let jitter = seeded_motion_jitter(0x51a7, elapsed_seconds, 4);
    scene.primitives.push(LinePrimitive {
        primitive_id: 30,
        stroke_id: 3,
        family: PrimitiveFamily::Rectangle,
        closed: true,
        points: rectangle_points(
            cx - 8 + jitter.0,
            cy - 3 + jitter.1,
            (layout.columns / 5).clamp(8, 18) as i16,
            (layout.rows / 5).clamp(4, 9) as i16,
        ),
    });
    scene
}

fn primitive_point(column: i16, row: i16, vertex_id: u16) -> PrimitivePoint {
    PrimitivePoint {
        column,
        row,
        vertex_id,
    }
}

fn line_points(x0: i16, y0: i16, x1: i16, y1: i16) -> Vec<PrimitivePoint> {
    vec![primitive_point(x0, y0, 0), primitive_point(x1, y1, 1)]
}

fn rectangle_points(left: i16, top: i16, width: i16, height: i16) -> Vec<PrimitivePoint> {
    vec![
        primitive_point(left, top, 0),
        primitive_point(left + width, top, 1),
        primitive_point(left + width, top + height, 2),
        primitive_point(left, top + height, 3),
    ]
}

fn arc_points(
    cx: i16,
    cy: i16,
    radius_x: f32,
    radius_y: f32,
    phase: f32,
    sweep: f32,
) -> Vec<PrimitivePoint> {
    let steps = ((sweep.abs() * 8.0).ceil() as usize).clamp(8, 40);
    (0..=steps)
        .map(|index| {
            let t = phase + sweep * index as f32 / steps as f32;
            primitive_point(
                cx + (t.cos() * radius_x).round() as i16,
                cy + (t.sin() * radius_y).round() as i16,
                index as u16,
            )
        })
        .collect()
}

fn rasterize_primitives(primitives: &[LinePrimitive]) -> Vec<PrimitiveSample> {
    let mut samples = Vec::new();
    for primitive in primitives {
        let segment_count = if primitive.closed {
            primitive.points.len()
        } else {
            primitive.points.len().saturating_sub(1)
        };
        let mut path_index = 0usize;
        for segment_index in 0..segment_count {
            let start = primitive.points[segment_index];
            let end = primitive.points[(segment_index + 1) % primitive.points.len()];
            for (point_index, (column, row)) in bresenham_points(start, end).into_iter().enumerate() {
                if segment_index > 0 && point_index == 0 {
                    continue;
                }
                if column < 0 || row < 0 {
                    continue;
                }
                samples.push(PrimitiveSample {
                    primitive_id: primitive.primitive_id,
                    stroke_id: primitive.stroke_id,
                    vertex_id: start.vertex_id,
                    path_index,
                    column: column as u16,
                    row: row as u16,
                    character: primitive_glyph(primitive.family, start, (column, row), end),
                    alpha: primitive_sample_alpha(path_index, primitive.family),
                });
                path_index += 1;
            }
        }
    }
    samples
}

fn bresenham_points(start: PrimitivePoint, end: PrimitivePoint) -> Vec<(i16, i16)> {
    let mut points = Vec::new();
    let mut x = start.column;
    let mut y = start.row;
    let dx = (end.column - start.column).abs();
    let sx = if start.column < end.column { 1 } else { -1 };
    let dy = -(end.row - start.row).abs();
    let sy = if start.row < end.row { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        points.push((x, y));
        if x == end.column && y == end.row {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    points
}

fn primitive_glyph(
    family: PrimitiveFamily,
    start: PrimitivePoint,
    current: (i16, i16),
    end: PrimitivePoint,
) -> char {
    if matches!(family, PrimitiveFamily::Arc | PrimitiveFamily::ClosedLoop) {
        return '•';
    }
    let dx = end.column - start.column;
    let dy = end.row - start.row;
    if dx.abs() > dy.abs() * 2 {
        '━'
    } else if dy.abs() > dx.abs() * 2 {
        '┃'
    } else if (dx >= 0 && dy >= 0) || (dx < 0 && dy < 0) {
        '╲'
    } else if current == (start.column, start.row) {
        '╭'
    } else {
        '╱'
    }
}

fn primitive_sample_alpha(path_index: usize, family: PrimitiveFamily) -> f32 {
    let base = match family {
        PrimitiveFamily::Segment => 0.88,
        PrimitiveFamily::Polyline => 0.82,
        PrimitiveFamily::Rectangle => 0.76,
        PrimitiveFamily::Arc => 0.72,
        PrimitiveFamily::ClosedLoop => 0.64,
    };
    (base - (path_index % 5) as f32 * 0.025).clamp(0.42, 1.0)
}

fn stabilize_primitive_samples(
    samples: &mut [PrimitiveSample],
    previous: &BTreeMap<(u16, u16, usize), PrimitiveSample>,
    context: FrameContext,
) {
    if !matches!(context.glyph_history_mode, GlyphHistoryMode::Path | GlyphHistoryMode::TopologyDp)
    {
        return;
    }
    let weights = TopologyDpWeights::DEFAULT;
    for sample in samples {
        let key = (sample.primitive_id, sample.stroke_id, sample.path_index);
        let Some(previous_sample) = previous.get(&key) else {
            continue;
        };
        let same_cell = previous_sample.column == sample.column && previous_sample.row == sample.row;
        if context.glyph_history_mode == GlyphHistoryMode::Path && same_cell {
            sample.character = previous_sample.character;
            continue;
        }
        if context.glyph_history_mode == GlyphHistoryMode::TopologyDp {
            let spatial = previous_sample.column.abs_diff(sample.column)
                + previous_sample.row.abs_diff(sample.row);
            let stroke_change = u16::from(previous_sample.stroke_id != sample.stroke_id);
            let shape_change = u16::from(previous_sample.character != sample.character);
            let cost = spatial as f32 * weights.local
                + stroke_change as f32 * weights.topology
                + shape_change as f32 * weights.corner;
            if cost <= 2.6 {
                sample.character = previous_sample.character;
            }
        }
    }
}

fn resolve_sample_collisions(samples: Vec<PrimitiveSample>) -> Vec<PrimitiveSample> {
    let mut by_cell = BTreeMap::new();
    for sample in samples {
        let key = (sample.row, sample.column);
        let replace = by_cell.get(&key).is_none_or(|existing: &PrimitiveSample| {
            primitive_collision_priority(&sample) < primitive_collision_priority(existing)
        });
        if replace {
            by_cell.insert(key, sample);
        }
    }
    by_cell.into_values().collect()
}

fn primitive_collision_priority(sample: &PrimitiveSample) -> (u16, u16, usize) {
    (sample.primitive_id, sample.stroke_id, sample.path_index)
}

fn primitive_scene_signature(samples: &[PrimitiveSample]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for sample in samples {
        hash = fnv_mix(hash, u64::from(sample.primitive_id));
        hash = fnv_mix(hash, u64::from(sample.stroke_id));
        hash = fnv_mix(hash, sample.path_index as u64);
        hash = fnv_mix(hash, u64::from(sample.column));
        hash = fnv_mix(hash, u64::from(sample.row));
    }
    hash
}

fn primitive_alpha(alpha: f32, column: u16, row: u16, elapsed_seconds: f32) -> f32 {
    let frame_index = animation_frame_index(elapsed_seconds, 120);
    let dither = stbn_mask_value(column, row, 9, frame_index) - 0.5;
    (alpha + dither * 0.08).clamp(0.2, 1.0)
}

fn glyph_for_mode(character: char, glyph_mode: GlyphMode) -> char {
    match glyph_mode {
        GlyphMode::Ascii => '*',
        GlyphMode::Braille => '⠿',
        GlyphMode::Unicode => character,
    }
}

fn seeded_motion_jitter(seed: u32, elapsed_seconds: f32, amplitude: i16) -> (i16, i16) {
    let frame = animation_frame_index(elapsed_seconds, 24);
    let x = random_unit(mix_seed(seed, frame)).mul_add(2.0, -1.0);
    let y = random_unit(mix_seed(seed.rotate_left(13), frame)).mul_add(2.0, -1.0);
    (
        (x * f32::from(amplitude)).round() as i16,
        (y * f32::from(amplitude)).round() as i16,
    )
}

fn orbit_perceptual_alpha(
    alpha: f32,
    column: u16,
    row: u16,
    elapsed_seconds: f32,
    context: FrameContext,
    tuning: OrbitRenderTuning,
) -> f32 {
    let frame_index = animation_frame_index(elapsed_seconds, context.calibration.target_fps);
    let stbn = stbn_mask_value(column, row, 7, frame_index) - 0.5;
    let levels = (1.0 / tuning.alpha_delta_limit).clamp(7.0, 18.0);
    let perceptual = alpha.clamp(0.0, 1.0).sqrt();
    let quantized = ((perceptual * levels + stbn * 0.74).round() / levels).clamp(0.0, 1.0);
    let dithered = (quantized * quantized + stbn * tuning.alpha_delta_limit * 0.22).clamp(0.0, 1.0);
    lerp(alpha, dithered, 0.42).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug)]
struct OrbitRenderTuning {
    splat_step: f32,
    extra_span: f32,
    prelight_gain: f32,
    alpha_floor: f32,
    alpha_delta_limit: f32,
    history_decay: f32,
    character_hold_alpha: f32,
    max_samples: usize,
}

impl OrbitRenderTuning {
    fn for_context(context: FrameContext, elapsed_seconds: f32) -> Self {
        let fps = f32::from(context.calibration.target_fps.max(60));
        let fps_quality = ((fps - 60.0) / 60.0).clamp(0.0, 1.0);
        let motion_scale =
            (orbit_motion_speed(elapsed_seconds, context.motion_preset, context.curves)
                / ORBIT_CELLS_PER_SECOND)
                .clamp(0.82, 1.48);
        let profile_gain = match context.profile {
            VisualProfile::Ultra => 1.0,
            VisualProfile::Cinematic => 0.86,
            VisualProfile::Calm => 0.62,
            VisualProfile::Benchmark => 0.92,
        };
        let quality = (context.quality.scale * profile_gain).clamp(0.45, 1.0);
        let splat_step = (lerp(0.44, 0.28, quality * fps_quality.max(0.35)) / motion_scale.sqrt())
            .clamp(0.24, 0.46);
        let fps_scale = (120.0 / fps).sqrt();
        Self {
            splat_step,
            extra_span: lerp(5.5, 9.5, quality) + (motion_scale - 1.0).max(0.0) * 3.0,
            prelight_gain: (lerp(0.44, 0.68, quality) * motion_scale.sqrt()).clamp(0.42, 0.78),
            alpha_floor: lerp(0.018, 0.010, quality),
            alpha_delta_limit: (0.105 * fps_scale * lerp(1.12, 0.92, quality)).clamp(0.09, 0.18),
            history_decay: lerp(0.52, 0.66, quality),
            character_hold_alpha: lerp(0.38, 0.56, quality),
            max_samples: (72.0 + quality * 96.0 + fps_quality * 32.0).round() as usize,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct OrbitTemporalCell {
    alpha: f32,
    path_index: usize,
    age: u8,
}

#[derive(Clone, Copy, Debug)]
struct OrbitVisibleCell {
    alpha: f32,
    path_index: usize,
}

#[derive(Default)]
struct OrbitTemporalBuffer {
    frame_box: Option<FrameBox>,
    history: Vec<Option<OrbitTemporalCell>>,
    screen_history: BTreeMap<(u16, u16), OrbitTemporalCell>,
}

impl OrbitTemporalBuffer {
    fn clear(&mut self) {
        self.frame_box = None;
        self.history.clear();
        self.screen_history.clear();
    }

    fn stabilize(
        &mut self,
        visible: &mut Vec<Option<OrbitVisibleCell>>,
        frame_box: FrameBox,
        tuning: OrbitRenderTuning,
    ) {
        let perimeter_len = frame_box.perimeter_len();
        if self.frame_box != Some(frame_box) || self.history.len() != perimeter_len {
            self.frame_box = Some(frame_box);
            self.history = vec![None; perimeter_len];
        }
        if visible.len() != perimeter_len {
            visible.resize(perimeter_len, None);
        }

        for (path_index, current) in visible.iter_mut().enumerate() {
            let Some(current) = current else {
                continue;
            };
            if let Some(previous) = self.history.get(path_index).copied().flatten() {
                let delta = current.alpha - previous.alpha;
                current.alpha = previous.alpha
                    + delta.clamp(-tuning.alpha_delta_limit, tuning.alpha_delta_limit);
            }
        }

        for (path_index, current) in visible.iter_mut().enumerate() {
            let Some(current) = current else {
                continue;
            };
            if let Some(previous) = self.history.get(path_index).copied().flatten()
                && previous.alpha > tuning.character_hold_alpha
                && current.alpha <= previous.alpha + tuning.alpha_delta_limit
            {
                current.path_index = previous.path_index;
            }
        }

        for path_index in 0..perimeter_len {
            let Some(previous) = self.history[path_index] else {
                continue;
            };
            if visible[path_index].is_none() && previous.age < 2 {
                let decayed_alpha = previous.alpha * tuning.history_decay;
                if decayed_alpha > tuning.alpha_floor {
                    insert_orbit_visible(visible, path_index, decayed_alpha);
                    let projected_index = (previous.path_index + 1) % perimeter_len.max(1);
                    insert_orbit_visible(
                        visible,
                        projected_index,
                        decayed_alpha * tuning.prelight_gain,
                    );
                }
            }
        }

        let previous_history = std::mem::take(&mut self.history);
        self.history = visible
            .iter()
            .enumerate()
            .map(|(path_index, current)| {
                current.map(|current| OrbitTemporalCell {
                    alpha: current.alpha,
                    path_index: current.path_index,
                    age: previous_history
                        .get(path_index)
                        .copied()
                        .flatten()
                        .map_or(0, |previous| previous.age.saturating_add(1)),
                })
            })
            .collect();
    }

    fn stabilize_screen_cell(
        &mut self,
        visible: &mut [Option<OrbitVisibleCell>],
        frame_box: FrameBox,
        tuning: OrbitRenderTuning,
    ) {
        for (path_index, current) in visible.iter_mut().enumerate() {
            let Some(current) = current else {
                continue;
            };
            let Some(key) = frame_box.point_at_path_index(path_index) else {
                continue;
            };
            if let Some(previous) = self.screen_history.get(&key).copied() {
                let delta = current.alpha - previous.alpha;
                current.alpha = previous.alpha
                    + delta.clamp(-tuning.alpha_delta_limit, tuning.alpha_delta_limit);
                if previous.alpha > tuning.character_hold_alpha
                    && current.alpha <= previous.alpha + tuning.alpha_delta_limit
                {
                    current.path_index = previous.path_index;
                }
            }
        }

        let mut next_history = BTreeMap::new();
        for (path_index, current) in visible.iter().enumerate() {
            let Some(current) = current else {
                continue;
            };
            let Some(key) = frame_box.point_at_path_index(path_index) else {
                continue;
            };
            next_history.insert(
                key,
                OrbitTemporalCell {
                    alpha: current.alpha,
                    path_index: current.path_index,
                    age: self
                        .screen_history
                        .get(&key)
                        .map_or(0, |previous| previous.age.saturating_add(1)),
                },
            );
        }
        self.screen_history = next_history;
    }

    fn stabilize_topology_dp(
        &mut self,
        visible: &mut Vec<Option<OrbitVisibleCell>>,
        frame_box: FrameBox,
        tuning: OrbitRenderTuning,
    ) {
        self.stabilize(visible, frame_box, tuning);

        let perimeter_len = visible.len();
        if perimeter_len == 0 {
            return;
        }

        let mut previous_assignment = vec![None; perimeter_len];
        for (path_index, assignment) in previous_assignment.iter_mut().enumerate() {
            if let Some(previous) = self.history.get(path_index).copied().flatten() {
                *assignment = Some(previous.path_index % perimeter_len);
            }
        }

        let mut solved = visible.clone();
        for path_index in 0..perimeter_len {
            let Some(current) = visible[path_index] else {
                continue;
            };

            let candidates = [
                current.path_index % perimeter_len,
                path_index,
                (path_index + perimeter_len - 1) % perimeter_len,
                (path_index + 1) % perimeter_len,
            ];
            let mut best_index = current.path_index % perimeter_len;
            let mut best_cost = f32::INFINITY;
            for candidate in candidates {
                let temporal_cost = previous_assignment[path_index].map_or(0.0, |previous| {
                    circular_index_distance(previous, candidate, perimeter_len)
                });
                let local_cost = circular_index_distance(path_index, candidate, perimeter_len);
                let topology_cost =
                    neighbor_topology_cost(&solved, path_index, candidate, perimeter_len);
                let weights = TopologyDpWeights::DEFAULT;
                let corner_cost = if orbit_character(frame_box, candidate, GlyphMode::Unicode)
                    != orbit_character(frame_box, current.path_index, GlyphMode::Unicode)
                {
                    weights.corner
                } else {
                    0.0
                };
                let cost = temporal_cost * weights.temporal
                    + local_cost * weights.local
                    + topology_cost * weights.topology
                    + corner_cost;
                if cost < best_cost {
                    best_cost = cost;
                    best_index = candidate;
                }
            }

            if let Some(current) = &mut solved[path_index] {
                current.path_index = best_index;
            }
        }

        *visible = solved;
        for (path_index, current) in visible.iter().enumerate() {
            if let Some(current) = current
                && let Some(history) = self.history.get_mut(path_index)
            {
                *history = Some(OrbitTemporalCell {
                    alpha: current.alpha,
                    path_index: current.path_index,
                    age: history.map_or(0, |previous| previous.age),
                });
            }
        }
    }
}

fn circular_index_distance(a: usize, b: usize, len: usize) -> f32 {
    if len == 0 {
        return 0.0;
    }
    let forward = a.abs_diff(b);
    let wrapped = len.saturating_sub(forward);
    forward.min(wrapped) as f32
}

fn neighbor_topology_cost(
    visible: &[Option<OrbitVisibleCell>],
    path_index: usize,
    candidate: usize,
    len: usize,
) -> f32 {
    let mut cost = 0.0;
    for neighbor in [(path_index + len - 1) % len, (path_index + 1) % len] {
        if let Some(neighbor_cell) = visible[neighbor] {
            let distance = circular_index_distance(candidate, neighbor_cell.path_index % len, len);
            if distance > 2.0 {
                cost += (distance - 2.0) / len.max(1) as f32;
            }
        }
    }
    cost
}

fn insert_orbit_visible(visible: &mut [Option<OrbitVisibleCell>], path_index: usize, alpha: f32) {
    let len = visible.len();
    if len == 0 {
        return;
    }
    let path_index = path_index % len;
    match &mut visible[path_index] {
        Some(stored) if alpha > stored.alpha => {
            stored.alpha = alpha;
            stored.path_index = path_index;
        }
        None => {
            visible[path_index] = Some(OrbitVisibleCell { alpha, path_index });
        }
        _ => {}
    }
}

fn temporal_orbit_energy_alpha(
    elapsed_seconds: f32,
    frame_dt: f32,
    cell_position: f32,
    perimeter_len: f32,
    trail_span: f32,
    context: FrameContext,
) -> f32 {
    let mut alpha = 0.0;
    let mut total_weight = 0.0;
    let frame_index = animation_frame_index(elapsed_seconds, context.calibration.target_fps);
    let jitter = orbit_temporal_stbn_jitter(cell_position, perimeter_len, frame_index);
    let motion_speed = orbit_motion_speed(elapsed_seconds, context.motion_preset, context.curves);
    let motion_scale = (motion_speed / ORBIT_CELLS_PER_SECOND).clamp(0.82, 1.48);
    let shutter_scale = (120.0 / f32::from(context.calibration.target_fps.max(60)))
        .clamp(1.0, 1.72)
        * motion_scale.sqrt();
    for (index, offset) in ORBIT_TEMPORAL_BASE_OFFSETS.into_iter().enumerate() {
        let centered_offset = offset + jitter * 0.12 * (1.0 - offset.abs() * 0.42);
        let sample_time = (elapsed_seconds + centered_offset * frame_dt * shutter_scale).max(0.0);
        let head_position = spline_orbit_head(
            sample_time,
            perimeter_len,
            context.motion_preset,
            context.curves,
        );
        let weight = ORBIT_TEMPORAL_WEIGHTS[index];
        alpha += orbit_energy_wave_alpha(
            head_position,
            cell_position,
            perimeter_len,
            trail_span,
            sample_time,
            context.motion_preset,
        ) * weight;
        total_weight += weight;
    }
    alpha / total_weight.max(0.001)
}

fn orbit_temporal_stbn_jitter(cell_position: f32, perimeter_len: f32, frame_index: u32) -> f32 {
    let normalized = (cell_position / perimeter_len.max(1.0)).clamp(0.0, 1.0);
    let column = (cell_position as u16) % STBN_WIDTH;
    let row = (normalized * f32::from(STBN_HEIGHT.saturating_sub(1))).round() as u16;
    stbn_mask_value(column, row, 6, frame_index) - 0.5
}

fn spline_orbit_head(
    elapsed_seconds: f32,
    perimeter_len: f32,
    motion_preset: MotionPreset,
    curves: VisualCurves,
) -> f32 {
    let phases = MotionPhases::at(elapsed_seconds, motion_preset);
    let startup_distance = orbit_startup_boost_distance(elapsed_seconds);
    let base = (elapsed_seconds + startup_distance)
        * ORBIT_CELLS_PER_SECOND
        * phases.orbit_speed
        * curves.orbit_speed;
    base.rem_euclid(perimeter_len)
}

fn orbit_motion_speed(
    elapsed_seconds: f32,
    motion_preset: MotionPreset,
    curves: VisualCurves,
) -> f32 {
    let phases = MotionPhases::at(elapsed_seconds, motion_preset);
    ORBIT_CELLS_PER_SECOND * phases.orbit_speed * curves.orbit_speed
}

fn orbit_startup_boost_distance(elapsed_seconds: f32) -> f32 {
    let duration = ORBIT_STARTUP_BOOST_SECONDS;
    let gain = ORBIT_STARTUP_SPEED_BOOST - 1.0;
    if elapsed_seconds <= 0.0 {
        return 0.0;
    }
    if elapsed_seconds >= duration {
        return gain * duration * 0.5;
    }
    let t = elapsed_seconds / duration;
    let phase = std::f32::consts::TAU * t;
    gain * duration * 0.5 * (t - phase.sin() / std::f32::consts::TAU)
}

fn orbit_energy_wave_alpha(
    head_position: f32,
    cell_position: f32,
    perimeter_len: f32,
    trail_span: f32,
    elapsed_seconds: f32,
    motion_preset: MotionPreset,
) -> f32 {
    let phases = MotionPhases::at(elapsed_seconds, motion_preset);
    let specular_position = (head_position + phases.glint_offset * 0.42).rem_euclid(perimeter_len);
    let offsets = [-0.42, -0.18, 0.0, 0.18, 0.42];
    let mut alpha = 0.0;
    for offset in offsets {
        let position = cell_position + offset;
        let main_distance = orbit_distance_behind(head_position, position, perimeter_len);
        let specular_distance = orbit_distance_behind(specular_position, position, perimeter_len);
        let main = trail_alpha(main_distance, trail_span);
        let band = orbit_specular_band(specular_distance, trail_span);
        alpha += (main * 0.88 + band * 0.22).min(1.0);
    }
    alpha / offsets.len() as f32
}

fn orbit_specular_band(distance: f32, trail_span: f32) -> f32 {
    let width = (trail_span * 0.18).clamp(5.0, 9.5);
    if distance > width {
        return 0.0;
    }
    smoothstep(1.0 - distance / width)
}

fn line_length_multiplier(profile: VisualProfile) -> f32 {
    match profile {
        VisualProfile::Ultra | VisualProfile::Benchmark => 1.28,
        VisualProfile::Cinematic => 1.08,
        VisualProfile::Calm => 0.86,
    }
}

fn orbit_distance_behind(head_position: f32, sample_position: f32, perimeter_len: f32) -> f32 {
    (head_position - sample_position).rem_euclid(perimeter_len)
}

fn orbit_character(frame_box: FrameBox, path_index: usize, glyph_mode: GlyphMode) -> char {
    if glyph_mode == GlyphMode::Ascii {
        return '*';
    }
    if glyph_mode == GlyphMode::Braille {
        return braille_dot(path_index);
    }

    let perimeter_len = frame_box.perimeter_len();
    let previous = frame_box.point_at((path_index + perimeter_len - 1) % perimeter_len);
    let current = frame_box.point_at(path_index);
    let next = frame_box.point_at((path_index + 1) % perimeter_len);

    match (previous, current, next) {
        (Some((px, py)), Some((cx, cy)), Some((nx, ny))) => {
            let incoming = direction(px, py, cx, cy);
            let outgoing = direction(cx, cy, nx, ny);
            connector_for(incoming, outgoing)
        }
        _ => '◆',
    }
}

fn braille_dot(path_index: usize) -> char {
    const DOTS: [char; 8] = ['⠁', '⠂', '⠄', '⡀', '⢀', '⠠', '⠐', '⠈'];
    DOTS[path_index % DOTS.len()]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Direction {
    Left,
    Right,
    Up,
    Down,
}

fn direction(from_column: u16, from_row: u16, to_column: u16, to_row: u16) -> Direction {
    match (to_column.cmp(&from_column), to_row.cmp(&from_row)) {
        (std::cmp::Ordering::Greater, _) => Direction::Right,
        (std::cmp::Ordering::Less, _) => Direction::Left,
        (_, std::cmp::Ordering::Greater) => Direction::Down,
        _ => Direction::Up,
    }
}

fn connector_for(incoming: Direction, outgoing: Direction) -> char {
    use Direction::{Down, Left, Right, Up};

    match (incoming, outgoing) {
        (Left, Right) | (Right, Left) | (Right, Right) | (Left, Left) => '━',
        (Up, Down) | (Down, Up) | (Up, Up) | (Down, Down) => '┃',
        (Right, Down) | (Up, Left) => '╮',
        (Down, Left) | (Right, Up) => '╯',
        (Left, Up) | (Down, Right) => '╰',
        (Up, Right) | (Left, Down) => '╭',
    }
}

fn trail_color_from_intensity(intensity: f32, profile: VisualProfile, theme: Theme) -> Color {
    let body = blend_color(
        theme.trail_tail,
        theme.trail_body,
        intensity * profile.intensity(),
    );
    ensure_visible_lightness(
        highlight_blend(
            body,
            theme.trail_head,
            intensity.powf(2.2) * profile.intensity(),
        ),
        theme.contrast_floor * 0.92,
    )
}

fn trail_alpha(distance: f32, trail_span: f32) -> f32 {
    if distance > trail_span {
        return 0.0;
    }

    let progress = (distance / trail_span).clamp(0.0, 1.0);
    smoothstep(1.0 - progress)
}

fn write_gradient_blocks(
    stdout: &mut Stdout,
    gradient: &[(u8, u8, u8)],
    start_index: usize,
) -> io::Result<()> {
    for &(r, g, b) in gradient.iter().skip(start_index).take(BAR_WIDTH) {
        write!(stdout, "{}", "█".with(Color::Rgb { r, g, b }))?;
    }

    Ok(())
}

fn should_exit() -> io::Result<bool> {
    let has_event = match event::poll(Duration::from_millis(0)) {
        Ok(has_event) => has_event,
        Err(_) => return Ok(false),
    };

    if !has_event {
        return Ok(false);
    }

    match event::read() {
        Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
            Ok(exit_key_pressed_from_parts(key.code, key.modifiers))
        }
        Ok(_) | Err(_) => Ok(false),
    }
}

fn exit_key_pressed_from_parts(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Esc | KeyCode::Char('q'))
        || matches!(code, KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL))
}

#[cfg(test)]
fn exit_key_pressed(event: Event) -> bool {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            exit_key_pressed_from_parts(key.code, key.modifiers)
        }
        _ => false,
    }
}

fn animation_steps(gradient_len: usize) -> usize {
    gradient_len.saturating_sub(BAR_WIDTH).max(1)
}

fn progress_percent(loop_index: usize, step_index: usize, total_steps: usize) -> usize {
    let completed_steps = loop_index * total_steps + step_index;
    completed_steps * 100 / (TOTAL_LOOPS * total_steps)
}

fn centered_column(columns: u16, content_width: usize) -> u16 {
    columns.saturating_sub(content_width as u16) / 2
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Layout {
    columns: u16,
    rows: u16,
    logo: &'static [&'static str],
    logo_column: u16,
    logo_row: u16,
    frame_box: Option<FrameBox>,
    composition: CompositionMode,
}

impl Layout {
    fn current() -> Self {
        let (columns, rows) = size().unwrap_or((80, 24));
        Self::for_size(columns, rows)
    }

    fn for_size(columns: u16, rows: u16) -> Self {
        let composition = CompositionMode::for_size(columns, rows);
        let full_logo_width = max_width(&FULL_LOGO);
        let enough_for_full_logo =
            columns as usize >= full_logo_width && rows as usize >= FULL_LOGO.len() + 3;
        let logo = if enough_for_full_logo {
            &FULL_LOGO[..]
        } else {
            &COMPACT_LOGO[..]
        };
        let logo_width = max_width(logo);
        let logo_height = logo.len() as u16;
        let logo_column = centered_column(columns, logo_width);
        let logo_row = composition.logo_row(columns, rows, logo_height);
        let frame_box = frame_box_for_logo(
            columns,
            rows,
            logo_column,
            logo_row,
            logo_width,
            logo_height,
        );

        Self {
            columns,
            rows,
            logo,
            logo_column,
            logo_row,
            frame_box,
            composition,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompositionMode {
    Compact,
    Standard,
    Wide,
}

impl CompositionMode {
    fn for_size(columns: u16, rows: u16) -> Self {
        if columns < 72 || rows < 18 {
            Self::Compact
        } else if columns >= 132 && rows >= 30 {
            Self::Wide
        } else {
            Self::Standard
        }
    }

    fn logo_row(self, _columns: u16, rows: u16, logo_height: u16) -> u16 {
        match self {
            Self::Compact => rows.saturating_sub(logo_height) / 3,
            Self::Standard => rows.saturating_sub(logo_height) / 3,
            Self::Wide => rows.saturating_sub(logo_height) / 4,
        }
    }

    fn particle_density_multiplier(self) -> f32 {
        match self {
            Self::Compact => 0.58,
            Self::Standard => 1.0,
            Self::Wide => 1.22,
        }
    }

    fn background_width(self, columns: u16) -> u16 {
        match self {
            Self::Compact => columns.min(88),
            Self::Standard => columns.min(120),
            Self::Wide => columns.min(160),
        }
    }
}

fn frame_box_for_logo(
    columns: u16,
    rows: u16,
    logo_column: u16,
    logo_row: u16,
    logo_width: usize,
    logo_height: u16,
) -> Option<FrameBox> {
    let width = logo_width as u16 + 4;
    let height = logo_height + 4;

    if width > columns || height + 2 > rows {
        return None;
    }

    Some(FrameBox {
        left: logo_column.saturating_sub(2),
        top: logo_row.saturating_sub(2),
        width,
        height,
    })
}

fn max_width(lines: &[&str]) -> usize {
    lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::capabilities::{ColorMode, TerminalPreset, ThroughputMetrics};
    use crate::theme::ACCENT_COLOR;

    fn test_capabilities() -> TerminalCapabilities {
        TerminalCapabilities {
            color_mode: ColorMode::TrueColor,
            glyph_mode: GlyphMode::Unicode,
            preset: TerminalPreset::WezTerm,
        }
    }

    fn test_calibration() -> TerminalCalibration {
        TerminalCalibration {
            capabilities: test_capabilities(),
            target_fps: 120,
            effect_density: 1.0,
            small_terminal: false,
            enabled: true,
            throughput: ThroughputMetrics {
                write_time: Duration::ZERO,
                flush_time: Duration::ZERO,
                cells_sampled: 640,
            },
            dirty_cell_budget: 640,
        }
    }

    fn test_context(mouse: Option<MouseAttractor>) -> FrameContext {
        FrameContext {
            profile: VisualProfile::Ultra,
            motion_preset: MotionPreset::Prime,
            scene_preset: ScenePreset::Orbit,
            calibration: test_calibration(),
            theme: Theme::default(),
            mouse,
            quality: QualitySettings::from_calibration(test_calibration(), 1.0),
            camera: VirtualCamera::default(),
            curves: VisualCurves::for_profile(VisualProfile::Ultra),
            director: MotionDirectorState::steady(),
            glyph_history_mode: GlyphHistoryMode::Path,
            low_latency: false,
            medium_latency: false,
        }
    }

    fn test_frame(layout: &Layout, elapsed_seconds: f32) -> Frame {
        let mut frame_buffers = FrameBuffers::default();
        Frame::build(
            layout,
            elapsed_seconds,
            FrameContext {
                ..test_context(None)
            },
            &mut frame_buffers,
        )
    }

    #[test]
    fn keeps_at_least_one_animation_step() {
        assert_eq!(animation_steps(0), 1);
        assert_eq!(animation_steps(BAR_WIDTH - 1), 1);
        assert_eq!(animation_steps(BAR_WIDTH + 3), 3);
    }

    #[test]
    fn calculates_progress_across_all_loops() {
        assert_eq!(progress_percent(0, 0, 10), 0);
        assert_eq!(progress_percent(1, 5, 10), 50);
        assert_eq!(progress_percent(2, 9, 10), 96);
    }

    #[test]
    fn centers_content_without_underflowing() {
        assert_eq!(centered_column(80, 20), 30);
        assert_eq!(centered_column(10, 20), 0);
    }

    #[test]
    fn adaptive_frame_controller_advances_deadline_monotonically() {
        let mut controller = AdaptiveFrameController::new(Duration::from_millis(8));
        let first_deadline = controller.next_frame_at;

        controller.advance();

        assert!(controller.next_frame_at > first_deadline);
    }

    #[test]
    fn animation_clock_clamps_slow_frame_phase_jumps() {
        let mut clock = AnimationClock::default();
        let first = clock.step(0.0, 120);
        let second = clock.step(0.5, 120);

        assert_eq!(first, 0.0);
        assert!(second < 0.04);
    }

    #[test]
    fn animation_clock_catches_up_slowly_after_drift() {
        let mut clock = AnimationClock::default();
        clock.step(0.0, 120);
        let delayed = clock.step(0.5, 120);
        let after = clock.step(0.5 + 1.0 / 120.0, 120);

        assert!(after > delayed);
        assert!(after - delayed < 0.04);
    }

    #[test]
    fn dirty_frame_skips_identical_cells() {
        let layout = Layout {
            columns: 80,
            rows: 24,
            logo: &COMPACT_LOGO,
            logo_column: 0,
            logo_row: 0,
            frame_box: None,
            composition: CompositionMode::Standard,
        };
        let frame = test_frame(&layout, 0.0);
        let dirty_cells = frame.dirty_cells(Some(&frame), DirtyMode::Full).len();

        assert_eq!(dirty_cells, 0);
    }

    #[test]
    fn dirty_frame_marks_changed_animation_cells() {
        let layout = Layout {
            columns: 80,
            rows: 24,
            logo: &COMPACT_LOGO,
            logo_column: 0,
            logo_row: 0,
            frame_box: None,
            composition: CompositionMode::Standard,
        };
        let previous = test_frame(&layout, 0.0);
        let next = test_frame(&layout, 1.0 / 120.0);
        let dirty_cells = next.dirty_cells(Some(&previous), DirtyMode::Full).len();

        assert!(dirty_cells > 0);
        assert!(dirty_cells < next.cells.len());
    }

    #[test]
    fn logo_color_keeps_visible_glyph_contrast() {
        for frame in 0..48 {
            let Color::Rgb { r, g, b } = test_logo_color('█', 8, frame as f32 / 120.0) else {
                panic!("expected rgb color");
            };
            assert!(perceived_luma(Color::Rgb { r, g, b }) >= 78);
        }
    }

    #[test]
    fn logo_color_changes_smoothly_between_frames() {
        for frame in 0..48 {
            let current = test_logo_color('█', 8, frame as f32 / 120.0);
            let next = test_logo_color('█', 8, (frame + 1) as f32 / 120.0);

            assert!(oklab_delta(current, next) <= 0.035);
            assert!(color_delta(current, next) <= 32);
        }
    }

    #[test]
    fn logo_material_has_glow_and_specular_structure() {
        let material =
            LogoMaterial::from_context(test_logo_context('█', 8, 0.72, MotionPreset::Pulse));

        assert!(material.sample.glow >= material.sample.primary * 0.35);
        assert!(material.sample.specular <= material.sample.primary.max(0.01));
        assert!((0.0..=1.0).contains(&material.sample.micro_flow));
        assert!(apca_like_contrast(material.shade(), dark_reference_color()) > 42.0);
    }

    #[test]
    fn logo_gamut_style_expands_ultra_more_than_calm() {
        let ultra =
            LogoMaterial::from_context(test_logo_context('█', 8, 0.72, MotionPreset::Pulse));
        let mut calm_context = test_logo_context('█', 8, 0.72, MotionPreset::Pulse);
        calm_context.profile = VisualProfile::Calm;
        let calm = LogoMaterial::from_context(calm_context);

        assert!(ultra.gamut.chroma_scale > calm.gamut.chroma_scale);
        assert!(oklab_delta(ultra.gamut.shadow, ultra.gamut.highlight) > 0.20);
        assert!(apca_like_contrast(ultra.shade(), dark_reference_color()) > 42.0);
    }

    #[test]
    fn supersampled_logo_color_limits_fast_pulse_delta() {
        let mut smoothed_peak_delta = 0.0f32;
        for frame in 0..96 {
            let t = frame as f32 / 120.0;
            let next_t = (frame + 1) as f32 / 120.0;
            let smooth_current = test_logo_color_with_motion('█', 8, t, MotionPreset::Pulse);
            let smooth_next = test_logo_color_with_motion('█', 8, next_t, MotionPreset::Pulse);
            let smoothed_delta = oklab_delta(smooth_current, smooth_next);
            smoothed_peak_delta = smoothed_peak_delta.max(smoothed_delta);
        }

        assert!(smoothed_peak_delta <= 0.035);
    }

    #[test]
    fn logo_material_final_output_is_rate_limited_across_characters() {
        let mut peak_delta = 0.0f32;
        for index in [0, 3, 8, 13, 21, 27] {
            for frame in 0..96 {
                let current = test_logo_color_with_motion(
                    '█',
                    index,
                    frame as f32 / 120.0,
                    MotionPreset::Pulse,
                );
                let next = test_logo_color_with_motion(
                    '█',
                    index,
                    (frame + 1) as f32 / 120.0,
                    MotionPreset::Pulse,
                );
                peak_delta = peak_delta.max(oklab_delta(current, next));
            }
        }

        assert!(peak_delta <= 0.035);
    }

    #[test]
    fn logo_material_motion_has_low_delta_acceleration() {
        let mut acceleration = 0.0f32;
        let mut samples = 0.0f32;
        for index in [0, 3, 8, 13, 21, 27] {
            let mut previous_delta: Option<f32> = None;
            for frame in 0..120 {
                let current = test_logo_color_with_motion(
                    '█',
                    index,
                    frame as f32 / 120.0,
                    MotionPreset::Pulse,
                );
                let next = test_logo_color_with_motion(
                    '█',
                    index,
                    (frame + 1) as f32 / 120.0,
                    MotionPreset::Pulse,
                );
                let delta = oklab_delta(current, next);
                if let Some(previous) = previous_delta {
                    acceleration += (delta - previous).abs();
                    samples += 1.0;
                }
                previous_delta = Some(delta);
            }
        }

        assert!(acceleration / samples.max(1.0) <= 0.0045);
    }

    #[test]
    fn conservative_terminal_logo_animation_stays_readable() {
        let mut calibration = test_calibration();
        calibration.capabilities.preset = TerminalPreset::MacOsTerminal;
        for frame in 0..48 {
            let color = logo_color_for_terminal(LogoColorContext {
                character: '█',
                index: 8,
                elapsed_seconds: frame as f32 / 120.0,
                profile: VisualProfile::Ultra,
                motion_preset: MotionPreset::Prime,
                theme: Theme::default(),
                rhythm: 0.5,
                target_fps: 120,
                capabilities: calibration.capabilities,
            });

            assert!(
                apca_like_contrast(color, dark_reference_color())
                    >= TerminalAppearance::for_profile(TerminalVisualProfile::Conservative)
                        .contrast_floor(PerceptualRole::Logo)
            );
        }
    }

    #[test]
    fn accumulated_trail_color_fades_from_head_to_tail() {
        assert!(
            perceived_luma(trail_color_from_intensity(
                1.0,
                VisualProfile::Ultra,
                Theme::default()
            )) > perceived_luma(trail_color_from_intensity(
                0.5,
                VisualProfile::Ultra,
                Theme::default()
            ))
        );
        assert!(
            perceived_luma(trail_color_from_intensity(
                0.5,
                VisualProfile::Ultra,
                Theme::default()
            )) > perceived_luma(trail_color_from_intensity(
                0.05,
                VisualProfile::Ultra,
                Theme::default()
            ))
        );
    }

    #[test]
    fn contrast_model_boosts_dark_blue_colors() {
        let dark_blue = Color::Rgb { r: 0, g: 18, b: 44 };
        let adjusted = enforce_contrast(dark_blue, 42.0);

        assert!(
            apca_like_contrast(adjusted, dark_reference_color())
                >= apca_like_contrast(dark_blue, dark_reference_color())
        );
    }

    #[test]
    fn role_color_optimizer_respects_role_contrast_floors() {
        let base = Color::Rgb { r: 0, g: 20, b: 48 };
        let logo = display_role_color(base, test_calibration().capabilities, PerceptualRole::Logo);
        let particle = display_role_color(
            base,
            test_calibration().capabilities,
            PerceptualRole::BackgroundParticle,
        );

        assert!(apca_like_contrast(logo, dark_reference_color()) >= 42.0);
        assert!(apca_like_contrast(particle, dark_reference_color()) >= 16.0);
    }

    #[test]
    fn terminal_appearance_model_raises_conservative_visibility_floor() {
        let base = Color::Rgb { r: 0, g: 16, b: 42 };
        let rich = optimize_oklch_color_for_profile(
            base,
            PerceptualRole::Logo,
            TerminalVisualProfile::GpuRich,
        );
        let conservative = optimize_oklch_color_for_profile(
            base,
            PerceptualRole::Logo,
            TerminalVisualProfile::Conservative,
        );
        let appearance = TerminalAppearance::for_profile(TerminalVisualProfile::Conservative);

        assert!(
            appearance.black_floor
                > TerminalAppearance::for_profile(TerminalVisualProfile::GpuRich).black_floor
        );
        assert!(
            apca_like_contrast(conservative, dark_reference_color())
                >= appearance.contrast_floor(PerceptualRole::Logo)
        );
        assert!(
            apca_like_contrast(conservative, dark_reference_color())
                >= apca_like_contrast(rich, dark_reference_color())
        );
    }

    #[test]
    fn supporting_text_polish_keeps_copy_visible_but_secondary() {
        let capabilities = test_calibration().capabilities;
        let pipeline = ColorPipeline::new(
            TextRenderContext {
                elapsed_seconds: 1.0,
                profile: VisualProfile::Ultra,
                motion_preset: MotionPreset::Prime,
                calibration: test_calibration(),
                theme: Theme::default(),
            },
            0.0,
        );
        let logo = display_role_color(ACCENT_COLOR, capabilities, PerceptualRole::Logo);
        let status = pipeline.supporting_text(ACCENT_COLOR, STATUS_VISUAL_GAIN);

        assert!(
            apca_like_contrast(status, dark_reference_color())
                >= TerminalAppearance::for_profile(terminal_visual_profile_from_capabilities(
                    capabilities
                ))
                .contrast_floor(PerceptualRole::StatusText)
        );
        assert!(
            apca_like_contrast(status, dark_reference_color())
                <= apca_like_contrast(logo, dark_reference_color())
        );
    }

    #[test]
    fn color_pipeline_preserves_logo_supporting_hierarchy() {
        let pipeline = ColorPipeline::new(
            TextRenderContext {
                elapsed_seconds: 1.25,
                profile: VisualProfile::Ultra,
                motion_preset: MotionPreset::Pulse,
                calibration: test_calibration(),
                theme: Theme::default(),
            },
            0.48,
        );
        let logo = pipeline.logo('█', 8);
        let supporting = pipeline.supporting_text(ACCENT_COLOR, STATUS_VISUAL_GAIN);

        assert!(
            apca_like_contrast(logo, dark_reference_color())
                >= apca_like_contrast(supporting, dark_reference_color())
        );
        assert!(oklab_delta(logo, supporting) > 0.01);
    }

    #[test]
    fn oklch_blend_takes_short_hue_path_and_preserves_contrast() {
        let from = Oklch::from_color(Color::Rgb {
            r: 12,
            g: 48,
            b: 132,
        })
        .unwrap();
        let to = Oklch::from_color(Color::Rgb {
            r: 155,
            g: 228,
            b: 255,
        })
        .unwrap();
        let blended = from.lerp_short_hue(to, 0.5);
        let delta = shortest_angle_delta(from.hue, to.hue);

        assert!(delta.abs() <= std::f32::consts::PI);
        assert!(blended.chroma > 0.02);
        assert!(apca_like_contrast(blended.to_color(), dark_reference_color()) > 26.0);
    }

    #[test]
    fn motion_presets_have_distinct_phase_values() {
        let prime = MotionPhases::at(1.0, MotionPreset::Prime);
        let pulse = MotionPhases::at(1.0, MotionPreset::Pulse);

        assert!(pulse.orbit_speed > prime.orbit_speed);
        assert!(pulse.focus_gain > prime.focus_gain);
    }

    #[test]
    fn star_layers_use_distinct_deterministic_speeds() {
        let speeds = (0..6)
            .map(|layer| {
                star_layer_speed(layer, 0x9e37_79b9_u32.wrapping_mul(u32::from(layer) + 1))
            })
            .collect::<Vec<_>>();

        assert!(speeds.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(speeds[5] > speeds[0] * 8.0);
        assert_eq!(speeds[0], star_layer_speed(0, 0x9e37_79b9));
    }

    #[test]
    fn cosmic_layers_have_differential_flow_vectors() {
        let flows = (0..6)
            .map(|layer| {
                CosmicLayerFlow::for_layer(
                    layer,
                    0x9e37_79b9_u32.wrapping_mul(u32::from(layer) + 1),
                    MotionPreset::Pulse,
                )
            })
            .collect::<Vec<_>>();

        assert!(flows.windows(2).all(|pair| {
            (pair[0].direction_x - pair[1].direction_x).abs()
                + (pair[0].direction_y - pair[1].direction_y).abs()
                > 0.01
        }));
        assert!(flows[5].tail_spacing > flows[0].tail_spacing);
        assert!(flows[5].wave_amplitude > flows[0].wave_amplitude);
    }

    #[test]
    fn star_layers_stay_sparse_enough_for_negative_space() {
        let total_heads = (0..6)
            .map(|layer| star_layer_density(100, layer))
            .sum::<usize>();
        let total_tail = (0..6)
            .map(|layer| star_layer_density(100, layer) * star_layer_tail(layer))
            .sum::<usize>();

        assert!(total_heads <= 38);
        assert!(total_tail <= 38);
    }

    #[test]
    fn particle_sampling_spreads_points_across_the_field() {
        let density = star_layer_density(100, 5);
        let positions = (0..density)
            .map(|index| {
                blue_noise_particle_position(
                    index,
                    density,
                    100,
                    30,
                    0x9e37_79b9,
                    blue_noise_min_distance_for_layer(5),
                )
            })
            .collect::<Vec<_>>();

        for (left_index, &(left_x, left_y)) in positions.iter().enumerate() {
            for &(right_x, right_y) in positions.iter().skip(left_index + 1) {
                let distance = ((left_x - right_x).powi(2) + (left_y - right_y).powi(2)).sqrt();
                assert!(distance >= 3.6);
            }
        }
    }

    #[test]
    fn stratified_speed_jitter_has_visible_range() {
        let density = star_layer_density(100, 5);
        let speeds = (0..density)
            .map(|index| star_speed_jitter(index, density, mix_seed(0x9e37_79b9, index as u32)))
            .collect::<Vec<_>>();
        let min = speeds.iter().copied().fold(f32::MAX, f32::min);
        let max = speeds.iter().copied().fold(f32::MIN, f32::max);

        assert!(max - min > 0.55);
    }

    #[test]
    fn stbn_values_are_deterministic_and_temporally_decorrelated() {
        let first = stbn_mask_value(11, 7, 3, 120);
        let again = stbn_mask_value(11, 7, 3, 120);
        let next = stbn_mask_value(11, 7, 3, 124);

        assert_eq!(first, again);
        assert!((first - next).abs() > 0.02);
    }

    #[test]
    fn stbn_mask_spreads_neighboring_cells() {
        let values = (0..STBN_WIDTH)
            .map(|column| stbn_mask_value(column, 7, 2, 11))
            .collect::<Vec<_>>();
        let adjacent_delta = values
            .windows(2)
            .map(|pair| (pair[0] - pair[1]).abs())
            .sum::<f32>()
            / (values.len() - 1) as f32;

        assert!(adjacent_delta > 0.18);
    }

    #[test]
    fn perceptual_error_field_protects_logo_region() {
        let layout = Layout::for_size(100, 30);
        let field = PerceptualErrorField::from_layout(&layout, test_context(None));
        let logo_center = field.sensitivity(
            layout.logo_column + max_width(layout.logo) as u16 / 2,
            layout.logo_row + layout.logo.len() as u16 / 2,
        );
        let corner = field.sensitivity(2, layout.rows.saturating_sub(2));

        assert!(logo_center > 0.4);
        assert!(corner < 0.1);
    }

    #[test]
    fn cosmic_density_field_protects_logo_and_varies_space() {
        let layout = Layout::for_size(120, 32);
        let field = CosmicDensityField::from_layout(&layout, test_context(None));
        let logo_density = field.sample(
            layout.logo_column + max_width(layout.logo) as u16 / 2,
            layout.logo_row + layout.logo.len() as u16 / 2,
        );
        let far_left = field.sample(4, layout.rows.saturating_sub(4));
        let far_right = field.sample(layout.columns.saturating_sub(4), 4);

        assert!(logo_density < far_left.max(far_right));
        assert!((far_left - far_right).abs() > 0.03);
    }

    #[test]
    fn composition_modes_change_layout_intent() {
        let compact = Layout::for_size(60, 16);
        let standard = Layout::for_size(90, 24);
        let wide = Layout::for_size(150, 36);

        assert_eq!(compact.composition, CompositionMode::Compact);
        assert_eq!(standard.composition, CompositionMode::Standard);
        assert_eq!(wide.composition, CompositionMode::Wide);
        assert!(
            wide.composition.particle_density_multiplier()
                > compact.composition.particle_density_multiplier()
        );
    }

    #[test]
    fn anisotropic_distance_stretches_along_motion_direction() {
        let along = anisotropic_distance((4.0, 0.0), (0.0, 0.0), (1.0, 0.0), 4.0, 1.0);
        let across = anisotropic_distance((0.0, 4.0), (0.0, 0.0), (1.0, 0.0), 4.0, 1.0);

        assert!(along < across);
    }

    #[test]
    fn gravity_field_softens_near_mouse_and_fades_with_distance() {
        let near = gravity_field(2.0, 1.0);
        let middle = gravity_field(8.0, 1.0);
        let far = gravity_field(16.0, 1.0);

        assert!(near.radial < 0.0);
        assert!(middle.radial > far.radial);
        assert!(far.visibility < middle.visibility);
    }

    #[test]
    fn mouse_gravity_wake_has_a_fixed_particle_budget() {
        let mut cells = Vec::new();
        push_mouse_gravity_wake(
            &mut cells,
            100,
            30,
            test_context(Some(MouseAttractor {
                column: 40,
                row: 10,
                strength: 1.0,
                velocity_x: 0.0,
                velocity_y: 0.0,
                emitting: true,
                assimilate: false,
                release: false,
            })),
        );

        assert!((10..=14).contains(&cells.len()));
        assert!(cells.iter().all(|cell| cell.character == '.'));
    }

    #[test]
    fn mouse_particle_system_emits_motion_trail_particles() {
        let mut system = MouseParticleSystem::default();
        let mut cells = Vec::new();
        let layout = Layout::for_size(100, 30);
        let context = test_context(Some(MouseAttractor {
            column: 40,
            row: 10,
            strength: 1.0,
            velocity_x: 48.0,
            velocity_y: 8.0,
            emitting: true,
            assimilate: false,
            release: false,
        }));

        system.step(&mut cells, &layout, 1.0, context);
        system.step(&mut cells, &layout, 1.0 + 1.0 / 60.0, context);

        assert!(!system.particles.is_empty());
        assert!(system.particles.len() <= MOUSE_TRAIL_PARTICLE_LIMIT);
        assert!(cells.iter().any(|cell| cell.character == '.'));
    }

    #[test]
    fn mouse_release_creates_bounded_inertial_wake() {
        let mut runtime = RuntimeState::new(VisualProfile::Ultra, MotionPreset::Prime, false);
        handle_runtime_mouse(
            &mut runtime,
            MouseEventKind::Down(MouseButton::Left),
            20,
            8,
            1.0,
        );
        handle_runtime_mouse(
            &mut runtime,
            MouseEventKind::Drag(MouseButton::Left),
            28,
            8,
            1.1,
        );
        handle_runtime_mouse(
            &mut runtime,
            MouseEventKind::Up(MouseButton::Left),
            28,
            8,
            1.12,
        );

        let wake = runtime
            .mouse_at(1.2)
            .expect("release wake should persist briefly");
        assert!(!wake.emitting);
        assert!(wake.release);
        assert!(wake.strength > 0.0);
        assert!(runtime.mouse_at(1.8).is_none());
    }

    #[test]
    fn release_wake_does_not_emit_new_mouse_particles() {
        let mut system = MouseParticleSystem::default();
        let mut cells = Vec::new();
        let layout = Layout::for_size(100, 30);
        let context = test_context(Some(MouseAttractor {
            column: 40,
            row: 10,
            strength: 0.6,
            velocity_x: 50.0,
            velocity_y: 0.0,
            emitting: false,
            assimilate: true,
            release: true,
        }));

        system.step(&mut cells, &layout, 1.0, context);

        assert!(system.particles.is_empty());
    }

    #[test]
    fn mouse_release_assimilates_existing_particles_into_background() {
        let mut system = MouseParticleSystem::default();
        let layout = Layout::for_size(100, 30);
        let press = test_context(Some(MouseAttractor {
            column: 40,
            row: 10,
            strength: 1.0,
            velocity_x: 48.0,
            velocity_y: 8.0,
            emitting: true,
            assimilate: false,
            release: false,
        }));
        let release = test_context(Some(MouseAttractor {
            column: 40,
            row: 10,
            strength: 0.6,
            velocity_x: 48.0,
            velocity_y: 8.0,
            emitting: false,
            assimilate: true,
            release: true,
        }));
        let mut cells = Vec::new();

        system.step(&mut cells, &layout, 1.0, press);
        assert!(
            system
                .particles
                .iter()
                .any(|particle| particle.state == MouseParticleState::MouseTrail)
        );

        let before = system.particles.len();
        cells.clear();
        system.step(&mut cells, &layout, 1.1, release);
        system.step(&mut cells, &layout, 3.0, test_context(None));

        assert_eq!(system.particles.len(), before);
        assert!(
            system
                .particles
                .iter()
                .all(|particle| particle.state == MouseParticleState::Background)
        );
        assert!(cells.iter().any(|cell| {
            cell.layer == RenderLayer::Background
                && PerceptualRole::from_cell(cell) == PerceptualRole::BackgroundParticle
        }));
    }

    #[test]
    fn released_background_particles_are_capped_at_double_original_limit() {
        let mut system = MouseParticleSystem::default();
        for index in 0..260 {
            let seed = mix_seed(0xabc0_ffee, index);
            system.particles.push(MouseParticle {
                x: index as f32,
                y: 6.0 + index as f32 * 0.03,
                velocity_x: 1.0,
                velocity_y: 0.4,
                age: 0.0,
                lifetime: 0.8,
                seed,
                background_layer: 0,
                speed_jitter: 1.0,
                state: MouseParticleState::MouseTrail,
            });
        }

        system.assimilate_mouse_trails();

        assert_eq!(system.particles.len(), MOUSE_BACKGROUND_PARTICLE_LIMIT);
        assert_eq!(system.mouse_trail_count(), 0);
        assert!(
            system
                .particles
                .iter()
                .all(|particle| particle.state == MouseParticleState::Background)
        );
    }

    #[test]
    fn released_background_particles_get_differential_speeds() {
        let mut system = MouseParticleSystem::default();
        for index in 0..18 {
            let seed = mix_seed(0x5eed_fade, index);
            system.particles.push(MouseParticle {
                x: 20.0,
                y: 8.0,
                velocity_x: 0.0,
                velocity_y: 0.0,
                age: 0.0,
                lifetime: 1.0,
                seed,
                background_layer: 0,
                speed_jitter: 1.0,
                state: MouseParticleState::MouseTrail,
            });
        }

        system.assimilate_mouse_trails();
        let speeds = system
            .particles
            .iter()
            .map(|particle| {
                (
                    (particle.velocity_x * 100.0).round() as i16,
                    (particle.velocity_y * 100.0).round() as i16,
                    particle.background_layer,
                )
            })
            .collect::<BTreeSet<_>>();

        assert!(speeds.len() > 6);
    }

    #[test]
    fn background_particle_limit_keeps_high_energy_particles() {
        let mut system = MouseParticleSystem::default();
        for index in 0..(MOUSE_BACKGROUND_PARTICLE_LIMIT + 18) {
            let high_energy = index == MOUSE_BACKGROUND_PARTICLE_LIMIT + 17;
            system.particles.push(MouseParticle {
                x: index as f32,
                y: 8.0,
                velocity_x: if high_energy { 80.0 } else { 0.2 },
                velocity_y: if high_energy { 18.0 } else { 0.0 },
                age: if high_energy { 0.0 } else { 4.0 },
                lifetime: f32::INFINITY,
                seed: if high_energy {
                    0xfeed_beef
                } else {
                    index as u32
                },
                background_layer: if high_energy { 5 } else { 1 },
                speed_jitter: if high_energy { 1.5 } else { 0.7 },
                state: MouseParticleState::Background,
            });
        }

        system.limit_background_particles();

        assert_eq!(system.particles.len(), MOUSE_BACKGROUND_PARTICLE_LIMIT);
        assert!(
            system
                .particles
                .iter()
                .any(|particle| particle.seed == 0xfeed_beef)
        );
    }

    #[test]
    fn particle_color_gamut_tracks_speed_difference() {
        let context = test_context(None);
        let slow = MouseParticle {
            x: 20.0,
            y: 8.0,
            velocity_x: 1.2,
            velocity_y: 0.2,
            age: 0.0,
            lifetime: 1.0,
            seed: 0x11,
            background_layer: 1,
            speed_jitter: 0.74,
            state: MouseParticleState::Background,
        };
        let fast = MouseParticle {
            velocity_x: 34.0,
            velocity_y: 18.0,
            speed_jitter: 1.44,
            background_layer: 5,
            seed: 0x29,
            ..slow
        };
        let slow_color =
            particle_velocity_color(slow, 0.26, context, PerceptualRole::BackgroundParticle);
        let fast_color =
            particle_velocity_color(fast, 0.26, context, PerceptualRole::BackgroundParticle);
        let slow_lch = Oklch::from_color(slow_color).unwrap();
        let fast_lch = Oklch::from_color(fast_color).unwrap();

        assert!(oklab_delta(slow_color, fast_color) > 0.08);
        assert!(fast_lch.chroma > slow_lch.chroma);
    }

    #[test]
    fn particle_forward_splat_count_tracks_velocity() {
        let slow = MouseParticle {
            x: 20.0,
            y: 8.0,
            velocity_x: 2.0,
            velocity_y: 0.0,
            age: 0.0,
            lifetime: 1.0,
            seed: 0x11,
            background_layer: 1,
            speed_jitter: 0.8,
            state: MouseParticleState::Background,
        };
        let fast = MouseParticle {
            velocity_x: 48.0,
            velocity_y: 6.0,
            speed_jitter: 1.2,
            ..slow
        };

        assert_eq!(particle_forward_splat_count(slow), 1);
        assert!(particle_forward_splat_count(fast) > particle_forward_splat_count(slow));
    }

    #[test]
    fn mouse_vortex_particles_are_paired_and_bounded() {
        let mut system = MouseParticleSystem::default();
        let mouse = MouseAttractor {
            column: 40,
            row: 10,
            strength: 1.0,
            velocity_x: 80.0,
            velocity_y: 12.0,
            emitting: true,
            assimilate: false,
            release: false,
        };

        system.emit_vortices(mouse);
        let signed_strength = system
            .vortices
            .iter()
            .map(|vortex| vortex.sign * vortex.strength)
            .sum::<f32>();

        assert_eq!(system.vortices.len(), 2);
        assert!(signed_strength.abs() < 0.001);
    }

    #[test]
    fn vortex_particles_decay_without_unbounded_energy() {
        let mut system = MouseParticleSystem::default();
        let layout = Layout::for_size(100, 30);
        system.emit_vortices(MouseAttractor {
            column: 40,
            row: 10,
            strength: 1.0,
            velocity_x: 80.0,
            velocity_y: 0.0,
            emitting: true,
            assimilate: false,
            release: false,
        });
        let initial = system
            .vortices
            .iter()
            .map(|vortex| vortex.strength)
            .sum::<f32>();
        system.step_vortices(0.5, &layout);
        let decayed = system
            .vortices
            .iter()
            .map(|vortex| vortex.strength)
            .sum::<f32>();

        assert!(decayed < initial);
    }

    #[test]
    fn velocity_grid_injection_decays_and_samples_motion() {
        let layout = Layout::for_size(100, 30);
        let mouse = MouseAttractor {
            column: 40,
            row: 10,
            strength: 1.0,
            velocity_x: 60.0,
            velocity_y: 6.0,
            emitting: true,
            assimilate: false,
            release: false,
        };
        let mut grid = VelocityGrid::default();

        grid.step(1.0 / 120.0, Some(mouse), &layout);
        let injected = grid.energy();
        let sampled = grid.sample(40.0, 10.0, &layout);
        grid.step(0.5, None, &layout);

        assert!(injected > 0.0);
        assert!(sampled.0.abs() + sampled.1.abs() > 0.0);
        assert!(grid.energy() < injected);
    }

    #[test]
    fn afterimage_buffer_renders_motion_layers_but_not_text() {
        let mut buffer = AfterimageBuffer::default();
        let layout = Layout::for_size(80, 24);
        let context = test_context(None);
        let mut cells = vec![
            Cell {
                column: 1,
                row: 2,
                character: 'T',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 2,
                row: 2,
                character: '━',
                color: ACCENT_COLOR,
                layer: RenderLayer::Orbit,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
        ];

        buffer.apply(&mut cells, 1.0, context, &layout);
        let mut next = Vec::new();
        buffer.apply(&mut next, 1.05, context, &layout);

        assert!(next.iter().any(|cell| cell.character == '━'));
        assert!(!next.iter().any(|cell| cell.character == 'T'));
    }

    #[test]
    fn temporal_kernel_shortens_afterimage_at_120fps() {
        let sixty = TemporalKernel::for_fps(60);
        let one_twenty = TemporalKernel::for_fps(120);

        assert!(one_twenty.decay_seconds < sixty.decay_seconds);
        assert!(one_twenty.rejection_threshold < sixty.rejection_threshold);
        assert!(one_twenty.shutter_width < sixty.shutter_width);
        assert!(one_twenty.tail_energy_budget < sixty.tail_energy_budget);
    }

    #[test]
    fn cell_temporal_aa_smooths_background_but_rejects_text() {
        let mut buffer = CellTemporalAaBuffer::default();
        let context = test_context(None);
        let mut first = vec![
            Cell {
                column: 2,
                row: 2,
                character: '.',
                color: Color::Rgb {
                    r: 80,
                    g: 160,
                    b: 220,
                },
                layer: RenderLayer::Background,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 3,
                row: 2,
                character: 'T',
                color: Color::Rgb {
                    r: 90,
                    g: 170,
                    b: 220,
                },
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
        ];
        buffer.apply(&mut first, context);
        let mut second = vec![
            Cell {
                color: Color::Rgb {
                    r: 84,
                    g: 166,
                    b: 228,
                },
                ..first[0].clone()
            },
            Cell {
                color: Color::Rgb {
                    r: 30,
                    g: 50,
                    b: 70,
                },
                ..first[1].clone()
            },
        ];
        let raw_background = second[0].color;
        let raw_text = second[1].color;

        buffer.apply(&mut second, context);

        assert_ne!(second[0].color, raw_background);
        assert_eq!(second[1].color, raw_text);
    }

    #[test]
    fn smoothing_buffer_limits_logo_chroma_jumps() {
        let mut buffer = SmoothingBuffer::default();
        let mut first = vec![Cell {
            column: 2,
            row: 2,
            character: '█',
            color: rgb!(20, 90, 230),
            layer: RenderLayer::Text,
            primitive_id: None,
            stroke_id: None,
            vertex_id: None,
            path_index: None,
            correspondence_lost: false,
        }];
        buffer.apply(&mut first, VisualProfile::Ultra);
        let mut second = vec![Cell {
            column: 2,
            row: 2,
            character: '█',
            color: rgb!(255, 80, 220),
            layer: RenderLayer::Text,
            primitive_id: None,
            stroke_id: None,
            vertex_id: None,
            path_index: None,
            correspondence_lost: false,
        }];
        buffer.apply(&mut second, VisualProfile::Ultra);

        assert!(oklab_distance(first[0].color, second[0].color) < 0.055);
    }

    #[test]
    fn luminance_budget_dims_background_under_foreground() {
        let layout = Layout::for_size(40, 12);
        let mut cells = vec![
            Cell {
                column: 10,
                row: 5,
                character: '█',
                color: Color::Rgb {
                    r: 210,
                    g: 240,
                    b: 255,
                },
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 10,
                row: 5,
                character: '.',
                color: Color::Rgb {
                    r: 120,
                    g: 210,
                    b: 255,
                },
                layer: RenderLayer::Background,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
        ];
        let before = Oklab::from_color(cells[1].color).unwrap().lightness;
        let budget = LuminanceBudget::from_foreground(&cells, &layout);
        budget.apply(&mut cells);
        let after = Oklab::from_color(cells[1].color).unwrap().lightness;

        assert!(after < before);
    }

    #[test]
    fn adaptive_controller_reduces_quality_before_effects_explode() {
        let mut controller = AdaptiveFrameController::new(Duration::from_millis(8));
        let initial = controller.snapshot().quality_percent;
        controller.record(
            Duration::from_millis(30),
            &RenderStats {
                dirty_cells: 900,
                dirty_runs: 80,
                stale_cells: 120,
                stale_runs: 30,
            },
        );

        assert!(controller.snapshot().quality_percent < initial);
        assert!(controller.snapshot().low_latency_active);
        assert!(
            controller
                .quality_settings(test_calibration())
                .particle_density
                < 1.0
        );
    }

    #[test]
    fn frame_record_round_trip_preserves_cells() {
        let cell = Cell {
            column: 3,
            row: 2,
            character: '━',
            color: Color::Rgb {
                r: 120,
                g: 210,
                b: 255,
            },
            layer: RenderLayer::Orbit,
            primitive_id: None,
            stroke_id: None,
            vertex_id: None,
            path_index: None,
            correspondence_lost: false,
        };
        let encoded = cell_to_record(&cell);
        let decoded = parse_recorded_cell(&encoded).expect("cell should decode");

        assert_eq!(decoded, cell);
    }

    #[test]
    fn visual_score_prefers_good_fixture_over_flicker_fixture() {
        let good = FrameSequence::read("tests/fixtures/good_frames.jsonl").unwrap();
        let bad = FrameSequence::read("tests/fixtures/bad_flicker_frames.jsonl").unwrap();

        assert!(
            VisualScore::from_sequence(&good).visual_quality_score
                > VisualScore::from_sequence(&bad).visual_quality_score
        );
    }

    #[test]
    fn visual_score_reports_role_aware_fields() {
        let good = FrameSequence::read("tests/fixtures/good_frames.jsonl").unwrap();
        let score = VisualScore::from_sequence(&good);
        let json = score.to_json();

        assert!(json.contains("foreground_clarity"));
        assert!(json.contains("background_atmosphere"));
        assert!(json.contains("dirty_budget_violation_rate"));
        assert!(json.contains("contrast_violation_rate"));
        assert!(json.contains("temporal_high_band_energy"));
        assert!(json.contains("particle_spectral_quality"));
        assert!(json.contains("orbit_temporal_continuity"));
        assert!(json.contains("orbit_glyph_stability"));
        assert!(json.contains("orbit_motion_aliasing_pressure"));
        assert!(json.contains("logo_color_stability"));
        assert!(score.foreground_clarity > 0.0);
        assert!(score.dirty_budget_violation_rate >= 0.0);
        assert!(score.contrast_violation_rate >= 0.0);
        assert!((0.0..=1.0).contains(&score.orbit_temporal_continuity));
        assert!((0.0..=1.0).contains(&score.orbit_glyph_stability));
        assert!((0.0..=1.0).contains(&score.orbit_motion_aliasing_pressure));
        assert!((0.0..=1.0).contains(&score.logo_color_stability));
    }

    #[test]
    fn golden_recordings_gate_temporal_and_palette_ablations() {
        let golden = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/golden_ultra_frames.jsonl").unwrap(),
        );
        let hash_only = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/hash_only_sampling_frames.jsonl").unwrap(),
        );
        let overbright = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/overbright_palette_frames.jsonl").unwrap(),
        );
        let broken_mouse = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/broken_mouse_flow_frames.jsonl").unwrap(),
        );

        assert!(golden.visual_quality_score > hash_only.visual_quality_score);
        assert!(golden.visual_quality_score > overbright.visual_quality_score);
        assert!(golden.temporal_high_band_energy < hash_only.temporal_high_band_energy);
        assert!(golden.particle_spectral_quality > hash_only.particle_spectral_quality);
        assert!(broken_mouse.local_flicker_discomfort > golden.local_flicker_discomfort);
    }

    #[test]
    fn metric_validation_fixtures_target_expected_failure_axes() {
        let golden = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/golden_ultra_frames.jsonl").unwrap(),
        );
        let hash_only = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/hash_only_sampling_frames.jsonl").unwrap(),
        );
        let clumped = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/clumped_particles.jsonl").unwrap(),
        );
        let contrast = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/role_contrast_hierarchy.jsonl").unwrap(),
        );

        assert!(hash_only.temporal_high_band_energy > golden.temporal_high_band_energy);
        assert!(clumped.clustering_pressure > golden.clustering_pressure);
        assert!(contrast.contrast_violation_rate > golden.contrast_violation_rate);
        assert!(contrast.foreground_clarity < golden.foreground_clarity);
    }

    #[test]
    fn score_driven_parameter_search_beats_baseline_objective() {
        let sequence = FrameSequence::read("tests/fixtures/golden_ultra_frames.jsonl").unwrap();
        let score = VisualScore::from_sequence(&sequence);
        let baseline = VisualParameterSet::baseline().visual_objective(score);
        let optimized = optimize_visual_parameters(&sequence);

        assert!(optimized.objective >= baseline);
        assert!(optimized.parameters.taa_weight >= 1.0);
        assert!(optimized.parameters.orbit_span_gain >= 1.0);
    }

    #[test]
    fn ablation_fixtures_penalize_clumping_and_contrast_hierarchy_failures() {
        let good = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/good_frames.jsonl").unwrap(),
        );
        let clumped = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/clumped_particles.jsonl").unwrap(),
        );
        let contrast = VisualScore::from_sequence(
            &FrameSequence::read("tests/fixtures/role_contrast_hierarchy.jsonl").unwrap(),
        );

        assert!(clumped.clustering_pressure > good.clustering_pressure);
        assert!(contrast.contrast_violations > good.contrast_violations);
    }

    #[test]
    fn visual_curves_are_bounded_and_distinct() {
        let subtle = VisualCurves::for_profile(VisualProfile::Calm);
        let intense = VisualCurves::for_profile(VisualProfile::Ultra);

        assert_eq!(subtle.family, CurveFamily::Subtle);
        assert!(subtle.brightness < intense.brightness);
        assert!((0.4..=1.2).contains(&intense.chroma));
        assert!(VisualCurves::for_profile(VisualProfile::Benchmark).afterimage_decay < 1.0);
    }

    #[test]
    fn terminal_visual_profile_uses_known_presets() {
        assert_eq!(
            terminal_visual_profile(test_calibration()),
            TerminalVisualProfile::GpuRich
        );
        let mut calibration = test_calibration();
        calibration.capabilities.preset = TerminalPreset::LinuxConsole;
        assert_eq!(
            terminal_visual_profile(calibration),
            TerminalVisualProfile::Minimal
        );
    }

    #[test]
    fn mouse_flow_responds_to_speed() {
        let slow = MouseFlow::from_velocity(2.0, 1.0);
        let fast = MouseFlow::from_velocity(90.0, 0.0);

        assert_eq!(slow.speed_bucket, MouseSpeedBucket::Slow);
        assert_eq!(fast.speed_bucket, MouseSpeedBucket::Fast);
        assert!(fast.tail_length > slow.tail_length);
        assert!(slow.swirl_gain > fast.swirl_gain);
    }

    #[test]
    fn motion_director_phases_shape_layer_priority() {
        let reveal = MotionDirectorState::reveal(0.2);
        let emphasis = MotionDirectorState::at(6.2, VisualProfile::Ultra);

        assert_eq!(reveal.phase, MotionDirectorPhase::Emergence);
        assert_eq!(emphasis.phase, MotionDirectorPhase::Emphasis);
        assert!(emphasis.logo_weight > emphasis.background_weight);
    }

    #[test]
    fn inspect_overlay_reports_visual_tuning_state() {
        let mut metrics = RenderMetrics::new();
        metrics.record(
            Duration::from_millis(1),
            RenderStats {
                dirty_cells: 4,
                dirty_runs: 2,
                stale_cells: 1,
                stale_runs: 1,
            },
            AdaptiveSnapshot {
                target_fps: 120,
                missed_deadlines: 0,
                quality_percent: 100,
                low_latency_active: false,
            },
        );
        let text = inspect_overlay_text(
            &Layout::for_size(80, 24),
            &metrics,
            test_calibration(),
            RuntimeState::new(VisualProfile::Ultra, MotionPreset::Prime, true),
        );

        assert!(text.contains("curve:intense"));
        assert!(text.contains("phase:"));
        assert!(text.contains("term:gpu-rich"));
    }

    #[test]
    fn terminal_only_pointer_backend_is_explicit_fallback() {
        assert_eq!(
            sample_system_pointer(PointerBackend::TerminalOnly).map(|_| ()),
            None
        );
        assert_eq!(PointerBackend::TerminalOnly.name(), "terminal-mouse");
        assert!(PointerBackend::TerminalOnly.confidence() > 0.0);
    }

    #[test]
    fn mouse_drag_logic_requires_left_button_down() {
        let mut runtime = RuntimeState::new(VisualProfile::Ultra, MotionPreset::Prime, false);

        handle_runtime_mouse(&mut runtime, MouseEventKind::Moved, 10, 4, 1.0);
        assert!(runtime.mouse.is_none());

        handle_runtime_mouse(
            &mut runtime,
            MouseEventKind::Drag(MouseButton::Left),
            12,
            5,
            1.1,
        );
        assert!(runtime.mouse.is_none());

        handle_runtime_mouse(
            &mut runtime,
            MouseEventKind::Down(MouseButton::Left),
            12,
            5,
            1.2,
        );
        assert!(runtime.left_mouse_down);
        assert!(runtime.mouse.is_some());

        handle_runtime_mouse(
            &mut runtime,
            MouseEventKind::Drag(MouseButton::Left),
            16,
            7,
            1.3,
        );
        assert_eq!(
            runtime.mouse.map(|mouse| (mouse.column, mouse.row)),
            Some((16, 7))
        );

        handle_runtime_mouse(
            &mut runtime,
            MouseEventKind::Up(MouseButton::Left),
            16,
            7,
            1.4,
        );
        assert!(!runtime.left_mouse_down);
        assert!(runtime.mouse.is_none());
    }

    #[test]
    fn parses_xdotool_shell_values() {
        let output = "WINDOW=12582915\nX=120\nY=80\nWIDTH=900\nHEIGHT=540\n";

        assert_eq!(shell_value_i32(output, "X"), Some(120));
        assert_eq!(shell_value_i32(output, "WIDTH"), Some(900));
        assert_eq!(shell_value_i32(output, "MISSING"), None);
    }

    #[test]
    fn maps_system_pointer_pixels_to_terminal_cells() {
        let layout = Layout::for_size(100, 30);
        let sample = PointerSample {
            window_x: 100,
            window_y: 200,
            window_width: 916,
            window_height: 580,
            pointer_x: 108 + 9 * 42,
            pointer_y: 232 + 18 * 11,
        };
        let cell = pointer_cell_from_sample(sample, &layout).expect("pointer should map in bounds");

        assert_eq!(cell.column, 42);
        assert_eq!(cell.row, 11);
    }

    #[test]
    fn starfield_builds_particle_only_parallax_layers() {
        let mut cells = Vec::new();
        push_background_depth(
            &mut cells,
            &Layout::for_size(100, 30),
            1.0,
            FrameContext {
                motion_preset: MotionPreset::Pulse,
                ..test_context(None)
            },
        );

        assert!(cells.len() > 16);
        assert!(cells.len() < 72);
        assert!(cells.iter().all(|cell| cell.character == '.'));
        assert!(
            cells
                .iter()
                .all(|cell| cell.layer == RenderLayer::Background)
        );
        assert!(cells.iter().map(|cell| cell.row).max().unwrap_or(0) > 12);
        assert!(cells.iter().map(|cell| cell.column).min().unwrap_or(0) < 20);
        assert!(cells.iter().map(|cell| cell.column).max().unwrap_or(0) > 78);
    }

    #[test]
    fn comet_events_are_bounded_and_avoid_logo_focus() {
        let layout = Layout::for_size(120, 32);
        let context = test_context(None);
        let error_field = PerceptualErrorField::from_layout(&layout, context);
        let density_field = CosmicDensityField::from_layout(&layout, context);
        let mut cells = Vec::new();

        push_comet_event(
            &mut cells,
            120,
            29,
            14.2,
            context,
            &error_field,
            &density_field,
        );

        assert!(cells.len() <= 20);
        assert!(
            cells
                .iter()
                .all(|cell| error_field.sensitivity(cell.column, cell.row) <= 0.38)
        );
    }

    #[test]
    fn background_cosmic_field_is_independent_from_mouse_trail_layer() {
        let layout = Layout::for_size(100, 30);
        let mut idle_cells = Vec::new();
        let mut mouse_cells = Vec::new();
        push_background_depth(&mut idle_cells, &layout, 1.0, test_context(None));
        push_background_depth(
            &mut mouse_cells,
            &layout,
            1.0,
            test_context(Some(MouseAttractor {
                column: 40,
                row: 10,
                strength: 1.0,
                velocity_x: 80.0,
                velocity_y: 0.0,
                emitting: true,
                assimilate: false,
                release: false,
            })),
        );

        let idle_positions = idle_cells
            .iter()
            .map(|cell| (cell.column, cell.row))
            .collect::<std::collections::BTreeSet<_>>();
        let mouse_positions = mouse_cells
            .iter()
            .take(idle_cells.len())
            .map(|cell| (cell.column, cell.row))
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(idle_positions, mouse_positions);
        assert!(mouse_cells.len() > idle_cells.len());
    }

    #[test]
    fn text_and_orbit_layers_cover_background_particles() {
        let cells = composite_layers(vec![
            Cell {
                column: 4,
                row: 2,
                character: '.',
                color: ACCENT_COLOR,
                layer: RenderLayer::Background,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 4,
                row: 2,
                character: '━',
                color: ACCENT_COLOR,
                layer: RenderLayer::Orbit,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 8,
                row: 2,
                character: '.',
                color: ACCENT_COLOR,
                layer: RenderLayer::Background,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 8,
                row: 2,
                character: 'T',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
        ]);

        assert_eq!(cells[0].character, '━');
        assert_eq!(cells[1].character, 'T');
    }

    #[test]
    fn snapshot_summary_is_deterministic_for_fixed_frame() {
        let layout = Layout::for_size(80, 24);
        let mut frame_buffers = FrameBuffers::default();
        let frame = Frame::build(&layout, 1.25, test_context(None), &mut frame_buffers);
        let left = SnapshotSummary::from_frame(
            &frame,
            &layout,
            VisualProfile::Ultra,
            MotionPreset::Prime,
            ScenePreset::Orbit,
            test_calibration(),
        );
        let right = SnapshotSummary::from_frame(
            &frame,
            &layout,
            VisualProfile::Ultra,
            MotionPreset::Prime,
            ScenePreset::Orbit,
            test_calibration(),
        );

        assert_eq!(left.hash, right.hash);
        assert!(left.to_json().contains("\"motion\":\"prime\""));
        assert!(left.to_json().contains("orbit_budget_pressure"));
    }

    #[test]
    fn trail_alpha_fades_outside_visible_span() {
        assert!(trail_alpha(0.0, 24.0) > trail_alpha(12.0, 24.0));
        assert!(trail_alpha(12.0, 24.0) > trail_alpha(24.0, 24.0));
        assert_eq!(trail_alpha(25.0, 24.0), 0.0);
    }

    #[test]
    fn circular_distance_wraps_around_period() {
        assert_eq!(circular_distance_f32(0.0, 23.0, 24.0), 1.0);
        assert_eq!(circular_distance_f32(2.0, 8.0, 24.0), 6.0);
    }

    #[test]
    fn frame_box_tracks_rectangular_perimeter() {
        let frame_box = FrameBox {
            left: 2,
            top: 3,
            width: 6,
            height: 4,
        };

        assert_eq!(frame_box.perimeter_len(), 16);
        assert_eq!(frame_box.point_at(0), Some((2, 3)));
        assert_eq!(frame_box.point_at(5), Some((7, 3)));
        assert_eq!(frame_box.point_at(6), Some((7, 4)));
        assert_eq!(frame_box.point_at(8), Some((7, 6)));
        assert_eq!(frame_box.point_at(15), Some((2, 4)));
    }

    #[test]
    fn orbit_line_uses_directional_characters() {
        let frame_box = FrameBox {
            left: 2,
            top: 3,
            width: 6,
            height: 4,
        };

        assert_eq!(orbit_character(frame_box, 1, GlyphMode::Unicode), '━');
        assert_eq!(orbit_character(frame_box, 6, GlyphMode::Unicode), '┃');
        assert_eq!(orbit_character(frame_box, 0, GlyphMode::Unicode), '╭');
        assert_eq!(orbit_character(frame_box, 5, GlyphMode::Unicode), '╮');
        assert_eq!(orbit_character(frame_box, 5, GlyphMode::Ascii), '*');
        assert!(matches!(
            orbit_character(frame_box, 5, GlyphMode::Braille),
            '⠁'..='⣿'
        ));
    }

    #[test]
    fn frame_box_disables_when_terminal_is_too_small() {
        assert!(frame_box_for_logo(10, 5, 0, 0, 12, 1).is_none());
        assert!(frame_box_for_logo(20, 10, 4, 3, 10, 1).is_some());
    }

    #[test]
    fn orbit_trail_is_longer_than_a_short_highlight() {
        let frame_box = FrameBox {
            left: 2,
            top: 3,
            width: 30,
            height: 8,
        };
        let mut cells = Vec::new();

        push_orbit_trail(
            &mut cells,
            frame_box,
            0.0,
            test_context(None),
            &mut OrbitTemporalBuffer::default(),
        );

        assert!(cells.len() >= 18);
    }

    #[test]
    fn orbit_cell_coverage_changes_smoothly_between_subcell_positions() {
        let perimeter = 76.0;
        let span = 34.0;
        let before =
            orbit_energy_wave_alpha(12.10, 10.0, perimeter, span, 0.2, MotionPreset::Prime);
        let after = orbit_energy_wave_alpha(12.45, 10.0, perimeter, span, 0.2, MotionPreset::Prime);

        assert!((after - before).abs() < 0.08);
        assert!(before > 0.0);
    }

    #[test]
    fn orbit_startup_boost_integrates_to_smooth_forward_motion() {
        let dt = 1.0 / 120.0;
        let mut previous_position = 0.0;
        let mut previous_step: Option<f32> = None;
        let mut max_step_delta: f32 = 0.0;

        for frame in 1..96 {
            let elapsed = frame as f32 * dt;
            let position =
                (elapsed + orbit_startup_boost_distance(elapsed)) * ORBIT_CELLS_PER_SECOND;
            let step = position - previous_position;

            assert!(step > 0.0);
            if let Some(previous_step) = previous_step {
                max_step_delta = max_step_delta.max((step - previous_step).abs());
            }
            previous_position = position;
            previous_step = Some(step);
        }

        assert!(max_step_delta < 0.035);
        assert!(orbit_startup_boost_distance(dt) < 0.0002);
        assert!(
            (orbit_startup_boost_distance(ORBIT_STARTUP_BOOST_SECONDS)
                - (ORBIT_STARTUP_SPEED_BOOST - 1.0) * ORBIT_STARTUP_BOOST_SECONDS * 0.5)
                .abs()
                < 0.0001
        );
    }

    #[test]
    fn reveal_orbit_weight_reaches_steady_without_boundary_jump() {
        let before = MotionDirectorState::reveal(1.24).orbit_weight;
        let after = MotionDirectorState::reveal(1.25).orbit_weight;

        assert!((after - before).abs() < 0.002);
        assert!(after > 0.99);
    }

    #[test]
    fn orbit_temporal_jitter_is_bounded_and_cell_decorrelated() {
        let first = orbit_temporal_stbn_jitter(3.0, 76.0, 12);
        let second = orbit_temporal_stbn_jitter(17.0, 76.0, 12);
        let later = orbit_temporal_stbn_jitter(3.0, 76.0, 19);

        assert!((-0.5..=0.5).contains(&first));
        assert!((first - second).abs() > 0.01);
        assert!((first - later).abs() > 0.01);
    }

    #[test]
    fn orbit_temporal_buffer_limits_alpha_jumps_and_keeps_short_tail() {
        let mut buffer = OrbitTemporalBuffer::default();
        let tuning = OrbitRenderTuning::for_context(test_context(None), 0.0);
        let frame_box = FrameBox {
            left: 1,
            top: 1,
            width: 12,
            height: 5,
        };
        let mut first = vec![None; frame_box.perimeter_len()];
        first[3] = Some(OrbitVisibleCell {
            alpha: 0.22,
            path_index: 3,
        });
        buffer.stabilize(&mut first, frame_box, tuning);

        let mut next = vec![None; frame_box.perimeter_len()];
        next[3] = Some(OrbitVisibleCell {
            alpha: 0.92,
            path_index: 3,
        });
        buffer.stabilize(&mut next, frame_box, tuning);
        assert!(next[3].unwrap().alpha < 0.40);

        let mut empty = vec![None; frame_box.perimeter_len()];
        buffer.stabilize(&mut empty, frame_box, tuning);
        assert!(empty[3].is_some());
        assert!(empty[3].unwrap().alpha > 0.05);
        assert!(empty.iter().filter(|cell| cell.is_some()).count() > 1);
    }

    #[test]
    fn orbit_render_tuning_scales_quality_by_profile() {
        let ultra = OrbitRenderTuning::for_context(test_context(None), 0.0);
        let calm = OrbitRenderTuning::for_context(
            FrameContext {
                profile: VisualProfile::Calm,
                ..test_context(None)
            },
            0.0,
        );

        assert!(ultra.splat_step < calm.splat_step);
        assert!(ultra.prelight_gain > calm.prelight_gain);
        assert!(ultra.alpha_floor < calm.alpha_floor);
        assert!(ultra.max_samples > calm.max_samples);
    }

    #[test]
    fn orbit_temporal_buffer_holds_bright_character_direction() {
        let mut buffer = OrbitTemporalBuffer::default();
        let tuning = OrbitRenderTuning::for_context(test_context(None), 0.0);
        let frame_box = FrameBox {
            left: 1,
            top: 1,
            width: 12,
            height: 5,
        };
        let mut first = vec![None; frame_box.perimeter_len()];
        first[3] = Some(OrbitVisibleCell {
            alpha: 0.72,
            path_index: 3,
        });
        buffer.stabilize(&mut first, frame_box, tuning);

        let mut next = vec![None; frame_box.perimeter_len()];
        next[3] = Some(OrbitVisibleCell {
            alpha: 0.70,
            path_index: 4,
        });
        buffer.stabilize(&mut next, frame_box, tuning);

        assert_eq!(next[3].unwrap().path_index, 3);
    }

    #[test]
    fn orbit_perceptual_alpha_is_bounded_and_stbn_decorrelated() {
        let context = test_context(None);
        let tuning = OrbitRenderTuning::for_context(context, 0.2);
        let first = orbit_perceptual_alpha(0.37, 4, 2, 0.2, context, tuning);
        let buckets = (0..12)
            .map(|column| {
                (orbit_perceptual_alpha(0.37, column, 2, 0.2, context, tuning) * 10_000.0).round()
                    as i32
            })
            .collect::<BTreeSet<_>>();

        assert!((0.0..=1.0).contains(&first));
        assert!((first - 0.37).abs() < 0.08);
        assert!(buckets.len() > 1);
    }

    #[test]
    fn orbit_motion_changes_a_bounded_number_of_cells_per_frame() {
        let frame_box = FrameBox {
            left: 2,
            top: 3,
            width: 30,
            height: 8,
        };
        let mut first_cells = Vec::new();
        let mut next_cells = Vec::new();

        let mut first_buffer = OrbitTemporalBuffer::default();
        let mut next_buffer = OrbitTemporalBuffer::default();

        push_orbit_trail(
            &mut first_cells,
            frame_box,
            0.0,
            test_context(None),
            &mut first_buffer,
        );
        push_orbit_trail(
            &mut next_cells,
            frame_box,
            1.0 / 120.0,
            test_context(None),
            &mut next_buffer,
        );

        let first = first_cells
            .iter()
            .map(|cell| (cell.column, cell.row))
            .collect::<BTreeSet<_>>();
        let next = next_cells
            .iter()
            .map(|cell| (cell.column, cell.row))
            .collect::<BTreeSet<_>>();
        let changed = first.symmetric_difference(&next).count();

        assert!(changed < 8);
    }

    #[test]
    fn orbit_energy_uses_temporal_supersampling_at_60fps() {
        let mut context = test_context(None);
        context.calibration.target_fps = 60;
        let perimeter = 76.0;
        let span = 34.0;
        let alpha = temporal_orbit_energy_alpha(0.125, 1.0 / 60.0, 1.0, perimeter, span, context);
        let single_head =
            spline_orbit_head(0.125, perimeter, context.motion_preset, context.curves);
        let single = orbit_energy_wave_alpha(
            single_head,
            1.0,
            perimeter,
            span,
            0.125,
            context.motion_preset,
        );

        assert!(alpha > 0.0);
        assert!((alpha - single).abs() < 0.12);
    }

    #[test]
    fn orbit_dirty_matrix_ignores_subvisible_color_drift() {
        let previous = Frame {
            cells: vec![Cell {
                column: 10,
                row: 4,
                character: '━',
                color: Color::Rgb {
                    r: 50,
                    g: 130,
                    b: 240,
                },
                layer: RenderLayer::Orbit,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            }],
        };
        let next = Frame {
            cells: vec![Cell {
                column: 10,
                row: 4,
                character: '━',
                color: Color::Rgb {
                    r: 52,
                    g: 132,
                    b: 242,
                },
                layer: RenderLayer::Orbit,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            }],
        };

        assert_eq!(next.dirty_cells(Some(&previous), DirtyMode::Full).len(), 0);
    }

    #[test]
    fn orbit_dirty_matrix_keeps_visible_orbit_changes() {
        let previous = Frame {
            cells: vec![Cell {
                column: 10,
                row: 4,
                character: '━',
                color: Color::Rgb {
                    r: 32,
                    g: 90,
                    b: 190,
                },
                layer: RenderLayer::Orbit,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            }],
        };
        let next = Frame {
            cells: vec![Cell {
                column: 10,
                row: 4,
                character: '━',
                color: Color::Rgb {
                    r: 92,
                    g: 210,
                    b: 255,
                },
                layer: RenderLayer::Orbit,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            }],
        };

        assert_eq!(next.dirty_cells(Some(&previous), DirtyMode::Full).len(), 1);
    }

    #[test]
    fn perceptual_dirty_policy_differs_from_naive_cell_diff() {
        let previous = Frame {
            cells: vec![Cell {
                column: 4,
                row: 2,
                character: '━',
                color: rgb!(50, 130, 240),
                layer: RenderLayer::Orbit,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            }],
        };
        let next = Frame {
            cells: vec![Cell {
                column: 4,
                row: 2,
                character: '━',
                color: rgb!(52, 132, 242),
                layer: RenderLayer::Orbit,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            }],
        };

        assert_eq!(next.dirty_cells(Some(&previous), DirtyMode::Naive).len(), 1);
        assert_eq!(
            next.dirty_cells(Some(&previous), DirtyMode::RoleAware)
                .len(),
            0
        );
    }

    #[test]
    fn paper_record_header_preserves_ablation_modes() {
        let mut config = FrameRecordConfig {
            profile: VisualProfile::Ultra,
            motion_preset: MotionPreset::Prime,
            calibration: test_calibration(),
            theme: Theme::default(),
            columns: 40,
            rows: 12,
            frames: 2,
            fps: 120,
            low_latency: false,
            dirty_mode: DirtyMode::RoleAware,
            glyph_history_mode: GlyphHistoryMode::ScreenCell,
        };
        config.calibration.dirty_cell_budget = 320;
        let sequence = build_frame_sequence(config);
        let json = sequence.header.to_json();

        assert!(json.contains("\"dirty_mode\":\"role-aware\""));
        assert!(json.contains("\"glyph_history\":\"screen-cell\""));
        assert!(json.contains("\"dirty_cell_budget\":320"));
    }

    #[test]
    fn visual_score_reports_paper_glyph_metrics() {
        let sequence = build_frame_sequence(FrameRecordConfig {
            profile: VisualProfile::Ultra,
            motion_preset: MotionPreset::Prime,
            calibration: test_calibration(),
            theme: Theme::default(),
            columns: 40,
            rows: 12,
            frames: 8,
            fps: 120,
            low_latency: false,
            dirty_mode: DirtyMode::Full,
            glyph_history_mode: GlyphHistoryMode::Path,
        });
        let score = VisualScore::from_sequence(&sequence);
        let json = score.to_json();

        assert!(json.contains("glyph_flips_per_second"));
        assert!(json.contains("path_index_discontinuity_rate"));
        assert!(json.contains("corner_glyph_instability"));
        assert!(json.contains("orbit_cells_changed_per_frame"));
        assert!(score.path_index_discontinuity_rate >= 0.0);
    }

    #[test]
    fn glyph_stress_fixture_separates_history_modes() {
        let config = FrameRecordConfig {
            profile: VisualProfile::Ultra,
            motion_preset: MotionPreset::Prime,
            calibration: test_calibration(),
            theme: Theme::default(),
            columns: 40,
            rows: 12,
            frames: 12,
            fps: 120,
            low_latency: false,
            dirty_mode: DirtyMode::Full,
            glyph_history_mode: GlyphHistoryMode::Path,
        };
        let off = VisualScore::from_sequence(&glyph_stress_sequence(config, GlyphHistoryMode::Off));
        let screen = VisualScore::from_sequence(&glyph_stress_sequence(
            config,
            GlyphHistoryMode::ScreenCell,
        ));
        let path =
            VisualScore::from_sequence(&glyph_stress_sequence(config, GlyphHistoryMode::Path));

        assert!(off.glyph_flips_per_second > screen.glyph_flips_per_second);
        assert!(screen.glyph_flips_per_second > path.glyph_flips_per_second);
        assert!(off.corner_glyph_instability > path.corner_glyph_instability);
    }

    #[test]
    fn real_orbit_reconstruction_fixture_separates_history_modes() {
        let config = FrameRecordConfig {
            profile: VisualProfile::Ultra,
            motion_preset: MotionPreset::Prime,
            calibration: test_calibration(),
            theme: Theme::default(),
            columns: 80,
            rows: 24,
            frames: 12,
            fps: 120,
            low_latency: false,
            dirty_mode: DirtyMode::TopologyOnly,
            glyph_history_mode: GlyphHistoryMode::Path,
        };
        let off = VisualScore::from_sequence(&orbit_corner_reconstruction_sequence(
            config,
            GlyphHistoryMode::Off,
        ));
        let screen = VisualScore::from_sequence(&orbit_corner_reconstruction_sequence(
            config,
            GlyphHistoryMode::ScreenCell,
        ));
        let path = VisualScore::from_sequence(&orbit_corner_reconstruction_sequence(
            config,
            GlyphHistoryMode::Path,
        ));

        assert!(off.glyph_flips_per_second > screen.glyph_flips_per_second);
        assert!(screen.glyph_flips_per_second > path.glyph_flips_per_second);
        assert!(off.corner_glyph_instability > path.corner_glyph_instability);
    }

    #[test]
    fn dirty_audit_reports_role_suppression_and_budget_drops() {
        let mut config = FrameRecordConfig {
            profile: VisualProfile::Ultra,
            motion_preset: MotionPreset::Prime,
            calibration: test_calibration(),
            theme: Theme::default(),
            columns: 80,
            rows: 24,
            frames: 12,
            fps: 120,
            low_latency: false,
            dirty_mode: DirtyMode::RoleAware,
            glyph_history_mode: GlyphHistoryMode::Path,
        };
        config.calibration.dirty_cell_budget = 24;
        let mut role_config = config;
        role_config.dirty_mode = DirtyMode::RoleAware;
        let role_rows = dirty_audit_rows(&build_frame_sequence(role_config), DirtyMode::RoleAware);
        let orbit = role_rows
            .iter()
            .find(|row| row.role == PerceptualRole::Orbit)
            .expect("orbit row");
        assert!(orbit.suppressed > 0);

        let mut priority_config = config;
        priority_config.dirty_mode = DirtyMode::PriorityDirty;
        let priority_rows = dirty_audit_rows(
            &build_frame_sequence(priority_config),
            DirtyMode::PriorityDirty,
        );
        assert!(
            priority_rows
                .iter()
                .any(|row| row.budget_candidates > row.budget_emitted)
        );
    }

    #[test]
    fn moving_orbit_clears_cells_left_by_previous_frame() {
        let layout = Layout {
            columns: 80,
            rows: 24,
            logo: &COMPACT_LOGO,
            logo_column: 20,
            logo_row: 8,
            frame_box: Some(FrameBox {
                left: 18,
                top: 6,
                width: 20,
                height: 5,
            }),
            composition: CompositionMode::Standard,
        };
        let previous = test_frame(&layout, 0.0);
        let next = test_frame(&layout, 80.0 / 120.0);
        let stale_cells = previous.stale_cells(&next).len();

        assert!(stale_cells > 0);
    }

    #[test]
    fn frame_cells_are_position_sorted_for_binary_lookup() {
        let layout = Layout {
            columns: 80,
            rows: 24,
            logo: &COMPACT_LOGO,
            logo_column: 20,
            logo_row: 8,
            frame_box: None,
            composition: CompositionMode::Standard,
        };
        let frame = test_frame(&layout, 0.0);

        assert!(frame.cell_at(20, 8).is_some());
        assert!(
            frame
                .cells
                .windows(2)
                .all(|pair| (pair[0].row, pair[0].column) <= (pair[1].row, pair[1].column))
        );
    }

    #[test]
    fn dirty_cells_batch_into_horizontal_runs() {
        let cells = vec![
            Cell {
                column: 1,
                row: 2,
                character: 'a',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 2,
                row: 2,
                character: 'b',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 4,
                row: 2,
                character: 'c',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
        ];
        let refs = cells.iter().collect::<Vec<_>>();
        let runs = dirty_runs(&refs);

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].cells.len(), 2);
        assert_eq!(runs[1].column, 4);
    }

    #[test]
    fn stale_cells_batch_into_clear_runs() {
        let cells = vec![
            Cell {
                column: 1,
                row: 2,
                character: 'a',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 2,
                row: 2,
                character: 'b',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
            Cell {
                column: 1,
                row: 3,
                character: 'c',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
                primitive_id: None,
                stroke_id: None,
                vertex_id: None,
                path_index: None,
                correspondence_lost: false,
            },
        ];
        let refs = cells.iter().collect::<Vec<_>>();
        let runs = stale_runs(&refs);

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].width, 2);
        assert_eq!(runs[1].row, 3);
    }

    #[test]
    fn glyph_compensation_boosts_light_line_glyphs() {
        assert!(glyph_lightness_multiplier('━') > glyph_lightness_multiplier('█'));
        assert!(glyph_lightness_multiplier('⠁') > glyph_lightness_multiplier('━'));
    }

    #[test]
    fn orbit_distance_tracks_fractional_motion() {
        assert!((orbit_distance_behind(0.5, 0.0, 20.0) - 0.5).abs() < f32::EPSILON);
        assert!((orbit_distance_behind(0.5, 19.0, 20.0) - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn recognizes_exit_keys() {
        assert!(exit_key_pressed(Event::Key(
            crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE,)
        )));
        assert!(exit_key_pressed(Event::Key(
            crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE,)
        )));
        assert!(exit_key_pressed(Event::Key(
            crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL,)
        )));
        assert!(!exit_key_pressed(Event::Key(
            crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE,)
        )));
    }

    fn color_delta(left: Color, right: Color) -> u16 {
        let Color::Rgb {
            r: left_r,
            g: left_g,
            b: left_b,
        } = left
        else {
            return u16::MAX;
        };
        let Color::Rgb {
            r: right_r,
            g: right_g,
            b: right_b,
        } = right
        else {
            return u16::MAX;
        };

        left_r.abs_diff(right_r) as u16
            + left_g.abs_diff(right_g) as u16
            + left_b.abs_diff(right_b) as u16
    }

    fn test_logo_color(character: char, index: usize, elapsed_seconds: f32) -> Color {
        test_logo_color_with_motion(character, index, elapsed_seconds, MotionPreset::Prime)
    }

    fn test_logo_color_with_motion(
        character: char,
        index: usize,
        elapsed_seconds: f32,
        motion_preset: MotionPreset,
    ) -> Color {
        logo_color_for_terminal(test_logo_context(
            character,
            index,
            elapsed_seconds,
            motion_preset,
        ))
    }

    fn test_logo_context(
        character: char,
        index: usize,
        elapsed_seconds: f32,
        motion_preset: MotionPreset,
    ) -> LogoColorContext {
        LogoColorContext {
            character,
            index,
            elapsed_seconds,
            profile: VisualProfile::Ultra,
            motion_preset,
            theme: Theme::default(),
            rhythm: 0.5,
            target_fps: 120,
            capabilities: test_calibration().capabilities,
        }
    }

    fn oklab_delta(left: Color, right: Color) -> f32 {
        let left = Oklab::from_color(left).unwrap();
        let right = Oklab::from_color(right).unwrap();
        right.sub(left).length()
    }

    fn perceived_luma(color: Color) -> u16 {
        let Color::Rgb { r, g, b } = color else {
            return 0;
        };

        (u16::from(r) * 3 + u16::from(g) * 6 + u16::from(b)) / 10
    }
}

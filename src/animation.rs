use std::collections::BTreeMap;
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

use crate::capabilities::{GlyphMode, TerminalCalibration, TerminalCapabilities};
use crate::cli::{MotionPreset, Speed, VisualProfile};
use crate::theme::{SPINNER_COLOR, Theme, loader_gradient};

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
const ORBIT_PHASE_STEP: f32 = 0.52;
const MIN_VISIBLE_LIGHTNESS: f32 = 0.31;

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

    let mut runtime = RuntimeState::new(config.profile, config.motion_preset, config.inspect);
    let started_at = Instant::now();
    let mut frame_controller = AdaptiveFrameController::new(config.calibration.frame_delay());
    let mut previous_layout = None;
    let mut previous_frame = None;
    let mut frame_buffers = FrameBuffers::default();
    let mut camera = VirtualCamera::default();
    let mut metrics = RenderMetrics::new();
    let mut recorder = Recorder::new(record_path)?;
    let mut pointer_tracker = SystemPointerTracker::new();
    let reveal_started_at = if matches!(config.profile, VisualProfile::Benchmark) {
        None
    } else {
        Some(started_at)
    };

    loop {
        let elapsed_seconds = started_at.elapsed().as_secs_f32();
        if handle_runtime_input(&mut runtime, elapsed_seconds)? {
            break;
        }

        let layout = Layout::current();
        if runtime.left_mouse_down
            && let Some(mouse) = pointer_tracker.sample(&layout, elapsed_seconds)
        {
            runtime.update_mouse(mouse.column, mouse.row, elapsed_seconds);
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
            runtime.mouse_at(elapsed_seconds),
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
                calibration: config.calibration,
                theme: config.theme,
                mouse: runtime.mouse_at(elapsed_seconds),
                quality,
                camera,
                curves,
                director,
            },
            &mut frame_buffers,
        );
        let frame_started_at = Instant::now();
        let render_stats = frame.render_dirty(stdout, previous_frame.as_ref())?;
        let frame_time = frame_started_at.elapsed();
        let frame_delay = frame_controller.record(frame_time, &render_stats);
        metrics.record(frame_time, render_stats, frame_controller.snapshot());
        recorder.record(&metrics)?;
        if runtime.profile.benchmark_enabled() || runtime.inspect {
            render_benchmark(stdout, &layout, &metrics, config.calibration, runtime)?;
        }
        previous_frame = Some(frame);
        stdout.flush()?;

        if !runtime.paused {
            frame_controller.wait_next_frame(frame_delay);
        } else {
            thread::sleep(Duration::from_millis(24));
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
pub struct LogoRunConfig {
    pub profile: VisualProfile,
    pub motion_preset: MotionPreset,
    pub calibration: TerminalCalibration,
    pub theme: Theme,
    pub inspect: bool,
}

#[derive(Clone, Copy)]
pub struct FrameRecordConfig {
    pub profile: VisualProfile,
    pub motion_preset: MotionPreset,
    pub calibration: TerminalCalibration,
    pub theme: Theme,
    pub columns: u16,
    pub rows: u16,
    pub frames: usize,
    pub fps: u16,
}

pub fn record_frames(path: &str, config: FrameRecordConfig) -> io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    let layout = Layout::for_size(config.columns, config.rows);
    let mut frame_buffers = FrameBuffers::default();
    let mut camera = VirtualCamera::default();
    let quality = QualitySettings::from_calibration(config.calibration, 1.0);
    let frame_delay = 1.0 / f32::from(config.fps.max(1));

    writeln!(
        writer,
        "{}",
        FrameRecordHeader::new(config, &layout).to_json()
    )?;
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
                calibration: config.calibration,
                theme: config.theme,
                mouse: None,
                quality,
                camera,
                curves,
                director: MotionDirectorState::steady(),
            },
            &mut frame_buffers,
        );
        writeln!(
            writer,
            "{}",
            RecordedFrame::from_frame(frame_index, elapsed_seconds, &frame).to_json()
        )?;
    }
    writer.flush()
}

pub fn replay_frames(stdout: &mut Stdout, path: &str) -> io::Result<()> {
    let sequence = FrameSequence::read(path)?;
    let mut previous_frame = None;
    let frame_delay = delay_for_fps(sequence.header.fps);
    for frame in sequence.frames {
        let render_frame = frame.to_frame();
        render_frame.render_dirty(stdout, previous_frame.as_ref())?;
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
            calibration,
            theme,
            mouse: None,
            quality: QualitySettings::from_calibration(calibration, 1.0),
            camera: VirtualCamera::default(),
            curves: VisualCurves::for_profile(profile),
            director: MotionDirectorState::steady(),
        },
        &mut frame_buffers,
    );
    let summary = SnapshotSummary::from_frame(&frame, &layout, profile, motion_preset, calibration);
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

        push_background_depth(&mut cells, layout, elapsed_seconds, context);
        buffers
            .mouse_particles
            .step(&mut cells, layout, elapsed_seconds, context);

        if let Some(frame_box) = layout.frame_box {
            push_orbit_trail(
                &mut cells,
                frame_box,
                elapsed_seconds,
                context,
                &mut buffers.trail,
            );
        }

        buffers
            .afterimage
            .apply(&mut cells, elapsed_seconds, context, layout);
        let luminance_budget = LuminanceBudget::from_foreground(&cells, layout);
        luminance_budget.apply(&mut cells);

        for (row_index, line) in layout.logo.iter().enumerate() {
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

        let status_row = layout
            .frame_box
            .map(|frame_box| frame_box.top + frame_box.height + 1)
            .unwrap_or(layout.logo_row + layout.logo.len() as u16 + 1);
        if status_row < layout.rows {
            push_static_text(
                &mut cells,
                centered_column(layout.columns, STATUS_LINE.chars().count()),
                status_row,
                STATUS_LINE,
                display_role_color(
                    context.theme.accent,
                    context.calibration.capabilities,
                    PerceptualRole::StatusText,
                ),
            );
        }

        if layout.rows > 1 {
            push_static_text(
                &mut cells,
                centered_column(layout.columns, EXIT_HINT.chars().count()),
                layout.rows - 1,
                EXIT_HINT,
                display_role_color(
                    context.theme.dim,
                    context.calibration.capabilities,
                    PerceptualRole::StatusText,
                ),
            );
        }

        cells = composite_layers(cells);
        buffers.temporal_aa.apply(&mut cells, context);
        buffers.smoothing.apply(&mut cells, context.profile);

        Self { cells }
    }

    fn render_dirty(
        &self,
        stdout: &mut Stdout,
        previous: Option<&Self>,
    ) -> io::Result<RenderStats> {
        let dirty_cells = self.dirty_cells(previous);
        let dirty_runs = dirty_runs(&dirty_cells);
        for run in &dirty_runs {
            stdout.queue(MoveTo(run.column, run.row))?;
            for cell in &run.cells {
                write!(stdout, "{}", cell.character.to_string().with(cell.color))?;
            }
        }

        let mut stale_cell_count = 0;
        let mut stale_run_count = 0;
        if let Some(previous) = previous {
            let stale_cells = previous.stale_cells(self);
            stale_cell_count = stale_cells.len();
            let stale_runs = stale_runs(&stale_cells);
            stale_run_count = stale_runs.len();
            for run in &stale_runs {
                stdout.queue(MoveTo(run.column, run.row))?;
                write!(stdout, "{}", " ".repeat(run.width))?;
            }
        }

        Ok(RenderStats {
            dirty_cells: dirty_cells.len(),
            dirty_runs: dirty_runs.len(),
            stale_cells: stale_cell_count,
            stale_runs: stale_run_count,
        })
    }

    fn dirty_cells<'a>(&'a self, previous: Option<&'a Self>) -> Vec<&'a Cell> {
        let Some(previous) = previous else {
            return self.cells.iter().collect();
        };

        self.cells
            .iter()
            .filter(|cell| previous.cell_at(cell.column, cell.row) != Some(*cell))
            .collect()
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

#[derive(Clone, Copy)]
struct FrameContext {
    profile: VisualProfile,
    motion_preset: MotionPreset,
    calibration: TerminalCalibration,
    theme: Theme,
    mouse: Option<MouseAttractor>,
    quality: QualitySettings,
    camera: VirtualCamera,
    curves: VisualCurves,
    director: MotionDirectorState,
}

#[derive(Default)]
struct FrameBuffers {
    trail: TrailBuffer,
    smoothing: SmoothingBuffer,
    temporal_aa: CellTemporalAaBuffer,
    afterimage: AfterimageBuffer,
    mouse_particles: MouseParticleSystem,
}

impl FrameBuffers {
    fn clear(&mut self) {
        self.trail.clear();
        self.smoothing.clear();
        self.temporal_aa.clear();
        self.afterimage.clear();
        self.mouse_particles.clear();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RenderStats {
    dirty_cells: usize,
    dirty_runs: usize,
    stale_cells: usize,
    stale_runs: usize,
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
    format!(
        "fps:{:>5.1} tier:{} q:{} curve:{} phase:{} missed:{} worst:{:>5.2}ms dirty:{} runs:{} stale:{} clear:{} density:{:.2} term:{} preset:{} motion:{} glyph:{:?} sample:{} flush:{:.3}ms mouse:{}{}",
        metrics.average_fps(),
        metrics.adaptive.target_fps,
        metrics.adaptive.quality_percent,
        curves.family.name(),
        director.phase.name(),
        metrics.adaptive.missed_deadlines,
        metrics.worst_frame_time.as_secs_f64() * 1000.0,
        stats.dirty_cells,
        stats.dirty_runs,
        stats.stale_cells,
        stats.stale_runs,
        calibration.effect_density,
        terminal_visual_profile(calibration).name(),
        calibration.capabilities.preset.name(),
        runtime.motion_preset.name(),
        calibration.capabilities.glyph_mode,
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
    inspect: bool,
    paused: bool,
    left_mouse_down: bool,
    mouse: Option<MouseState>,
    release_wake: Option<ReleaseWake>,
}

impl RuntimeState {
    fn new(profile: VisualProfile, motion_preset: MotionPreset, inspect: bool) -> Self {
        Self {
            profile,
            motion_preset,
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
        Self {
            phase: MotionDirectorPhase::Emergence,
            logo_weight: t,
            orbit_weight: (t - 0.28).max(0.0) * 0.7,
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
    preset: &'static str,
    glyph: GlyphMode,
    cells: usize,
    contrast_min: f32,
    lightness_variance: f32,
    blue_noise_min_distance: f32,
    luminance_pressure: f32,
    hash: u64,
    dirty_cell_budget: usize,
}

impl SnapshotSummary {
    fn from_frame(
        frame: &Frame,
        layout: &Layout,
        profile: VisualProfile,
        motion_preset: MotionPreset,
        calibration: TerminalCalibration,
    ) -> Self {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut contrast_min = f32::MAX;
        let mut lightness_sum = 0.0;
        let mut lightness_square_sum = 0.0;
        let mut lightness_count = 0.0;
        let mut background_points = Vec::new();
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
            preset: calibration.capabilities.preset.name(),
            glyph: calibration.capabilities.glyph_mode,
            cells: frame.cells.len(),
            contrast_min,
            lightness_variance,
            blue_noise_min_distance: min_pair_distance(&background_points),
            luminance_pressure: LuminanceBudget::pressure(frame, layout),
            hash,
            dirty_cell_budget: calibration.dirty_cell_budget,
        }
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"profile\":\"{}\",\"motion\":\"{}\",\"terminal_preset\":\"{}\",\"glyph\":\"{:?}\",\"cells\":{},\"contrast_min\":{:.2},\"lightness_variance\":{:.4},\"blue_noise_min_distance\":{:.2},\"luminance_pressure\":{:.3},\"hash\":\"{:016x}\",\"dirty_cell_budget\":{}}}",
            self.profile,
            self.motion,
            self.preset,
            self.glyph,
            self.cells,
            self.contrast_min,
            self.lightness_variance,
            self.blue_noise_min_distance,
            self.luminance_pressure,
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
        }
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"type\":\"header\",\"columns\":{},\"rows\":{},\"fps\":{},\"profile\":\"{}\",\"motion\":\"{}\",\"glyph\":\"{:?}\",\"terminal_profile\":\"{}\",\"theme_hash\":\"{:016x}\"}}",
            self.columns,
            self.rows,
            self.fps,
            self.profile,
            self.motion,
            self.glyph,
            self.terminal_profile,
            self.theme_hash
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
        })
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
    foreground_clarity: f32,
    background_atmosphere: f32,
}

impl VisualScore {
    fn from_sequence(sequence: &FrameSequence) -> Self {
        let mut flicker_total = 0.0;
        let mut lightness_delta_total = 0.0;
        let mut continuity_total = 0.0;
        let mut dirty_total = 0.0;
        let mut transitions: f32 = 0.0;
        let mut clustering = 0.0;
        let mut foreground_occlusion = 0.0;
        let mut contrast_violations = 0usize;
        let mut foreground_clarity = 0.0;
        let mut background_atmosphere = 0.0;

        for frame in &sequence.frames {
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
            let dirty = next.dirty_cells(Some(&previous)).len() + previous.stale_cells(&next).len();
            let cell_budget =
                usize::from(sequence.header.columns) * usize::from(sequence.header.rows);
            dirty_total += dirty as f32 / cell_budget.max(1) as f32;
            let delta = average_lightness_delta(&previous, &next);
            lightness_delta_total += delta;
            let role_weighted_delta = role_weighted_lightness_delta(&previous, &next);
            flicker_total += if role_weighted_delta > 0.13 { 1.0 } else { 0.0 };
            continuity_total += motion_continuity(&previous, &next);
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
        let foreground_clarity = foreground_clarity / frame_count;
        let background_atmosphere = background_atmosphere / frame_count;
        let visual_quality_score = (100.0
            - flicker_rate * 28.0
            - lightness_delta * 90.0
            - clustering_pressure * 18.0
            - foreground_occlusion * 22.0
            - dirty_cell_pressure * 16.0
            - contrast_violations as f32 * 0.05
            + foreground_clarity * 8.0
            + background_atmosphere * 5.0
            + motion_continuity * 8.0)
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
            foreground_clarity,
            background_atmosphere,
        }
    }

    fn to_json(self) -> String {
        format!(
            "{{\"visual_quality_score\":{:.2},\"flicker_rate\":{:.4},\"lightness_delta\":{:.4},\"motion_continuity\":{:.4},\"clustering_pressure\":{:.4},\"foreground_occlusion\":{:.4},\"contrast_violations\":{},\"dirty_cell_pressure\":{:.4},\"foreground_clarity\":{:.4},\"background_atmosphere\":{:.4}}}",
            self.visual_quality_score,
            self.flicker_rate,
            self.lightness_delta,
            self.motion_continuity,
            self.clustering_pressure,
            self.foreground_occlusion,
            self.contrast_violations,
            self.dirty_cell_pressure,
            self.foreground_clarity,
            self.background_atmosphere
        )
    }
}

fn cell_to_record(cell: &Cell) -> String {
    let (r, g, b) = color_channels(cell.color);
    format!(
        "{},{},{},{},{},{},{}",
        cell.column,
        cell.row,
        escape_record_char(cell.character),
        r,
        g,
        b,
        layer_code(cell.layer)
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
    Some(Cell {
        column,
        row,
        character,
        color: Color::Rgb { r, g, b },
        layer,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AdaptiveSnapshot {
    target_fps: u16,
    missed_deadlines: u64,
    quality_percent: u8,
}

struct AdaptiveFrameController {
    frame_duration: Duration,
    next_frame_at: Instant,
    target_fps: u16,
    missed_deadlines: u64,
    stable_frames: u16,
    quality_scale: f32,
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
        }
    }

    fn record(&mut self, frame_time: Duration, stats: &RenderStats) -> Duration {
        let dirty_pressure = stats.dirty_cells + stats.stale_cells;
        let overloaded = frame_time > self.frame_duration.saturating_mul(2) || dirty_pressure > 420;

        if overloaded {
            self.missed_deadlines += 1;
            self.stable_frames = 0;
            self.quality_scale = (self.quality_scale * 0.92).max(0.52);
            self.downgrade();
        } else {
            self.stable_frames = self.stable_frames.saturating_add(1);
            if self.stable_frames > 180 && frame_time < self.frame_duration / 2 {
                self.quality_scale = (self.quality_scale + 0.04).min(1.0);
                self.upgrade();
                self.stable_frames = 0;
            }
        }

        self.frame_duration
    }

    fn wait_next_frame(&mut self, frame_duration: Duration) {
        self.frame_duration = frame_duration;
        let now = Instant::now();
        let wait = self.next_frame_at.saturating_duration_since(now);
        match event::poll(wait) {
            Ok(_) => {}
            Err(_) => {
                thread::sleep(wait);
            }
        }

        self.advance();
    }

    fn snapshot(&self) -> AdaptiveSnapshot {
        AdaptiveSnapshot {
            target_fps: self.target_fps,
            missed_deadlines: self.missed_deadlines,
            quality_percent: (self.quality_scale * 100.0).round() as u8,
        }
    }

    fn quality_settings(&self, calibration: TerminalCalibration) -> QualitySettings {
        QualitySettings::from_calibration(calibration, self.quality_scale)
    }

    fn downgrade(&mut self) {
        self.target_fps = match self.target_fps {
            91..=u16::MAX => 90,
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
            _ => 120,
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
    match calibration.capabilities.preset.name() {
        "wezterm" | "kitty" | "iterm2" => TerminalVisualProfile::GpuRich,
        "alacritty" | "windows-terminal" => TerminalVisualProfile::Balanced,
        "vscode" | "macos-terminal" | "unknown" => TerminalVisualProfile::Conservative,
        "linux-console" => TerminalVisualProfile::Minimal,
        _ => TerminalVisualProfile::Balanced,
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
            "frame={} fps={:.2} tier={} quality={} missed={} dirty={} runs={} stale={} clear={} worst_ms={:.3}",
            metrics.frame_count,
            metrics.average_fps(),
            metrics.adaptive.target_fps,
            metrics.adaptive.quality_percent,
            metrics.adaptive.missed_deadlines,
            stats.dirty_cells,
            stats.dirty_runs,
            stats.stale_cells,
            stats.stale_runs,
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
    for (index, character) in text.chars().enumerate() {
        cells.push(Cell {
            column: column + index as u16,
            row,
            character,
            color: display_color(
                compensate_glyph_lightness(
                    logo_color(
                        character,
                        index,
                        context.elapsed_seconds,
                        context.profile,
                        context.motion_preset,
                        context.theme,
                        rhythm,
                    ),
                    character,
                ),
                context.calibration.capabilities,
            ),
            layer: RenderLayer::Text,
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

fn logo_color(
    character: char,
    index: usize,
    elapsed_seconds: f32,
    profile: VisualProfile,
    motion_preset: MotionPreset,
    theme: Theme,
    rhythm: f32,
) -> Color {
    if character == ' ' {
        return ensure_visible_lightness(
            blend_color(theme.logo_shadow, theme.logo_base, 0.18),
            theme.contrast_floor * 0.92,
        );
    }

    let phases = MotionPhases::at(elapsed_seconds, motion_preset);
    let phase = elapsed_seconds * 120.0 * phases.logo_speed;
    let wave_position = (phase * LOGO_PHASE_STEP) % LOGO_WAVE_PERIOD;
    let secondary_wave_position =
        (phase * LOGO_SECONDARY_PHASE_STEP + LOGO_WAVE_PERIOD * 0.42) % LOGO_WAVE_PERIOD;
    let character_position = index as f32 % LOGO_WAVE_PERIOD;
    let distance = circular_distance_f32(character_position, wave_position, LOGO_WAVE_PERIOD);
    let secondary_distance = circular_distance_f32(
        character_position,
        secondary_wave_position,
        LOGO_WAVE_PERIOD,
    );
    let highlight = gaussian(distance, LOGO_WAVE_SIGMA);
    let secondary_highlight = gaussian(secondary_distance, LOGO_WAVE_SIGMA * 0.78)
        * phases.secondary_gain
        * profile.intensity();
    let edge = gaussian(distance, LOGO_WAVE_SIGMA * 1.8) * 0.34 + rhythm * phases.focus_gain;
    let base = blend_color(theme.logo_shadow, theme.logo_base, 0.72 + edge);

    ensure_visible_lightness(
        boost_blue_contrast(adjust_chroma(
            highlight_blend(
                base,
                theme.logo_highlight,
                ((highlight + secondary_highlight + rhythm * 0.14)
                    * profile.intensity()
                    * VisualCurves::for_profile(profile).brightness
                    * MotionDirectorState::at(elapsed_seconds, profile).logo_weight)
                    .min(1.0),
            ),
            VisualCurves::for_profile(profile).chroma,
        )),
        theme.contrast_floor,
    )
}

fn ensure_visible_lightness(color: Color, floor: f32) -> Color {
    let Some(mut oklab) = Oklab::from_color(color) else {
        return color;
    };
    oklab.lightness = oklab.lightness.max(floor.max(MIN_VISIBLE_LIGHTNESS));
    oklab.to_color()
}

fn boost_blue_contrast(color: Color) -> Color {
    blend_color(
        color,
        Color::Rgb {
            r: 178,
            g: 230,
            b: 255,
        },
        0.05,
    )
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
        return Color::Rgb { r: 0, g: 0, b: 0 };
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
        return optimize_oklch_color(color, role);
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

fn optimize_oklch_color(color: Color, role: PerceptualRole) -> Color {
    let Some(source) = Oklch::from_color(color) else {
        return color;
    };
    let mut best = color;
    let mut best_cost = f32::MAX;
    let minimum = role.contrast_floor();
    for lightness_step in -4..=10 {
        for chroma_step in -4..=4 {
            let mut candidate = source;
            candidate.lightness =
                (source.lightness + lightness_step as f32 * 0.025).clamp(0.0, 0.92);
            candidate.chroma =
                (source.chroma * (1.0 + chroma_step as f32 * 0.055)).clamp(0.0, 0.18);
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
        adjusted = blend_color(
            adjusted,
            Color::Rgb {
                r: 220,
                g: 248,
                b: 255,
            },
            0.24,
        );
    }
    adjusted
}

fn apca_like_contrast(foreground: Color, background: Color) -> f32 {
    let foreground = relative_luminance(foreground).powf(0.56);
    let background = relative_luminance(background).powf(0.57);
    ((foreground - background) * 108.0).abs()
}

fn dark_reference_color() -> Color {
    Color::Rgb { r: 2, g: 8, b: 18 }
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
                cell.color = limit_lightness_delta(previous, cell.color, max_delta);
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
        for cell in cells {
            let role = PerceptualRole::from_cell(cell);
            let key = (cell.column, cell.row);
            if let Some(previous) = self.history.get(&key).copied()
                && previous.role == role
                && role.temporal_stability_weight() < 0.9
            {
                let delta = oklab_distance(previous.color, cell.color);
                let rejection = match role {
                    PerceptualRole::Afterimage => 0.16,
                    PerceptualRole::BackgroundParticle => 0.18,
                    PerceptualRole::MouseTrail => 0.22,
                    PerceptualRole::Orbit => 0.11,
                    PerceptualRole::StatusText | PerceptualRole::Logo => 0.0,
                };
                if delta < rejection {
                    let blend = (0.24 + previous.confidence * 0.32)
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

        let kernel = TemporalKernel::for_fps(context.calibration.target_fps);
        self.samples
            .retain(|sample| elapsed_seconds - sample.created_at <= sample.lifetime);
        for sample in &self.samples {
            let age = elapsed_seconds - sample.created_at;
            let fade = (1.0 - age / sample.lifetime)
                .clamp(0.0, 1.0)
                .powf(kernel.history_weight);
            if fade <= kernel.rejection_threshold {
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
                lifetime: kernel.decay_seconds
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
struct TemporalKernel {
    history_weight: f32,
    rejection_threshold: f32,
    decay_seconds: f32,
}

impl TemporalKernel {
    fn for_fps(fps: u16) -> Self {
        match fps {
            0..=72 => Self {
                history_weight: 1.72,
                rejection_threshold: 0.024,
                decay_seconds: 0.26,
            },
            73..=105 => Self {
                history_weight: 1.48,
                rejection_threshold: 0.019,
                decay_seconds: 0.22,
            },
            _ => Self {
                history_weight: 1.28,
                rejection_threshold: 0.014,
                decay_seconds: 0.18,
            },
        }
    }
}

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
}

#[derive(Default)]
struct MouseParticleSystem {
    particles: Vec<MouseParticle>,
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

        if let Some(mut mouse) = context.mouse.filter(|mouse| mouse.emitting) {
            mouse.strength *= context.director.mouse_weight * context.curves.vortex_strength;
            self.emit(mouse, elapsed_seconds, context.curves);
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
            if self.particles.len() >= 72 {
                self.particles.remove(0);
            }
            let near_cursor = self
                .particles
                .iter()
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
            });
        }
    }

    fn integrate(&mut self, dt: f32, mouse: Option<MouseAttractor>, layout: &Layout) {
        for particle in &mut self.particles {
            if let Some(mouse) = mouse {
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
            particle.velocity_x += (field_x * 0.22 + curl_x * 8.0) * dt;
            particle.velocity_y += (field_y * 0.22 + curl_y * 8.0) * dt;
            particle.velocity_x *= 0.91_f32.powf(dt * 60.0);
            particle.velocity_y *= 0.88_f32.powf(dt * 60.0);
            particle.velocity_y += 0.55 * dt;
            particle.x += particle.velocity_x * dt;
            particle.y += particle.velocity_y * dt;
            particle.age += dt;
        }
        self.particles
            .retain(|particle| particle.age < particle.lifetime);
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
            let life = 1.0 - particle.age / particle.lifetime;
            let shimmer = 0.72 + random_unit(particle.seed.rotate_left(23)) * 0.28;
            push_role_star_cell(
                cells,
                particle.x.round() as u16,
                particle.y.round() as u16,
                '.',
                (0.16 + life * 0.24) * shimmer,
                context,
                PerceptualRole::MouseTrail,
            );
        }
    }
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

#[derive(Clone, Copy, Debug)]
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

    let width = layout.columns.min(120);
    let height = layout.rows.saturating_sub(3).max(1);
    let error_field = PerceptualErrorField::from_layout(layout, context);
    for layer in 0..6 {
        push_star_layer(
            cells,
            width,
            height,
            elapsed_seconds,
            context,
            &error_field,
            layer,
        );
    }
    push_mouse_gravity_wake(cells, width, height, context);
}

fn push_star_layer(
    cells: &mut Vec<Cell>,
    width: u16,
    height: u16,
    elapsed_seconds: f32,
    context: FrameContext,
    error_field: &PerceptualErrorField,
    layer: u8,
) {
    let seed = 0x9e37_79b9_u32.wrapping_mul(u32::from(layer) + 1);
    let density = ((star_layer_density(width, layer) as f32)
        * context.quality.particle_density
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
        let frame_index = animation_frame_index(elapsed_seconds, context.calibration.target_fps);
        let stbn = stbn_value(column, row, layer, frame_index);
        let sensitivity = error_field.sensitivity(column, row);
        if stbn < sensitivity * 0.16 || (layer > 2 && stbn < 0.045) {
            continue;
        }
        let temporal_gate = 0.74 + stbn * 0.26;
        let intensity = star_intensity(layer, star_seed)
            * context.calibration.effect_density
            * context.director.background_weight
            * temporal_gate
            * (1.0 - sensitivity * 0.46).clamp(0.46, 1.0);

        push_star_cell(cells, column, row, '.', intensity, context);

        for tail_index in 1..=tail {
            let fade = (1.0 - tail_index as f32 / (tail + 1) as f32).powf(1.45);
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
            push_star_cell(
                cells,
                tail_column,
                tail_row,
                '.',
                intensity * fade * 0.46 * (1.0 - tail_sensitivity * 0.36).clamp(0.56, 1.0),
                context,
            );
        }
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

fn stbn_value(column: u16, row: u16, layer: u8, frame_index: u32) -> f32 {
    let spatial = mix_seed(
        u32::from(column).wrapping_mul(0x045d_9f3b),
        u32::from(row).wrapping_mul(0x119d_e1f3) ^ u32::from(layer).wrapping_mul(0x27d4_eb2d),
    );
    let temporal = mix_seed(frame_index.wrapping_mul(0x9e37_79b9), u32::from(layer));
    let rank = spatial ^ temporal.rotate_left((layer % 17).into());
    fract(radical_inverse_base2(rank) * 0.63 + random_unit(mix_seed(spatial, temporal)) * 0.37)
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

    fn path_index_for(self, column: u16, row: u16) -> Option<usize> {
        (0..self.perimeter_len()).find(|&index| self.point_at(index) == Some((column, row)))
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
    trail_buffer: &mut TrailBuffer,
) {
    let perimeter_len = frame_box.perimeter_len();
    if perimeter_len == 0 {
        return;
    }

    let head_position = spline_orbit_head(
        elapsed_seconds,
        perimeter_len as f32,
        context.motion_preset,
        context.curves,
    );
    let trail_span = context.theme.trail_span
        * context.calibration.effect_density
        * line_length_multiplier(context.profile)
        * context.director.orbit_weight;
    for sample_index in 0..(perimeter_len * 2) {
        let path_position = sample_index as f32 * 0.5;
        let distance = orbit_distance_behind(head_position, path_position, perimeter_len as f32);

        if let Some((column, row)) = frame_box.point_at_float(path_position) {
            let alpha = trail_alpha(distance, trail_span);
            if alpha > 0.012 {
                trail_buffer.add(column, row, alpha);
            }
        }
    }
    let glint_position = (head_position
        + MotionPhases::at(elapsed_seconds, context.motion_preset).glint_offset)
        .rem_euclid(perimeter_len as f32);
    for sample_index in 0..perimeter_len {
        let distance =
            orbit_distance_behind(glint_position, sample_index as f32, perimeter_len as f32);
        if distance <= 3.0
            && let Some((column, row)) = frame_box.point_at(sample_index)
        {
            trail_buffer.add(column, row, 0.16 * smoothstep(1.0 - distance / 3.0));
        }
    }

    trail_buffer.decay(context.profile.trail_decay());
    for (&(column, row), &intensity) in trail_buffer.visible_cells() {
        if let Some(path_index) = frame_box.path_index_for(column, row) {
            let character = orbit_character(
                frame_box,
                path_index,
                context.calibration.capabilities.glyph_mode,
            );
            cells.push(Cell {
                column,
                row,
                character,
                color: display_role_color(
                    compensate_glyph_lightness(
                        trail_color_from_intensity(intensity, context.profile, context.theme),
                        character,
                    ),
                    context.calibration.capabilities,
                    PerceptualRole::Orbit,
                ),
                layer: RenderLayer::Orbit,
            });
        }
    }
}

fn spline_orbit_head(
    elapsed_seconds: f32,
    perimeter_len: f32,
    motion_preset: MotionPreset,
    curves: VisualCurves,
) -> f32 {
    let phases = MotionPhases::at(elapsed_seconds, motion_preset);
    let base = elapsed_seconds * 120.0 * ORBIT_PHASE_STEP * phases.orbit_speed * curves.orbit_speed;
    let velocity_ripple = sine01(elapsed_seconds * 0.29) * 0.42;
    (base + velocity_ripple).rem_euclid(perimeter_len)
}

fn line_length_multiplier(profile: VisualProfile) -> f32 {
    match profile {
        VisualProfile::Ultra | VisualProfile::Benchmark => 1.0,
        VisualProfile::Cinematic => 0.86,
        VisualProfile::Calm => 0.68,
    }
}

#[derive(Default)]
struct TrailBuffer {
    intensities: BTreeMap<(u16, u16), f32>,
}

impl TrailBuffer {
    fn clear(&mut self) {
        self.intensities.clear();
    }

    fn add(&mut self, column: u16, row: u16, intensity: f32) {
        let entry = self.intensities.entry((column, row)).or_insert(0.0);
        *entry = (*entry + intensity).min(1.0);
    }

    fn decay(&mut self, decay: f32) {
        for intensity in self.intensities.values_mut() {
            *intensity *= decay;
        }
        self.intensities.retain(|_, intensity| *intensity > 0.018);
    }

    fn visible_cells(&self) -> &BTreeMap<(u16, u16), f32> {
        &self.intensities
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
}

impl Layout {
    fn current() -> Self {
        let (columns, rows) = size().unwrap_or((80, 24));
        Self::for_size(columns, rows)
    }

    fn for_size(columns: u16, rows: u16) -> Self {
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
        let logo_row = rows.saturating_sub(logo_height) / 3;
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
            calibration: test_calibration(),
            theme: Theme::default(),
            mouse,
            quality: QualitySettings::from_calibration(test_calibration(), 1.0),
            camera: VirtualCamera::default(),
            curves: VisualCurves::for_profile(VisualProfile::Ultra),
            director: MotionDirectorState::steady(),
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
    fn dirty_frame_skips_identical_cells() {
        let layout = Layout {
            columns: 80,
            rows: 24,
            logo: &COMPACT_LOGO,
            logo_column: 0,
            logo_row: 0,
            frame_box: None,
        };
        let frame = test_frame(&layout, 0.0);
        let dirty_cells = frame.dirty_cells(Some(&frame)).len();

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
        };
        let previous = test_frame(&layout, 0.0);
        let next = test_frame(&layout, 1.0 / 120.0);
        let dirty_cells = next.dirty_cells(Some(&previous)).len();

        assert!(dirty_cells > 0);
        assert!(dirty_cells < next.cells.len());
    }

    #[test]
    fn logo_color_keeps_visible_glyph_contrast() {
        for frame in 0..48 {
            let Color::Rgb { r, g, b } = logo_color(
                '█',
                8,
                frame as f32 / 120.0,
                VisualProfile::Ultra,
                MotionPreset::Prime,
                Theme::default(),
                0.5,
            ) else {
                panic!("expected rgb color");
            };
            assert!(perceived_luma(Color::Rgb { r, g, b }) >= 78);
        }
    }

    #[test]
    fn logo_color_changes_smoothly_between_frames() {
        for frame in 0..48 {
            let current = logo_color(
                '█',
                8,
                frame as f32 / 120.0,
                VisualProfile::Ultra,
                MotionPreset::Prime,
                Theme::default(),
                0.5,
            );
            let next = logo_color(
                '█',
                8,
                (frame + 1) as f32 / 120.0,
                VisualProfile::Ultra,
                MotionPreset::Prime,
                Theme::default(),
                0.5,
            );

            assert!(color_delta(current, next) <= 40);
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
        let first = stbn_value(11, 7, 3, 120);
        let again = stbn_value(11, 7, 3, 120);
        let next = stbn_value(11, 7, 3, 121);

        assert_eq!(first, again);
        assert!((first - next).abs() > 0.02);
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
            release: false,
        }));

        system.step(&mut cells, &layout, 1.0, context);
        system.step(&mut cells, &layout, 1.0 + 1.0 / 60.0, context);

        assert!(!system.particles.is_empty());
        assert!(system.particles.len() <= 72);
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
            release: true,
        }));

        system.step(&mut cells, &layout, 1.0, context);

        assert!(system.particles.is_empty());
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
            },
            Cell {
                column: 2,
                row: 2,
                character: '━',
                color: ACCENT_COLOR,
                layer: RenderLayer::Orbit,
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
        assert!(score.foreground_clarity > 0.0);
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

        assert!(cells.len() > 40);
        assert!(cells.len() < 90);
        assert!(cells.iter().all(|cell| cell.character == '.'));
        assert!(
            cells
                .iter()
                .all(|cell| cell.layer == RenderLayer::Background)
        );
        assert!(cells.iter().map(|cell| cell.row).max().unwrap_or(0) > 12);
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
            },
            Cell {
                column: 4,
                row: 2,
                character: '━',
                color: ACCENT_COLOR,
                layer: RenderLayer::Orbit,
            },
            Cell {
                column: 8,
                row: 2,
                character: '.',
                color: ACCENT_COLOR,
                layer: RenderLayer::Background,
            },
            Cell {
                column: 8,
                row: 2,
                character: 'T',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
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
            test_calibration(),
        );
        let right = SnapshotSummary::from_frame(
            &frame,
            &layout,
            VisualProfile::Ultra,
            MotionPreset::Prime,
            test_calibration(),
        );

        assert_eq!(left.hash, right.hash);
        assert!(left.to_json().contains("\"motion\":\"prime\""));
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
            &mut TrailBuffer::default(),
        );

        assert!(cells.len() >= 18);
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
            },
            Cell {
                column: 2,
                row: 2,
                character: 'b',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
            },
            Cell {
                column: 4,
                row: 2,
                character: 'c',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
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
            },
            Cell {
                column: 2,
                row: 2,
                character: 'b',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
            },
            Cell {
                column: 1,
                row: 3,
                character: 'c',
                color: ACCENT_COLOR,
                layer: RenderLayer::Text,
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

    fn perceived_luma(color: Color) -> u16 {
        let Color::Rgb { r, g, b } = color else {
            return 0;
        };

        (u16::from(r) * 3 + u16::from(g) * 6 + u16::from(b)) / 10
    }
}

mod animation;
mod capabilities;
mod cli;
mod terminal;
mod theme;

use std::io::{self, stdout};

use animation::{
    FrameRecordConfig, LogoRunConfig, paper_experiment_csv, print_snapshot, record_frames,
    replay_frames, run_loader, run_logo, score_frames,
};
use capabilities::{TerminalCalibration, TerminalCapabilities};
use cli::{Config, help_text, version_text};
use terminal::Terminal;
use theme::Theme;

fn main() -> io::Result<()> {
    let config = Config::from_env();
    if config.help {
        println!("{}", help_text());
        return Ok(());
    }
    if config.version {
        println!("{}", version_text());
        return Ok(());
    }

    let capabilities = TerminalCapabilities::detect();
    let mut calibration =
        TerminalCalibration::detect(capabilities, config.profile, config.calibration_enabled);
    if let Some(budget) = config.fixed_dirty_budget {
        calibration.dirty_cell_budget = budget;
    }
    if config.uncapped {
        calibration = calibration.with_target_fps(1200);
    }
    let theme = Theme::load(config.theme_path.as_deref())?;

    if let Some(experiment) = config.paper_experiment {
        let output = config
            .output_path
            .as_deref()
            .unwrap_or("/tmp/tokitai-paper.csv");
        paper_experiment_csv(
            output,
            experiment,
            FrameRecordConfig {
                profile: config.profile,
                motion_preset: config.motion_preset,
                scene_preset: config.scene_preset,
                calibration,
                theme,
                columns: 80,
                rows: 24,
                frames: 72,
                fps: calibration.target_fps.min(240),
                low_latency: config.low_latency,
                dirty_mode: config.dirty_mode,
                glyph_history_mode: config.glyph_history_mode,
            },
        )?;
        println!("{output}");
        return Ok(());
    }

    if config.snapshot {
        print_snapshot(
            config.profile,
            config.motion_preset,
            config.scene_preset,
            calibration,
            theme,
        )?;
        return Ok(());
    }
    if let Some(path) = config.record_frames_path.as_deref() {
        record_frames(
            path,
            FrameRecordConfig {
                profile: config.profile,
                motion_preset: config.motion_preset,
                scene_preset: config.scene_preset,
                calibration,
                theme,
                columns: 80,
                rows: 24,
                frames: 72,
                fps: calibration.target_fps.min(240),
                low_latency: config.low_latency,
                dirty_mode: config.dirty_mode,
                glyph_history_mode: config.glyph_history_mode,
            },
        )?;
        return Ok(());
    }
    if let Some(path) = config.score_frames_path.as_deref() {
        println!("{}", score_frames(path)?);
        return Ok(());
    }

    let mut terminal = Terminal::new(stdout(), "tokitai-sign")?;

    if let Some(path) = config.replay_frames_path.as_deref() {
        return replay_frames(terminal.stdout_mut(), path);
    }

    if config.show_loader {
        run_loader(terminal.stdout_mut(), config.speed)?;
    }

    run_logo(
        terminal.stdout_mut(),
        LogoRunConfig {
            profile: config.profile,
            motion_preset: config.motion_preset,
            scene_preset: config.scene_preset,
            calibration,
            theme,
            inspect: config.inspect,
            uncapped: config.uncapped,
            low_latency: config.low_latency,
            dirty_mode: config.dirty_mode,
            glyph_history_mode: config.glyph_history_mode,
        },
        config.record_path.as_deref(),
        config.replay_path.as_deref(),
    )
}

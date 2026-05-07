//! macOS menu-bar (tray) icon for Verbatim.
//!
//! Idle: shows the bundled app logo.
//! Recording: replaces the icon with an animated 5-bar audio waveform
//! driven by live mic RMS, and the dropdown's status row counts up the
//! recording duration in `M:SS`.
//! Processing: replaces the icon with a rotating spinner so the user can
//! see the app is still working after they stop recording.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use image::GenericImageView;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

use crate::state::AppState;

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/32x32.png");
const WAVEFORM_BARS: usize = 5;
const SPINNER_SPOKES: usize = 12;

pub struct TrayHandles {
    /// Toggled by the STT-event forwarding loop in main.rs to drive the
    /// waveform animation while recording.
    pub is_recording: Arc<AtomicBool>,
    /// Toggled by the STT-event forwarding loop while the app is
    /// transcribing / post-processing — drives the spinner animation.
    pub is_processing: Arc<AtomicBool>,
    /// Status row in the dropdown ("Idle" / "Recording…" / "Processing…").
    /// Kept behind a Mutex because tauri menu items are not Sync across the
    /// async tasks that update them.
    pub status_item: Arc<Mutex<MenuItem<tauri::Wry>>>,
    /// "Start recording" / "Stop recording" menu item — text flips based on
    /// the current recording state.
    pub toggle_item: Arc<Mutex<MenuItem<tauri::Wry>>>,
}

pub fn install(
    app: &AppHandle,
    recording_level: Arc<std::sync::atomic::AtomicU32>,
) -> tauri::Result<TrayHandles> {
    let img = image::load_from_memory(TRAY_ICON_BYTES)
        .expect("bundled tray icon must decode");
    let (width, height) = img.dimensions();
    let base_rgba: Vec<u8> = img.to_rgba8().into_raw();

    let base_icon = Image::new_owned(base_rgba.clone(), width, height);

    let status_item = MenuItem::with_id(app, "status", "Idle", false, None::<&str>)?;
    let open_item = MenuItem::with_id(app, "open", "Open Verbatim", true, None::<&str>)?;
    let toggle_item = MenuItem::with_id(app, "toggle", "Start recording", true, None::<&str>)?;
    let copy_recent_item = MenuItem::with_id(app, "copy_recent", "Copy recent recording", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Verbatim", true, Some("Cmd+Q"))?;

    let menu = MenuBuilder::new(app)
        .item(&status_item)
        .separator()
        .item(&open_item)
        .item(&toggle_item)
        .item(&copy_recent_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let tray = TrayIconBuilder::with_id("verbatim-tray")
        .icon(base_icon.clone())
        .icon_as_template(false)
        .tooltip("Verbatim")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "toggle" => {
                let state = app.state::<AppState>();
                if let Err(e) = state
                    .stt_cmd_tx
                    .send(verbatim_core::app::SttCommand::ToggleRecording)
                {
                    tracing::warn!("failed to send ToggleRecording from tray: {}", e);
                }
            }
            "copy_recent" => {
                let state = app.state::<AppState>();
                let result = {
                    let db = match state.db.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            tracing::warn!("copy_recent: db lock failed: {}", e);
                            return;
                        }
                    };
                    db.get_recent(1)
                };
                match result {
                    Ok(rows) => match rows.into_iter().next() {
                        Some(t) => {
                            if let Err(e) = verbatim_core::clipboard::copy_to_clipboard(&t.text) {
                                tracing::warn!("copy_recent: clipboard write failed: {}", e);
                            } else {
                                tracing::debug!(chars = t.text.len(), "copy_recent: copied most recent transcription");
                            }
                        }
                        None => {
                            tracing::debug!("copy_recent: no recordings to copy");
                        }
                    },
                    Err(e) => {
                        tracing::warn!("copy_recent: db query failed: {}", e);
                    }
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    let is_recording = Arc::new(AtomicBool::new(false));
    let is_processing = Arc::new(AtomicBool::new(false));
    let status_arc = Arc::new(Mutex::new(status_item));
    let toggle_arc = Arc::new(Mutex::new(toggle_item));

    // Animation task: ~15 fps. While recording, drive 5 bars off the live
    // mic RMS — each bar holds a smoothed copy of recent levels, sampled
    // through a different lag, so they appear to dance independently. The
    // status row in the dropdown is updated to "Recording M:SS" on each
    // whole-second tick. While processing (transcribing / post-processing),
    // render a rotating spinner so it's clear the app is still working.
    let is_rec_for_anim = is_recording.clone();
    let is_proc_for_anim = is_processing.clone();
    let tray_for_anim = tray.clone();
    let base_rgba_for_anim = base_rgba.clone();
    let level_for_anim = recording_level.clone();
    let status_for_anim = status_arc.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(66));
        let mut last_was_recording = false;
        let mut last_was_processing = false;
        let mut spinner_phase: usize = 0;
        let mut started_at: Option<Instant> = None;
        let mut last_secs_shown: u64 = u64::MAX;
        let mut bar_levels = [0.0f32; WAVEFORM_BARS];
        let mut history = [0.0f32; 16];
        let mut hist_idx: usize = 0;
        loop {
            interval.tick().await;
            let recording = is_rec_for_anim.load(Ordering::Relaxed);
            let processing = !recording && is_proc_for_anim.load(Ordering::Relaxed);
            if recording {
                if !last_was_recording {
                    started_at = Some(Instant::now());
                    last_secs_shown = u64::MAX;
                }

                let raw_bits = level_for_anim.load(Ordering::Relaxed);
                let raw_rms = f32::from_bits(raw_bits).clamp(0.0, 1.0);
                let scaled = (raw_rms * 14.0).powf(0.55).min(1.0);

                history[hist_idx] = scaled;
                hist_idx = (hist_idx + 1) % history.len();

                let lags = [0usize, 2, 4, 6, 8];
                let smooth = [0.55f32, 0.50, 0.45, 0.40, 0.35];
                for i in 0..WAVEFORM_BARS {
                    let slot = (hist_idx + history.len() - lags[i]) % history.len();
                    let target = history[slot];
                    let alpha = smooth[i];
                    bar_levels[i] = bar_levels[i] * (1.0 - alpha) + target * alpha;
                }

                let frame_rgba = compose_waveform_frame(width, height, &bar_levels);
                let frame = Image::new_owned(frame_rgba, width, height);
                if let Err(e) = tray_for_anim.set_icon(Some(frame)) {
                    tracing::warn!("tray.set_icon (waveform frame) failed: {}", e);
                }

                // Update status row when whole-second boundary crosses.
                if let Some(start) = started_at {
                    let secs = start.elapsed().as_secs();
                    if secs != last_secs_shown {
                        last_secs_shown = secs;
                        let label = format!("Recording {}:{:02}", secs / 60, secs % 60);
                        let status = status_for_anim.clone();
                        tauri::async_runtime::spawn(async move {
                            let item = status.lock().await;
                            let _ = item.set_text(&label);
                        });
                    }
                }

                last_was_recording = true;
                last_was_processing = false;
            } else if processing {
                if last_was_recording {
                    bar_levels = [0.0; WAVEFORM_BARS];
                    history = [0.0; 16];
                    started_at = None;
                    last_secs_shown = u64::MAX;
                }
                spinner_phase = (spinner_phase + 1) % SPINNER_SPOKES;
                let frame_rgba = compose_spinner_frame(width, height, spinner_phase);
                let frame = Image::new_owned(frame_rgba, width, height);
                if let Err(e) = tray_for_anim.set_icon(Some(frame)) {
                    tracing::warn!("tray.set_icon (spinner frame) failed: {}", e);
                }
                last_was_recording = false;
                last_was_processing = true;
            } else if last_was_recording || last_was_processing {
                bar_levels = [0.0; WAVEFORM_BARS];
                history = [0.0; 16];
                started_at = None;
                last_secs_shown = u64::MAX;
                spinner_phase = 0;
                let base = Image::new_owned(base_rgba_for_anim.clone(), width, height);
                if let Err(e) = tray_for_anim.set_icon(Some(base)) {
                    tracing::warn!("tray.set_icon (idle reset) failed: {}", e);
                }
                last_was_recording = false;
                last_was_processing = false;
            }
        }
    });

    Ok(TrayHandles {
        is_recording,
        is_processing,
        status_item: status_arc,
        toggle_item: toggle_arc,
    })
}

/// Render a 12-spoke spinner where the spoke at `phase` is brightest and
/// older spokes (going counter-clockwise) fade out — the classic macOS
/// `BeachBall`-style indeterminate progress indicator.
fn compose_spinner_frame(width: u32, height: u32, phase: usize) -> Vec<u8> {
    let w = width as i32;
    let h = height as i32;
    let mut out = vec![0u8; (width * height * 4) as usize];

    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let r_outer = (w.min(h) as f32) * 0.42;
    let r_inner = r_outer * 0.45;
    let spoke_half_w = ((w.min(h) as f32) * 0.07).max(1.0);

    for s in 0..SPINNER_SPOKES {
        // Spoke `phase` is brightest; intensity falls off going backwards
        // around the dial.
        let age = (SPINNER_SPOKES + phase - s) % SPINNER_SPOKES;
        let intensity = 1.0 - (age as f32 / SPINNER_SPOKES as f32);
        let alpha = (60.0 + 195.0 * intensity).clamp(0.0, 255.0) as u8;

        // Angle 0 = straight up, increasing clockwise.
        let theta = (s as f32) * std::f32::consts::TAU / (SPINNER_SPOKES as f32);
        let dx = theta.sin();
        let dy = -theta.cos();
        // Perpendicular direction for spoke thickness.
        let px = -dy;
        let py = dx;

        let steps = 24;
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let r = r_inner + (r_outer - r_inner) * t;
            let bx = cx + dx * r;
            let by = cy + dy * r;
            for w_off in (-spoke_half_w as i32)..=(spoke_half_w as i32) {
                let ox = (bx + px * w_off as f32).round() as i32;
                let oy = (by + py * w_off as f32).round() as i32;
                if ox < 0 || oy < 0 || ox >= w || oy >= h {
                    continue;
                }
                let idx = ((oy as u32 * width + ox as u32) * 4) as usize;
                out[idx] = 255;
                out[idx + 1] = 255;
                out[idx + 2] = 255;
                if out[idx + 3] < alpha {
                    out[idx + 3] = alpha;
                }
            }
        }
    }

    out
}

/// Render a 5-bar waveform on a transparent canvas in solid white. Bar
/// heights come from caller-supplied normalized levels (0.0..1.0).
fn compose_waveform_frame(width: u32, height: u32, levels: &[f32; WAVEFORM_BARS]) -> Vec<u8> {
    let w = width as i32;
    let h = height as i32;
    let mut out = vec![0u8; (width * height * 4) as usize];

    let bar_w = (w / 8).max(2);
    let gap = (bar_w / 2).max(1);
    let total_w = WAVEFORM_BARS as i32 * bar_w + (WAVEFORM_BARS as i32 - 1) * gap;
    let x0 = (w - total_w) / 2;
    let mid_y = h / 2;
    let max_half_h = (h as f32 * 0.42) as i32;
    let min_half_h = (h as f32 * 0.08) as i32;

    for (i, level) in levels.iter().enumerate() {
        let n = level.clamp(0.0, 1.0);
        let half_h = min_half_h + ((max_half_h - min_half_h) as f32 * n) as i32;

        let bx = x0 + i as i32 * (bar_w + gap);
        let by_top = (mid_y - half_h).max(0);
        let by_bot = (mid_y + half_h).min(h - 1);

        for y in by_top..=by_bot {
            for dx in 0..bar_w {
                let x = bx + dx;
                if x < 0 || x >= w {
                    continue;
                }
                let idx = ((y as u32 * width + x as u32) * 4) as usize;
                out[idx] = 255;
                out[idx + 1] = 255;
                out[idx + 2] = 255;
                out[idx + 3] = 255;
            }
        }
    }

    out
}

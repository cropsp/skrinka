#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod capture;
mod config;
mod dialog;
mod hotkeys;
mod icon;
mod output;
mod overlay;
mod settings;
mod strings;
mod text;
mod tray;
mod util;
mod version;
mod window;

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use log::{error, info, LevelFilter};
use winit::event_loop::EventLoop;

use crate::app::{App, UserEvent};
use crate::config::Config;

/// The log file is truncated at startup when it grows beyond this size.
const LOG_MAX_BYTES: u64 = 2 * 1024 * 1024;

fn main() {
    // Installed before anything else: release builds have no console, so the log is the only
    // place a panic can be seen.
    install_panic_hook();

    // A second instance must not touch the log or steal the hotkeys.
    if !acquire_single_instance() {
        return;
    }

    if let Err(e) = run() {
        let text = format!("{e:#}");
        error!("fatal: {text}");
        // The event loop is gone (or never started), so a blocking dialog is fine here.
        dialog::error(&text);
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let dir = config::app_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    init_logging(&dir.join("skrinka.log"))?;

    let config_path = config::config_path()?;
    info!("{} starting", version::full());
    info!("config file: {}", config_path.display());
    let config = Config::load_or_create(&config_path)?;

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .context("cannot create the event loop")?;
    let proxy = Arc::new(Mutex::new(event_loop.create_proxy()));
    let mut app = App::new(config, config_path, proxy);
    event_loop.run_app(&mut app).context("event loop failed")?;

    if let Some(text) = app.take_fatal_error() {
        anyhow::bail!(text);
    }
    info!("Skrinka exiting");
    Ok(())
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_owned());
        let message = info.payload_as_str().unwrap_or("<non-string panic payload>");
        error!("panic at {location}: {message}");
        default_hook(info);
    }));
}

/// Returns false when another Skrinka is already running.
fn acquire_single_instance() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;

    // The handle is never closed on purpose: the mutex has to live as long as the process.
    match unsafe { CreateMutexW(None, false, w!("Local\\Skrinka.SingleInstance")) } {
        Ok(_handle) => unsafe { GetLastError() != ERROR_ALREADY_EXISTS },
        // Without a guard we still run; refusing to start would be worse.
        Err(_) => true,
    }
}

fn init_logging(path: &Path) -> anyhow::Result<()> {
    let too_big = fs::metadata(path).is_ok_and(|m| m.len() > LOG_MAX_BYTES);
    let file = if too_big {
        File::create(path)
    } else {
        OpenOptions::new().create(true).append(true).open(path)
    }
    .with_context(|| format!("cannot open log file {}", path.display()))?;

    let mut log_config = simplelog::ConfigBuilder::new();
    log_config.set_time_format_rfc3339();
    let _ = log_config.set_time_offset_to_local();
    simplelog::WriteLogger::init(LevelFilter::Info, log_config.build(), file)
        .context("cannot initialise logging")?;
    Ok(())
}

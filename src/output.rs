//! Everything that happens to a finished screenshot. Runs on a worker thread so the event loop
//! never waits for PNG encoding, the clipboard or a dialog.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;

use anyhow::Context;
use chrono::format::StrftimeItems;
use chrono::{DateTime, Local};
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use log::{error, info, warn};

use crate::app::{send_event, SharedProxy, UserEvent};
use crate::config::{Config, DEFAULT_FILENAME_FORMAT};
use crate::dialog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Clipboard, then a PNG in `save_dir`.
    ClipboardAndAutoSave,
    ClipboardOnly,
    /// "Save as" dialog; clipboard and PNG only if the user picks a file.
    SaveAs,
}

#[derive(Debug)]
pub struct WorkerResult {
    pub action: Action,
    pub saved_path: Option<PathBuf>,
    pub error: Option<String>,
}

pub fn spawn(image: RgbaImage, action: Action, config: Config, proxy: SharedProxy) {
    thread::spawn(move || {
        let mut errors = Vec::new();
        // Clipboard first: the user can paste while the PNG is still being encoded.
        let saved_path = match action {
            Action::ClipboardAndAutoSave => {
                keep_error(&mut errors, set_clipboard(&image));
                keep_error(&mut errors, auto_save(&image, &config))
            }
            Action::ClipboardOnly => {
                keep_error(&mut errors, set_clipboard(&image));
                None
            }
            Action::SaveAs => match ask_save_path(&config) {
                Some(path) => {
                    keep_error(&mut errors, set_clipboard(&image));
                    keep_error(&mut errors, save_as(&image, path))
                }
                None => {
                    info!("save as cancelled");
                    None
                }
            },
        };
        // Freed before reporting back, so the main thread can hand the memory back to Windows.
        drop(image);
        let error = (!errors.is_empty()).then(|| errors.join("\n\n"));
        if let Some(text) = &error {
            error!("{action:?} failed: {text}");
            dialog::error(text);
        }
        send_event(&proxy, UserEvent::WorkerDone(WorkerResult { action, saved_path, error }));
    });
}

fn keep_error<T>(errors: &mut Vec<String>, result: anyhow::Result<T>) -> Option<T> {
    result.map_err(|e| errors.push(format!("{e:#}"))).ok()
}

fn set_clipboard(image: &RgbaImage) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut clipboard = arboard::Clipboard::new().context("cannot open the clipboard")?;
    clipboard
        .set_image(arboard::ImageData {
            width: image.width() as usize,
            height: image.height() as usize,
            bytes: Cow::Borrowed(image.as_raw()),
        })
        .context("cannot copy the image to the clipboard")?;
    info!(
        "copied {}x{} to the clipboard in {} ms",
        image.width(),
        image.height(),
        started.elapsed().as_millis()
    );
    Ok(())
}

fn auto_save(image: &RgbaImage, config: &Config) -> anyhow::Result<PathBuf> {
    let dir = &config.save_dir;
    fs::create_dir_all(dir).with_context(|| format!("cannot create the folder {}", dir.display()))?;
    let name = format_filename(&config.filename_format, &Local::now());
    let (path, file) = create_unique(dir, &name)
        .with_context(|| format!("cannot create {}", dir.join(&name).display()))?;
    save_png(image, file, &path)?;
    Ok(path)
}

/// The native "Save as" dialog, opened in the last "Save as" folder (else `save_dir`) with the
/// generated name. `None` if the user cancels.
fn ask_save_path(config: &Config) -> Option<PathBuf> {
    let dir = if config.last_save_as_dir.is_dir() {
        &config.last_save_as_dir
    } else {
        // The dialog falls back to some other folder if this one does not exist yet.
        let _ = fs::create_dir_all(&config.save_dir);
        &config.save_dir
    };
    rfd::FileDialog::new()
        .set_title("Save screenshot")
        .set_directory(dir)
        .set_file_name(format_filename(&config.filename_format, &Local::now()))
        .add_filter("PNG image", &["png"])
        .save_file()
}

/// Saves where the user chose; the dialog has already asked about overwriting.
fn save_as(image: &RgbaImage, path: PathBuf) -> anyhow::Result<PathBuf> {
    let path = with_png_extension(path);
    let file = File::create(&path).with_context(|| format!("cannot create {}", path.display()))?;
    save_png(image, file, &path)?;
    Ok(path)
}

fn with_png_extension(path: PathBuf) -> PathBuf {
    let is_png = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
    if is_png {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".png");
        PathBuf::from(name)
    }
}

/// Encodes into an already created file; removes the file again if that fails.
fn save_png(image: &RgbaImage, file: File, path: &Path) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut writer = BufWriter::new(file);
    let result = PngEncoder::new(&mut writer)
        .write_image(image.as_raw(), image.width(), image.height(), ExtendedColorType::Rgba8)
        .map_err(anyhow::Error::from)
        .and_then(|()| writer.flush().map_err(anyhow::Error::from));
    drop(writer);
    if let Err(e) = result {
        let _ = fs::remove_file(path);
        return Err(e.context(format!("cannot save {}", path.display())));
    }
    info!(
        "saved {} ({}x{}) in {} ms",
        path.display(),
        image.width(),
        image.height(),
        started.elapsed().as_millis()
    );
    Ok(())
}

/// File name from a strftime template, safe for Windows, always ending in `.png`.
pub fn format_filename(template: &str, now: &DateTime<Local>) -> String {
    // chrono panics while formatting an invalid template, so it is validated first.
    let items = StrftimeItems::new(template).parse().unwrap_or_else(|_| {
        warn!("invalid filename_format {template:?}; using {DEFAULT_FILENAME_FORMAT:?}");
        StrftimeItems::new(DEFAULT_FILENAME_FORMAT).parse().expect("the default template is valid")
    });
    let mut formatted = String::new();
    if write!(formatted, "{}", now.format_with_items(items.iter())).is_err() {
        warn!("cannot format filename_format {template:?}; using {DEFAULT_FILENAME_FORMAT:?}");
        formatted = now.format(DEFAULT_FILENAME_FORMAT).to_string();
    }
    let name = sanitize(&formatted);
    if name.to_lowercase().ends_with(".png") {
        name
    } else {
        format!("{name}.png")
    }
}

fn sanitize(raw: &str) -> String {
    let replaced: String = raw
        .chars()
        .map(|c| if c.is_control() || r#"\/:*?"<>|"#.contains(c) { '-' } else { c })
        .collect();
    let trimmed = replaced.trim_matches(|c: char| c.is_whitespace() || c == '.');
    if trimmed.is_empty() {
        "Screenshot".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Creates `dir/name`, or `name (2).png`, `name (3).png`, … when it is taken. Creating the file
/// (instead of only checking that it is free) keeps two captures in the same second apart.
pub fn create_unique(dir: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    for n in 1..=10_000 {
        let candidate = if n == 1 { name.to_owned() } else { format!("{stem} ({n}){extension}") };
        let path = dir.join(candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other(format!("too many files named like {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 7, 16, 9, 5, 3).unwrap()
    }

    #[test]
    fn default_template() {
        assert_eq!(format_filename(DEFAULT_FILENAME_FORMAT, &at()), "Screenshot 2026-07-16 09-05-03.png");
    }

    #[test]
    fn invalid_template_falls_back_to_default() {
        assert_eq!(format_filename("%Q%", &at()), "Screenshot 2026-07-16 09-05-03.png");
        assert_eq!(format_filename("broken %", &at()), "Screenshot 2026-07-16 09-05-03.png");
    }

    #[test]
    fn forbidden_characters_are_replaced() {
        assert_eq!(format_filename("Screenshot at %H:%M:%S", &at()), "Screenshot at 09-05-03.png");
        assert_eq!(format_filename(r#"a\b/c*d?e"f<g>h|i"#, &at()), "a-b-c-d-e-f-g-h-i.png");
        assert_eq!(format_filename("tab\there", &at()), "tab-here.png");
    }

    #[test]
    fn whitespace_and_dots_are_trimmed() {
        assert_eq!(format_filename("  .shot %Y.  ", &at()), "shot 2026.png");
        assert_eq!(format_filename(" ... ", &at()), "Screenshot.png");
        assert_eq!(format_filename("", &at()), "Screenshot.png");
    }

    #[test]
    fn png_extension_is_added_once() {
        assert_eq!(format_filename("shot", &at()), "shot.png");
        assert_eq!(format_filename("shot.png", &at()), "shot.png");
        assert_eq!(format_filename("shot.PNG", &at()), "shot.PNG");
        assert_eq!(format_filename("shot.jpg", &at()), "shot.jpg.png");
    }

    #[test]
    fn save_as_path_gets_png_extension() {
        assert_eq!(with_png_extension(PathBuf::from(r"D:\a\shot")), PathBuf::from(r"D:\a\shot.png"));
        assert_eq!(with_png_extension(PathBuf::from(r"D:\a\shot.PNG")), PathBuf::from(r"D:\a\shot.PNG"));
        assert_eq!(with_png_extension(PathBuf::from(r"D:\a\shot.jpg")), PathBuf::from(r"D:\a\shot.jpg.png"));
        assert_eq!(with_png_extension(PathBuf::from(r"D:\a\v1.2")), PathBuf::from(r"D:\a\v1.2.png"));
    }

    #[test]
    fn unique_names_get_a_counter() {
        let dir = std::env::temp_dir().join(format!("skrinka-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let names: Vec<String> = (0..3)
            .map(|_| {
                let (path, _file) = create_unique(&dir, "Screenshot 1.png").unwrap();
                path.file_name().unwrap().to_string_lossy().into_owned()
            })
            .collect();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, ["Screenshot 1.png", "Screenshot 1 (2).png", "Screenshot 1 (3).png"]);
    }
}

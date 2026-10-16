//! Build info for `src/version.rs` (build number, commit, build date) and, on Windows, the exe's
//! icon, version resource and manifest. Git problems never fail the build.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Common controls v6: themed controls and the `SysLink` class of the Settings window. No DPI
/// section: winit makes the process per-monitor DPI aware (v2) at runtime.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency><dependentAssembly>
    <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
                      processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
  </dependentAssembly></dependency>
</assembly>
"#;

const ICON: &str = "assets/skrinka.ico";

fn main() -> std::io::Result<()> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    git_info(&root);

    println!("cargo:rerun-if-changed=build.rs");
    // Keeps the "-dirty" flag current while editing; git only touches the index on `git add`.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    println!("cargo:rustc-env=SKRINKA_BUILD_DATE={}", build_date());

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        resources(&root)?;
    }
    Ok(())
}

/// `SKRINKA_BUILD_NUMBER` (commit count), `SKRINKA_GIT_SHA`, `SKRINKA_GIT_DIRTY`, and the files
/// that change when HEAD moves, so cargo reruns this script then.
fn git_info(root: &Path) {
    let git = |args: &[&str]| -> Option<String> {
        let output = Command::new("git").args(args).current_dir(root).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let info = (|| {
        let number = git(&["rev-list", "--count", "HEAD"])?;
        let sha = git(&["rev-parse", "--short=7", "HEAD"])?;
        let dirty = !git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty();
        Some((number, sha, dirty))
    })();
    let (number, sha, dirty) = info.unwrap_or_else(|| ("0".to_owned(), "unknown".to_owned(), false));
    println!("cargo:rustc-env=SKRINKA_BUILD_NUMBER={number}");
    println!("cargo:rustc-env=SKRINKA_GIT_SHA={sha}");
    println!("cargo:rustc-env=SKRINKA_GIT_DIRTY={}", if dirty { "1" } else { "0" });

    // In a worktree, HEAD and the index are in the worktree's git dir, refs in the common one.
    let dir = |arg: &str| git(&["rev-parse", arg]).map(|d| root.join(d));
    let mut watched = Vec::new();
    if let Some(git_dir) = dir("--git-dir") {
        watched.push(git_dir.join("HEAD"));
        watched.push(git_dir.join("index"));
    }
    if let Some(common) = dir("--git-common-dir") {
        watched.push(common.join("packed-refs"));
        if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
            watched.push(common.join(branch));
        }
    }
    // A missing path would make cargo rerun the script on every build.
    for path in watched.iter().filter(|p| p.exists()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

/// UTC date, `YYYY-MM-DD`, from `SOURCE_DATE_EPOCH` if set (reproducible builds), else now.
fn build_date() -> String {
    let seconds = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
        });
    let (y, m, d) = civil_from_days(seconds.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Icon, version info (FileVersion/ProductVersion come from the package version) and manifest,
/// compiled with rc.exe from the Windows SDK.
fn resources(root: &Path) -> std::io::Result<()> {
    let mut res = winresource::WindowsResource::new();
    // `cargo run --example gen_icon` creates the icon, and building it runs this script first.
    if root.join(ICON).exists() {
        res.set_icon(ICON);
    } else {
        println!("cargo:warning={ICON} is missing; run `cargo run --example gen_icon`");
    }
    res.set("FileDescription", "Skrinka screenshot tool")
        .set("ProductName", "Skrinka")
        .set("LegalCopyright", "Copyright © 2026 Serhii")
        .set_manifest(MANIFEST);
    res.compile()
}

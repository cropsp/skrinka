//! Version and build info; `build.rs` sets the `SKRINKA_*` variables.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_NUMBER: &str = env!("SKRINKA_BUILD_NUMBER");
const GIT_SHA: &str = env!("SKRINKA_GIT_SHA");
const GIT_DIRTY: &str = env!("SKRINKA_GIT_DIRTY");
const BUILD_DATE: &str = env!("SKRINKA_BUILD_DATE");

/// "Skrinka 0.3.0"
pub fn title() -> String {
    format!("Skrinka {VERSION}")
}

/// "Build 72 · a1b2c3d · 2026-10-20"
pub fn build_line() -> String {
    format_build_line(BUILD_NUMBER, GIT_SHA, GIT_DIRTY == "1", BUILD_DATE)
}

/// "Skrinka 0.3.0 (Build 72 · a1b2c3d · 2026-10-20)"
pub fn full() -> String {
    format!("{} ({})", title(), build_line())
}

fn format_build_line(number: &str, sha: &str, dirty: bool, date: &str) -> String {
    let dirty = if dirty { "-dirty" } else { "" };
    format!("Build {number} · {sha}{dirty} · {date}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_line_format() {
        assert_eq!(format_build_line("72", "a1b2c3d", false, "2026-10-20"), "Build 72 · a1b2c3d · 2026-10-20");
        assert_eq!(
            format_build_line("72", "a1b2c3d", true, "2026-10-20"),
            "Build 72 · a1b2c3d-dirty · 2026-10-20"
        );
        assert_eq!(format_build_line("0", "unknown", false, "1970-01-01"), "Build 0 · unknown · 1970-01-01");
    }

    #[test]
    fn full_contains_title_and_build_line() {
        assert_eq!(full(), format!("Skrinka {VERSION} ({})", build_line()));
        assert!(build_line().starts_with("Build "));
    }
}

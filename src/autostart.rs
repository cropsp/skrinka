//! "Run on startup": a value in the per-user Run key.

use std::io;

use anyhow::Context;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Skrinka";

pub fn is_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|key| key.get_raw_value(VALUE_NAME))
        .is_ok()
}

pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(RUN_KEY)
        .with_context(|| format!(r"cannot open HKCU\{RUN_KEY}"))?;
    if enabled {
        let exe = std::env::current_exe().context("cannot find the path of skrinka.exe")?;
        key.set_value(VALUE_NAME, &format!("\"{}\"", exe.display()))
            .context("cannot write the autostart registry value")?;
    } else {
        match key.delete_value(VALUE_NAME) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).context("cannot remove the autostart registry value"),
        }
    }
    Ok(())
}

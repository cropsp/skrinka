# TASK v3: Skrinka 0.3.0 (foundation) and 0.4.0 (annotation editor)

> Read `CLAUDE.md`, then `README.md`, then this file. `TASK.md` (v1) and `TASK-v2.md` (v2) are the
> earlier specs; their rules on coordinates/DPI, threading, pitfalls and verification still apply.
> Every crate version, API name and Win32 signature below was checked against the crate sources
> (Cargo.lock pins) or Microsoft docs on 2026-10-16. Where this file says "verified", trust it over
> memory; where it says "check", read the source before using it.

This task has **two parts with two releases**. Finish Part A completely (tag `v0.3.0`, CI release
green) before starting Part B (tag `v0.4.0`). Commit per step as in v1/v2. Stop and ask the owner
only where this file says so.

## 0. Scope

**Part A — 0.3.0:** publish to GitHub (`cropsp/skrinka`, public) with CI and tag releases; build
number/commit hash/build date in the binary; a native Windows **Settings** window that replaces
editing `config.toml` in Notepad and shows the version; tray menu cleanup; exe icon and file
description.

**Part B — 0.4.0:** the **annotation editor** (arrows, text, rectangles, numbered badges,
highlighter, pixelate, pen; select/move/delete; undo/redo), opened from the region overlay or from
"Edit last screenshot" in the tray.

**Still out of scope:** uploads, JPG, Ukrainian UI (planned later; keep all strings in one place
so it is easy), cross-monitor selection, settings tabs, a selection resize for annotations (move
only), curved arrows, stickers, crop. **Never auto-update or download anything.**

**Hard constraints (unchanged):** software rendering only, no GUI toolkit, no GPU; exe stays a few
MB (expect ≈ 2.0 MB after Part B); idle memory after use at the v2 level; all shortcuts by
**physical key** (`PhysicalKey::Code(..)`), never by `logical_key`, because the owner switches
between English and Ukrainian layouts.

---

# PART A — 0.3.0

## A1. Repository hygiene (before anything is pushed)

1. `git status` clean, on `main`, tags `v0.1.0` and `v0.2.0` present.
2. **Rewrite the commit author/committer e-mail** to the owner's GitHub noreply address, which is
   already public in his other repo (`cropsp/MovaCore`): `66942372+cropsp@users.noreply.github.com`,
   name `cropsp`. Nothing has been pushed yet, so this is free. Preserve tags:
   ```
   git filter-branch -f --env-filter '
     export GIT_AUTHOR_NAME="cropsp"; export GIT_AUTHOR_EMAIL="66942372+cropsp@users.noreply.github.com"
     export GIT_COMMITTER_NAME="cropsp"; export GIT_COMMITTER_EMAIL="66942372+cropsp@users.noreply.github.com"
   ' --tag-name-filter cat -- --all
   git for-each-ref --format="%(refname)" refs/original/ | ForEach-Object { git update-ref -d $_ }
   git log --all --format="%an <%ae> | %cn <%ce>" | sort -u      # must show only the noreply address
   git config user.name cropsp
   git config user.email 66942372+cropsp@users.noreply.github.com
   ```
   (`git filter-branch` is built in; `git-filter-repo` is fine too if Python is available. Keep the
   `Co-Authored-By` trailers as they are.) Check `git tag -n1` still lists both tags after the rewrite.
3. **Scrub personal paths and machine details** from `README.md`, `CLAUDE.md`, `TASK.md`,
   `TASK-v2.md`: the local project folder, the local install folder, machine names, "the owner
   runs a copy from …". Replace with neutral wording ("the folder you installed it in"). Run
   `git grep -n -I -E "ai-projects|D:\\\\|@gmail\.com|[Tt]oken|[Ss]ecret"` and fix every hit.
   Old revisions keep the paths; that is acceptable (they are not secrets).
4. Move `TASK.md` → `docs/design/TASK-v1.md`, `TASK-v2.md` → `docs/design/TASK-v2.md`, this file →
   `docs/design/TASK-v3.md` (`git mv`), and fix the references in `CLAUDE.md`.
5. `.gitignore`:
   ```
   /target
   *.rs.bk
   *.pdb
   rustc-ice-*.txt
   /dist
   .claude/settings.local.json
   ```
6. `Cargo.toml` metadata (verified against the manifest reference; category slugs exist on crates.io):
   ```toml
   [package]
   name = "skrinka"
   version = "0.3.0"
   edition = "2021"
   description = "Lightweight Windows screenshot tool: hotkey, region select, clipboard or file"
   repository = "https://github.com/cropsp/skrinka"
   license = "MIT"
   readme = "README.md"
   keywords = ["screenshot", "screen-capture", "windows", "clipboard", "tray"]
   categories = ["multimedia::images", "os::windows-apis", "gui"]
   publish = false
   ```
   Keep `Cargo.lock` committed (binary crate); CI builds with `--locked`.
7. `LICENSE` — MIT, same as MovaCore: `gh repo license view MIT > LICENSE` (run in Git Bash or
   `cmd /c`, not Windows PowerShell 5.1, which writes UTF-16), then replace `[year] [fullname]`
   with `2026 Serhii`. README gets a "License" section stating MIT and that it covers all
   versions including v0.1.0 and v0.2.0.
   ScreenCloud (GPL-2.0) imposes nothing: Skrinka took no code, strings, icons or docs from it,
   only the same default hotkeys, the strftime-template idea and one RGB value, none of which is
   protected expression. Keep it that way; "inspired by ScreenCloud" in README is fine.
8. `THIRD-PARTY-LICENSES.html` generated once with cargo-about (release 0.9.x) and committed:
   ```
   cargo install --locked --features cli cargo-about
   cargo about init
   # about.toml: accepted = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "Zlib", "Unicode-3.0", "ISC", "MPL-2.0"]
   #             targets  = ["x86_64-pc-windows-msvc"]
   cargo about generate about.hbs > THIRD-PARTY-LICENSES.html
   ```
   If a licence is rejected, add it to `accepted` only if it is permissive; stop and ask otherwise.
   Dependency licences are all permissive (winit Apache-2.0; rfd and winreg MIT; the rest
   MIT OR Apache-2.0; fontdue also Zlib).
9. `CHANGELOG.md` in Keep a Changelog 1.1.0 format: `## [Unreleased]`, then `## [0.2.0] - 2026-10-16`
   and `## [0.1.0] - 2026-10-16` written from the tag messages, git log and README; link refs at
   the bottom (`[0.2.0]: https://github.com/cropsp/skrinka/compare/v0.1.0...v0.2.0`). Part A's
   changes go under `[Unreleased]` until the release step moves them to `[0.3.0]`.

Commit: "Prepare the repository for publishing".

## A2. Build info in the binary

Same convention as MovaCore's settings header: a big `Skrinka 0.3.0` and under it
`Build 72 · a1b2c3d · 2026-10-20` (build number = commit count, short hash, UTC build date; a
dirty tree shows `a1b2c3d-dirty`).

### A2.1 `build.rs` (own code, no new dependency)

- Run `git rev-list --count HEAD` → build number; `git rev-parse --short=7 HEAD` → sha;
  `git status --porcelain --untracked-files=no` → dirty if any output; `git rev-parse --git-dir`
  → the git dir (handles worktrees); `git symbolic-ref -q HEAD` → current branch ref.
- If git is missing or any command fails: build number `0`, sha `unknown`, dirty `0`. The build
  must never fail because of git.
- Build date: `SOURCE_DATE_EPOCH` if set, else `SystemTime::now()`; convert seconds to a civil
  date with the ~10-line days-to-civil algorithm (no chrono in build.rs). Format `YYYY-MM-DD`.
- Emit `cargo:rustc-env=SKRINKA_BUILD_NUMBER=…`, `SKRINKA_GIT_SHA=…`, `SKRINKA_GIT_DIRTY=0|1`,
  `SKRINKA_BUILD_DATE=…`.
- Emit `cargo:rerun-if-changed=` for: `build.rs`, `<gitdir>/HEAD`, `<gitdir>/refs/heads/<branch>`
  (from the symbolic ref), `<gitdir>/packed-refs`, `<gitdir>/index`. (Once any rerun-if-changed is
  emitted, Cargo stops rerunning the script on every build, so these lines are what keeps the
  values fresh.)
- Also in `build.rs`, Windows only: embed the exe icon, version info and the common-controls
  manifest with **`winresource = "0.1"`** (0.1.34, 2026-10-12; uses `rc.exe` from the Windows SDK,
  present with the MSVC Build Tools and on `windows-latest`). Verified API:
  `winresource::WindowsResource::new()`, `.set_icon("assets/skrinka.ico")`,
  `.set("FileDescription", "Skrinka screenshot tool")`, `.set("ProductName", "Skrinka")`,
  `.set("LegalCopyright", "Copyright © 2026 Serhii")`, `.set_manifest(MANIFEST)`, `.compile()?`.
  `FileVersion`/`ProductVersion` default to `package.version`. The manifest (needed for themed
  controls and the `SysLink` class in A3):
  ```xml
  <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
  <assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <dependency><dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
                        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly></dependency>
  </assembly>
  ```
  Do **not** add a DPI-awareness section: winit sets Per-Monitor V2 at runtime and a manifest value
  would only duplicate it.
- `assets/skrinka.ico`: generate once from the existing tray icon generator (`tray::icon_rgba`,
  generalised to a size parameter) at 16, 32, 48 and 256 px with the `image` crate's ICO encoder
  (`image` feature `ico`, as a **dev-dependency** or behind a `--features gen-icon` example:
  `cargo run --example gen_icon`). Commit the `.ico`.

### A2.2 `src/version.rs`

```rust
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub fn title() -> String      // "Skrinka 0.3.0"
pub fn build_line() -> String // "Build 72 · a1b2c3d · 2026-10-20"  ("-dirty" appended to the sha when SKRINKA_GIT_DIRTY == "1")
pub fn full() -> String       // "Skrinka 0.3.0 (Build 72 · a1b2c3d · 2026-10-20)"
```
Use `full()` in the startup log line and `title()` in the tray tooltip. Unit-test `build_line()`
formatting with fake values (make the formatting a pure function of its inputs).

## A3. Settings window (native Win32 dialog, own thread)

### A3.1 Decision and why

A **modal dialog built from an in-memory `DLGTEMPLATEEX`** and run with
`DialogBoxIndirectParamW` **on its own `std::thread`**, using only the `windows` crate we already
have. Reasons: dialogs created from templates are scaled automatically per monitor in a
Per-Monitor-V2 process (the owner has two monitors), the look is native (same as MovaCore's
WinForms window), and it adds ≈ 12 KB to the exe. `native-windows-gui` (unmaintained since 2022,
confirmed `cbSize` bug) and `winsafe` (no `WM_DPICHANGED` handling) were evaluated and rejected.

`windows` features to add: `Win32_UI_Controls`, `Win32_UI_Input_KeyboardAndMouse`,
`Win32_UI_HiDpi`, `Win32_UI_Shell_Common` (for IFileDialog if used directly — see A3.6),
`Win32_System_Com`, `Win32_System_LibraryLoader`. Already present: `Win32_Foundation`,
`Win32_UI_WindowsAndMessaging`, `Win32_Graphics_Gdi`, `Win32_UI_Shell`, `Win32_System_Threading`.

### A3.2 Layout (one page, no tabs; dialog units; font "Segoe UI" 9 pt via `DS_SETFONT`)

```
┌ Skrinka — Settings ───────────────────────────────────────────┐
│ [icon 32px]  Skrinka 0.3.0                (title font ×1.6, semibold)
│              Build 72 · a1b2c3d · 2026-10-20   (grey)
│ ┌ Hotkeys ───────────────────────────────────────────────────┐ │
│ │ Capture region        [Shift+Alt+2        ] [ Change ]      │ │
│ │ Capture full screen   [Shift+Alt+1        ] [ Change ]      │ │
│ │ Capture window        [Shift+Alt+3        ] [ Change ]      │ │
│ └────────────────────────────────────────────────────────────┘ │
│ ┌ Saving ────────────────────────────────────────────────────┐ │
│ │ Folder          [C:\Users\…\Pictures\Screenshots] [Browse…]│ │
│ │ File name       [Screenshot %Y-%m-%d %H-%M-%S     ]         │ │
│ │ strftime codes: %Y year, %m month, %d day, %H:%M:%S (grey)  │ │
│ └────────────────────────────────────────────────────────────┘ │
│ ┌ Capture ───────────────────────────────────────────────────┐ │
│ │ Full screen captures   [Monitor under the cursor      ▾]    │ │
│ │ Window captures        [Visible window (with title bar) ▾]  │ │
│ │ Notifications          [After saving a file           ▾]    │ │
│ └────────────────────────────────────────────────────────────┘ │
│ [x] Run on startup                                             │
│ github.com/cropsp/skrinka (SysLink)        [ Save ] [ Cancel ] │
└────────────────────────────────────────────────────────────────┘
```
Combo texts ↔ config values: fullscreen `cursor_monitor` = "Monitor under the cursor",
`all_monitors` = "All monitors"; window `visible` = "Visible window (with title bar)", `client` =
"Window contents only"; notifications `saves` = "After saving a file", `all` = "After every
capture", `off` = "Off".

### A3.3 Template builder (verified facts)

- There is **no `DLGTEMPLATEEX` struct in `windows` 0.62**; write the words into a `Vec<u16>`
  stored inside a `Vec<u32>` (the whole template must be DWORD-aligned):
  header `dlgVer=1, signature=0xFFFF, helpID=0, exStyle, style, cDlgItems, x, y, cx, cy, menu=0,
  windowClass=0, title (UTF-16, NUL), pointsize=9, weight=400, italic=0, charset=1, typeface
  "Segoe UI"`; style `WS_POPUP|WS_CAPTION|WS_SYSMENU|DS_MODALFRAME|DS_SETFONT|DS_FIXEDSYS`
  (`DS_SHELLFONT` is not defined in the crate; `DS_SETFONT|DS_FIXEDSYS` is its value). Do not use
  `DS_CENTER` (it centres on the primary monitor): in `WM_INITDIALOG` centre the dialog on the work
  area of the monitor under the cursor (`GetCursorPos` → `MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)`
  → `GetMonitorInfoW(..).rcWork` → `SetWindowPos`).
  Each item, DWORD-aligned: `helpID=0, exStyle, style, x, y, cx, cy, id (DWORD), class, title,
  extraCount=0`. Class atoms: `0xFFFF,0x0080` Button, `0x0081` Edit, `0x0082` Static, `0x0085`
  ComboBox; `SysLink` by name. Group boxes are Buttons with `BS_GROUPBOX`.
- Run: `DialogBoxIndirectParamW(Some(hinstance), template.as_ptr() as *const DLGTEMPLATE, None, Some(dlg_proc), LPARAM(state_ptr))`
  → `isize`. **Owner must be `None`**: a winit window as owner would attach the two threads' input
  queues. The dialog manager then handles Tab, Enter (default button = Save), Esc (= Cancel).
- Thread start: `SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)` (harmless,
  inherited anyway), `CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)`,
  `InitCommonControlsEx(&INITCOMMONCONTROLSEX { dwSize, dwICC: ICC_LINK_CLASS | ICC_STANDARD_CLASSES })`.
- Dialog proc essentials: `WM_INITDIALOG` (fill controls, `SetWindowLongPtrW(hdlg, GWLP_USERDATA, lparam)`,
  set the title font, set the icon), `WM_COMMAND` (buttons, `IDOK`/`IDCANCEL`), `WM_NOTIFY`
  (`NM_CLICK`/`NM_RETURN` from the SysLink → `ShellExecuteW(None, w!("open"), url, None, None, SW_SHOWNORMAL)`),
  `WM_CTLCOLORSTATIC` (grey build line: return a brush, `SetTextColor`), `WM_DPICHANGED` (recreate
  the title font from `SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS, …, GetDpiForWindow)`
  after `SetDialogControlDpiChangeBehavior(title, DCDC_DISABLE_FONT_UPDATE, DCDC_DISABLE_FONT_UPDATE)`;
  return 0 so the dialog manager still rescales everything else).
- Header icon: a `STATIC` with `SS_ICON` (0x3) + `STM_SETICON`; create the `HICON` from the icon
  generator at 32·dpi/96 px via `CreateIconIndirect` with a 32-bit DIB (or `LoadIconWithScaleDown`
  from the embedded resource id 1, since A2 embeds the `.ico`).
- Title font: `CreateFontIndirectW` from `lfMessageFont` with height ×1.6 and `FW_SEMIBOLD`.

### A3.4 Hotkey fields

A read-only `EDIT` showing the current hotkey plus a **Change** button (MovaCore pattern).
Clicking Change puts that field into capture mode: text becomes `Press keys…`, the field gets focus,
and the field is subclassed (`SetWindowSubclass`/`DefSubclassProc` from `Win32_UI_Shell`):
- `WM_GETDLGCODE` → `DLGC_WANTALLKEYS | DLGC_WANTCHARS`;
- `WM_KEYDOWN`/`WM_SYSKEYDOWN` with a non-modifier key: build the string from `GetKeyState` of
  Ctrl/Shift/Alt/Win plus the key; require **at least one modifier** unless the key is F1–F24 or
  PrintScreen; PrintScreen arrives only on `WM_KEYUP`, so handle it there; Esc cancels capture
  (restores the old text), Backspace clears the field (= hotkey disabled);
- swallow `WM_CHAR`/`WM_SYSCHAR` so nothing is typed and nothing beeps;
- leave capture mode after a key is accepted or on focus loss.

**String format must round-trip through `global_hotkey::hotkey::HotKey::from_str`** (parser verified
in global-hotkey 0.8.0 `hotkey.rs`): modifiers `Ctrl`, `Shift`, `Alt`, `Super` (the parser has no
`Win` token — write `Super`, display it as "Win" if you like but store `Super`), joined with `+`, key
last. Accepted key tokens include `A`–`Z`, `0`–`9`, `F1`–`F24`, `Space`, `Enter`, `Tab`,
`Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `PrintScreen`, `ScrollLock`,
`Pause`, `Escape`, `Up/Down/Left/Right`, `Num0`–`Num9`, `NumAdd`, `NumSubtract`, `NumMultiply`,
`NumDivide`, `NumDecimal`, `NumEnter`, `Minus`, `Equal`, `BracketLeft`, `BracketRight`, `Semicolon`,
`Quote`, `Backquote`, `Backslash`, `Comma`, `Period`, `Slash`. Map VK codes to these names in one
table; **unit-test that every string the table can produce parses**, and that the three defaults
round-trip. Reject (keep old value, `MessageBeep`) a key with no mapping.

**While the dialog is open the main thread unregisters the global hotkeys** (otherwise they swallow
the keys being recorded and trigger captures) and re-registers on close. Avoid `&`-accelerators on
the dialog's buttons for the same reason.

### A3.5 Validation and the save flow

Thread → main through the existing `EventLoopProxy` (clone it; `EventLoopProxy<T>` is `Send`).
New events: `SettingsSaved(Config)`, `SettingsClosed`.

- Before `EndDialog(IDOK)` the dialog validates: folder non-empty; file-name template passes
  `StrftimeItems::new(t).parse()` (expose `output::is_valid_template`); each hotkey parses (or is
  empty = disabled); hotkeys are distinct. On failure: `MessageBoxW` with the reason, stay open.
- Main on `SettingsSaved(new)`: if `new != self.config` → `Config::save`, `self.config = new`,
  re-register hotkeys (problems → the existing warning dialog), `autostart::set_enabled` if changed,
  then a tray balloon **"Settings saved and applied."** (reuse `Tray::notify`, ignore the
  `notifications` setting for this one). Then re-register hotkeys in any case.
- Main on `SettingsClosed`: re-register hotkeys.
- Single instance: keep the dialog `HWND` in an `AtomicIsize` in `App`; a second "Settings…" click
  does `ShowWindow(SW_RESTORE)` + `SetForegroundWindow` instead of opening another. On Quit, if a
  dialog is open: `PostMessageW(hdlg, WM_CLOSE, ..)` (never `SendMessage` across threads).
- While the dialog is open, captures started from the tray menu are ignored too (the overlay is
  always-on-top and would cover the dialog); the hotkeys are unregistered anyway.

### A3.6 Folder picker

`rfd::FileDialog::new().set_directory(current).pick_folder()` called on the dialog thread (rfd does
its own COM init per call). Disable the dialog with `EnableWindow(hdlg, false)` while the picker
is up and re-enable after, since without a parent handle the picker is not modal to the dialog.
(If you prefer the direct `IFileOpenDialog` with `FOS_PICKFOLDERS` and `Show(Some(hdlg))`, that is
≈ 25 lines and properly modal; either is fine.)

### A3.7 Tray menu after A3

```
Capture region        Shift+Alt+2
Capture full screen   Shift+Alt+1
Capture window        Shift+Alt+3
───────────
Open screenshots folder
Settings…
───────────
[Debug: dump capture]   (debug builds only)
Quit
```
Removed: "Open config file", "Reload config", "Run on startup" (all inside Settings now). The config
file stays hand-editable; README says hand edits apply at the next start or after opening and
saving Settings. Double-click on the icon still opens the folder. Part B adds "Edit last screenshot".

## A4. GitHub and CI

1. `gh --version`; `gh auth status`. If gh is missing: `winget install --id GitHub.cli --source winget`,
   then open a **new terminal window**. If not authenticated: **stop and ask the owner** to run
   `gh auth login --web --git-protocol https` himself. Never use `--with-token`, never ask for or
   store a token. Then `gh auth setup-git`; `gh api user --jq .login` must print `cropsp`.
2. Create and push (commit A1–A3 first):
   ```
   gh repo create skrinka --public --source=. --remote=origin --push --description "Lightweight Windows screenshot tool: hotkey, region select, clipboard or file"
   git push origin --tags            # `gh repo create --push` pushes HEAD only, not tags (verified in gh source)
   gh repo edit --add-topic rust,windows,screenshot,screen-capture,tray-app
   gh repo view --json defaultBranchRef --jq .defaultBranchRef.name    # main
   ```
3. Workflow `.github/workflows/ci.yml` (action versions verified: actions/checkout v7,
   dtolnay/rust-toolchain `@stable`, Swatinem/rust-cache v2, softprops/action-gh-release v3;
   `windows-latest` = Windows Server 2025 with MSVC and rustup). **`fetch-depth: 0` is mandatory**
   or `git rev-list --count` returns 1 and every CI build is "Build 1".
   ```yaml
   name: CI
   on:
     push: { branches: [main], tags: ["v*"] }
     pull_request:
     workflow_dispatch:
   permissions: { contents: read }
   env: { CARGO_TERM_COLOR: always }
   jobs:
     check:
       name: Build, test, clippy
       runs-on: windows-latest
       steps:
         - uses: actions/checkout@v7
           with: { fetch-depth: 0 }
         - uses: dtolnay/rust-toolchain@stable
           with: { components: clippy }
         - uses: Swatinem/rust-cache@v2
           with: { shared-key: win-x64, save-if: "${{ github.ref == 'refs/heads/main' }}" }
         - run: cargo build --release --locked
         - run: cargo test --locked
         - run: cargo clippy --release --all-targets --locked -- -D warnings
     release:
       name: Publish GitHub Release
       needs: check
       if: startsWith(github.ref, 'refs/tags/v')
       runs-on: windows-latest
       permissions: { contents: write }
       steps:
         - uses: actions/checkout@v7
           with: { fetch-depth: 0 }
         - uses: dtolnay/rust-toolchain@stable
         - uses: Swatinem/rust-cache@v2
           with: { shared-key: win-x64, save-if: false }
         - run: cargo build --release --locked
         - name: Package
           shell: pwsh
           run: |
             $tag = $env:GITHUB_REF_NAME; $stage = "skrinka-$tag-windows-x64"
             New-Item -ItemType Directory -Force -Path dist, $stage | Out-Null
             Copy-Item target/release/skrinka.exe dist/skrinka.exe
             Copy-Item target/release/skrinka.exe, README.md, LICENSE, CHANGELOG.md, THIRD-PARTY-LICENSES.html $stage
             Compress-Archive -Path $stage -DestinationPath "dist/$stage.zip"
             Get-FileHash dist/skrinka.exe, "dist/$stage.zip" -Algorithm SHA256 | ForEach-Object { "$($_.Hash)  $(Split-Path $_.Path -Leaf)" } | Set-Content dist/SHA256SUMS.txt
         - name: Release notes from CHANGELOG.md
           shell: bash
           run: |
             ver="${GITHUB_REF_NAME#v}"
             awk -v ver="$ver" '/^## \[/ { if (found) exit; if (index($0, "## [" ver "]") == 1) { found = 1; next } } /^\[[^]]+\]: / { if (found) exit } found { print }' CHANGELOG.md > RELEASE_NOTES.md
             [ -s RELEASE_NOTES.md ] || echo "Release $GITHUB_REF_NAME" > RELEASE_NOTES.md
         - uses: softprops/action-gh-release@v3
           with:
             name: Skrinka ${{ github.ref_name }}
             body_path: RELEASE_NOTES.md
             fail_on_unmatched_files: true
             files: |
               dist/skrinka.exe
               dist/skrinka-${{ github.ref_name }}-windows-x64.zip
               dist/SHA256SUMS.txt
   ```
   Note: the local toolchain may be older than CI's stable clippy; run `rustup update` locally
   before relying on `-D warnings` parity.
4. README additions: Download section pointing at Releases; the SmartScreen note (unsigned exe →
   "Windows protected your PC" → More info → Run anyway; or build it yourself with
   `cargo install --git https://github.com/cropsp/skrinka --locked`); SHA256SUMS mention; Settings
   window; the new build line; License section.
5. Push `main`, wait for the `check` job to go green (`gh run watch`), fix if not.
6. Release: move `[Unreleased]` to `## [0.3.0] - <date>` in CHANGELOG.md, commit
   "Release v0.3.0", `git tag -a v0.3.0 -m "Skrinka 0.3.0: settings window, build info, GitHub releases"`,
   `git push origin main v0.3.0`, `gh run watch`, then `gh release view v0.3.0` must list
   `skrinka.exe`, the zip and `SHA256SUMS.txt`. Download the exe from the release and check that
   its Properties → Details show the description and version, and that its Settings window shows
   a build line with the CI commit count (not "Build 1").

## A5. Part A acceptance

1. `git log` shows only the noreply e-mail; `git grep` finds no personal paths.
2. Startup log: `Skrinka 0.3.0 (Build N · sha · date) starting`; tray tooltip `Skrinka 0.3.0`.
3. Settings opens from the tray on the monitor where the cursor is, looks native (Segoe UI,
   themed controls), Tab cycles controls, Enter = Save, Esc = Cancel, the GitHub link opens the
   browser. Open it on monitor 2: same size and crisp.
4. Change → press `Ctrl+Shift+F9` → field shows `Ctrl+Shift+F9`; Save → the hotkey works
   immediately; log shows re-registration; config.toml updated. Pressing a bare letter while
   recording is rejected; Esc restores; Backspace clears (hotkey disabled, menu label says so).
5. While Settings is open, `Shift+Alt+2` does nothing (hotkeys unregistered); after Cancel it
   works again.
6. Browse… picks a folder; an invalid template (`%Q%`) is rejected with a message; the balloon
   "Settings saved and applied." appears after Save.
7. Second "Settings…" click brings the existing dialog to front. Quit with the dialog open closes it.
8. Explorer shows the exe with the Skrinka icon and "Skrinka screenshot tool" in Details.
9. CI green on main; the v0.3.0 Release has three assets; the released exe runs.
10. Idle memory unchanged after opening/closing Settings a few times (the dialog thread exits).

---

# PART B — 0.4.0: the annotation editor

## B1. Decisions

- **Rendering:** `tiny-skia = { version = "=0.12.0", default-features = false, features = ["std", "simd"] }`
  (+≈262 KB measured) for anti-aliased vector shapes, fontdue (existing) for text. egui was
  measured at +2.2–4 MB plus a software backend three versions behind; rejected.
- **Everything stays vector until export.** Shapes can be selected, moved, recoloured, resized
  in S/M/L, deleted; undo/redo over all of it. Pixelate/redact are the only raster operations and
  apply to the base image only (never to shapes above it).
- **One normal window** (decorated, resizable, taskbar icon), canvas on a dark backdrop with a
  fixed 44 px (× scale factor) toolbar at the bottom. The image is shown at 100 % when it fits the
  monitor's work area, else scaled down to fit (uniform). No zoom in this version.
- **Keyboard-first**, all by physical key; text entry uses `KeyEvent.text`.
- **Same output keys as the overlay:** Enter = clipboard + auto-save + close; Ctrl+C = clipboard +
  close; Ctrl+S = Save as (+ clipboard) + close. Esc is layered (B5.6).

## B2. Modules and data model

```
src/editor/mod.rs        EditorSession: window, surface, view, document, tool state, text entry, toolbar
src/editor/document.rs   Shape enum, Document, History (undo/redo), export()
src/editor/render.rs     shape → tiny-skia paths; text halo/label; pixelate; arrow head geometry
src/editor/toolbar.rs    layout, hit-testing, icon glyphs, tooltips, readout
src/editor/text_entry.rs caret, insert/delete, Enter/Shift+Enter, paste
```

```rust
pub enum Tool { Select, Arrow, Text, Rect, Number, Highlight, Pixelate, Pen }
pub enum Size { S, M, L }
pub struct Style { color: u8 /* palette index 0..8 */, size: Size }
pub enum ArrowKind { Arrow, Line, DoubleArrow }
pub enum RectKind { Outline, Ellipse, Filled }
pub enum TextKind { Halo, Label }
pub enum PixKind { Pixelate, Redact }

pub enum Shape {                              // all coordinates in IMAGE pixels (f32)
    Arrow     { a: Pt, b: Pt, kind: ArrowKind, style: Style },
    Rect      { r: RectF, kind: RectKind, style: Style },
    Text      { at: Pt, text: String, kind: TextKind, style: Style },   // `at` = top-left of the text box
    Number    { at: Pt, n: u32, tail: Option<Pt>, style: Style },
    Highlight { r: RectF, style: Style },
    Pixelate  { r: RectF, kind: PixKind, style: Style },
    Pen       { pts: Vec<Pt>, style: Style },
}
pub struct Document { base: tiny_skia::Pixmap, shapes: Vec<Shape>, next_number: u32 }
pub struct History { past: Vec<(Vec<Shape>, u32)>, future: Vec<(Vec<Shape>, u32)> }  // snapshots; cap 100
```
A snapshot is pushed before every committed change (new shape, move end, delete, recolour,
resize, text commit). Undo pops to `past`, pushes to `future`; any new change clears `future`.
Undo of a Number gives its number back (`next_number` is part of the snapshot).

## B3. Rendering pipeline (measured on a 2.1 GHz Xeon, 1920×1080)

- `base` (Pixmap, RGBA premultiplied — the screenshot is opaque so premultiplied == straight).
- `flat` = `base` with Pixelate/Redact shapes applied; recomputed only when that set changes
  (pixelate 600×400 at 12 px blocks: 0.8 ms).
- `committed` = `flat` + all shapes except the one being drawn/dragged/edited; recomputed on every
  commit, undo, move end (50 arrows 1.9 ms; 40 rects/ellipses 4.9 ms).
- `view` = `committed` scaled to the canvas (`draw_pixmap` with `FilterQuality::Bilinear` when
  scale < 1 — 14–17 ms, acceptable because it only happens on commit/resize; at 100 % skip the
  draw and convert directly), converted to `0x00RRGGBB` `Vec<u32>` (1–2 ms). Cache it.
- Per frame: copy `view` into the softbuffer buffer, draw the **live** element (the shape being
  drawn/dragged, the selection outline + handles, the text caret) into a small transparent Pixmap
  the size of its bounding box and alpha-blend it into the buffer, draw the toolbar, `present()`.
  A full-frame present costs ≈ 1–2 ms; `present_with_damage` is an optional optimisation.
- **Never wrap the softbuffer buffer in `PixmapMut`**: its byte order is B,G,R,0 (red/blue swap
  and alpha 0 break blending) and softbuffer requires the top byte to be 0.
- Verified tiny-skia 0.12 API: `Pixmap::new(w,h)`, `Pixmap::from_vec(Vec<u8>, IntSize)`,
  `data()/data_mut()`, `PixmapMut::from_bytes(&mut [u8], w, h)` (RGBA premultiplied, stride = w·4),
  `PathBuilder::{new, move_to, line_to, quad_to, cubic_to, close, push_rect, push_oval, push_circle, finish}`,
  `PathBuilder::{from_rect, from_oval, from_circle}`, `Paint { shader, blend_mode, anti_alias, .. }`
  with `set_color_rgba8`, `BlendMode::{SourceOver, Multiply, Source, ..}`,
  `Stroke { width, line_cap: LineCap::{Butt,Round,Square}, line_join: LineJoin::{Miter,Round,Bevel}, dash: Option<StrokeDash>, .. }`,
  `StrokeDash::new(Vec<f32>, offset)`, `FillRule::{Winding, EvenOdd}`, `Transform::{from_scale, from_translate, post_translate, invert, map_point}`,
  `fill_path(&Path, &Paint, FillRule, Transform, Option<&Mask>)`, `stroke_path(&Path, &Paint, &Stroke, Transform, Option<&Mask>)`,
  `fill_rect(Rect, &Paint, Transform, Option<&Mask>)`, `draw_pixmap(x, y, PixmapRef, &PixmapPaint { opacity, blend_mode, quality }, Transform, Option<&Mask>)`.
  tiny-skia has no text: blend fontdue coverage by hand into `data_mut()` (`d = c·a + d·(255−a)`,
  alpha the same way; 43 glyphs = 0.03 ms).
- Export: `committed` at 1:1 → `RgbaImage::from_raw(w, h, pixmap.data().to_vec())` (already RGBA).

Memory: a 3840×1080 image means ≈ 50 MB while the editor is open; all of it is dropped on close,
followed by `release_freed_memory()`.

## B4. Window and view

- Font: `segoeuib.ttf` (Segoe UI Bold) for annotations and toolbar text; fallback `arialbd.ttf`,
  then `segoeui.ttf`; none → text tool disabled, label says so. Loaded on a helper thread while
  the window is created (same pattern as the overlay) and dropped with the editor.
- Create with `with_title("Skrinka — Editor")`, `with_window_icon` **and**
  `WindowAttributesExtWindows::with_taskbar_icon` (same RGBA generator), `with_inner_size(LogicalSize)`,
  `with_min_inner_size(LogicalSize::new(640, 360))`, `with_visible(false)`, then position it centred
  on the cursor's monitor **work area** (`GetCursorPos` → `MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)`
  → `GetMonitorInfoW(..).rcWork`), draw once, `set_visible(true)`. Use `LogicalSize` (not Physical):
  winit creates the window on the primary monitor and moves it, and a physical size would be
  scaled twice when the DPI differs.
- Initial logical size: image size / scale_factor + toolbar, clamped to the work area minus 48 px;
  the image is then shown at `scale = min(1, canvas_w / img_w, canvas_h / img_h)`, centred.
- `Resized(0,0)` (minimised) → ignore (`Surface::resize` needs NonZero). Other resizes → recompute
  the view. `ScaleFactorChanged` → re-layout the toolbar at the new scale (`toolbar_h = 44·scale`,
  button 32·scale, UI font 13·scale px).
- Mouse → image coords via the inverse view transform; clamp drawing to the image bounds.
- `CloseRequested` → same as Esc on a clean document; with unsaved annotations → the Esc hint
  (B5.6). Drop the window + surface together; ignore late events for unknown `WindowId`s.
- `has_focus()` is false until `Focused(true)` arrives; don't call `focus_window()` (Alt trick).

## B5. Tools and interaction

### B5.1 Defaults (from the UX research)

Palette, keys `1`–`8` (physical Digit1–Digit8): 1 red `#EF4444` (default), 2 orange `#F97316`,
3 yellow `#FACC15` (highlighter default), 4 green `#22C55E`, 5 blue `#3B82F6`, 6 pink `#EC4899`,
7 black `#111111`, 8 white `#FFFFFF`. Contrast colour for halos/labels/badge text: black when the
colour's relative luminance > 0.5, else white.

| Size | Stroke | Font px | Badge ⌀ | Pixelate block | Highlighter |
|---|---|---|---|---|---|
| S | 2 | 16 | 22 | 8 | 16 px tall band when dragged as a line; as a rect: the rect |
| **M (default)** | **4** | **24** | **30** | **12** | |
| L | 8 | 36 | 40 | 20 | |

- **Arrow:** head length `10 + 4·stroke` (26 at M), half-angle ≈ 23° (base ≈ 0.85·length), filled
  triangle with a back notch of depth = stroke, shaft ends inside the head, head capped at 40 % of
  the arrow length for short arrows; a 1.5–2 px halo in the contrast colour around shaft and head.
  Tab cycles Arrow → Line → DoubleArrow.
- **Rect:** stroke in the colour, `LineJoin::Miter`; Tab cycles Outline → Ellipse → Filled (fill at
  alpha 1.0 in the colour). Shift makes a square/circle, Alt draws from the centre.
- **Text:** Halo = text in the colour with an outline in the contrast colour, radius ⌈font/8⌉ px
  (3 at M; draw the glyph coverage at 8 offsets, then the fill). Label = rounded rect (radius 6,
  padding 6×10) in the colour, text in the contrast colour. Tab toggles. Multi-line, left-aligned,
  line height = font ascent − descent.
- **Number:** filled circle ⌀ per size in the colour, 2 px contrast ring, bold number ≈ 0.6·⌀ (two
  digits shrink to fit); click places, click-and-drag > 10 px adds a tail (a stroke-width line from
  the circle edge to the drag end, with halo). The next number is previewed on the cursor.
- **Highlight:** rect filled with the colour at alpha 0.38 and `BlendMode::Multiply`. Tool colour
  defaults to 3 (yellow) the first time it is picked.
- **Pixelate:** rect snapped to the block grid, block = average colour. Tab → Redact = solid fill
  in `#111111`. These two edit `flat`, not a vector layer, and are listed in `shapes` so they can
  be moved/undone.
- **Pen:** polyline with `LineCap::Round`/`LineJoin::Round`, points thinned (drop points < 2 px
  apart); Shift makes it a straight line from the first point.

### B5.2 Keys (physical)

| Key | Action |
|---|---|
| `V` `A` `T` `R` `N` `H` `B` `P` | Select / Arrow / Text / Rect / Number / Highlight / Pixelate (B for "blur") / Pen |
| `Tab` | cycle the current tool's variant (also changes the selected shape's variant) |
| `1`–`8` | colour (applies to the selected shape too) |
| `[` / `]`, mouse wheel over the canvas | size S/M/L (applies to the selected shape too) |
| `Shift` while drawing | 45° lines/arrows, squares/circles, straight pen |
| `Alt` while drawing | rect/ellipse from the centre |
| `Space` held while drawing | move the shape being drawn |
| `Ctrl+Z` / `Ctrl+Y` / `Ctrl+Shift+Z` | undo / redo / redo |
| `Delete` | delete the selected shape; `Shift+Delete` clears all (undoable) |
| arrows (with a selection) | nudge 1 px, Shift 10 px |
| `Enter` | clipboard + auto-save + close (when not typing text) |
| `Ctrl+C` | clipboard + close |
| `Ctrl+S` | Save as + clipboard + close |
| `Esc` | layered, see B5.6 |
| `?` (Shift+Slash) | toggle the shortcut sheet overlay |

Tool keys and colour digits are **disabled while a text caret is active**; `Ctrl+V` pastes into the
text; `Ctrl+Z` inside text entry cancels the entry.

### B5.3 Mouse

- Tools stay active after drawing (sticky). The shape just drawn becomes **selected**, so a colour
  key, size key, Tab or Delete acts on it right away. A press on empty canvas with a drawing tool
  starts a new shape (and deselects).
- **Select tool (V):** press on a shape selects it (topmost first; hit test = bounding box
  expanded by `max(6, stroke)`; for Arrow/Line the distance to the segment ≤ that; for Pen the
  bbox then distance to any segment), drag moves it (all points offset, clamped so the bbox stays
  inside the image), release commits. Double-click on a Text re-opens editing at its end.
  Press on empty canvas deselects. The selection shows a dashed 1 px outline of the bbox in the
  contrast colour (no resize handles in this version).
- Right-click: with a selection → deselect; otherwise nothing (no context menu).

### B5.4 Text entry

Click with the Text tool places a caret; typing inserts `KeyEvent.text` **only when
`!(ctrl && !alt)`** (with Ctrl held winit still delivers layout text, e.g. `м` for Ctrl+V on the
Ukrainian layout) and after filtering control characters (`\r`, `\x08`, `\t`, `\x1b` arrive as text).
Enter commits, Shift+Enter inserts a newline, Esc commits (empty text → discarded), click elsewhere
commits and starts a new caret. Backspace, Left/Right, Home/End work; `Ctrl+V` inserts
`arboard::Clipboard::get_text()` with `\r` stripped. Dead keys: `text == None` on the dead key, the
combined character comes with the next key — nothing to do. `set_ime_allowed(true)` on the editor
window so the Win+. emoji panel and IMEs deliver `WindowEvent::Ime(Ime::Commit(s))`; insert it.

### B5.5 Toolbar (bottom, 44·scale px)

Left to right: `[V]` │ `[A] [T] [R] [N] [H] [B] [P]` │ 8 colour dots (the active one ringed, keys
1–8 drawn tiny below) │ `S M L` │ undo, redo │ right-aligned: `Save as…`, `Save`, **`Copy`**
(accent-filled, label text; it does clipboard + close like Ctrl+C), `×` close. Buttons are
icon-only 32·scale px squares with the hotkey letter in the corner; the active tool is filled with
the accent colour; a tool button shows a small badge for its current Tab variant. Icons are simple
tiny-skia glyphs (arrow, "T" glyph from the font, rectangle, circle with "1", marker stroke, 3×3
pixel grid, pen stroke, cursor arrow, undo/redo arcs). Tooltip after 400 ms hover:
"Arrow — A · Tab: line/double · Shift: 45°". A short **on-canvas readout** ("M · Red") appears for
1 s after a colour/size change.

### B5.6 Esc and closing

Esc, in order: cancel the drag in progress → commit the text being typed → deselect → if the
document has no shapes, close; otherwise show "Press Esc again to discard" for 2 s and close only
on a second Esc within that time. `CloseRequested` (title-bar ×) behaves the same.

### B5.7 Export and close

Enter / Ctrl+C / Ctrl+S: commit any text being typed, build the 1:1 `RgbaImage` from `committed`,
drop the editor (window, surface, pixmaps, font), call `release_freed_memory()`, then
`output::spawn(image, action, ..)` exactly as the overlay does (so toasts, save-as folder memory
and clipboard behaviour are identical).

## B6. Entry points

- **Region overlay:** `E` opens the editor with the Arrow tool; `A T R N H B P` open it with that
  tool active. New `Outcome::Edit(RgbaImage, Tool)`. These letters are free in the overlay today
  (its keys are Enter, Ctrl+C/S, Esc, arrows). Update the overlay HELP line:
  `… · E: edit · Esc: cancel`.
- **Tray "Edit last screenshot"** (after "Capture window"): `App.last_saved: Option<PathBuf>` is
  set from every `WorkerDone.saved_path`; the item loads it with `image::open` (add the `png`
  decoder — `image` already has the `png` feature, which covers decoding) and opens the editor.
  Disabled (`set_enabled(false)`) until something was saved this session; if the file is gone, an
  error dialog.
- Full-screen and window captures do not open the editor.
- Only one editor at a time; a capture hotkey while the editor is open brings the editor to front
  instead (`focus_window()` is fine here — no Shift is held).

## B7. Tests

Unit (pure functions): arrow head points for a horizontal arrow at M; contrast colour for the 8
palette entries; hit test for each shape kind (inside, 6 px outside the bbox, far); undo/redo
sequence incl. `next_number`; pixelate of a 4×4 known image with 2 px blocks; text halo offsets;
view transform round trip (window ↔ image) at scale 0.5; key-string helpers.

End to end (injection, as in v2): region overlay → `A` → drag an arrow → Enter → PNG exists and
differs from the plain capture **only** inside the arrow's bbox expanded by the halo (0 pixels
differ elsewhere); `T` → type `Привіт 123` with the Ukrainian layout → Enter → PNG differs only in
the text box; `N` three clicks → `Ctrl+Z` → `N` → the badge shows 3 again; Esc twice discards;
"Edit last screenshot" reopens the file.

## B8. Release 0.4.0

CHANGELOG `[0.4.0]`, README (editor section with the key table and a GIF or screenshot under
`docs/media/` — a screenshot is enough), `CLAUDE.md` (editor architecture: pipeline, coordinate
spaces, the PixmapMut warning, text-input rules), `Cargo.toml` version 0.4.0, clippy/test/build,
commit "Release v0.4.0", tag `v0.4.0`, push main + tag, `gh run watch`, check the Release assets.

## B9. Implementation order

1. tiny-skia dep + `document.rs`/`render.rs` with Arrow and Rect only + unit tests + the window,
   view, pipeline, toolbar skeleton with Copy/Save/Save as/close → **checkpoint: draw an arrow,
   Enter saves it** (e2e test 1).
2. Select/move/delete, undo/redo, colour/size/variant keys, readout.
3. Text entry (halo/label), Number, Highlight, Pixelate/Redact, Pen.
4. Tooltips, `?` sheet, Esc layering, "Edit last screenshot", overlay entry keys.
5. e2e tests, memory check (idle after closing the editor ≈ v2 level), docs, release.

---

## Appendix — things that are easy to get wrong (all verified)

- `EventLoopProxy<T>` is `Send` but not `Sync`; the dialog thread needs only a clone.
- `global-hotkey` and `muda` handlers are set once per process; hotkey ids change on
  re-registration — the existing `Arc<Mutex<Ids>>` pattern must be reused when hotkeys are
  unregistered/re-registered around the Settings dialog.
- `tray-icon` 0.26.1 internal uID is **2** (verified at runtime in v2); keep the startup check.
- `rfd` dialogs: never on the winit thread (existing rule). The Settings dialog thread is fine.
- `KeyEvent.text` with Ctrl held is layout text, not a control code; shortcuts by `physical_key`.
- winit: `has_focus()` is cached and false until `Focused(true)`; `Resized(0,0)` on minimise;
  windows are created on the primary monitor then moved, so use `LogicalSize`.
- Win32 dialog owner must be `None` (cross-thread owner attaches input queues); close it from the
  main thread with `PostMessageW(WM_CLOSE)`, never `SendMessageW`.
- `git rev-list --count` in CI needs `fetch-depth: 0`.
- `gh repo create --push` does not push tags.
- Keep every user-visible string in one module (`strings.rs`) to make the later Ukrainian UI cheap.

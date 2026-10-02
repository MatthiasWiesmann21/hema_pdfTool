# hema_pdfTool — build notes

## Build / run

Use `build.cmd` instead of plain `cargo` on this machine:

```cmd
build.cmd build          # or: run, clippy, test, build --release
```

Why: rustc's `vswhere` probe does not detect the VS 2026 (prerelease)
installation, so without `VsDevCmd` env vars it falls back to `link.exe`
on PATH — which resolves to Git's GNU `link.exe`. `build.cmd` calls
`VsDevCmd.bat -arch=x64` first so PATH/LIB/INCLUDE are correct.

Requires in the project root: `pdfium.dll` (from
github.com/bblanchon/pdfium-binaries, pdfium-win-x64.tgz). `build.rs`
copies it next to the exe in `target/<profile>/`.

## Headless / scheduled mode

```cmd
hema_pdf_tool.exe --merge-folder <dir> --output <file.pdf>
```

Merges all `*.pdf` in `<dir>` (alphabetical) into `<file>`. Logs to
`%APPDATA%\hema_pdfTool\merge.log`. The GUI's "Schedule…" panel registers
this command as Windows Task Scheduler task `HemaPdfTool-AutoMerge`.

## GUI startup diagnostics

The release exe is `windows_subsystem = "windows"` — no console, so all
startup output would be lost. Instead, GUI startup/panics log to
`%APPDATA%\hema_pdfTool\gui.log`.

GPU crashes (driver access violations) can't be caught in-process, so
the GUI runs renderers in child processes: the launcher spawns the exe
with `HEMA_PDFTOOL_RENDERER=wgpu|glow`, waits, and on a non-zero exit
tries the next renderer. The working renderer is persisted in
settings.json and tried first next launch. The exe statically links the
CRT (`.cargo/config.toml`), so no VC++ redistributable is needed.

## Windows installer

```cmd
build.cmd build --release
"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" installer.iss
```

Produces `installer\hema_pdfTool-Setup-<version>.exe` (Inno Setup 6).
Per-user install into `%LOCALAPPDATA%\Programs\hema pdfTool` — no admin
needed. Bundles `pdfium.dll`, creates Start Menu + optional desktop
shortcut, registers in Settings > Apps with an uninstaller. Bump
`AppVersion` in `installer.iss` when the crate version changes.

## Test fixtures

```cmd
build.cmd run --example make_test_pdfs   # writes testdata/test1.pdf, test2.pdf
build.cmd run --example verify_export    # asserts order/rotation/drop logic
```

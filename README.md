# hema pdfTool

Drag & drop PDF tool for Windows: drop PDFs onto the window, rearrange pages
on a thumbnail grid, then merge everything into a single PDF — or extract
just the pages you selected.

## Features

- **Drag & drop** PDF files onto the window (or "Add")
- **Layouts** — small / normal / large tiles or a list view; drag pages to reorder,
  click a page to select it
- Every page shows its **source file** (with a per-document color) and its
  **original page number** ("Page 3 of 12")
- **Quick Look preview** — Space (or the eye icon) opens a large preview,
  Space / Esc / ✕ closes it, ← → browses
- Per page: **rotate** left/right, **delete**
- **Languages**: German (default), English, French, Spanish, Italian
- **Light / dark / system** theme
- **Merge all…** → save as a single PDF; **Extract selected…** → save a subset
- **Schedule…** — registers a Windows Task Scheduler task that auto-merges
  all PDFs in a folder on a daily / weekly / logon schedule

## Build

```cmd
build.cmd build
build.cmd run
```

`pdfium.dll` (from github.com/bblanchon/pdfium-binaries) must sit in the
project root; it is copied next to the exe automatically.

## Headless usage

```cmd
hema_pdf_tool.exe --merge-folder C:\inbox --output C:\out\merged.pdf
```

This is the command the scheduled task runs. Results are logged to
`%APPDATA%\hema_pdfTool\merge.log`.

## Shipping

Copy `target\release\hema_pdf_tool.exe` **and** `pdfium.dll` together.

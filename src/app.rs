//! egui GUI: drop PDFs, reorder pages on a thumbnail grid, merge / extract.

use crate::pdf::{rotate_rgba, PageOp, PdfEngine};
use crate::scheduler::{self, Frequency, ScheduleConfig};
use crate::settings::{self, Settings};
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const THUMB_WIDTH: i32 = 140;
const CARD_SIZE: egui::Vec2 = egui::Vec2::new(170.0, 215.0);

/// A single page shown in the grid. `order` holds the ids.
struct PageItem {
    source: PathBuf,
    file_name: String,
    page_index: i32,
    /// Clockwise quarter-turns applied on top of the intrinsic rotation.
    rotation: u8,
    /// Unrotated thumbnail pixels (rotation is applied on top for display).
    base_w: usize,
    base_h: usize,
    base_rgba: std::rc::Rc<Vec<u8>>,
    texture: egui::TextureHandle,
    selected: bool,
}

enum CardAction {
    Delete(usize),
    Rotate(usize, u8),
}

struct SchedulerUi {
    open: bool,
    status: String,
}

pub struct PdfToolApp {
    engine: PdfEngine,
    /// Ids in display/output order (this Vec is what egui_dnd reorders).
    order: Vec<usize>,
    pages: HashMap<usize, PageItem>,
    next_id: usize,
    status: String,
    settings: Settings,
    sched: SchedulerUi,
    help_open: bool,
}

impl PdfToolApp {
    pub fn new(engine: PdfEngine, ctx: &egui::Context) -> Self {
        apply_style(ctx);
        Self {
            engine,
            order: Vec::new(),
            pages: HashMap::new(),
            next_id: 0,
            status: "Drop PDF files to get started.".into(),
            settings: settings::load(),
            sched: SchedulerUi {
                open: false,
                status: String::new(),
            },
            help_open: false,
        }
    }

    /// Adds one PDF and returns how many pages were added.
    fn add_pdf(&mut self, ctx: &egui::Context, path: &Path) -> anyhow::Result<usize> {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let thumbs = self.engine.render_thumbnails(path, THUMB_WIDTH)?;
        anyhow::ensure!(!thumbs.is_empty(), "file has no pages");
        let count = thumbs.len();

        for (i, t) in thumbs.into_iter().enumerate() {
            let id = self.next_id;
            self.next_id += 1;
            // Unique texture name per page id — re-adding the same file must
            // not collide with existing textures in egui's texture manager.
            let texture = ctx.load_texture(
                format!("page{id}"),
                egui::ColorImage::from_rgba_unmultiplied([t.width, t.height], &t.rgba),
                egui::TextureOptions::LINEAR,
            );
            self.pages.insert(
                id,
                PageItem {
                    source: path.to_path_buf(),
                    file_name: file_name.clone(),
                    page_index: i as i32,
                    rotation: 0,
                    base_w: t.width,
                    base_h: t.height,
                    base_rgba: std::rc::Rc::new(t.rgba),
                    texture,
                    selected: false,
                },
            );
            self.order.push(id);
        }
        Ok(count)
    }

    fn add_files(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        let mut added_files = 0usize;
        let mut added_pages = 0usize;
        let mut problems: Vec<String> = Vec::new();

        for path in paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
            {
                problems.push(format!("{name} (not a PDF)"));
            } else {
                match self.add_pdf(ctx, &path) {
                    Ok(n) => {
                        added_files += 1;
                        added_pages += n;
                    }
                    Err(e) => problems.push(format!("{name} ({e:#})")),
                }
            }
        }

        self.status = match (added_files, problems.is_empty()) {
            (0, true) => "Nothing to add.".into(),
            (0, false) => format!("Skipped: {}", problems.join("; ")),
            (_, true) => format!("Added {added_files} file(s), {added_pages} page(s)."),
            (_, false) => format!(
                "Added {added_files} file(s), {added_pages} page(s). Skipped: {}",
                problems.join("; ")
            ),
        };
    }

    fn selected_count(&self) -> usize {
        self.pages.values().filter(|p| p.selected).count()
    }

    fn remove_selected(&mut self) {
        let before = self.order.len();
        self.order
            .retain(|id| !self.pages.get(id).is_some_and(|p| p.selected));
        self.pages.retain(|_, p| !p.selected);
        let removed = before - self.order.len();
        if removed > 0 {
            self.status = format!("Removed {removed} page(s).");
        }
    }

    /// Ctrl+A selects all pages, Delete removes the selected ones.
    /// Ignored while a text field has keyboard focus.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.order.is_empty() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let (select_all, delete) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::A),
                i.key_pressed(egui::Key::Delete),
            )
        });
        if select_all {
            for p in self.pages.values_mut() {
                p.selected = true;
            }
        }
        if delete {
            self.remove_selected();
        }
    }

    fn collect_ops(&self, only_selected: bool) -> Vec<PageOp> {
        self.order
            .iter()
            .filter_map(|id| self.pages.get(id))
            .filter(|p| !only_selected || p.selected)
            .map(|p| PageOp {
                source: p.source.clone(),
                page_index: p.page_index,
                rotation_delta: p.rotation,
            })
            .collect()
    }

    fn save_dialog(&self, default_name: &str) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(default_name);
        if let Some(dir) = &self.settings.last_dir {
            dialog = dialog.set_directory(dir);
        }
        dialog.save_file()
    }

    fn export(&mut self, only_selected: bool, default_name: &str) {
        let ops = self.collect_ops(only_selected);
        if ops.is_empty() {
            self.status = if only_selected {
                "No pages selected.".into()
            } else {
                "No pages to merge.".into()
            };
            return;
        }
        let Some(out) = self.save_dialog(default_name) else {
            return;
        };
        match self.engine.export(&ops, &out) {
            Ok(()) => {
                self.settings.last_dir = out.parent().map(|p| p.to_path_buf());
                settings::save(&self.settings);
                self.status = format!("Wrote {} ({} pages).", out.display(), ops.len());
            }
            Err(e) => self.status = format!("Export failed: {e:#}"),
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_files(ctx, dropped);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("hema pdfTool").size(17.0).strong());
                ui.separator();

                if ui
                    .button("Add PDFs…")
                    .on_hover_text("Open PDF files — or just drop them onto the window")
                    .clicked()
                    && let Some(paths) = rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .pick_files()
                {
                    self.add_files(ui.ctx(), paths);
                }

                if ui
                    .button("Clear all")
                    .on_hover_text("Remove every page from the list")
                    .clicked()
                {
                    self.pages.clear();
                    self.order.clear();
                    self.status = "Cleared.".into();
                }

                ui.separator();

                let selected = self.selected_count();
                if ui
                    .add_enabled(!self.order.is_empty(), egui::Button::new("Merge all…"))
                    .on_hover_text("Save all pages, in order, as one PDF")
                    .clicked()
                {
                    self.export(false, "merged.pdf");
                }
                if ui
                    .add_enabled(
                        selected > 0,
                        egui::Button::new(format!("Extract selected ({selected})…")),
                    )
                    .on_hover_text("Save only the selected pages as a new PDF")
                    .clicked()
                {
                    self.export(true, "extract.pdf");
                }
                if ui
                    .button("Select all")
                    .on_hover_text("Select every page (Ctrl+A)")
                    .clicked()
                {
                    for p in self.pages.values_mut() {
                        p.selected = true;
                    }
                }
                if ui
                    .button("Select none")
                    .on_hover_text("Clear the selection")
                    .clicked()
                {
                    for p in self.pages.values_mut() {
                        p.selected = false;
                    }
                }
                if ui
                    .add_enabled(selected > 0, egui::Button::new("Remove sel."))
                    .on_hover_text(
                        "Remove the selected pages from the list (Delete key). \
                         The source files are not modified.",
                    )
                    .clicked()
                {
                    self.remove_selected();
                }

                ui.separator();
                if ui
                    .button("Schedule…")
                    .on_hover_text(
                        "Automatically merge a folder on a schedule (Windows Task Scheduler)",
                    )
                    .clicked()
                {
                    self.sched.open = true;
                    self.sched.status = scheduler::query_task()
                        .unwrap_or_else(|| "No scheduled task registered.".into());
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new("❓ Help").selected(self.help_open))
                        .on_hover_text("How to use hema pdfTool")
                        .clicked()
                    {
                        self.help_open = !self.help_open;
                    }
                });
            });
            ui.add_space(6.0);
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} page(s)", self.order.len()));
                let selected = self.selected_count();
                if selected > 0 {
                    ui.separator();
                    ui.label(format!("{selected} selected"));
                }
                ui.separator();
                ui.label(&self.status);
            });
        });
    }

    fn page_grid(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            if self.order.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "Drop PDF files here\n\n\
                             or click \"Add PDFs…\" — \"❓ Help\" shows a quick tour",
                        )
                        .size(18.0)
                        .weak(),
                    );
                });
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut action: Option<CardAction> = None;
                    let Self { order, pages, .. } = self;
                    let position = std::cell::Cell::new(1usize);

                    egui_dnd::dnd(ui, "page_grid").show_vec_sized(
                        order,
                        CARD_SIZE,
                        |ui, id, handle, state| {
                            if let Some(item) = pages.get_mut(id) {
                                page_card(
                                    ui,
                                    *id,
                                    item,
                                    position.get(),
                                    handle,
                                    state.dragged,
                                    &mut action,
                                );
                            }
                            position.set(position.get() + 1);
                        },
                    );

                    let ctx = ui.ctx().clone();
                    match action {
                        Some(CardAction::Delete(id)) => {
                            pages.remove(&id);
                            order.retain(|x| *x != id);
                        }
                        Some(CardAction::Rotate(id, q)) => {
                            if let Some(item) = pages.get_mut(&id) {
                                item.rotation = (item.rotation + q) % 4;
                                let (w, h, rgba) = rotate_rgba(
                                    &item.base_rgba,
                                    item.base_w,
                                    item.base_h,
                                    item.rotation,
                                );
                                item.texture = ctx.load_texture(
                                    format!("page{id}:rot{}", item.rotation),
                                    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba),
                                    egui::TextureOptions::LINEAR,
                                );
                            }
                        }
                        None => {}
                    }
                });
            });
        });
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = self.help_open;
        egui::Window::new("Help — hema pdfTool")
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(4.0);
                    help_section(
                        ui,
                        "Add files",
                        "Drag PDF files onto the window, or click \"Add PDFs…\".\n\
                         Source files are only read — they are never modified.",
                    );
                    help_section(
                        ui,
                        "Arrange pages",
                        "Drag a page card to change its position in the output.\n\
                         Click a thumbnail (or its checkbox) to select it — Ctrl+A selects all.",
                    );
                    help_section(
                        ui,
                        "Rotate pages  (↺ CCW / ↻ CW)",
                        "↺  CCW = counter-clockwise — turns the page 90° to the left.\n\
                         ↻  CW = clockwise — turns the page 90° to the right.\n\
                         The badge (e.g. 180°) shows the total rotation, which is\n\
                         written into the exported PDF.",
                    );
                    help_section(
                        ui,
                        "Remove pages",
                        "× on a card removes that page from the list.\n\
                         \"Remove sel.\" or the Delete key removes every selected page.\n\
                         Removing pages never changes the source PDF files.",
                    );
                    help_section(
                        ui,
                        "Save",
                        "\"Merge all…\" writes all pages, in order, into one PDF.\n\
                         \"Extract selected (n)…\" writes only the selected pages.",
                    );
                    help_section(
                        ui,
                        "Schedule",
                        "\"Schedule…\" installs a Windows Task Scheduler task that merges\n\
                         every PDF in a folder automatically — daily, weekly, or at logon.",
                    );
                });
            });
        self.help_open = open;
    }

    fn scheduler_window(&mut self, ctx: &egui::Context) {
        let Self {
            sched,
            settings,
            ..
        } = self;
        if !sched.open {
            return;
        }

        let mut open = sched.open;
        egui::Window::new("Scheduled auto-merge")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Merges all PDFs in the input folder into the output file.");
                ui.label("Uses Windows Task Scheduler.");
                ui.add_space(8.0);

                egui::Grid::new("sched_grid")
                    .num_columns(3)
                    .show(ui, |ui| {
                        ui.label("Input folder:");
                        ui.add(
                            egui::TextEdit::singleline(&mut settings.sched_input_dir)
                                .desired_width(300.0),
                        );
                        if ui.button("Browse…").clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            settings.sched_input_dir = dir.display().to_string();
                        }
                        ui.end_row();

                        ui.label("Output file:");
                        ui.add(
                            egui::TextEdit::singleline(&mut settings.sched_output)
                                .desired_width(300.0),
                        );
                        if ui.button("Browse…").clicked()
                            && let Some(f) = rfd::FileDialog::new()
                                .add_filter("PDF", &["pdf"])
                                .set_file_name("merged.pdf")
                                .save_file()
                            {
                            settings.sched_output = f.display().to_string();
                        }
                        ui.end_row();

                        ui.label("Frequency:");
                        egui::ComboBox::from_id_salt("sched_freq")
                            .selected_text(
                                ["Daily", "Weekly", "At logon"]
                                    .get(settings.sched_freq)
                                    .copied()
                                    .unwrap_or("Daily"),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut settings.sched_freq, 0, "Daily");
                                ui.selectable_value(&mut settings.sched_freq, 1, "Weekly");
                                ui.selectable_value(&mut settings.sched_freq, 2, "At logon");
                            });
                        ui.end_row();

                        if settings.sched_freq == 1 {
                            ui.label("Weekday:");
                            egui::ComboBox::from_id_salt("sched_day")
                                .selected_text(
                                    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                                        .get(settings.sched_weekday)
                                        .copied()
                                        .unwrap_or("Mon"),
                                )
                                .show_ui(ui, |ui| {
                                    for (i, d) in
                                        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                                            .iter()
                                            .enumerate()
                                    {
                                        ui.selectable_value(&mut settings.sched_weekday, i, *d);
                                    }
                                });
                            ui.end_row();
                        }

                        if settings.sched_freq != 2 {
                            ui.label("Time (HH:MM):");
                            ui.text_edit_singleline(&mut settings.sched_time);
                            ui.end_row();
                        }
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Create / update task").clicked() {
                        let cfg = ScheduleConfig {
                            input_dir: settings.sched_input_dir.clone(),
                            output: settings.sched_output.clone(),
                            time: settings.sched_time.clone(),
                            frequency: match settings.sched_freq {
                                1 => Frequency::Weekly(settings.sched_weekday),
                                2 => Frequency::OnLogon,
                                _ => Frequency::Daily,
                            },
                        };
                        sched.status = match scheduler::validate_config(&cfg)
                            .and_then(|_| scheduler::install_task(&cfg))
                        {
                            Ok(out) => {
                                settings::save(settings);
                                format!("Task created.\n{out}")
                            }
                            Err(e) => format!("Failed: {e:#}"),
                        };
                    }
                    if ui.button("Run now").clicked() {
                        sched.status = match scheduler::run_task_now() {
                            Ok(out) => format!("Triggered.\n{out}"),
                            Err(e) => format!("Failed: {e:#}"),
                        };
                    }
                    if ui.button("Remove task").clicked() {
                        sched.status = match scheduler::remove_task() {
                            Ok(out) => format!("Removed.\n{out}"),
                            Err(e) => format!("Failed: {e:#}"),
                        };
                    }
                    if ui.button("Refresh status").clicked() {
                        sched.status = scheduler::query_task()
                            .unwrap_or_else(|| "No scheduled task registered.".into());
                    }
                });

                if !sched.status.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add(
                        egui::TextEdit::multiline(&mut sched.status)
                            .desired_width(500.0)
                            .desired_rows(6),
                    );
                }
            });

        sched.open = open;
        if !open {
            settings::save(settings);
        }
    }
}

impl eframe::App for PdfToolApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_drops(&ctx);
        self.shortcuts(&ctx);
        self.toolbar(ui);
        self.status_bar(ui);
        self.page_grid(ui);
        self.scheduler_window(&ctx);
        self.help_window(&ctx);
    }
}

/// Slightly rounder, airier defaults for both light and dark themes.
fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.window_margin = egui::Margin::same(10);
        style.visuals.window_corner_radius = egui::CornerRadius::same(8);
        let widgets = &mut style.visuals.widgets;
        for w in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            w.corner_radius = egui::CornerRadius::same(5);
        }
        widgets.noninteractive.corner_radius = egui::CornerRadius::same(5);
    });
}

fn help_section(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.label(egui::RichText::new(title).strong());
    ui.add_space(2.0);
    ui.label(body);
    ui.add_space(10.0);
}

fn page_card(
    ui: &mut egui::Ui,
    id: usize,
    item: &mut PageItem,
    position: usize,
    handle: egui_dnd::Handle,
    dragged: bool,
    action: &mut Option<CardAction>,
) {
    let visuals = ui.visuals().clone();
    let frame = egui::Frame::new()
        .fill(if item.selected {
            visuals.selection.bg_fill.gamma_multiply(0.25)
        } else {
            visuals.faint_bg_color
        })
        .stroke(if item.selected {
            egui::Stroke::new(2.0, visuals.selection.stroke.color)
        } else {
            visuals.widgets.noninteractive.bg_stroke
        })
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(4));

    frame.show(ui, |ui| {
        ui.vertical_centered(|ui| {
            handle.ui(ui, |ui| {
                let img = egui::Image::new(egui::load::SizedTexture::new(
                    item.texture.id(),
                    item.texture.size_vec2(),
                ))
                .max_size(egui::vec2(140.0, 160.0))
                .sense(egui::Sense::click());
                let resp = ui
                    .add(img)
                    .on_hover_text("Click to select • drag to reorder");
                if resp.clicked() && !dragged {
                    item.selected = !item.selected;
                }
                if dragged {
                    ui.small("(dragging)");
                }
            });

            ui.add(
                egui::Label::new(format!(
                    "#{position}  {} · p.{}",
                    item.file_name,
                    item.page_index + 1
                ))
                .truncate(),
            )
            .on_hover_text(format!("{} — page {}", item.file_name, item.page_index + 1));

            ui.horizontal(|ui| {
                if ui
                    .small_button("↺")
                    .on_hover_text(
                        "Rotate counter-clockwise (CCW)\nTurn the page 90° to the left",
                    )
                    .clicked()
                {
                    *action = Some(CardAction::Rotate(id, 3));
                }
                if ui
                    .small_button("↻")
                    .on_hover_text("Rotate clockwise (CW)\nTurn the page 90° to the right")
                    .clicked()
                {
                    *action = Some(CardAction::Rotate(id, 1));
                }
                if item.rotation != 0 {
                    ui.small(format!("{}°", item.rotation * 90));
                }
                if ui
                    .small_button("×")
                    .on_hover_text(
                        "Remove this page from the list\n(the source file is not changed)",
                    )
                    .clicked()
                {
                    *action = Some(CardAction::Delete(id));
                }
                ui.checkbox(&mut item.selected, "");
            });
        });
    });
}

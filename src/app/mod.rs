//! egui GUI: drop PDFs, reorder pages, preview them, merge / extract.

mod pages;
mod preview;
mod theme;
mod windows;

use crate::i18n::{fmt, Lang, Tr};
use crate::pdf::{rotate_rgba, PageOp, PdfEngine};
use crate::scheduler;
use crate::settings::{self, Layout, Settings, ThemeMode};
use eframe::egui::{self, Color32, Margin, RichText};
use egui_phosphor::regular as icon;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Thumbnail render width — large enough for the "large tiles" layout;
/// smaller layouts scale the texture down.
const THUMB_WIDTH: i32 = 240;

/// A single page shown in the grid. `order` holds the ids.
struct PageItem {
    source: PathBuf,
    file_name: String,
    page_index: i32,
    /// Page count of the source document.
    page_total: i32,
    /// Color that marks which document the page comes from.
    color: Color32,
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
    Preview(usize),
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
    /// Last status message; `None` shows the localized "ready" hint.
    status: Option<String>,
    settings: Settings,
    sched: SchedulerUi,
    help_open: bool,
    doc_colors: HashMap<PathBuf, Color32>,
    /// Page under the mouse during the previous frame.
    hovered: Option<usize>,
    /// Last clicked page.
    focused: Option<usize>,
    preview: Option<preview::Preview>,
}

impl PdfToolApp {
    pub fn new(engine: PdfEngine, ctx: &egui::Context) -> Self {
        theme::install(ctx);
        let settings = settings::load();
        apply_theme(ctx, settings.theme);
        Self {
            engine,
            order: Vec::new(),
            pages: HashMap::new(),
            next_id: 0,
            status: None,
            settings,
            sched: SchedulerUi {
                open: false,
                status: String::new(),
            },
            help_open: false,
            doc_colors: HashMap::new(),
            hovered: None,
            focused: None,
            preview: None,
        }
    }

    fn tr(&self) -> &'static Tr {
        self.settings.language.tr()
    }

    /// Adds one PDF and returns how many pages were added.
    fn add_pdf(&mut self, ctx: &egui::Context, path: &Path) -> anyhow::Result<usize> {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let thumbs = self.engine.render_thumbnails(path, THUMB_WIDTH)?;
        anyhow::ensure!(!thumbs.is_empty(), "{}", self.tr().no_pages);
        let count = thumbs.len();

        let n = self.doc_colors.len();
        let color = *self
            .doc_colors
            .entry(path.to_path_buf())
            .or_insert(theme::DOC_PALETTE[n % theme::DOC_PALETTE.len()]);

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
                    page_total: count as i32,
                    color,
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
        let t = self.tr();
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
                problems.push(format!("{name} ({})", t.not_pdf));
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

        let skipped = fmt(t.status_skipped, &[("list", &problems.join("; "))]);
        let added = fmt(
            t.status_added,
            &[("files", &added_files), ("pages", &added_pages)],
        );
        self.status = Some(match (added_files, problems.is_empty()) {
            (0, true) => t.status_nothing.into(),
            (0, false) => skipped,
            (_, true) => added,
            (_, false) => format!("{added} {skipped}"),
        });
    }

    fn pick_files(&mut self, ctx: &egui::Context) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .pick_files()
        {
            self.add_files(ctx, paths);
        }
    }

    fn clear_all(&mut self) {
        self.pages.clear();
        self.order.clear();
        self.doc_colors.clear();
        self.preview = None;
        self.status = Some(self.tr().status_cleared.into());
    }

    fn selected_count(&self) -> usize {
        self.pages.values().filter(|p| p.selected).count()
    }

    fn set_all_selected(&mut self, selected: bool) {
        for p in self.pages.values_mut() {
            p.selected = selected;
        }
    }

    fn remove_selected(&mut self) {
        let before = self.order.len();
        self.order
            .retain(|id| !self.pages.get(id).is_some_and(|p| p.selected));
        self.pages.retain(|_, p| !p.selected);
        let removed = before - self.order.len();
        if removed > 0 {
            self.status = Some(fmt(self.tr().status_removed, &[("n", &removed)]));
        }
    }

    /// The page Space / "Preview" acts on: hovered, else last clicked,
    /// else first selected, else the first page.
    fn current_page(&self) -> Option<usize> {
        let exists = |id: &usize| self.pages.contains_key(id);
        self.hovered
            .filter(exists)
            .or(self.focused.filter(exists))
            .or_else(|| {
                self.order
                    .iter()
                    .copied()
                    .find(|id| self.pages.get(id).is_some_and(|p| p.selected))
            })
            .or(self.order.first().copied())
    }

    fn open_preview(&mut self, id: usize) {
        self.focused = Some(id);
        self.preview = Some(preview::Preview::new(id));
    }

    fn rotate(&mut self, ctx: &egui::Context, id: usize, quarters: u8) {
        if let Some(item) = self.pages.get_mut(&id) {
            item.rotation = (item.rotation + quarters) % 4;
            let (w, h, rgba) =
                rotate_rgba(&item.base_rgba, item.base_w, item.base_h, item.rotation);
            item.texture = ctx.load_texture(
                format!("page{id}:rot{}", item.rotation),
                egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba),
                egui::TextureOptions::LINEAR,
            );
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: CardAction) {
        match action {
            CardAction::Delete(id) => {
                self.pages.remove(&id);
                self.order.retain(|x| *x != id);
            }
            CardAction::Rotate(id, q) => self.rotate(ctx, id, q),
            CardAction::Preview(id) => self.open_preview(id),
        }
    }

    /// Space toggles the preview, Ctrl+A selects all pages, Delete removes
    /// the selected ones. Ignored while a text field has keyboard focus.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        // Consumed up front so Space never "clicks" a focused button.
        let space = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        if self.preview.is_some() {
            if space {
                self.preview = None;
            }
            return;
        }
        if space && let Some(id) = self.current_page() {
            self.open_preview(id);
            return;
        }
        if self.order.is_empty() {
            return;
        }
        let (select_all, delete) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::A),
                i.key_pressed(egui::Key::Delete),
            )
        });
        if select_all {
            self.set_all_selected(true);
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
        let t = self.tr();
        let ops = self.collect_ops(only_selected);
        if ops.is_empty() {
            self.status = Some(
                if only_selected {
                    t.no_selected
                } else {
                    t.no_pages_merge
                }
                .into(),
            );
            return;
        }
        let Some(out) = self.save_dialog(default_name) else {
            return;
        };
        self.status = Some(match self.engine.export(&ops, &out) {
            Ok(()) => {
                self.settings.last_dir = out.parent().map(|p| p.to_path_buf());
                settings::save(&self.settings);
                fmt(
                    t.status_wrote,
                    &[("file", &out.display()), ("n", &ops.len())],
                )
            }
            Err(e) => fmt(t.export_failed, &[("err", &format!("{e:#}"))]),
        });
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

    fn header(&mut self, ui: &mut egui::Ui) {
        let t = self.tr();
        let frame = egui::Frame::new()
            .fill(theme::bar_fill(ui.visuals()))
            .inner_margin(Margin::symmetric(14, 10));
        egui::Panel::top("header").frame(frame).show(ui, |ui| {
            // Only the frequent actions carry text; the rest are icons with
            // tooltips. Below this width those labels collapse too.
            let compact = ui.available_width() < 960.0;
            let empty = self.order.is_empty();
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::FILE_PDF).size(26.0).color(theme::ACCENT));
                ui.label(RichText::new("hema pdfTool").size(18.0).strong());
                ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).small().weak());
                ui.add_space(14.0);

                if ui
                    .button(theme::label(icon::PLUS, t.add, compact))
                    .on_hover_text(t.add_tip)
                    .clicked()
                {
                    self.pick_files(ui.ctx());
                }
                if ui
                    .add_enabled(
                        !empty,
                        egui::Button::new(RichText::new(icon::BROOM).size(16.0)),
                    )
                    .on_hover_text(format!("{}\n{}", t.clear, t.clear_tip))
                    .clicked()
                {
                    self.clear_all();
                }

                ui.add_space(10.0);
                self.layout_switcher(ui);
                if ui
                    .add_enabled(
                        !empty,
                        egui::Button::new(theme::label(icon::EYE, t.preview, compact)),
                    )
                    .on_hover_text(t.preview_tip)
                    .clicked()
                    && let Some(id) = self.current_page()
                {
                    self.open_preview(id);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(icon::QUESTION).size(16.0))
                                .selected(self.help_open),
                        )
                        .on_hover_text(format!("{}\n{}", t.help, t.help_tip))
                        .clicked()
                    {
                        self.help_open = !self.help_open;
                    }

                    let (theme_icon, theme_name) = match self.settings.theme {
                        ThemeMode::System => (icon::CIRCLE_HALF, t.theme_system),
                        ThemeMode::Light => (icon::SUN, t.theme_light),
                        ThemeMode::Dark => (icon::MOON, t.theme_dark),
                    };
                    if ui
                        .add(theme::icon_button(theme_icon))
                        .on_hover_text(fmt(t.theme_tip, &[("name", &theme_name)]))
                        .clicked()
                    {
                        self.settings.theme = self.settings.theme.next();
                        apply_theme(ui.ctx(), self.settings.theme);
                        settings::save(&self.settings);
                    }

                    self.language_picker(ui);
                    ui.separator();

                    if ui
                        .button(RichText::new(icon::CALENDAR_DOTS).size(16.0))
                        .on_hover_text(format!("{}\n{}", t.schedule, t.schedule_tip))
                        .clicked()
                    {
                        self.sched.open = true;
                        self.sched.status =
                            scheduler::query_task().unwrap_or_else(|| t.no_task.into());
                    }
                });
            });
        });
    }

    /// Segmented control with one icon per layout.
    fn layout_switcher(&mut self, ui: &mut egui::Ui) {
        let t = self.tr();
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .corner_radius(10)
            .inner_margin(Margin::same(2))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for layout in Layout::ALL {
                    let (glyph, tip) = match layout {
                        Layout::Small => (icon::GRID_NINE, t.layout_small),
                        Layout::Normal => (icon::SQUARES_FOUR, t.layout_normal),
                        Layout::Large => (icon::SQUARE, t.layout_large),
                        Layout::List => (icon::LIST_DASHES, t.layout_list),
                    };
                    let selected = self.settings.layout == layout;
                    if ui
                        .add(theme::icon_button(glyph).selected(selected))
                        .on_hover_text(tip)
                        .clicked()
                        && !selected
                    {
                        self.settings.layout = layout;
                        settings::save(&self.settings);
                    }
                }
            });
    }

    fn language_picker(&mut self, ui: &mut egui::Ui) {
        let current = self.settings.language;
        let mut chosen = current;
        egui::ComboBox::from_id_salt("language")
            .selected_text(format!("{}  {}", icon::GLOBE, current.native_name()))
            .width(130.0)
            .show_ui(ui, |ui| {
                for lang in Lang::ALL {
                    ui.selectable_value(&mut chosen, lang, lang.native_name());
                }
            })
            .response
            .on_hover_text(self.tr().language_tip);
        if chosen != current {
            self.settings.language = chosen;
            settings::save(&self.settings);
        }
    }

    fn action_bar(&mut self, ui: &mut egui::Ui) {
        let t = self.tr();
        let frame = egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(Margin::symmetric(14, 8));
        egui::Panel::top("actions")
            .frame(frame)
            .show_separator_line(false)
            .show(ui, |ui| {
                let compact = ui.available_width() < 900.0;
                let selected = self.selected_count();
                let empty = self.order.is_empty();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!empty, theme::primary(theme::label(icon::ARROWS_MERGE, t.merge, false)))
                        .on_hover_text(t.merge_tip)
                        .clicked()
                    {
                        self.export(false, "merged.pdf");
                    }
                    let extract = fmt(t.extract, &[("n", &selected)]);
                    if ui
                        .add_enabled(
                            selected > 0,
                            egui::Button::new(theme::label(icon::EXPORT, &extract, false)),
                        )
                        .on_hover_text(t.extract_tip)
                        .clicked()
                    {
                        self.export(true, "extract.pdf");
                    }

                    ui.separator();

                    if ui
                        .add_enabled(
                            !empty,
                            egui::Button::new(theme::label(icon::SELECTION_ALL, t.select_all, compact)),
                        )
                        .on_hover_text(t.select_all_tip)
                        .clicked()
                    {
                        self.set_all_selected(true);
                    }
                    if ui
                        .add_enabled(
                            selected > 0,
                            egui::Button::new(theme::label(icon::SELECTION_SLASH, t.select_none, compact)),
                        )
                        .on_hover_text(t.select_none_tip)
                        .clicked()
                    {
                        self.set_all_selected(false);
                    }
                    if ui
                        .add_enabled(
                            selected > 0,
                            egui::Button::new(theme::label(icon::TRASH, t.remove_sel, compact)),
                        )
                        .on_hover_text(t.remove_sel_tip)
                        .clicked()
                    {
                        self.remove_selected();
                    }
                });
            });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        let t = self.tr();
        let frame = egui::Frame::new()
            .fill(theme::bar_fill(ui.visuals()))
            .inner_margin(Margin::symmetric(14, 6));
        egui::Panel::bottom("status").frame(frame).show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::soft_pill(ui, fmt(t.pages_count, &[("n", &self.order.len())]));
                let docs: HashSet<&PathBuf> = self.pages.values().map(|p| &p.source).collect();
                if !docs.is_empty() {
                    theme::soft_pill(ui, fmt(t.docs_count, &[("n", &docs.len())]));
                }
                let selected = self.selected_count();
                if selected > 0 {
                    theme::pill(
                        ui,
                        fmt(t.selected_count, &[("n", &selected)]),
                        theme::ACCENT,
                        Color32::WHITE,
                    );
                }
                ui.add_space(4.0);
                let status = self.status.as_deref().unwrap_or(t.status_ready);
                ui.add(egui::Label::new(RichText::new(status).weak()).truncate())
                    .on_hover_text(status);
            });
        });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        let frame = egui::Frame::central_panel(ui.style()).inner_margin(Margin::same(16));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            let files_hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
            let area = ui.max_rect();
            if self.order.is_empty() {
                if pages::empty_state(ui, self.tr(), files_hovering) {
                    self.pick_files(ui.ctx());
                }
            } else {
                self.page_area(ui);
                if files_hovering {
                    pages::drop_overlay(ui, area, self.tr());
                }
            }
        });
    }
}

fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_theme(match mode {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::Dark => egui::ThemePreference::Dark,
    });
}

impl eframe::App for PdfToolApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_drops(&ctx);
        self.shortcuts(&ctx);
        self.header(ui);
        if !self.order.is_empty() {
            self.action_bar(ui);
        }
        self.status_bar(ui);
        self.central(ui);
        self.preview_window(&ctx);
        self.scheduler_window(&ctx);
        self.help_window(&ctx);
    }
}

//! Quick-Look style preview: a large overlay with the page in high
//! resolution. Space / Esc / ✕ / backdrop click close it, ← → browse.

use super::{theme, CardAction, PdfToolApp};
use crate::i18n::fmt;
use crate::pdf::rotate_rgba;
use eframe::egui::{self, vec2, Color32, Margin, Rect, RichText, Sense};
use egui_phosphor::regular as icon;

/// Upper bound of the preview render size in pixels.
const MAX_W: i32 = 1600;
const MAX_H: i32 = 2000;

const TEXTURE_OPTIONS: egui::TextureOptions = egui::TextureOptions {
    mipmap_mode: Some(egui::TextureFilter::Linear),
    ..egui::TextureOptions::LINEAR
};

pub(super) struct Preview {
    id: usize,
    /// Rendered texture (or error message), keyed by (page id, rotation).
    cache: Option<((usize, u8), Result<egui::TextureHandle, String>)>,
}

impl Preview {
    pub fn new(id: usize) -> Self {
        Self { id, cache: None }
    }
}

impl PdfToolApp {
    /// Renders the previewed page if it isn't cached for its current rotation.
    fn ensure_preview_texture(&mut self, ctx: &egui::Context) {
        let t = self.tr();
        let Some(p) = self.preview.as_mut() else {
            return;
        };
        let Some(item) = self.pages.get(&p.id) else {
            return;
        };
        let key = (p.id, item.rotation);
        if p.cache.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let result = self
            .engine
            .render_page(&item.source, item.page_index, MAX_W, MAX_H)
            .map(|th| {
                let (w, h, rgba) = rotate_rgba(&th.rgba, th.width, th.height, item.rotation);
                ctx.load_texture(
                    "page_preview",
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba),
                    TEXTURE_OPTIONS,
                )
            })
            .map_err(|e| fmt(t.preview_failed, &[("err", &format!("{e:#}"))]));
        p.cache = Some((key, result));
    }

    pub(super) fn preview_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.preview.as_ref().map(|p| p.id) else {
            return;
        };
        let Some(pos) = self.order.iter().position(|x| *x == id) else {
            self.preview = None;
            return;
        };
        let t = self.tr();
        let total = self.order.len();

        let (left, right) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
            )
        });
        let mut goto = match (left, right) {
            (true, _) if pos > 0 => Some(pos - 1),
            (_, true) if pos + 1 < total => Some(pos + 1),
            _ => None,
        };

        self.ensure_preview_texture(ctx);
        let tex = self
            .preview
            .as_ref()
            .and_then(|p| p.cache.as_ref())
            .map(|(_, r)| r.clone());
        let Some(item) = self.pages.get(&id) else {
            return;
        };

        let content = ctx.content_rect();
        let size = vec2(content.width() * 0.92, content.height() * 0.92);
        let mut close = false;
        let mut action = None;
        let mut toggle_selected = false;

        let modal = egui::Modal::new(egui::Id::new("page_preview"))
            .backdrop_color(Color32::from_black_alpha(170))
            .frame(egui::Frame::window(&ctx.global_style()).inner_margin(Margin::same(14)))
            .show(ctx, |ui| {
                ui.set_width(size.x - 28.0);
                ui.set_height(size.y - 28.0);

                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, item.color);
                    ui.label(RichText::new(&item.file_name).size(16.0).strong());
                    ui.label(
                        RichText::new(fmt(
                            t.page_of,
                            &[("n", &(item.page_index + 1)), ("total", &item.page_total)],
                        ))
                        .weak(),
                    );
                    theme::soft_pill(ui, format!("#{} / {total}", pos + 1));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(theme::icon_button(icon::X))
                            .on_hover_text(t.preview_close)
                            .clicked()
                        {
                            close = true;
                        }
                        ui.separator();
                        let label = if item.selected {
                            t.preview_selected
                        } else {
                            t.preview_select
                        };
                        if ui
                            .add(
                                egui::Button::new(theme::label(icon::CHECK_CIRCLE, label, false))
                                    .selected(item.selected),
                            )
                            .clicked()
                        {
                            toggle_selected = true;
                        }
                        if ui
                            .add(theme::icon_button(icon::ARROW_CLOCKWISE))
                            .on_hover_text(t.rotate_cw_tip)
                            .clicked()
                        {
                            action = Some(CardAction::Rotate(id, 1));
                        }
                        if ui
                            .add(theme::icon_button(icon::ARROW_COUNTER_CLOCKWISE))
                            .on_hover_text(t.rotate_ccw_tip)
                            .clicked()
                        {
                            action = Some(CardAction::Rotate(id, 3));
                        }
                    });
                });
                ui.separator();

                let footer_h = 36.0;
                let avail = ui.available_size() - vec2(0.0, footer_h);
                let (rect, _) = ui.allocate_exact_size(avail, Sense::hover());
                match &tex {
                    Some(Ok(tex)) => paint_page(ui, rect, tex),
                    Some(Err(e)) => {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            e,
                            egui::FontId::proportional(14.0),
                            ui.visuals().error_fg_color,
                        );
                    }
                    None => {}
                }

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(pos > 0, egui::Button::new(RichText::new(icon::CARET_LEFT).size(16.0)))
                        .on_hover_text(t.preview_prev)
                        .clicked()
                    {
                        goto = Some(pos - 1);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                pos + 1 < total,
                                egui::Button::new(RichText::new(icon::CARET_RIGHT).size(16.0)),
                            )
                            .on_hover_text(t.preview_next)
                            .clicked()
                        {
                            goto = Some(pos + 1);
                        }
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new(t.preview_hint).small().weak());
                        });
                    });
                });
            });

        if modal.should_close() || close {
            self.preview = None;
            return;
        }
        if toggle_selected && let Some(item) = self.pages.get_mut(&id) {
            item.selected = !item.selected;
        }
        if let Some(a) = action {
            self.apply(ctx, a);
        }
        if let Some(new_pos) = goto {
            let new_id = self.order[new_pos];
            self.focused = Some(new_id);
            if let Some(p) = self.preview.as_mut() {
                p.id = new_id;
            }
        }
    }
}

/// Fits the page into `rect`, centered, with a soft drop shadow.
fn paint_page(ui: &egui::Ui, rect: Rect, tex: &egui::TextureHandle) {
    let size = tex.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y).min(1.5);
    let img_rect = Rect::from_center_size(rect.center(), size * scale);
    let shadow = egui::Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(70),
    };
    ui.painter().add(shadow.as_shape(img_rect, 3));
    ui.painter().rect_filled(img_rect, 3.0, Color32::WHITE);
    egui::Image::new(egui::load::SizedTexture::new(tex.id(), size))
        .corner_radius(3)
        .paint_at(ui, img_rect);
}

//! Page views: tiles in three sizes, the list view, empty state and drop overlay.

use super::{theme, CardAction, PageItem, PdfToolApp};
use crate::i18n::{fmt, Tr};
use crate::settings::Layout;
use eframe::egui::{
    self, pos2, vec2, Color32, CornerRadius, FontId, Margin, Painter, Pos2, Rect, RichText, Sense,
    Shadow, Shape, Stroke, Vec2,
};
use egui_phosphor::regular as icon;

struct Dims {
    card: Vec2,
    thumb: Vec2,
    small: bool,
}

/// Padding + stroke around the card contents.
const CARD_INSET: f32 = 24.0;

fn dims(layout: Layout) -> Dims {
    let (card, thumb_h, small) = match layout {
        Layout::Small => (vec2(140.0, 250.0), 130.0, true),
        Layout::Large => (vec2(280.0, 450.0), 330.0, false),
        Layout::Normal | Layout::List => (vec2(192.0, 320.0), 200.0, false),
    };
    Dims {
        card,
        thumb: vec2(card.x - CARD_INSET, thumb_h),
        small,
    }
}

/// Per-frame state shared by every card/row.
struct ItemCtx<'a> {
    t: &'static Tr,
    prev_hovered: Option<usize>,
    hovered: &'a mut Option<usize>,
    focused: &'a mut Option<usize>,
    action: &'a mut Option<CardAction>,
}

impl PdfToolApp {
    pub(super) fn page_area(&mut self, ui: &mut egui::Ui) {
        let layout = self.settings.layout;
        let mut action = None;
        let mut hovered = None;
        let mut cx = ItemCtx {
            t: self.tr(),
            prev_hovered: self.hovered,
            hovered: &mut hovered,
            focused: &mut self.focused,
            action: &mut action,
        };
        let (order, pages) = (&mut self.order, &mut self.pages);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if layout == Layout::List {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    egui_dnd::dnd(ui, "page_list").show_vec(order, |ui, id, handle, state| {
                        if let Some(item) = pages.get_mut(id) {
                            list_row(ui, &mut cx, *id, item, state.index + 1, handle, state.dragged);
                        }
                    });
                } else {
                    let d = dims(layout);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
                        egui_dnd::dnd(ui, "page_grid").show_vec_sized(
                            order,
                            d.card,
                            |ui, id, handle, state| {
                                if let Some(item) = pages.get_mut(id) {
                                    card(
                                        ui,
                                        &mut cx,
                                        &d,
                                        *id,
                                        item,
                                        state.index + 1,
                                        handle,
                                        state.dragged,
                                    );
                                }
                            },
                        );
                    });
                }
            });

        self.hovered = hovered;
        if let Some(a) = action {
            self.apply(ui.ctx(), a);
        }
    }
}

fn item_stroke(ui: &egui::Ui, selected: bool, hover: bool) -> Stroke {
    if selected {
        Stroke::new(2.0, theme::ACCENT)
    } else if hover {
        Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.55))
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke
    }
}

fn item_shadow(ui: &egui::Ui, dragged: bool) -> Shadow {
    let dark = ui.visuals().dark_mode;
    Shadow {
        offset: [0, if dragged { 10 } else { 2 }],
        blur: if dragged { 26 } else { 8 },
        spread: 0,
        color: Color32::from_black_alpha(match (dark, dragged) {
            (true, _) => 70,
            (false, true) => 50,
            (false, false) => 14,
        }),
    }
}

/// Handles clicks on a thumbnail: select, double-click previews.
fn thumb_interaction(
    cx: &mut ItemCtx,
    id: usize,
    item: &mut PageItem,
    resp: &egui::Response,
    dragged: bool,
) {
    if resp.hovered() {
        *cx.hovered = Some(id);
    }
    if resp.double_clicked() {
        // The first click of the double-click already toggled — undo it.
        item.selected = !item.selected;
        *cx.action = Some(CardAction::Preview(id));
    } else if resp.clicked() && !dragged {
        item.selected = !item.selected;
        *cx.focused = Some(id);
    }
}

/// Rotate left / rotate right / preview buttons shared by tiles and rows.
/// `reverse` is for right-to-left layouts, so they still read ↺ ↻ 👁.
fn action_buttons(ui: &mut egui::Ui, cx: &mut ItemCtx, id: usize, reverse: bool) {
    let t = cx.t;
    let mut buttons = [
        (icon::ARROW_COUNTER_CLOCKWISE, t.rotate_ccw_tip, CardAction::Rotate(id, 3)),
        (icon::ARROW_CLOCKWISE, t.rotate_cw_tip, CardAction::Rotate(id, 1)),
        (icon::EYE, t.preview_card_tip, CardAction::Preview(id)),
    ];
    if reverse {
        buttons.reverse();
    }
    for (glyph, tip, action) in buttons {
        if ui.add(theme::icon_button(glyph)).on_hover_text(tip).clicked() {
            *cx.action = Some(action);
        }
    }
}

fn delete_button(ui: &mut egui::Ui, cx: &mut ItemCtx, id: usize) {
    if ui
        .add(theme::icon_button(icon::TRASH))
        .on_hover_text(cx.t.delete_tip)
        .clicked()
    {
        *cx.action = Some(CardAction::Delete(id));
    }
}

fn page_label(t: &Tr, item: &PageItem, short: bool) -> String {
    fmt(
        if short { t.page_short } else { t.page_of },
        &[("n", &(item.page_index + 1)), ("total", &item.page_total)],
    )
}

/// Small colored dot marking the source document.
fn doc_dot(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
}

#[allow(clippy::too_many_arguments)]
fn card(
    ui: &mut egui::Ui,
    cx: &mut ItemCtx,
    d: &Dims,
    id: usize,
    item: &mut PageItem,
    position: usize,
    handle: egui_dnd::Handle,
    dragged: bool,
) {
    let hover = cx.prev_hovered == Some(id);
    let stroke = item_stroke(ui, item.selected, hover);
    let resp = egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(stroke)
        .corner_radius(theme::CARD_RADIUS)
        .inner_margin(Margin::same(10))
        .shadow(item_shadow(ui, dragged))
        .show(ui, |ui| {
            // The grid's `horizontal_wrapped` layout is inherited here, so the
            // card has to stack its rows explicitly.
            ui.vertical(|ui| {
                ui.set_width(d.card.x - CARD_INSET);
                ui.set_min_height(d.card.y - CARD_INSET);
                ui.spacing_mut().item_spacing.y = 3.0;

                handle.ui(ui, |ui| {
                    let (rect, resp) = ui.allocate_exact_size(d.thumb, Sense::click());
                    paint_thumb(ui, rect, item, Some(position), hover);
                    let resp = resp.on_hover_text(cx.t.card_tip);
                    thumb_interaction(cx, id, item, &resp, dragged);
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    doc_dot(ui, item.color);
                    ui.add(egui::Label::new(RichText::new(&item.file_name).strong()).truncate())
                        .on_hover_text(item.source.display().to_string());
                });
                ui.label(RichText::new(page_label(cx.t, item, d.small)).small().weak());

                ui.add_space(2.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.spacing_mut().button_padding = vec2(5.0, 3.0);
                    action_buttons(ui, cx, id, false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        delete_button(ui, cx, id);
                    });
                });
            });
        })
        .response;

    if resp.contains_pointer() {
        *cx.hovered = Some(id);
    }
    // Document color strip along the top edge.
    let r = resp.rect;
    let radius = theme::CARD_RADIUS;
    ui.painter().rect_filled(
        Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + 4.0)),
        CornerRadius {
            nw: radius,
            ne: radius,
            sw: 0,
            se: 0,
        },
        item.color,
    );
}

fn list_row(
    ui: &mut egui::Ui,
    cx: &mut ItemCtx,
    id: usize,
    item: &mut PageItem,
    position: usize,
    handle: egui_dnd::Handle,
    dragged: bool,
) {
    let t = cx.t;
    let hover = cx.prev_hovered == Some(id);
    let resp = egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(item_stroke(ui, item.selected, hover))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(10, 6))
        .shadow(item_shadow(ui, dragged))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                handle.ui(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon::DOTS_SIX_VERTICAL).size(18.0).weak())
                            .on_hover_text(t.drag_tip);
                        let (bar, _) = ui.allocate_exact_size(vec2(4.0, 52.0), Sense::hover());
                        ui.painter().rect_filled(bar, 2.0, item.color);
                        let (rect, resp) = ui.allocate_exact_size(vec2(52.0, 56.0), Sense::click());
                        paint_thumb(ui, rect, item, None, false);
                        let resp = resp.on_hover_text(t.card_tip);
                        thumb_interaction(cx, id, item, &resp, dragged);
                    });
                });
                ui.add_sized(
                    [44.0, 20.0],
                    egui::Label::new(RichText::new(format!("#{position}")).monospace().weak()),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    delete_button(ui, cx, id);
                    action_buttons(ui, cx, id, true);
                    let (glyph, color) = if item.selected {
                        (icon::CHECK_SQUARE, theme::ACCENT)
                    } else {
                        (icon::SQUARE, ui.visuals().weak_text_color())
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(glyph).size(18.0).color(color))
                                .frame_when_inactive(false),
                        )
                        .on_hover_text(t.select_tip)
                        .clicked()
                    {
                        item.selected = !item.selected;
                        *cx.focused = Some(id);
                    }
                    if item.rotation != 0 {
                        ui.add_space(6.0);
                        theme::soft_pill(ui, format!("{} {}°", icon::ARROW_CLOCKWISE, item.rotation as u32 * 90));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.add(
                                egui::Label::new(RichText::new(&item.file_name).strong())
                                    .truncate(),
                            )
                            .on_hover_text(item.source.display().to_string());
                            ui.label(RichText::new(page_label(t, item, false)).small().weak());
                        });
                    });
                });
            });
        })
        .response;
    if resp.contains_pointer() {
        *cx.hovered = Some(id);
    }
}

/// Rounded badge with white text; returns its rect.
fn badge(painter: &Painter, pos: Pos2, text: String, fill: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), Color32::WHITE);
    let rect = Rect::from_min_size(pos, galley.size() + vec2(12.0, 4.0));
    painter.rect_filled(rect, CornerRadius::same(255), fill);
    painter.galley(rect.min + vec2(6.0, 2.0), galley, Color32::WHITE);
    rect
}

/// Draws the page thumbnail centered in `rect`, plus position/rotation
/// badges and the selection check.
fn paint_thumb(ui: &egui::Ui, rect: Rect, item: &PageItem, position: Option<usize>, hover: bool) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, ui.visuals().extreme_bg_color);

    let size = item.texture.size_vec2();
    let inner = rect.shrink(if position.is_some() { 8.0 } else { 3.0 });
    let scale = (inner.width() / size.x).min(inner.height() / size.y);
    let img_rect = Rect::from_center_size(inner.center(), size * scale);
    painter.rect_filled(
        img_rect.translate(vec2(0.0, 1.5)).expand(1.0),
        3.0,
        Color32::from_black_alpha(40),
    );
    egui::Image::new(egui::load::SizedTexture::new(item.texture.id(), size))
        .corner_radius(2)
        .paint_at(ui, img_rect);

    let Some(position) = position else {
        return;
    };
    let dark = Color32::from_black_alpha(160);
    let r = badge(&painter, rect.min + vec2(6.0, 6.0), format!("#{position}"), dark);
    if item.rotation != 0 {
        badge(
            &painter,
            pos2(r.max.x + 4.0, r.min.y),
            format!("{} {}°", icon::ARROW_CLOCKWISE, item.rotation as u32 * 90),
            Color32::from_rgb(234, 128, 32),
        );
    }

    let c = pos2(rect.max.x - 15.0, rect.min.y + 15.0);
    if item.selected {
        painter.circle_filled(c, 10.0, theme::ACCENT);
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            icon::CHECK,
            FontId::proportional(13.0),
            Color32::WHITE,
        );
    } else if hover {
        painter.circle(
            c,
            9.0,
            Color32::from_white_alpha(200),
            Stroke::new(1.5, Color32::from_gray(150)),
        );
    }
}

/// Large dashed drop zone shown when no pages are loaded.
/// Returns `true` if its "Add" button was clicked.
pub(super) fn empty_state(ui: &mut egui::Ui, t: &Tr, files_hovering: bool) -> bool {
    let avail = ui.available_rect_before_wrap();
    let size = vec2(avail.width().min(640.0), avail.height().min(360.0));
    let rect = Rect::from_center_size(avail.center(), size);
    let v = ui.visuals();
    let (line, fill) = if files_hovering {
        (theme::ACCENT, theme::ACCENT.gamma_multiply(0.10))
    } else {
        (v.weak_text_color().gamma_multiply(0.6), v.faint_bg_color)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, fill);
    let r = rect.shrink(1.0);
    painter.extend(Shape::dashed_line(
        &[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()],
        Stroke::new(1.5, line),
        9.0,
        6.0,
    ));

    let mut clicked = false;
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(24.0))
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            ui.add_space((rect.height() - 230.0).max(0.0) / 2.0);
            ui.label(RichText::new(icon::FILE_PDF).size(60.0).color(theme::ACCENT));
            ui.add_space(6.0);
            ui.label(RichText::new(t.empty_title).size(21.0).strong());
            ui.label(RichText::new(t.empty_hint).weak());
            ui.add_space(16.0);
            clicked = ui
                .add(theme::primary(theme::label(icon::PLUS, t.add, false)).min_size(vec2(170.0, 36.0)))
                .clicked();
        },
    );
    clicked
}

/// Highlight over the page area while files are dragged over the window.
pub(super) fn drop_overlay(ui: &egui::Ui, rect: Rect, t: &Tr) {
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("drop_overlay"),
    ));
    painter.rect(
        rect.shrink(4.0),
        14.0,
        theme::ACCENT.gamma_multiply(0.12),
        Stroke::new(2.0, theme::ACCENT),
        egui::StrokeKind::Inside,
    );
    let galley = painter.layout_no_wrap(
        format!("{}  {}", icon::DOWNLOAD_SIMPLE, t.drop_release),
        FontId::proportional(17.0),
        Color32::WHITE,
    );
    let pill = Rect::from_center_size(rect.center(), galley.size() + vec2(32.0, 16.0));
    painter.rect_filled(pill, CornerRadius::same(255), theme::ACCENT);
    painter.galley(pill.min + vec2(16.0, 8.0), galley, Color32::WHITE);
}

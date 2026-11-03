//! Help and scheduler windows.

use super::{theme, PdfToolApp};
use crate::i18n::fmt;
use crate::scheduler::{self, Frequency, ScheduleConfig};
use crate::settings;
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

const HELP_ICONS: [&str; 7] = [
    icon::PLUS,
    icon::DOTS_SIX_VERTICAL,
    icon::EYE,
    icon::ARROW_CLOCKWISE,
    icon::TRASH,
    icon::ARROWS_MERGE,
    icon::CALENDAR_DOTS,
];

impl PdfToolApp {
    pub(super) fn help_window(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            return;
        }
        let t = self.tr();
        let mut open = self.help_open;
        egui::Window::new(t.help_title)
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(500.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for ((title, body), glyph) in t.help_sections.iter().zip(HELP_ICONS) {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(glyph).size(20.0).color(theme::ACCENT));
                            ui.vertical(|ui| {
                                ui.label(RichText::new(*title).strong());
                                ui.label(RichText::new(*body).weak());
                            });
                        });
                        ui.add_space(10.0);
                    }
                });
            });
        self.help_open = open;
    }

    pub(super) fn scheduler_window(&mut self, ctx: &egui::Context) {
        let t = self.tr();
        let Self {
            sched,
            settings,
            ..
        } = self;
        if !sched.open {
            return;
        }

        let mut open = sched.open;
        egui::Window::new(t.sched_title)
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new(t.sched_desc).weak());
                ui.add_space(10.0);

                egui::Grid::new("sched_grid")
                    .num_columns(3)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(t.input_folder);
                        ui.add(
                            egui::TextEdit::singleline(&mut settings.sched_input_dir)
                                .desired_width(300.0),
                        );
                        if ui.button(theme::label(icon::FOLDER_OPEN, t.browse, false)).clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            settings.sched_input_dir = dir.display().to_string();
                        }
                        ui.end_row();

                        ui.label(t.output_file);
                        ui.add(
                            egui::TextEdit::singleline(&mut settings.sched_output)
                                .desired_width(300.0),
                        );
                        if ui.button(theme::label(icon::FOLDER_OPEN, t.browse, false)).clicked()
                            && let Some(f) = rfd::FileDialog::new()
                                .add_filter("PDF", &["pdf"])
                                .set_file_name("merged.pdf")
                                .save_file()
                        {
                            settings.sched_output = f.display().to_string();
                        }
                        ui.end_row();

                        ui.label(t.frequency);
                        egui::ComboBox::from_id_salt("sched_freq")
                            .selected_text(t.freqs.get(settings.sched_freq).copied().unwrap_or(t.freqs[0]))
                            .show_ui(ui, |ui| {
                                for (i, f) in t.freqs.iter().enumerate() {
                                    ui.selectable_value(&mut settings.sched_freq, i, *f);
                                }
                            });
                        ui.end_row();

                        if settings.sched_freq == 1 {
                            ui.label(t.weekday);
                            egui::ComboBox::from_id_salt("sched_day")
                                .selected_text(
                                    t.weekdays
                                        .get(settings.sched_weekday)
                                        .copied()
                                        .unwrap_or(t.weekdays[0]),
                                )
                                .show_ui(ui, |ui| {
                                    for (i, d) in t.weekdays.iter().enumerate() {
                                        ui.selectable_value(&mut settings.sched_weekday, i, *d);
                                    }
                                });
                            ui.end_row();
                        }

                        if settings.sched_freq != 2 {
                            ui.label(t.time);
                            ui.text_edit_singleline(&mut settings.sched_time);
                            ui.end_row();
                        }
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(theme::primary(theme::label(icon::CHECK, t.create_task, false)))
                        .clicked()
                    {
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
                                format!("{}\n{out}", t.task_created)
                            }
                            Err(e) => fmt(t.failed, &[("err", &format!("{e:#}"))]),
                        };
                    }
                    if ui.button(theme::label(icon::PLAY, t.run_now, false)).clicked() {
                        sched.status = match scheduler::run_task_now() {
                            Ok(out) => format!("{}\n{out}", t.triggered),
                            Err(e) => fmt(t.failed, &[("err", &format!("{e:#}"))]),
                        };
                    }
                    if ui.button(theme::label(icon::TRASH, t.remove_task, false)).clicked() {
                        sched.status = match scheduler::remove_task() {
                            Ok(out) => format!("{}\n{out}", t.removed),
                            Err(e) => fmt(t.failed, &[("err", &format!("{e:#}"))]),
                        };
                    }
                    if ui
                        .button(theme::label(icon::ARROWS_CLOCKWISE, t.refresh, false))
                        .clicked()
                    {
                        sched.status = scheduler::query_task().unwrap_or_else(|| t.no_task.into());
                    }
                });

                if !sched.status.is_empty() {
                    ui.add_space(10.0);
                    ui.add(
                        egui::TextEdit::multiline(&mut sched.status)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(560.0)
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

//! The lock screen: pick the 4-digit number on first start, type it to open the app.
//! While it shows, nothing else of the app is drawn.

use super::*;
use crate::lock::{clean_input, Outcome, Screen, PIN_LEN};

const INPUT_ID: &str = "lock_pin_input";

impl DeskApp {
    /// Locks now (the top-bar button and Ctrl+L). With no number set yet, asks to pick one.
    pub(super) fn lock_now(&mut self) {
        if self.settings.lock_pin.is_empty() {
            self.lock = crate::lock::Lock::starting("");
        } else {
            self.lock.lock();
        }
    }

    /// 설정 > 잠금 번호 바꾸기: the current number first (when there is one).
    pub(super) fn begin_pin_change(&mut self) {
        self.lock.begin_change();
        if self.settings.lock_pin.is_empty() {
            self.lock.screen = Screen::Setup { first: None };
        }
    }

    pub(super) fn lock_screen(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let wait = self.lock.waiting_secs(now);
        if wait > 0 {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if self.lock.changing && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.lock.cancel();
            return;
        }
        let (title, note) = match &self.lock.screen {
            Screen::Setup { first: None } => (crate::i18n::tr("잠금 번호 정하기"), crate::i18n::tr("이 PC에서 MyDocDesk를 열 때 쓸 숫자 4자리를 정하세요")),
            Screen::Setup { first: Some(_) } => (crate::i18n::tr("잠금 번호 정하기"), crate::i18n::tr("확인을 위해 같은 번호를 한 번 더 입력해 주세요")),
            Screen::ChangeVerify => (crate::i18n::tr("지금 쓰는 잠금 번호를 입력하세요"), crate::i18n::tr("확인되면 새 번호를 정합니다")),
            _ => (crate::i18n::tr("잠금 번호를 입력하세요"), crate::i18n::tr("숫자 4자리를 넣으면 바로 열립니다")),
        };
        let mut submit = false;
        let mut cancel = false;
        egui::CentralPanel::default().frame(egui::Frame::default().fill(BG())).show(ctx, |ui| {
            let h = ui.available_height();
            ui.vertical_centered(|ui| {
                ui.add_space((h * 0.22).max(24.0));
                card().inner_margin(Margin::symmetric(40, 32)).show(ui, |ui| {
                    ui.set_width(380.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🔒").size(30.0).color(ACCENT()));
                        ui.add_space(6.0);
                        ui.label(bold("MyDocDesk", 14.0).color(PURPLE()));
                        ui.add_space(14.0);
                        ui.label(bold(title, 20.0));
                        ui.add_space(4.0);
                        ui.label(muted(note));
                        ui.add_space(18.0);

                        // four dots: how many digits are in
                        let n = self.lock.input.chars().count();
                        let (r, _) = ui.allocate_exact_size(vec2(PIN_LEN as f32 * 28.0, 20.0), Sense::hover());
                        for k in 0..PIN_LEN {
                            let c = egui::pos2(r.left() + 14.0 + k as f32 * 28.0, r.center().y);
                            if k < n {
                                ui.painter().circle_filled(c, 7.0, ACCENT());
                            } else {
                                ui.painter().circle_stroke(c, 7.0, Stroke::new(1.5, LINE2()));
                            }
                        }
                        ui.add_space(12.0);

                        let edit = egui::TextEdit::singleline(&mut self.lock.input)
                            .id(egui::Id::new(INPUT_ID))
                            .password(true)
                            .char_limit(PIN_LEN)
                            .hint_text(crate::i18n::tr("숫자 4자리"))
                            .horizontal_align(Align::Center)
                            .desired_width(160.0);
                        let resp = ui.add_enabled(wait == 0, edit);
                        if wait == 0 && !resp.has_focus() {
                            resp.request_focus();
                        }
                        let cleaned = clean_input(&self.lock.input);
                        if cleaned != self.lock.input {
                            self.lock.input = cleaned;
                        }
                        if self.lock.input.chars().count() == PIN_LEN {
                            submit = true;
                        }

                        ui.add_space(10.0);
                        if wait > 0 {
                            ui.label(RichText::new(crate::ft!("{}초 뒤에 다시 입력할 수 있습니다", "Có thể nhập lại sau {} giây", wait)).color(WARN()));
                        } else if !self.lock.error.is_empty() {
                            ui.label(RichText::new(self.lock.error.clone()).color(BAD()));
                        }
                        if matches!(self.lock.screen, Screen::Setup { .. }) {
                            ui.add_space(6.0);
                            ui.label(RichText::new(crate::i18n::tr("번호는 이 PC에만 저장됩니다")).size(12.0).color(MUTED()));
                        }
                        if self.lock.changing {
                            ui.add_space(10.0);
                            if ui.button(crate::i18n::tr("취소")).clicked() {
                                cancel = true;
                            }
                        }
                    });
                });
            });
        });
        if cancel {
            self.lock.cancel();
            return;
        }
        if submit {
            match self.lock.submit(&mut self.settings.lock_pin, now) {
                Outcome::PinSet => {
                    self.settings_dirty = true;
                    // only claim it's done when it is on disk (a failure keeps retrying and is shown)
                    if self.save_now().is_ok() {
                        self.toast(crate::i18n::tr("잠금 번호를 정했습니다. 자리를 비울 땐 🔒 잠금 (Ctrl+L)"), GOOD());
                    }
                }
                Outcome::Unlocked => {}
                _ => {}
            }
            ctx.memory_mut(|m| m.request_focus(egui::Id::new(INPUT_ID)));
        }
    }
}

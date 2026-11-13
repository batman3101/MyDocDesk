//! The "AI 연결" page: subscriptions first, then AI on this PC, then API keys; a model can be
//! chosen for every service.

use super::*;
use crate::cli::CliInfo;
use crate::local::LocalAi;
use crate::model::Route;
use std::collections::HashMap;

type DetectResult = (Vec<LocalAi>, Vec<CliInfo>);

#[derive(Default)]
pub struct AiConn {
    pub rx: Option<Receiver<DetectResult>>,
    pub scanned: bool,
    pub local: Vec<LocalAi>,
    pub clis: Vec<CliInfo>,
    pub models: HashMap<Provider, Vec<String>>,
    pub models_rx: Option<Receiver<(Provider, Result<Vec<String>, String>)>>,
    pub models_msg: String,
    pub start_rx: Option<Receiver<Result<(), String>>>,
    /// The model picked in each service's box before pressing "이걸로 사용".
    pub pick: HashMap<Provider, String>,
    pub api_tab: Option<Provider>,
}

fn brand_color(p: Provider) -> Color32 {
    match p {
        Provider::ClaudeSub | Provider::Anthropic => Color32::from_rgb(0xc9, 0x64, 0x42),
        Provider::ChatGptSub | Provider::OpenAi => Color32::from_rgb(0x10, 0x8a, 0x6c),
        Provider::GeminiSub | Provider::Gemini => Color32::from_rgb(0x2a, 0x78, 0xd6),
        Provider::LmStudio => Color32::from_rgb(0x5b, 0x4b, 0xc4),
        _ => Color32::from_rgb(0x52, 0x5b, 0x6c),
    }
}

fn logo(ui: &mut egui::Ui, p: Provider, letter: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(6), brand_color(p));
    ui.painter().text(r.center(), Align2::CENTER_CENTER, letter, FontId::new(15.0, egui::FontFamily::Name("bold".into())), Color32::WHITE);
}

/// A model box: a list to pick from plus free typing.
pub(super) fn model_box(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[String], hint: &str, width: f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        changed |= ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(width)).changed();
        egui::ComboBox::from_id_salt(id).selected_text(crate::i18n::tr("목록")).width(56.0).show_ui(ui, |ui| {
            if options.is_empty() {
                ui.label(muted(crate::i18n::tr("목록 없음 — 직접 입력")));
            }
            for o in options {
                if ui.selectable_label(value == o, o).clicked() {
                    *value = o.clone();
                    changed = true;
                }
            }
        });
    });
    changed
}

impl DeskApp {
    pub(super) fn start_ai_detect(&mut self) {
        if self.aic.rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let local = crate::local::detect();
            let clis = crate::cli::detect();
            let _ = tx.send((local, clis));
        });
        self.aic.rx = Some(rx);
    }

    /// For screenshots: what this PC looked like when the feature was built (no programs are run).
    pub(super) fn demo_ai(&mut self) {
        use crate::local::LocalModel;
        self.aic.scanned = true;
        self.aic.clis = vec![
            CliInfo { provider: Provider::ClaudeSub, path: Some("C:\\Users\\me\\.local\\bin\\claude.exe".into()), version: "2.x (Claude Code)".into(), logged_in: Some(true) },
            CliInfo { provider: Provider::ChatGptSub, path: Some("C:\\Users\\me\\AppData\\Roaming\\npm\\codex.cmd".into()), version: "codex-cli".into(), logged_in: Some(true) },
            CliInfo { provider: Provider::GeminiSub, path: Some("C:\\Users\\me\\AppData\\Roaming\\npm\\gemini.cmd".into()), version: "gemini-cli".into(), logged_in: Some(true) },
        ];
        self.aic.local = vec![LocalAi {
            name: "LM Studio",
            provider: Provider::LmStudio,
            base_url: "http://127.0.0.1:1234/v1".into(),
            running: false,
            installed: true,
            note: crate::i18n::tr("서버가 꺼져 있습니다 — '서버 켜기'를 누르면 켜고 고른 모델을 자동으로 불러옵니다").into(),
            models: vec![
                LocalModel { id: "qwen3-vl-8b-instruct".into(), vision: true },
                LocalModel { id: "qwen3.5-4b".into(), vision: true },
                LocalModel { id: "qwen3.8-2b".into(), vision: false },
            ],
        }];
        self.settings.provider = Provider::ClaudeSub;
        self.settings.model = "sonnet".into();
    }

    pub(super) fn poll_ai_conn(&mut self) {
        if let Some(rx) = &self.aic.rx {
            if let Ok((local, clis)) = rx.try_recv() {
                for c in &clis {
                    if let Some(p) = &c.path {
                        self.settings.cli_paths.insert(c.provider.id().into(), p.display().to_string());
                    }
                }
                for l in &local {
                    self.aic.models.insert(l.provider, l.models.iter().map(|m| m.id.clone()).collect());
                }
                self.aic.local = local;
                self.aic.clis = clis;
                self.aic.scanned = true;
                self.aic.rx = None;
                self.settings_dirty = true;
            }
        }
        if let Some(rx) = &self.aic.models_rx {
            if let Ok((p, r)) = rx.try_recv() {
                match r {
                    Ok(list) => {
                        self.aic.models_msg = crate::ft!("{} 모델 {}개를 불러왔습니다", "Đã tải {1} mô hình của {0}", p.short(), list.len());
                        self.aic.models.insert(p, list);
                    }
                    Err(e) => self.aic.models_msg = e,
                }
                self.aic.models_rx = None;
            }
        }
        if let Some(rx) = &self.aic.start_rx {
            if let Ok(r) = rx.try_recv() {
                self.aic.start_rx = None;
                match r {
                    Ok(_) => {
                        self.toast(crate::i18n::tr("로컬 AI 서버를 켰습니다"), GOOD());
                        self.aic.rx = None;
                        self.start_ai_detect();
                    }
                    Err(e) => self.toast(e, BAD()),
                }
            }
        }
    }

    fn load_models(&mut self, p: Provider) {
        let s = self.settings.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send((p, ai::list_models(&s, p)));
        });
        self.aic.models_rx = Some(rx);
        self.aic.models_msg = crate::ft!("{} 모델 목록을 불러오는 중…", "Đang tải danh sách mô hình của {}…", p.short());
    }

    fn use_provider(&mut self, p: Provider, model: String, base_url: String) {
        self.settings.provider = p;
        self.settings.model = model;
        self.settings.base_url = base_url;
        self.settings_dirty = true;
        self.test_msg.clear();
        self.toast(crate::ft!("이제 {}로 답합니다", "Từ giờ trả lời bằng {}", p.short()), GOOD());
    }

    pub(super) fn models_for(&self, p: Provider) -> Vec<String> {
        let mut v = self.aic.models.get(&p).cloned().unwrap_or_default();
        if v.is_empty() {
            v = p.preset_models().iter().map(|s| s.to_string()).collect();
        }
        v
    }

    pub(super) fn page_ai(&mut self, ui: &mut egui::Ui) {
        if !self.aic.scanned && self.aic.rx.is_none() && !self.store.read_only {
            self.start_ai_detect();
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            self.ai_current(ui);
            ui.add_space(10.0);
            self.ai_subscriptions(ui);
            ui.add_space(10.0);
            self.ai_local(ui);
            ui.add_space(10.0);
            self.ai_api(ui);
            ui.add_space(10.0);
            self.ai_verdict(ui);
        });
    }

    fn section(ui: &mut egui::Ui, icon: &str, title: &str, note: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon).size(15.0).color(TEXT2()));
            ui.label(bold(title, 15.5));
            ui.label(RichText::new(note).color(TEXT2()).size(12.5));
        });
        ui.add_space(4.0);
    }

    fn ai_current(&mut self, ui: &mut egui::Ui) {
        let p = self.settings.provider;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let letter = match p {
                    Provider::ClaudeSub | Provider::Anthropic => "C",
                    Provider::ChatGptSub | Provider::OpenAi | Provider::GeminiSub | Provider::Gemini => "G",
                    Provider::Ollama => "O",
                    Provider::LmStudio | Provider::LocalOther => "L",
                    Provider::None => "-",
                };
                logo(ui, p, letter);
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::i18n::tr("지금 답하는 AI")).color(TEXT2()).size(12.5));
                    ui.horizontal(|ui| {
                        ui.label(bold(p.label(), 16.0));
                        let (t, c) = match p.route() {
                            Route::Off => (crate::i18n::tr("검색만"), MUTED()),
                            Route::Local => (crate::i18n::tr("내 PC · 무료"), GOOD()),
                            Route::Api => (crate::i18n::tr("API · 사용한 만큼 과금"), WARN()),
                            Route::Subscription => (crate::i18n::tr("구독 · 추가 비용 없음"), ACCENT()),
                        };
                        chip(ui, t, c);
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if p != Provider::None {
                        if ui.add_enabled(self.test_rx.is_none(), egui::Button::new(crate::i18n::tr("연결 테스트"))).clicked() {
                            let s = self.settings.clone();
                            let (tx, rx) = channel();
                            std::thread::spawn(move || {
                                let _ = tx.send(ai::ask(&s, crate::i18n::tr("짧게 답하세요."), crate::i18n::tr("'연결되었습니다'라고만 답하세요.")));
                            });
                            self.test_rx = Some(rx);
                            self.test_msg = crate::i18n::tr("확인 중… (구독 연결은 첫 답에 10~30초 걸릴 수 있음)").into();
                        }
                    }
                    if p != Provider::None && ui.button(crate::i18n::tr("끄기")).on_hover_text(crate::i18n::tr("AI 없이 내 PC 검색만 사용")).clicked() {
                        self.use_provider(Provider::None, String::new(), String::new());
                    }
                });
            });
            if p != Provider::None {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::i18n::tr("모델")).color(TEXT2()));
                    let opts = self.models_for(p);
                    let hint = if p.default_model().is_empty() { crate::i18n::tr("비우면 서비스 기본 모델").to_string() } else { crate::ft!("비우면 {}", "Để trống: {}", p.default_model()) };
                    if model_box(ui, "cur_model", &mut self.settings.model, &opts, &hint, 260.0) {
                        self.settings_dirty = true;
                    }
                    if matches!(p.route(), Route::Api | Route::Local) && ui.small_button(crate::i18n::tr("목록 새로 불러오기")).clicked() {
                        self.load_models(p);
                    }
                    ui.label(RichText::new(&self.test_msg).color(TEXT2()).size(12.5));
                });
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.settings.vision, crate::i18n::tr("문서 속 그림·차트·스캔도 AI로 해석")).changed() {
                        self.settings_dirty = true;
                    }
                    if self.settings.vision {
                        ui.label(RichText::new(crate::i18n::tr("그림용 모델")).color(TEXT2()));
                        let hint = ai::vision_model(&self.settings);
                        let opts = self.models_for(p);
                        if model_box(ui, "vis_model", &mut self.settings.vision_model, &opts, &hint, 200.0) {
                            self.settings_dirty = true;
                        }
                    }
                    if matches!(p, Provider::ClaudeSub | Provider::GeminiSub) {
                        ui.label(RichText::new(crate::i18n::tr("이 연결은 그림 해석을 못 합니다")).color(WARN()).size(12.5));
                    }
                });
                if self.settings.local_only && p.is_cloud() {
                    ui.label(RichText::new(crate::i18n::tr("⚠ '내 PC 전용 모드'가 켜져 있어 구독·API로는 보내지 않습니다. '계정·동기화'에서 끄면 쓸 수 있습니다.")).color(WARN()));
                }
            }
        });
    }

    fn ai_subscriptions(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "⭐", crate::i18n::tr("구독 계정으로 연결"), crate::i18n::tr("이미 내는 구독료 안에서 쓰기 — 각 회사의 공식 프로그램에 로그인해 두면 이 앱이 그 프로그램을 통해 묻습니다"));
        let scanning = self.aic.rx.is_some();
        let clis = self.aic.clis.clone();
        ui.columns(3, |cols| {
            for (i, p) in [Provider::ClaudeSub, Provider::ChatGptSub, Provider::GeminiSub].into_iter().enumerate() {
                let info = clis.iter().find(|c| c.provider == p).cloned();
                let ui = &mut cols[i];
                card().show(ui, |ui| {
                    let w = ui.available_width();
                    ui.set_width(w);
                    ui.set_max_width(w);
                    ui.set_min_height(232.0);
                    let (name, letter, plan, tool) = match p {
                        Provider::ClaudeSub => ("Claude", "C", "Claude Pro · Max", "Claude Code"),
                        Provider::ChatGptSub => ("ChatGPT", "G", "ChatGPT Plus · Pro · Business", "Codex CLI"),
                        _ => ("Gemini", "G", crate::i18n::tr("Google 계정 · AI Pro"), "Gemini CLI"),
                    };
                    ui.horizontal(|ui| {
                        logo(ui, p, letter);
                        ui.vertical(|ui| {
                            ui.label(bold(name, 15.5));
                            ui.label(RichText::new(plan).color(TEXT2()).size(12.0));
                        });
                    });
                    ui.add_space(4.0);
                    let installed = info.as_ref().map(|i| i.path.is_some()).unwrap_or(false);
                    let logged = info.as_ref().and_then(|i| i.logged_in);
                    ui.horizontal_wrapped(|ui| {
                        if scanning && info.is_none() {
                            chip(ui, crate::i18n::tr("찾는 중…"), MUTED());
                        } else if p == Provider::GeminiSub && installed {
                            chip(ui, crate::i18n::tr("개인 로그인 중단됨 → API 권장"), WARN());
                        } else if !installed {
                            chip(ui, &crate::ft!("{tool} 설치 필요", "Cần cài {tool}", tool = tool), MUTED());
                        } else if logged == Some(false) {
                            chip(ui, crate::i18n::tr("로그인 필요"), WARN());
                        } else {
                            chip(ui, crate::i18n::tr("사용 가능"), GOOD());
                        }
                        if self.settings.provider == p {
                            chip(ui, crate::i18n::tr("사용 중"), ACCENT());
                        }
                    });
                    let note = match p {
                        Provider::ClaudeSub => crate::i18n::tr("질문마다 약 1천 토큰이 더해집니다 (이 앱이 Claude Code의 부가 기능을 끄고 부름). 그림 해석은 안 됨."),
                        Provider::ChatGptSub => crate::i18n::tr("Codex의 기본 지시 때문에 질문마다 약 3만 토큰이 구독 한도에서 빠집니다. 그림 해석 가능."),
                        _ => crate::i18n::tr("이 PC에서 확인: 구글이 Gemini CLI의 개인 계정 로그인을 중단했습니다. Gemini는 아래 'API 키'(무료 한도 있음)로 연결하세요."),
                    };
                    ui.add(egui::Label::new(RichText::new(note).color(TEXT2()).size(12.5)).wrap());
                    if let Some(i) = &info {
                        if let Some(path) = &i.path {
                            ui.label(RichText::new(format!("{} · {}", tool, if i.version.is_empty() { path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() } else { i.version.clone() })).color(MUTED()).size(11.5));
                        }
                    }
                    ui.add_space(4.0);
                    let mut pick = self.aic.pick.get(&p).cloned().unwrap_or_else(|| if self.settings.provider == p { self.settings.model.clone() } else { p.default_model().to_string() });
                    let opts: Vec<String> = p.preset_models().iter().map(|s| s.to_string()).collect();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(crate::i18n::tr("모델")).color(TEXT2()).size(12.5));
                        if model_box(ui, &format!("sub_{}", p.id()), &mut pick, &opts, crate::i18n::tr("비우면 기본"), (w - 210.0).max(80.0)) {
                            self.aic.pick.insert(p, pick.clone());
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(installed && p != Provider::GeminiSub, egui::Button::new(RichText::new(crate::i18n::tr("이걸로 사용")).color(ACCENT_INK())).fill(ACCENT())).clicked() {
                            self.use_provider(p, pick.clone(), String::new());
                        }
                        if installed && ui.button(crate::i18n::tr("로그인")).on_hover_text(crate::i18n::tr("공식 프로그램의 로그인 창을 엽니다")).clicked() {
                            if let Some(path) = info.as_ref().and_then(|i| i.path.clone()) {
                                if let Err(e) = crate::cli::open_login(p, &path) {
                                    self.toast(e, BAD());
                                }
                            }
                        }
                        if ui.button(crate::i18n::tr("설치 안내")).clicked() {
                            let _ = open::that(p.help_url());
                        }
                    });
                });
            }
        });
    }

    fn ai_local(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            Self::section(ui, "🖥", crate::i18n::tr("내 PC의 AI"), crate::i18n::tr("인터넷 없이 무료 — 설치된 프로그램을 찾아 모델을 고릅니다"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.aic.rx.is_some() {
                    ui.spinner();
                } else if ui.button(crate::i18n::tr("🔄 다시 찾기")).clicked() && !self.store.read_only {
                    self.start_ai_detect();
                }
            });
        });
        let locals = self.aic.local.clone();
        if locals.is_empty() {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(if self.aic.rx.is_some() { crate::i18n::tr("찾는 중…") } else { crate::i18n::tr("이 PC에서 실행 중이거나 설치된 로컬 AI를 찾지 못했습니다.") }).color(TEXT2()));
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("Ollama 받기")).clicked() {
                        let _ = open::that(Provider::Ollama.help_url());
                    }
                    if ui.button(crate::i18n::tr("LM Studio 받기")).clicked() {
                        let _ = open::that(Provider::LmStudio.help_url());
                    }
                });
            });
        }
        for l in locals {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    logo(ui, l.provider, &l.name.chars().next().unwrap_or('L').to_string());
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(bold(l.name, 15.0));
                            chip(ui, if l.running { crate::i18n::tr("켜짐") } else { crate::i18n::tr("꺼짐") }, if l.running { GOOD() } else { MUTED() });
                            if self.settings.provider == l.provider {
                                chip(ui, crate::i18n::tr("사용 중"), ACCENT());
                            }
                        });
                        ui.label(RichText::new(crate::ft!("{} · 모델 {}개", "{} · {} mô hình", l.base_url, l.models.len())).color(MUTED()).size(12.0));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !l.running && l.installed && matches!(l.provider, Provider::Ollama | Provider::LmStudio) {
                            if self.aic.start_rx.is_some() {
                                ui.spinner();
                            } else if ui.button(crate::i18n::tr("서버 켜기")).clicked() && !self.store.read_only {
                                let p = l.provider;
                                let (tx, rx) = channel();
                                std::thread::spawn(move || {
                                    let r = crate::local::start(p);
                                    std::thread::sleep(Duration::from_secs(2));
                                    let _ = tx.send(r);
                                });
                                self.aic.start_rx = Some(rx);
                            }
                        }
                    });
                });
                if !l.note.is_empty() {
                    ui.label(RichText::new(&l.note).color(TEXT2()).size(12.5));
                }
                ui.add_space(4.0);
                egui::Grid::new(("lm", l.name)).num_columns(3).striped(true).spacing(vec2(18.0, 6.0)).show(ui, |ui| {
                    for m in &l.models {
                        let using = self.settings.provider == l.provider && self.settings.model == m.id;
                        ui.label(RichText::new(&m.id).color(TEXT()).monospace());
                        ui.horizontal(|ui| {
                            if m.vision {
                                chip(ui, crate::i18n::tr("그림 가능"), ACCENT());
                            }
                        });
                        if using {
                            chip(ui, crate::i18n::tr("사용 중"), GOOD());
                        } else if ui.small_button(crate::i18n::tr("이 모델 사용")).clicked() {
                            self.use_provider(l.provider, m.id.clone(), l.base_url.clone());
                            if m.vision {
                                self.settings.vision_model = m.id.clone();
                            }
                        }
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
        }
    }

    fn ai_api(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "🔑", crate::i18n::tr("API 키로 연결"), crate::i18n::tr("가장 빠르고 그림 해석까지 되지만 쓴 만큼 요금이 나갑니다 — 키는 이 기기에만 저장"));
        let tab = self.aic.api_tab.unwrap_or(if self.settings.provider.route() == Route::Api { self.settings.provider } else { Provider::OpenAi });
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for p in [Provider::OpenAi, Provider::Anthropic, Provider::Gemini] {
                    if theme::tab(ui, tab == p, p.short()).clicked() {
                        self.aic.api_tab = Some(p);
                    }
                }
            });
            ui.add_space(8.0);
            let p = tab;
            egui::Grid::new("apiform").num_columns(2).spacing(vec2(14.0, 10.0)).show(ui, |ui| {
                ui.label(RichText::new(crate::i18n::tr("API 키")).color(TEXT2()));
                ui.horizontal(|ui| {
                    let mut key = self.settings.key_for(p);
                    if ui.add(egui::TextEdit::singleline(&mut key).password(true).hint_text(crate::i18n::tr("이 기기에만 저장되고 동기화하지 않습니다")).desired_width(360.0)).changed() {
                        self.settings.api_keys.insert(p.id().into(), key);
                        self.settings_dirty = true;
                    }
                    if ui.button(crate::i18n::tr("키 발급 페이지")).clicked() {
                        let _ = open::that(p.help_url());
                    }
                });
                ui.end_row();
                if p == Provider::OpenAi {
                    ui.label(RichText::new(crate::i18n::tr("주소")).color(TEXT2()));
                    ui.horizontal(|ui| {
                        let mut url = if self.settings.provider == p { self.settings.base_url.clone() } else { String::new() };
                        if ui.add(egui::TextEdit::singleline(&mut url).hint_text(p.default_url()).desired_width(360.0)).changed() && self.settings.provider == p {
                            self.settings.base_url = url;
                            self.settings_dirty = true;
                        }
                        ui.label(muted(crate::i18n::tr("OpenRouter 같은 호환 서비스도 주소만 바꾸면 됩니다")));
                    });
                    ui.end_row();
                }
                ui.label(RichText::new(crate::i18n::tr("모델")).color(TEXT2()));
                ui.horizontal(|ui| {
                    let mut pick = self.aic.pick.get(&p).cloned().unwrap_or_else(|| if self.settings.provider == p { self.settings.model.clone() } else { p.default_model().to_string() });
                    let opts = self.aic.models.get(&p).cloned().unwrap_or_default();
                    if model_box(ui, &format!("api_{}", p.id()), &mut pick, &opts, p.default_model(), 260.0) {
                        self.aic.pick.insert(p, pick.clone());
                    }
                    if ui.add_enabled(self.aic.models_rx.is_none(), egui::Button::new(crate::i18n::tr("모델 목록 불러오기"))).clicked() {
                        self.load_models(p);
                    }
                });
                ui.end_row();
                ui.label(RichText::new(crate::i18n::tr("가격 (100만 토큰당 $)")).color(TEXT2()));
                ui.horizontal(|ui| {
                    ui.label(muted(crate::i18n::tr("질문")));
                    if ui.add(egui::DragValue::new(&mut self.settings.price_in_per_m).speed(0.1).range(0.0..=100.0)).changed() {
                        self.settings_dirty = true;
                    }
                    ui.label(muted(crate::i18n::tr("답변")));
                    if ui.add(egui::DragValue::new(&mut self.settings.price_out_per_m).speed(0.1).range(0.0..=200.0)).changed() {
                        self.settings_dirty = true;
                    }
                    ui.label(muted(crate::i18n::tr("대시보드의 예상 비용 계산에만 씁니다")));
                });
                ui.end_row();
            });
            if !self.aic.models_msg.is_empty() {
                ui.label(RichText::new(&self.aic.models_msg).color(TEXT2()).size(12.5));
            }
            ui.add_space(4.0);
            let has_key = !self.settings.key_for(p).trim().is_empty();
            if ui.add_enabled(has_key, egui::Button::new(RichText::new(crate::ft!("{} 사용", "Dùng {}", p.short())).color(ACCENT_INK())).fill(ACCENT())).clicked() {
                let m = self.aic.pick.get(&p).cloned().unwrap_or_else(|| p.default_model().to_string());
                let url = if self.settings.provider == p { self.settings.base_url.clone() } else { String::new() };
                self.use_provider(p, m, url);
            }
        });
    }

    fn ai_verdict(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "ℹ", crate::i18n::tr("어떤 방식이 가능한가요"), crate::i18n::tr("2026년 11월 이 PC에서 실제로 시험한 결과"));
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("verdict").num_columns(4).striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
                for h in [crate::i18n::tr("서비스"), crate::i18n::tr("구독 계정"), crate::i18n::tr("API 키"), crate::i18n::tr("모델 고르기")] {
                    ui.label(RichText::new(h).color(TEXT2()).strong());
                }
                ui.end_row();
                let rows: [(&str, &str, Color32, &str, &str); 4] = [
                    ("Claude", crate::i18n::tr("가능 — Claude Code 로그인"), GOOD(), crate::i18n::tr("가능"), crate::i18n::tr("sonnet·opus·haiku 또는 정식 이름 / API는 목록 자동")),
                    ("ChatGPT", crate::i18n::tr("가능 — Codex 로그인"), GOOD(), crate::i18n::tr("가능"), crate::i18n::tr("직접 입력 또는 예시 / API는 목록 자동")),
                    ("Gemini", crate::i18n::tr("불가 — 구글이 개인 로그인 중단"), BAD(), crate::i18n::tr("가능 (무료 한도 있음)"), crate::i18n::tr("API 목록 자동")),
                    (crate::i18n::tr("내 PC (LM Studio·Ollama 등)"), crate::i18n::tr("해당 없음"), MUTED(), crate::i18n::tr("필요 없음"), crate::i18n::tr("설치된 모델 자동 검색")),
                ];
                for (s, sub, c, api, models) in rows {
                    ui.label(RichText::new(s).color(TEXT()));
                    chip(ui, sub, c);
                    ui.label(RichText::new(api).color(TEXT2()));
                    ui.label(RichText::new(models).color(TEXT2()).size(12.5));
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(crate::i18n::tr("구독 상품은 다른 앱에 API 키를 주지 않습니다. 그래서 회사가 직접 만든 명령줄 프로그램에 구독 계정으로 로그인해 두고, 이 앱은 그 프로그램에 질문만 넘깁니다. 로그인 정보를 꺼내 쓰는 방식은 약관 위반이라 쓰지 않습니다. 구독 연결은 첫 답이 10~30초로 느리고 구독 사용 한도를 함께 쓰므로, 그림 해석이나 평가처럼 많이 묻는 작업은 API나 내 PC의 AI가 낫습니다."))
                        .color(TEXT2())
                        .size(12.5),
                )
                .wrap(),
            );
        });
    }
}

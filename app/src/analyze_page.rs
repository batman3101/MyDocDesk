//! The "문서 분석" page: choose a file and options → watch the steps → read the result.

use super::*;
use crate::analyze::{self, Analysis, Block, Kind, Msg, Opts, STEPS};

pub struct AnalyzeState {
    pub file: Option<PathBuf>,
    pub opts: Opts,
    pub rx: Option<Receiver<Msg>>,
    pub step: usize,
    pub pct: f32,
    pub msg: String,
    pub result: Option<Analysis>,
    pub tab: u8,
    pub saved: bool,
    pub error: String,
}

impl Default for AnalyzeState {
    fn default() -> Self {
        AnalyzeState { file: None, opts: Opts::default(), rx: None, step: 0, pct: 0.0, msg: String::new(), result: None, tab: 0, saved: false, error: String::new() }
    }
}

fn level_colors() -> [Color32; 4] {
    [GOOD(), ACCENT(), WARN(), BAD()]
}
const LEVEL_NAMES: [&str; 4] = ["L1 일반 글", "L2 제목·목록", "L3 표", "L4 표 안의 표·차트·그림"];

fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(12), if *on { GOOD() } else { LINE2() });
    let x = egui::lerp(rect.left() + 12.0..=rect.right() - 12.0, t);
    p.circle_filled(egui::pos2(x, rect.center().y), 8.5, Color32::WHITE);
    resp
}

fn arc_points(c: egui::Pos2, r: f32, a0: f32, a1: f32) -> Vec<egui::Pos2> {
    let n = ((a1 - a0).abs() * 30.0).ceil().max(2.0) as usize;
    (0..=n).map(|i| {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        egui::pos2(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect()
}

fn donut(ui: &mut egui::Ui, values: [u32; 4], size: f32, center: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let r = size / 2.0 - 10.0;
    let total: u32 = values.iter().sum();
    p.add(egui::Shape::line(arc_points(c, r, 0.0, std::f32::consts::TAU), Stroke::new(14.0, LINE())));
    if total > 0 {
        let mut a = -std::f32::consts::FRAC_PI_2;
        for (i, v) in values.iter().enumerate() {
            if *v == 0 {
                continue;
            }
            let span = *v as f32 / total as f32 * std::f32::consts::TAU;
            p.add(egui::Shape::line(arc_points(c, r, a, a + span), Stroke::new(14.0, level_colors()[i])));
            a += span;
        }
    }
    p.text(c, Align2::CENTER_CENTER, center, FontId::new(22.0, egui::FontFamily::Name("bold".into())), TEXT());
}

fn gauge(ui: &mut egui::Ui, value: u8, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, w * 0.55), Sense::hover());
    let p = ui.painter();
    let c = egui::pos2(rect.center().x, rect.bottom() - 6.0);
    let r = w / 2.0 - 12.0;
    use std::f32::consts::PI;
    p.add(egui::Shape::line(arc_points(c, r, PI, 2.0 * PI), Stroke::new(12.0, LINE())));
    let col = if value >= 85 { GOOD() } else if value >= 60 { WARN() } else { BAD() };
    p.add(egui::Shape::line(arc_points(c, r, PI, PI + PI * value as f32 / 100.0), Stroke::new(12.0, col)));
    p.text(c - vec2(0.0, 18.0), Align2::CENTER_CENTER, format!("{value}%"), FontId::new(24.0, egui::FontFamily::Name("bold".into())), TEXT());
}

fn dashed_rect(ui: &egui::Ui, rect: egui::Rect, color: Color32) {
    let pts = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
    ui.painter().extend(egui::Shape::dashed_line(&pts, Stroke::new(1.5, color), 8.0, 6.0));
}

impl DeskApp {
    pub(super) fn poll_analyze(&mut self) {
        let Some(rx) = &self.an.rx else { return };
        let mut done = None;
        let (msgs, gone) = drain(rx);
        if gone && !msgs.iter().any(|m| matches!(m, Msg::Done(_))) {
            // the analysis thread ended without its result
            self.an.rx = None;
            self.toast(crate::ft!("문서 분석이 중간에 중단되었습니다. 다시 시도해 주세요.", "Phân tích tài liệu bị dừng giữa chừng. Hãy thử lại."), BAD());
            return;
        }
        for m in msgs {
            match m {
                Msg::Step(s, p, msg) => {
                    self.an.step = s;
                    self.an.pct = p;
                    self.an.msg = msg;
                }
                Msg::Done(r) => done = Some(r),
            }
        }
        if let Some(r) = done {
            self.an.rx = None;
            match *r {
                Ok(a) => {
                    self.lib.usage.extend(a.usage.iter().cloned());
                    self.lib_dirty = true;
                    self.an.saved = false;
                    self.an.tab = 0;
                    let save = self.an.opts.save;
                    self.an.result = Some(a);
                    if save {
                        self.save_analysis_to_library();
                    }
                    self.toast(crate::i18n::tr("분석을 마쳤습니다"), GOOD());
                }
                Err(e) => {
                    self.an.error = e.clone();
                    self.toast(e, BAD());
                }
            }
        }
    }

    fn save_analysis_to_library(&mut self) {
        let Some(a) = &self.an.result else { return };
        if self.an.saved {
            return;
        }
        let d = a.doc.clone();
        if !self.store.read_only && !a.bytes.is_empty() {
            let _ = std::fs::write(self.store.file_path(&d), &a.bytes);
        }
        self.selected_doc = Some(d.id.clone());
        self.lib.docs.push(d);
        self.touch_lib();
        self.an.saved = true;
    }

    pub(super) fn start_analysis(&mut self) {
        let Some(path) = self.an.file.clone() else { return };
        let (tx, rx) = channel();
        let opts = self.an.opts;
        let settings = self.settings.clone();
        std::thread::spawn(move || analyze::run(path, opts, settings, tx));
        self.an.rx = Some(rx);
        self.an.step = 0;
        self.an.pct = 0.0;
        self.an.msg.clear();
        self.an.error.clear();
        self.an.result = None;
    }

    /// For screenshots: a finished analysis of a sample contract (no AI, nothing saved).
    pub(super) fn demo_analysis(&mut self) {
        let text = "# 근로 계약서\n\n(주)샘플상사(이하 \"사업주\")와 근로자(이하 \"근로자\")는 다음과 같이 근로계약을 체결한다.\n\n1. 근로자 인적사항\n성명 | 생년월일 | 연락처 | 주소\n홍길동 | 1990-01-01 | [표: 휴대폰 · 010-1234-5678 / 비상 · 02-555-0101] | 서울시 샘플구\n\n2. 근무 조건\n- 근무 장소: 본사 3층\n- 근무 시간: 09:00 ~ 18:00 (휴게 1시간)\n- 업무 내용: 고객 상담 및 자료 정리\n\n3. 임금\n구분 | 금액 | 지급일\n기본급 | 2,500,000원 | 매월 25일\n식대 | 200,000원 | 매월 25일\n\n본 계약에 정하지 않은 사항은 근로기준법에 따른다. 계약서는 2부를 작성해 각자 1부씩 보관한다.\n\n[차트 · 막대] 연차 사용 계획\n- 2026: 1분기=3, 2분기=4, 3분기=5, 4분기=3";
        let mut doc = extract::make_doc("근로 계약서 (샘플)".into(), "근로 계약서 (샘플).hwp".into(), "hwp", text.into(), 48_128, false);
        doc.tables = 2;
        doc.nested_tables = 1;
        doc.charts = 1;
        let opts = Opts { mask_pii: true, ..Default::default() };
        let mut a = analyze::build(doc, Vec::new(), Vec::new(), "hwp", &opts, None, &|_, _, _| {});
        a.summary = vec![
            "사업주와 근로자 사이의 근로 조건을 정한 계약서 양식입니다.".into(),
            "인적사항, 근무 장소·시간, 임금과 지급일이 표로 정리되어 있습니다.".into(),
            "계약 체결 전 조건을 서로 확인하고 보관하는 데 쓰입니다.".into(),
        ];
        a.summary_by_ai = true;
        a.seconds = 1.8;
        self.an.opts = opts;
        self.an.saved = true;
        self.an.result = Some(a);
    }

    pub(super) fn page_analyze(&mut self, ui: &mut egui::Ui) {
        if self.an.rx.is_some() {
            self.analyze_running(ui);
        } else if self.an.result.is_some() {
            self.analyze_result(ui);
        } else {
            self.analyze_setup(ui);
        }
    }

    fn analyze_setup(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let left_w = (avail.x * 0.6).floor();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(left_w, avail.y), Layout::top_down(Align::Min), |ui| {
                card().show(ui, |ui| {
                    ui.set_width(left_w - 36.0);
                    let (rect, resp) = ui.allocate_exact_size(vec2(left_w - 36.0, 260.0), Sense::click());
                    let hovered = resp.hovered() || ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
                    ui.painter().rect_filled(rect, CornerRadius::same(12), if hovered { ACCENT_SOFT() } else { FIELD() });
                    dashed_rect(ui, rect.shrink(1.0), if hovered { ACCENT() } else { LINE() });
                    let c = rect.center();
                    match &self.an.file {
                        None => {
                            ui.painter().circle_filled(c - vec2(0.0, 52.0), 28.0, ACCENT().gamma_multiply(0.2));
                            ui.painter().text(c - vec2(0.0, 52.0), Align2::CENTER_CENTER, "📄", FontId::proportional(26.0), ACCENT());
                            ui.painter().text(c + vec2(0.0, 2.0), Align2::CENTER_CENTER, crate::i18n::tr("분석할 문서를 여기로 끌어다 놓으세요"), FontId::new(18.0, egui::FontFamily::Name("bold".into())), TEXT());
                            ui.painter().text(c + vec2(0.0, 32.0), Align2::CENTER_CENTER, crate::i18n::tr("또는 눌러서 파일 고르기"), FontId::proportional(14.0), MUTED());
                        }
                        Some(p) => {
                            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                            let kind = extract::kind_label(&ext);
                            let col = kind_color(&kind);
                            let b = egui::Rect::from_center_size(c - vec2(0.0, 48.0), vec2(56.0, 56.0));
                            ui.painter().rect_filled(b, CornerRadius::same(12), col.gamma_multiply(0.25));
                            ui.painter().text(b.center(), Align2::CENTER_CENTER, ext.to_uppercase(), FontId::proportional(15.0), col);
                            ui.painter().text(c + vec2(0.0, 6.0), Align2::CENTER_CENTER, &name, FontId::new(18.0, egui::FontFamily::Name("bold".into())), TEXT());
                            ui.painter().text(c + vec2(0.0, 34.0), Align2::CENTER_CENTER, crate::ft!("{} · {} · 눌러서 다른 파일 고르기", "{} · {} · bấm để chọn tệp khác", kind, fmt_size(size)), FontId::proportional(13.5), MUTED());
                        }
                    }
                    if resp.clicked() {
                        if let Some(f) = rfd::FileDialog::new().add_filter(crate::i18n::tr("문서"), extract::SUPPORTED).pick_file() {
                            self.an.file = Some(f);
                        }
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ui.add_space(10.0);
                    ui.label(muted(crate::i18n::tr("읽을 수 있는 형식")));
                    ui.horizontal_wrapped(|ui| {
                        for e in ["HWP", "HWPX", "DOCX", "PPTX", "XLSX", "PDF", "ODT", "HTML", "TXT", "MD", "CSV", "PNG·JPG"] {
                            chip(ui, e, TEXT2());
                        }
                    });
                    if !self.an.error.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("⚠ {}", self.an.error)).color(BAD()));
                    }
                    let docs: Vec<Document> = self.lib.live_docs().filter(|(_, d)| d.has_file && self.store.file_path(d).exists()).map(|(_, d)| d.clone()).take(4).collect();
                    if !docs.is_empty() {
                        ui.add_space(10.0);
                        ui.label(muted(crate::i18n::tr("보관함 문서 다시 분석")));
                        for d in docs {
                            if ui.add(egui::Button::new(RichText::new(format!("{}  ·  {}", d.title, d.kind)).color(TEXT2())).frame(false)).clicked() {
                                self.an.file = Some(self.store.file_path(&d));
                            }
                        }
                    }
                });
            });
            ui.add_space(4.0);
            let right_w = avail.x - left_w - 12.0;
            ui.allocate_ui_with_layout(vec2(right_w, avail.y), Layout::top_down(Align::Min), |ui| {
                card().show(ui, |ui| {
                    ui.set_width(right_w - 36.0);
                    ui.label(bold(crate::i18n::tr("분석 옵션"), 16.0));
                    ui.add_space(6.0);
                    let ai_ok = analyze::ai_ready(&self.settings);
                    let o = &mut self.an.opts;
                    let rows: [(&mut bool, &str, &str); 5] = [
                        (&mut o.summary, crate::i18n::tr("핵심 요약 만들기"), if ai_ok { crate::i18n::tr("AI가 세 문장으로 요약") } else { crate::i18n::tr("AI 없이 핵심 문장을 골라 냄") }),
                        (&mut o.vision, crate::i18n::tr("그림·차트·스캔 해석"), if ai_ok { crate::i18n::tr("그림을 볼 수 있는 AI로 글자와 수치를 받아 적음") } else { crate::i18n::tr("AI를 연결해야 동작") }),
                        (&mut o.metrics, crate::i18n::tr("신뢰도·복잡도 측정"), crate::i18n::tr("얼마나 온전히 읽었는지, 얼마나 복잡한지")),
                        (&mut o.mask_pii, crate::i18n::tr("개인정보 가리기"), crate::i18n::tr("주민번호·전화·이메일·카드번호를 *로 바꿈")),
                        (&mut o.save, crate::i18n::tr("끝나면 보관함에 넣기"), crate::i18n::tr("질문하기에서 바로 쓸 수 있게")),
                    ];
                    for (v, title, desc) in rows {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(right_w - 110.0);
                                ui.label(RichText::new(title).color(TEXT()));
                                ui.label(muted(desc));
                            });
                            toggle(ui, v);
                        });
                        ui.add_space(4.0);
                    }
                    ui.add_space(8.0);
                    let can = self.an.file.is_some();
                    if ui.add_enabled(can, egui::Button::new(RichText::new(crate::i18n::tr("▶  분석 시작")).color(ACCENT_INK()).size(16.0)).fill(ACCENT()).min_size(vec2(right_w - 36.0, 44.0))).clicked() {
                        self.start_analysis();
                    }
                    ui.add_space(4.0);
                    ui.label(muted(if ai_ok {
                        crate::ft!("AI: {} · 문서 내용은 {}", "AI: {} · nội dung tài liệu {}", ai::model_name(&self.settings), if self.settings.provider.is_cloud() { crate::i18n::tr("클라우드로 보내집니다") } else { crate::i18n::tr("이 PC 안에서만 처리됩니다") })
                    } else {
                        crate::i18n::tr("AI 없이 이 PC 안에서만 분석합니다 (설정에서 AI 연결 가능)").into()
                    }));
                });
            });
        });
    }

    fn analyze_running(&mut self, ui: &mut egui::Ui) {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            let w = 760.0f32.min(ui.available_width());
            card().show(ui, |ui| {
                ui.set_width(w - 36.0);
                let name = self.an.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                ui.vertical_centered(|ui| {
                    ui.label(bold(crate::i18n::tr("문서를 분석하고 있습니다"), 22.0));
                    ui.label(muted(name));
                });
                ui.add_space(24.0);
                // horizontal stepper
                let (rect, _) = ui.allocate_exact_size(vec2(w - 36.0, 96.0), Sense::hover());
                let p = ui.painter();
                let slot = rect.width() / 4.0;
                for i in 0..4 {
                    let cx = rect.left() + slot * (i as f32 + 0.5);
                    let cy = rect.top() + 20.0;
                    if i < 3 {
                        let col = if i < self.an.step { ACCENT() } else { LINE() };
                        p.line_segment([egui::pos2(cx + 22.0, cy), egui::pos2(cx + slot - 22.0, cy)], Stroke::new(3.0, col));
                    }
                    let (fill, fg, mark) = if i < self.an.step {
                        (ACCENT(), ACCENT_INK(), "✔".to_string())
                    } else if i == self.an.step {
                        (ACCENT_SOFT(), ACCENT(), format!("{}", i + 1))
                    } else {
                        (CARD_HI(), MUTED(), format!("{}", i + 1))
                    };
                    p.circle_filled(egui::pos2(cx, cy), 18.0, fill);
                    if i == self.an.step {
                        p.circle_stroke(egui::pos2(cx, cy), 18.0, Stroke::new(2.0, ACCENT()));
                    }
                    p.text(egui::pos2(cx, cy), Align2::CENTER_CENTER, mark, FontId::proportional(15.0), fg);
                    p.text(egui::pos2(cx, cy + 36.0), Align2::CENTER_CENTER, crate::i18n::tr(STEPS[i].0), FontId::new(14.0, egui::FontFamily::Name("bold".into())), if i <= self.an.step { TEXT() } else { MUTED() });
                    p.text(egui::pos2(cx, cy + 58.0), Align2::CENTER_CENTER, crate::i18n::tr(STEPS[i].1), FontId::proportional(11.5), MUTED());
                }
                ui.add_space(14.0);
                theme::bar(ui, self.an.pct, w - 90.0, ACCENT(), &format!("{:.0}%", self.an.pct * 100.0));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(&self.an.msg).color(TEXT2()));
                });
            });
        });
    }

    fn analyze_result(&mut self, ui: &mut egui::Ui) {
        let mut new_doc = false;
        let mut save_lib = false;
        let mut open_lib = false;
        let mut export = false;
        let Some(a) = &self.an.result else { return };
        // header
        ui.horizontal(|ui| {
            kind_badge(ui, &a.doc.kind, 40.0);
            ui.vertical(|ui| {
                ui.label(bold(&a.doc.title, 18.0));
                ui.label(muted(crate::ft!("{} · {} · {:.1}초 만에 분석{}", "{} · {} · phân tích trong {:.1} giây{}", a.doc.kind, fmt_size(a.doc.size_bytes), a.seconds, if a.masked > 0 { crate::ft!(" · 개인정보 {}건 가림", " · đã che {} thông tin cá nhân", a.masked) } else { String::new() })));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if primary_button(ui, crate::i18n::tr("＋ 새 문서 분석")).clicked() {
                    new_doc = true;
                }
                if self.an.saved {
                    if ui.button(crate::i18n::tr("📚 보관함에서 보기")).clicked() {
                        open_lib = true;
                    }
                } else if ui.button(crate::i18n::tr("📚 보관함에 넣기")).clicked() {
                    save_lib = true;
                }
                if ui.button(crate::i18n::tr("💾 파일로 저장")).clicked() {
                    export = true;
                }
            });
        });
        ui.add_space(10.0);
        // metric tiles
        ui.columns(4, |c| {
            card().show(&mut c[0], |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(150.0);
                ui.label(muted(crate::i18n::tr("해석 신뢰도 (추정)")));
                ui.vertical_centered(|ui| gauge(ui, a.confidence, 150.0));
            });
            card().show(&mut c[1], |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(150.0);
                ui.label(muted(crate::i18n::tr("문서 복잡도")));
                ui.horizontal(|ui| {
                    ui.label(bold(format!("{}", a.doc.complexity), 32.0).color(complexity_color(a.doc.complexity)));
                    ui.label(RichText::new(format!("/100 · {}", extract::complexity_label(a.doc.complexity))).color(TEXT2()));
                });
                let total: u32 = a.levels.iter().sum::<u32>().max(1);
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
                let mut x = rect.left();
                for (i, v) in a.levels.iter().enumerate() {
                    let w = rect.width() * *v as f32 / total as f32;
                    ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(w, 10.0)), CornerRadius::same(2), level_colors()[i]);
                    x += w;
                }
                ui.label(muted(crate::ft!("구역 {}개 · 어려운 구역(L3·L4) {}개", "{} khối · {} khối khó (L3 · L4)", total, a.levels[2] + a.levels[3])));
            });
            card().show(&mut c[2], |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(150.0);
                ui.label(muted(crate::i18n::tr("구조")));
                egui::Grid::new("an_struct").num_columns(2).spacing(vec2(16.0, 2.0)).show(ui, |ui| {
                    for (k, v) in [(crate::i18n::tr("표"), a.doc.tables), (crate::i18n::tr("표 안의 표"), a.doc.nested_tables), (crate::i18n::tr("차트"), a.doc.charts), (crate::i18n::tr("제목·조항"), a.doc.headings)] {
                        ui.label(RichText::new(k).color(TEXT2()));
                        ui.label(bold(format!("{v}"), 15.0));
                        ui.end_row();
                    }
                });
            });
            card().show(&mut c[3], |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(150.0);
                ui.label(muted(crate::i18n::tr("그림과 민감정보")));
                egui::Grid::new("an_media").num_columns(2).spacing(vec2(16.0, 2.0)).show(ui, |ui| {
                    for (k, v) in [(crate::i18n::tr("그림"), a.doc.images), (crate::i18n::tr("AI가 해석한 그림"), a.doc.ai_pictures), (crate::i18n::tr("가린 개인정보"), a.masked as u32), (crate::i18n::tr("글자 수"), a.doc.text.chars().count() as u32)] {
                        ui.label(RichText::new(k).color(TEXT2()));
                        ui.label(bold(format!("{v}"), 15.0));
                        ui.end_row();
                    }
                });
            });
        });
        ui.add_space(6.0);
        let avail = ui.available_size();
        let left_w = (avail.x * 0.34).floor();
        let mut tab = self.an.tab;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(left_w, avail.y), Layout::top_down(Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("an_left").auto_shrink([false, false]).show(ui, |ui| {
                    card().show(ui, |ui| {
                        ui.set_width(left_w - 40.0);
                        ui.horizontal(|ui| {
                            ui.label(bold(crate::i18n::tr("핵심 요약"), 15.0));
                            chip(ui, if a.summary_by_ai { crate::i18n::tr("AI 요약") } else { crate::i18n::tr("핵심 문장 (AI 없음)") }, if a.summary_by_ai { PURPLE() } else { MUTED() });
                        });
                        ui.add_space(4.0);
                        if a.summary.is_empty() {
                            ui.label(muted(crate::i18n::tr("요약을 끈 상태로 분석했습니다")));
                        }
                        for s in &a.summary {
                            ui.horizontal_top(|ui| {
                                ui.label(RichText::new("•").color(ACCENT()));
                                ui.add(egui::Label::new(RichText::new(s).color(TEXT())).wrap());
                            });
                        }
                    });
                    ui.add_space(6.0);
                    card().show(ui, |ui| {
                        ui.set_width(left_w - 40.0);
                        ui.label(bold(crate::i18n::tr("복잡도 단계"), 15.0));
                        ui.horizontal(|ui| {
                            let total: u32 = a.levels.iter().sum();
                            donut(ui, a.levels, 120.0, &format!("{}", a.doc.complexity));
                            ui.vertical(|ui| {
                                for i in 0..4 {
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                                        ui.painter().circle_filled(r.center(), 5.0, level_colors()[i]);
                                        ui.label(RichText::new(crate::i18n::tr(LEVEL_NAMES[i])).size(12.5).color(TEXT2()));
                                        ui.label(muted(format!("{:.0}%", a.levels[i] as f32 * 100.0 / total.max(1) as f32)));
                                    });
                                }
                            });
                        });
                    });
                    ui.add_space(6.0);
                    card().show(ui, |ui| {
                        ui.set_width(left_w - 40.0);
                        ui.label(bold(crate::i18n::tr("확인할 점"), 15.0));
                        if a.notes.is_empty() {
                            ui.label(RichText::new(crate::i18n::tr("✔ 빠진 부분 없이 읽었습니다")).color(GOOD()));
                        }
                        for n in &a.notes {
                            ui.add(egui::Label::new(RichText::new(format!("⚠ {n}")).color(WARN()).size(13.0)).wrap());
                        }
                    });
                });
            });
            ui.add_space(4.0);
            let right_w = avail.x - left_w - 12.0;
            ui.allocate_ui_with_layout(vec2(right_w, avail.y), Layout::top_down(Align::Min), |ui| {
                card().show(ui, |ui| {
                    ui.set_width(right_w - 36.0);
                    ui.set_min_height(avail.y - 40.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (i, l) in [crate::i18n::tr("미리보기"), "Markdown", "JSON", crate::i18n::tr("원문 텍스트")].iter().enumerate() {
                            if theme::tab(ui, tab == i as u8, l).clicked() {
                                tab = i as u8;
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(crate::i18n::tr("📋 복사")).clicked() {
                                let t = match tab {
                                    1 => a.markdown.clone(),
                                    2 => a.json.clone(),
                                    _ => a.doc.text.clone(),
                                };
                                ui.ctx().copy_text(t);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::both().id_salt("an_out").auto_shrink([false, false]).show(ui, |ui| match tab {
                        0 => preview(ui, &a.blocks),
                        1 => code(ui, &a.markdown),
                        2 => code(ui, &a.json),
                        _ => code(ui, &a.doc.text),
                    });
                });
            });
        });
        self.an.tab = tab;
        if new_doc {
            self.an.result = None;
            self.an.file = None;
        }
        if save_lib {
            self.save_analysis_to_library();
            self.toast(crate::i18n::tr("보관함에 넣었습니다"), GOOD());
        }
        if open_lib {
            self.page = Page::Library;
        }
        if export {
            if let Some(a) = &self.an.result {
                let md = a.markdown.clone();
                let js = a.json.clone();
                if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.md", a.doc.title)).add_filter("Markdown", &["md"]).add_filter("JSON", &["json"]).save_file() {
                    let is_json = p.extension().map(|e| e.eq_ignore_ascii_case("json")).unwrap_or(false);
                    match std::fs::write(&p, if is_json { js } else { md }) {
                        Ok(_) => self.toast(crate::i18n::tr("저장했습니다"), GOOD()),
                        Err(e) => self.toast(crate::ft!("저장하지 못했습니다: {e}", "Không lưu được: {e}", e = e), BAD()),
                    }
                }
            }
        }
    }
}

fn code(ui: &mut egui::Ui, s: &str) {
    egui::Frame::default().fill(FIELD()).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(12)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let t: String = s.chars().take(60_000).collect();
        ui.add(egui::Label::new(RichText::new(t).monospace().size(12.5).color(TEXT2())).selectable(true));
    });
}

fn level_chip(ui: &mut egui::Ui, b: &Block) {
    chip(ui, &format!("L{}", b.level), level_colors()[(b.level.clamp(1, 4) - 1) as usize]);
}

fn preview(ui: &mut egui::Ui, blocks: &[Block]) {
    for (bi, b) in blocks.iter().enumerate() {
        match b.kind {
            Kind::Heading => {
                ui.add_space(6.0);
                ui.label(bold(&b.text, 17.0));
            }
            Kind::Paragraph => {
                ui.add(egui::Label::new(RichText::new(&b.text).color(TEXT2())).wrap());
            }
            Kind::List => {
                for l in b.text.lines() {
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new("•").color(ACCENT()));
                        ui.add(egui::Label::new(RichText::new(l.trim_start_matches(['-', '•', '·']).trim()).color(TEXT2())).wrap());
                    });
                }
            }
            Kind::Table => {
                ui.horizontal(|ui| {
                    level_chip(ui, b);
                    ui.label(muted(if b.level == 4 { crate::i18n::tr("표 안에 표가 있는 표") } else { crate::i18n::tr("표") }));
                });
                egui::Frame::default().stroke(Stroke::new(1.0, LINE())).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(8)).show(ui, |ui| {
                    egui::Grid::new(("an_tbl", bi)).striped(true).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
                        for (ri, r) in b.rows.iter().enumerate() {
                            for c in r {
                                if c.contains("[표:") {
                                    egui::Frame::default().fill(WARN().gamma_multiply(0.12)).corner_radius(CornerRadius::same(6)).inner_margin(Margin::symmetric(6, 3)).show(ui, |ui| {
                                        ui.label(RichText::new(c).color(WARN()).size(13.0));
                                    });
                                } else if ri == 0 {
                                    ui.label(RichText::new(c).color(TEXT()).strong());
                                } else {
                                    ui.label(RichText::new(c).color(TEXT2()));
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
            }
            Kind::Chart | Kind::Figure => {
                egui::Frame::default().fill(PURPLE().gamma_multiply(0.1)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        level_chip(ui, b);
                        ui.label(RichText::new(if b.kind == Kind::Chart { crate::i18n::tr("📊 차트 데이터") } else { crate::i18n::tr("✨ 그림 해석") }).color(PURPLE()));
                    });
                    ui.add(egui::Label::new(RichText::new(&b.text).color(TEXT2()).size(13.0)).wrap());
                });
            }
        }
        ui.add_space(6.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An analysis whose thread ended without a result isn't shown as running forever.
    #[test]
    fn analysis_that_ends_without_a_result_is_not_left_running() {
        let mut a = DeskApp::new(crate::Args { screenshot: Some(PathBuf::from("unused.png")), page: None });
        a.shot = None;
        let (tx, rx) = channel::<crate::analyze::Msg>();
        drop(tx);
        a.an.rx = Some(rx);
        a.poll_analyze();
        assert!(a.an.rx.is_none());
        assert!(a.toast.as_ref().is_some_and(|t| t.0.contains("중단")), "the person is told");
    }
}

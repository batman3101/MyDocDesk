//! The "자동 수집" page: watched folders, the processing flow and the run history.

use super::*;
use crate::pipeline::{self, Known, Msg, STAGES};

pub struct PipeState {
    pub rx: Option<Receiver<Msg>>,
    pub msg: String,
    pub last_scan: Option<Instant>,
}

impl Default for PipeState {
    fn default() -> Self {
        PipeState { rx: None, msg: String::new(), last_scan: None }
    }
}

impl DeskApp {
    fn known_files(&self) -> Vec<Known> {
        self.lib
            .live_docs()
            .filter(|(_, d)| !d.source_path.is_empty())
            .map(|(_, d)| Known { id: d.id.clone(), path: d.source_path.clone(), mtime: d.source_mtime, size: d.size_bytes })
            .collect()
    }

    pub(super) fn start_scan(&mut self) {
        if self.pipe.rx.is_some() || self.settings.watch_folders.is_empty() || self.store.read_only {
            return;
        }
        let (tx, rx) = channel();
        let folders = self.settings.watch_folders.clone();
        let known = self.known_files();
        let store = self.store.clone();
        std::thread::spawn(move || {
            let r = pipeline::scan(folders, known, store, Some(tx.clone()));
            let _ = tx.send(Msg::Done(Box::new(r)));
        });
        self.pipe.rx = Some(rx);
        self.pipe.last_scan = Some(Instant::now());
    }

    /// Scans at start-up and every 10 minutes when auto-collect is on.
    pub(super) fn auto_scan_tick(&mut self) {
        if !self.settings.auto_scan || self.settings.watch_folders.is_empty() || self.store.read_only {
            return;
        }
        let due = self.pipe.last_scan.map(|t| t.elapsed() > Duration::from_secs(600)).unwrap_or(true);
        if due {
            self.start_scan();
        }
    }

    pub(super) fn poll_scan(&mut self) {
        let Some(rx) = &self.pipe.rx else { return };
        let mut done = None;
        let (msgs, gone) = drain(rx);
        for m in msgs {
            match m {
                Msg::Progress(s) => self.pipe.msg = s,
                Msg::Done(r) => done = Some(r),
            }
        }
        if done.is_none() && gone {
            // the scan thread ended without its result: record it, don't stay "collecting"
            self.pipe.rx = None;
            self.pipe.msg.clear();
            let why = crate::ft!("수집 작업이 결과 없이 중단되었습니다", "Việc thu thập bị dừng mà không có kết quả");
            self.settings.runs.insert(0, PipelineRun { ms: now_ms(), folders: self.settings.watch_folders.len(), failed: 1, errors: vec![why.clone()], ..Default::default() });
            self.settings.runs.truncate(30);
            self.settings_dirty = true;
            self.toast(why, BAD());
            return;
        }
        let Some(r) = done else { return };
        self.pipe.rx = None;
        let mut r = *r;
        // The result describes the library as it was when the scan started. What the person did
        // meanwhile wins: deleted documents stay deleted, disconnected folders add nothing.
        let watched = |p: &str, folders: &[String]| folders.iter().any(|f| pipeline::under(p, f));
        let (mut n_upd, mut n_rem, mut n_add) = (0, 0, 0);
        for u in r.updated {
            if let Some(d) = self.lib.docs.iter_mut().find(|d| d.id == u.id && !d.deleted) {
                let (title, tags, added) = (d.title.clone(), d.tags.clone(), d.added_ms);
                *d = u;
                d.title = title;
                d.tags = tags;
                d.added_ms = added;
                d.updated_ms = now_ms();
                n_upd += 1;
            }
        }
        for id in &r.removed {
            if let Some(d) = self.lib.docs.iter_mut().find(|d| &d.id == id && !d.deleted) {
                d.deleted = true;
                d.text.clear();
                d.updated_ms = now_ms();
                let _ = std::fs::remove_file(self.store.file_path(d));
                n_rem += 1;
            }
        }
        for d in r.added {
            let have = self.lib.live_docs().any(|(_, x)| !x.source_path.is_empty() && x.source_path.eq_ignore_ascii_case(&d.source_path));
            if have || !watched(&d.source_path, &self.settings.watch_folders) {
                let _ = std::fs::remove_file(self.store.file_path(&d));
                continue;
            }
            self.lib.docs.push(d);
            n_add += 1;
        }
        r.run.added = n_add;
        r.run.updated = n_upd;
        r.run.removed = n_rem;
        if n_add + n_upd + n_rem > 0 {
            self.touch_lib();
            self.toast(crate::ft!("자동 수집: 새 문서 {} · 바뀐 문서 {} · 지운 문서 {}", "Tự động thu thập: {} mới · {} đã đổi · {} đã xóa", n_add, n_upd, n_rem), GOOD());
        }
        self.settings.runs.insert(0, r.run);
        self.settings.runs.truncate(30);
        self.settings_dirty = true;
        self.pipe.msg.clear();
    }

    #[cfg(test)]
    pub(super) fn deliver_scan(&mut self, r: pipeline::ScanResult) {
        let (tx, rx) = channel();
        tx.send(Msg::Done(Box::new(r))).unwrap();
        self.pipe.rx = Some(rx);
        self.poll_scan();
    }

    pub(super) fn page_pipeline(&mut self, ui: &mut egui::Ui) {
        let last = self.settings.runs.first().cloned();
        let docs = self.lib.live_docs().filter(|(_, d)| !d.source_path.is_empty() && self.settings.watch_folders.iter().any(|f| d.source_path.to_lowercase().starts_with(&f.to_lowercase()))).count();
        self.ensure_index();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // the flow
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(bold(crate::i18n::tr("처리 흐름"), 16.0));
                    ui.label(muted(crate::i18n::tr("연결한 폴더의 파일이 아래 순서로 지식이 됩니다. 바뀐 파일만 다시 처리합니다.")));
                });
                ui.add_space(10.0);
                let counts = [
                    last.as_ref().map(|r| crate::ft!("파일 {}개 확인", "Đã kiểm tra {} tệp", r.scanned)).unwrap_or_else(|| crate::i18n::tr("아직 실행 전").into()),
                    last.as_ref().map(|r| crate::ft!("새 {} · 바뀜 {} · 실패 {}", "Mới {} · đổi {} · lỗi {}", r.added, r.updated, r.failed)).unwrap_or_else(|| "-".into()),
                    crate::ft!("조각 {}개", "{} đoạn", self.index.chunks.len()),
                    crate::ft!("연결 폴더 문서 {}개", "{} tài liệu trong thư mục kết nối", docs),
                ];
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), Sense::hover());
                let p = ui.painter();
                let gap = 34.0;
                let w = (rect.width() - gap * 3.0) / 4.0;
                let running = self.pipe.rx.is_some();
                for i in 0..4 {
                    let r = egui::Rect::from_min_size(egui::pos2(rect.left() + i as f32 * (w + gap), rect.top()), vec2(w, 84.0));
                    let active = running && self.pipe.msg.starts_with(STAGES[i].0);
                    p.rect_filled(r, CornerRadius::same(12), if active { ACCENT_SOFT() } else { CARD_HI() });
                    p.rect_stroke(r, CornerRadius::same(12), Stroke::new(1.0, if active { ACCENT() } else { LINE() }), egui::StrokeKind::Inside);
                    p.circle_filled(r.left_top() + vec2(22.0, 24.0), 12.0, ACCENT().gamma_multiply(0.25));
                    p.text(r.left_top() + vec2(22.0, 24.0), Align2::CENTER_CENTER, format!("{}", i + 1), FontId::proportional(12.0), ACCENT());
                    p.text(r.left_top() + vec2(42.0, 24.0), Align2::LEFT_CENTER, crate::i18n::tr(STAGES[i].0), FontId::new(15.0, egui::FontFamily::Name("bold".into())), TEXT());
                    p.text(r.left_top() + vec2(14.0, 50.0), Align2::LEFT_CENTER, crate::i18n::tr(STAGES[i].1), FontId::proportional(12.0), MUTED());
                    p.text(r.left_top() + vec2(14.0, 70.0), Align2::LEFT_CENTER, &counts[i], FontId::proportional(12.5), TEXT2());
                    if i < 3 {
                        let a = egui::pos2(r.right() + 8.0, r.center().y);
                        let b = egui::pos2(r.right() + gap - 8.0, r.center().y);
                        p.arrow(a, b - a, Stroke::new(2.0, ACCENT()));
                    }
                }
                // feedback loop
                let y = rect.top() + 104.0;
                let x0 = rect.left() + w / 2.0;
                let x1 = rect.right() - w / 2.0;
                p.line_segment([egui::pos2(x1, rect.top() + 86.0), egui::pos2(x1, y)], Stroke::new(1.2, PURPLE()));
                p.line_segment([egui::pos2(x1, y), egui::pos2(x0, y)], Stroke::new(1.2, PURPLE()));
                p.arrow(egui::pos2(x0, y), vec2(0.0, -16.0), Stroke::new(1.2, PURPLE()));
                let label = crate::i18n::tr("질문하기 근거 확인·품질 평가 결과로 문서를 보강하면 다음 수집에 반영됩니다");
                let g = p.layout_no_wrap(label.into(), FontId::proportional(12.0), PURPLE());
                let lr = egui::Rect::from_center_size(egui::pos2((x0 + x1) / 2.0, y), g.size() + vec2(20.0, 8.0));
                p.rect_filled(lr, CornerRadius::same(100), CARD());
                p.rect_stroke(lr, CornerRadius::same(100), Stroke::new(1.0, PURPLE()), egui::StrokeKind::Inside);
                p.galley(lr.center() - g.size() / 2.0, g, PURPLE());
            });
            ui.add_space(6.0);
            ui.columns(2, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(bold(crate::i18n::tr("연결 폴더"), 16.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let busy = self.pipe.rx.is_some();
                            if ui.add_enabled(!busy && !self.settings.watch_folders.is_empty(), egui::Button::new(RichText::new(crate::i18n::tr("🔄 지금 수집")).color(ACCENT_INK())).fill(ACCENT())).clicked() {
                                self.start_scan();
                            }
                            if ui.button(crate::i18n::tr("📁 폴더 추가")).clicked() {
                                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                                    let s = p.display().to_string();
                                    if !self.settings.watch_folders.contains(&s) {
                                        self.settings.watch_folders.push(s);
                                        self.settings_dirty = true;
                                        self.start_scan();
                                    }
                                }
                            }
                        });
                    });
                    ui.label(muted(crate::i18n::tr("이 컴퓨터의 폴더만 연결됩니다. 문서는 보관함에 들어가고, 계정 동기화를 켜면 다른 기기에서도 보입니다.")));
                    ui.add_space(6.0);
                    let mut remove = None;
                    for (i, f) in self.settings.watch_folders.iter().enumerate() {
                        egui::Frame::default().fill(FIELD()).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("📁").size(16.0));
                                let n = self.lib.live_docs().filter(|(_, d)| d.source_path.to_lowercase().starts_with(&f.to_lowercase())).count();
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(f).color(TEXT()).monospace().size(12.5));
                                    ui.label(muted(crate::ft!("문서 {n}개{}", "{n} tài liệu{}", if std::path::Path::new(f).is_dir() { "" } else { crate::i18n::tr(" · 폴더를 찾을 수 없음") }, n = n)));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button(crate::i18n::tr("연결 끊기")).on_hover_text(crate::i18n::tr("보관함의 문서는 그대로 둡니다")).clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
                        });
                    }
                    if let Some(i) = remove {
                        self.settings.watch_folders.remove(i);
                        self.settings_dirty = true;
                    }
                    if self.settings.watch_folders.is_empty() {
                        ui.label(RichText::new(crate::i18n::tr("연결한 폴더가 없습니다. '폴더 추가'로 문서 폴더를 고르세요.")).color(WARN()));
                    }
                    ui.add_space(6.0);
                    if ui.checkbox(&mut self.settings.auto_scan, crate::i18n::tr("앱을 켤 때와 10분마다 자동으로 수집")).changed() {
                        self.settings_dirty = true;
                    }
                    if self.pipe.rx.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(&self.pipe.msg).color(ACCENT()));
                        });
                    }
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(bold(crate::i18n::tr("실행 기록"), 16.0));
                    ui.add_space(4.0);
                    if self.settings.runs.is_empty() {
                        ui.label(muted(crate::i18n::tr("아직 실행한 적이 없습니다.")));
                    }
                    egui::Grid::new("runs").num_columns(6).striped(true).spacing(vec2(14.0, 6.0)).show(ui, |ui| {
                        if !self.settings.runs.is_empty() {
                            for h in [crate::i18n::tr("시각"), crate::i18n::tr("확인"), crate::i18n::tr("새"), crate::i18n::tr("바뀜"), crate::i18n::tr("지움"), crate::i18n::tr("걸린 시간")] {
                                ui.label(muted(h));
                            }
                            ui.end_row();
                        }
                        for r in self.settings.runs.iter().take(10) {
                            ui.label(RichText::new(fmt_time(r.ms)).color(TEXT2()));
                            ui.label(format!("{}", r.scanned));
                            ui.label(RichText::new(format!("{}", r.added)).color(if r.added > 0 { GOOD() } else { TEXT2() }));
                            ui.label(RichText::new(format!("{}", r.updated)).color(if r.updated > 0 { ACCENT() } else { TEXT2() }));
                            ui.label(RichText::new(format!("{}", r.removed)).color(if r.removed > 0 { WARN() } else { TEXT2() }));
                            ui.label(RichText::new(crate::ft!("{:.1}초{}", "{:.1} giây{}", r.seconds, if r.failed > 0 { crate::ft!(" · 실패 {}", " · lỗi {}", r.failed) } else { String::new() })).color(if r.failed > 0 { BAD() } else { TEXT2() }));
                            ui.end_row();
                        }
                    });
                    if let Some(r) = &last {
                        for e in r.errors.iter().take(4) {
                            ui.add(egui::Label::new(RichText::new(format!("⚠ {e}")).color(WARN()).size(12.0)).wrap());
                        }
                    }
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> DeskApp {
        let mut a = DeskApp::new(crate::Args { screenshot: Some(PathBuf::from("unused.png")), page: None });
        a.shot = None;
        a.lib = Library::default();
        a.settings.watch_folders = vec!["D:\\docs".into()];
        a
    }
    fn doc(id: &str, path: &str) -> Document {
        Document { id: id.into(), title: id.into(), text: "본문".into(), source_path: path.into(), has_file: true, ..Default::default() }
    }

    /// audit M-03: deleting a document while a scan runs is not undone by that scan's result.
    #[test]
    fn late_scan_result_does_not_bring_back_a_deleted_document() {
        let mut a = app();
        let mut d = doc("x", "D:\\docs\\a.txt");
        d.deleted = true;
        d.text.clear();
        a.lib.docs.push(d);
        let mut changed = doc("x", "D:\\docs\\a.txt");
        changed.text = "스캔이 읽은 내용".into();
        a.deliver_scan(pipeline::ScanResult { added: vec![], updated: vec![changed], removed: vec![], run: Default::default() });
        assert!(a.lib.docs[0].deleted, "still deleted");
        assert!(a.lib.docs[0].text.is_empty());
    }

    /// audit M-03: files from a folder disconnected during the scan, and files already in the
    /// library, are not added.
    #[test]
    fn late_scan_result_skips_disconnected_folders_and_duplicates() {
        let mut a = app();
        a.lib.docs.push(doc("have", "D:\\docs\\have.txt"));
        let added = vec![doc("n1", "E:\\old\\new.txt"), doc("n2", "D:\\docs\\have.txt"), doc("n3", "D:\\docs\\ok.txt")];
        a.deliver_scan(pipeline::ScanResult { added, updated: vec![], removed: vec![], run: Default::default() });
        let ids: Vec<&str> = a.lib.docs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["have", "n3"]);
    }

    /// A scan whose thread ended without a result (it crashed) isn't shown as running forever.
    #[test]
    fn scan_that_ends_without_a_result_is_not_left_running() {
        let mut a = app();
        let (tx, rx) = channel::<Msg>();
        drop(tx);
        a.pipe.rx = Some(rx);
        a.poll_scan();
        assert!(a.pipe.rx.is_none(), "no longer 'collecting'");
        assert!(a.settings.runs.first().is_some_and(|r| !r.errors.is_empty()), "the run is recorded as failed");
    }
}

//! "AI 답변까지 평가": each test question is answered by the AI from the retrieved passages,
//! then scored for accuracy, evidence, personal-info safety, and by an AI judge.

use super::*;
use crate::analyze::mask_pii;
use crate::rag;

fn bar_row(ui: &mut egui::Ui, name: &str, desc: &str, v: Option<f32>, w: f32) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            ui.label(RichText::new(name).color(TEXT()));
            ui.label(muted(desc));
        });
        let (rect, _) = ui.allocate_exact_size(vec2(w, 14.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(7), LINE());
        if let Some(v) = v {
            let col = if v >= 0.8 { GOOD() } else if v >= 0.6 { WARN() } else { BAD() };
            ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, vec2(rect.width() * v.clamp(0.0, 1.0), 14.0)), CornerRadius::same(7), col);
        }
        ui.label(bold(v.map(|v| format!("{:.0}%", v * 100.0)).unwrap_or_else(|| "-".into()), 15.0));
    });
}

fn avg(xs: impl Iterator<Item = f32>) -> Option<f32> {
    let v: Vec<f32> = xs.collect();
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<f32>() / v.len() as f32)
    }
}

impl DeskApp {
    pub(super) fn start_eval_ai(&mut self) {
        self.ensure_index();
        let agent = self.current_agent();
        let mut jobs = Vec::new();
        for (i, e) in self.lib.live_evals() {
            let hits = self.index.search(&e.question, agent.top_k.max(3), |_| true);
            let sources: Vec<SourceRef> = hits
                .iter()
                .map(|(ci, s)| {
                    let c = &self.index.chunks[*ci];
                    let d = &self.lib.docs[c.doc];
                    SourceRef { doc_id: d.id.clone(), title: d.title.clone(), excerpt: excerpt(&c.text, 900), score: *s, chunk: c.ord + 1 }
                })
                .collect();
            let p = rag::prepare(&self.settings, &agent, &sources, &e.question);
            jobs.push((i, e.clone(), p));
        }
        if jobs.is_empty() {
            return;
        }
        self.eval_running = jobs.len();
        let (tx, rx) = channel();
        let agent_name = agent.name.clone();
        std::thread::spawn(move || {
            for (i, case, p) in jobs {
                let mut r = case.last.clone().unwrap_or_default();
                let mut usage = None;
                match ai::ask(&p.settings, &p.system, &p.user) {
                    Ok(rep) => {
                        let ans = rep.text.clone();
                        let low = ans.to_lowercase();
                        let kws: Vec<String> = case.expected.split(',').map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).collect();
                        let found = kws.iter().filter(|k| low.contains(k.as_str())).count();
                        r.answer_score = Some(if kws.is_empty() { 1.0 } else { found as f32 / kws.len() as f32 });
                        r.cites = Some(rag::has_citation(&ans));
                        r.pii_safe = Some(mask_pii(&ans).1 == 0);
                        let mut pt = rep.prompt_tokens;
                        let mut ct = rep.completion_tokens;
                        let judge = ai::ask(
                            &p.settings,
                            crate::i18n::tr("당신은 엄격한 채점자입니다. 질문, 정답에 있어야 할 낱말, AI 답변을 보고 답변이 정확하고 자료에 근거했는지 1~5점으로 매기세요. 숫자 하나만 출력하세요."),
                            &crate::ft!("질문: {}\n정답 낱말: {}\nAI 답변: {}", "Câu hỏi: {}\nTừ đáp án: {}\nTrả lời AI: {}", case.question, case.expected, ans),
                        );
                        if let Ok(j) = judge {
                            r.judge = j.text.chars().find(|c| ('1'..='5').contains(c)).and_then(|c| c.to_digit(10)).map(|d| d as u8);
                            pt += j.prompt_tokens;
                            ct += j.completion_tokens;
                        }
                        r.answer = crate::search::excerpt(&ans, 600);
                        usage = Some(UsageEntry {
                            id: new_id(),
                            ms: now_ms(),
                            model: rep.model,
                            prompt_tokens: pt,
                            completion_tokens: ct,
                            cost_usd: ai::cost(&p.settings, pt, ct),
                            agent: crate::ft!("품질 평가 · {agent_name}", "Đánh giá chất lượng · {agent_name}", agent_name = agent_name),
                        });
                    }
                    Err(e) => {
                        r.answer = crate::ft!("AI 오류: {e}", "Lỗi AI: {e}", e = e);
                        r.judge = Some(1);
                        r.answer_score = Some(0.0);
                    }
                }
                r.ms = now_ms();
                if tx.send((i, r, usage)).is_err() {
                    break;
                }
            }
        });
        self.eval_rx = Some(rx);
    }

    pub(super) fn poll_eval_ai(&mut self) {
        let Some(rx) = &self.eval_rx else { return };
        let mut got = Vec::new();
        let mut closed = false;
        loop {
            match rx.try_recv() {
                Ok(x) => got.push(x),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }
        for (i, r, u) in got {
            if let Some(e) = self.lib.evals.get_mut(i) {
                e.last = Some(r);
                e.updated_ms = now_ms();
            }
            if let Some(u) = u {
                self.lib.usage.push(u);
            }
            self.eval_running = self.eval_running.saturating_sub(1);
            self.lib_dirty = true;
        }
        if closed || self.eval_running == 0 {
            self.eval_rx = None;
            self.eval_running = 0;
            if closed {
                self.toast(crate::i18n::tr("AI 답변 평가를 마쳤습니다"), GOOD());
            }
        }
    }

    pub(super) fn eval_report(&mut self, ui: &mut egui::Ui) {
        let rs: Vec<EvalResult> = self.lib.live_evals().filter_map(|(_, e)| e.last.clone()).collect();
        let retrieval = avg(rs.iter().map(|r| if r.passed { 1.0 } else { 0.0 }));
        let answer = avg(rs.iter().filter_map(|r| r.answer_score));
        let cites = avg(rs.iter().filter_map(|r| r.cites.map(|c| if c { 1.0 } else { 0.0 })));
        let pii = avg(rs.iter().filter_map(|r| r.pii_safe.map(|c| if c { 1.0 } else { 0.0 })));
        let judge = avg(rs.iter().filter_map(|r| r.judge.map(|j| (j as f32 - 1.0) / 4.0)));
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(bold(crate::i18n::tr("평가 결과 보고서"), 16.0));
                ui.label(muted(crate::i18n::tr("성능(정확성·근거)과 안전성(개인정보)을 나눠 봅니다")));
            });
            ui.add_space(6.0);
            let w = (ui.available_width() / 2.0 - 280.0).max(120.0);
            ui.columns(2, |c| {
                c[0].label(RichText::new(crate::i18n::tr("성능")).color(ACCENT()).strong());
                bar_row(&mut c[0], crate::i18n::tr("검색 정확도"), crate::i18n::tr("정답 구절이 상위 3개 안에"), retrieval, w);
                bar_row(&mut c[0], crate::i18n::tr("답변 정확도"), crate::i18n::tr("답에 정답 낱말이 들어감"), answer, w);
                bar_row(&mut c[0], crate::i18n::tr("AI 판정"), crate::i18n::tr("채점 AI의 1~5점 평가"), judge, w);
                c[1].label(RichText::new(crate::i18n::tr("신뢰성·안전성")).color(GOOD()).strong());
                bar_row(&mut c[1], crate::i18n::tr("근거 표시"), crate::i18n::tr("답에 [1] 같은 근거 번호"), cites, w);
                bar_row(&mut c[1], crate::i18n::tr("개인정보 안전"), crate::i18n::tr("답에 전화·주민번호 등이 없음"), pii, w);
                if answer.is_none() {
                    c[1].label(muted(crate::i18n::tr("AI 항목은 'AI 답변까지 평가'를 실행하면 채워집니다.")));
                }
            });
        });
    }
}

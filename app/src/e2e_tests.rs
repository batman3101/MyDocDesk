//! E2E: the real DeskApp, run headless by egui_kittest. Tests find widgets by their
//! accessibility label (what a screen reader would read), click them and check what the page shows.
//!
//! The app runs on demo data with a read-only store, so nothing on this PC is written.
//! Run only these: `cargo test e2e`

use super::*;
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::sync::MutexGuard;

/// The screen language is one global switch (`i18n::set_vi`), so E2E tests take turns
/// with every other language-dependent test, and each starts on Korean.
fn lock_screen() -> MutexGuard<'static, ()> {
    crate::i18n::test_lock()
}

/// Starts the app the way `main` does, on `page`, in language `lang` ("ko" or "vi").
fn launch(page: &str, lang: &str) -> Harness<'static, DeskApp> {
    let page = page.to_string();
    let lang = lang.to_string();
    let mut h = Harness::builder().with_size(vec2(1320.0, 1400.0)).build_eframe(move |cc| {
        theme::install_fonts(&cc.egui_ctx); // same start-up as main.rs (the "bold" family lives here)
        theme::apply(&cc.egui_ctx);
        // demo data + read-only store (the screenshot path is never written: `shot` is cleared)
        let mut app = DeskApp::new(Args { screenshot: Some(PathBuf::from("e2e-unused.png")), page: Some(page) });
        app.shot = None;
        app.settings.language = lang;
        app
    });
    h.run_steps(3);
    h
}

#[test]
fn e2e_settings_from_rail_has_no_workboard_phrase() {
    let _screen = lock_screen();
    for (lang, settings, info) in [("ko", "설정", "정보"), ("vi", "Cài đặt", "Thông tin")] {
        let mut app = launch("home", lang);
        assert!(app.state().page == Page::Home);

        // click 설정 in the left icon rail, like a person would
        app.get_by_role_and_label(Role::Button, settings).click();
        app.run_steps(3);
        assert!(app.state().page == Page::Settings, "[{lang}] the rail button should open settings");

        // the page is there (its 정보 section)...
        assert!(app.query_by_label(info).is_some(), "[{lang}] settings should show the '{info}' section");
        // ...and the design-credit sentence that was removed stays gone
        assert!(
            app.query_by_label_contains("WorkBoard").is_none(),
            "[{lang}] settings must not mention WorkBoard"
        );
    }
}

// ---------------- 4-digit lock (the login) ----------------

/// Starts the app with the lock in force: `pin` "" = first start (pick a number), else that number is set.
fn launch_locked(page: &str, pin: &str) -> Harness<'static, DeskApp> {
    let mut h = launch(page, "ko");
    let stored = if pin.is_empty() { String::new() } else { crate::lock::hash_pin(pin) };
    h.state_mut().lock = crate::lock::Lock::starting(&stored);
    h.state_mut().settings.lock_pin = stored;
    h.run_steps(3);
    h
}

fn type_pin(app: &mut Harness<'static, DeskApp>, pin: &str) {
    app.get_by_role(Role::PasswordInput).type_text(pin);
    app.run_steps(3);
}

/// The menus (and so every page) are hidden while locked.
fn app_visible(app: &Harness<'static, DeskApp>) -> bool {
    app.query_by_role_and_label(Role::Button, "설정").is_some()
}

#[test]
fn e2e_first_start_picks_number_confirms_and_opens() {
    let _screen = lock_screen();
    let mut app = launch_locked("home", "");
    assert!(app.query_by_label("잠금 번호 정하기").is_some(), "first start asks for a number");
    assert!(!app_visible(&app), "nothing of the app shows before a number is set");

    type_pin(&mut app, "1234");
    assert!(app.query_by_label_contains("한 번 더 입력").is_some(), "asks to type it again");
    assert!(!app_visible(&app));

    type_pin(&mut app, "1234");
    assert!(app_visible(&app), "confirmed: the app opens right away");
    assert!(crate::lock::verify_pin(&app.state().settings.lock_pin, "1234"));
    assert!(!app.state().settings.lock_pin.contains("1234"), "only a hash is kept");
}

#[test]
fn e2e_first_start_mismatch_starts_over() {
    let _screen = lock_screen();
    let mut app = launch_locked("home", "");
    type_pin(&mut app, "1234");
    type_pin(&mut app, "9999");
    assert!(app.query_by_label_contains("두 번 입력한 번호가 다릅니다").is_some());
    assert!(app.query_by_label("잠금 번호 정하기").is_some(), "back to picking a number");
    assert!(!app_visible(&app));
    assert!(app.state().settings.lock_pin.is_empty());
}

#[test]
fn e2e_lock_while_away_and_open_with_four_digits() {
    let _screen = lock_screen();
    let mut app = launch_locked("home", "1234");
    assert!(app.query_by_label("잠금 번호를 입력하세요").is_some(), "a set number means the app starts locked");
    type_pin(&mut app, "1234");
    assert!(app_visible(&app));

    // stepping away: the lock button in the top bar
    app.get_by_role_and_label(Role::Button, "잠금").click();
    app.run_steps(3);
    assert!(!app_visible(&app), "locked: the app is hidden");

    type_pin(&mut app, "0000");
    assert!(app.query_by_label_contains("번호가 맞지 않습니다").is_some());
    assert!(!app_visible(&app));

    type_pin(&mut app, "1234");
    assert!(app_visible(&app), "the 4 digits alone open it");

    // and the keyboard shortcut locks too
    app.press_key_modifiers(egui::Modifiers::COMMAND, egui::Key::L);
    app.run_steps(3);
    assert!(!app_visible(&app), "Ctrl+L locks");
}

#[test]
fn e2e_change_number_from_settings() {
    let _screen = lock_screen();
    let mut app = launch_locked("settings", "1234");
    type_pin(&mut app, "1234");
    app.get_by_role_and_label(Role::Button, "잠금 번호 바꾸기").click();
    app.run_steps(3);
    assert!(app.query_by_label("지금 쓰는 잠금 번호를 입력하세요").is_some());
    type_pin(&mut app, "1234");
    assert!(app.query_by_label("잠금 번호 정하기").is_some());
    type_pin(&mut app, "2468");
    type_pin(&mut app, "2468");
    assert!(app_visible(&app));
    assert!(crate::lock::verify_pin(&app.state().settings.lock_pin, "2468"));
    assert!(!crate::lock::verify_pin(&app.state().settings.lock_pin, "1234"));
}

#[test]
fn e2e_lock_screen_in_vietnamese() {
    let _screen = lock_screen();
    let mut app = launch("home", "vi");
    let stored = crate::lock::hash_pin("1234");
    app.state_mut().lock = crate::lock::Lock::starting(&stored);
    app.state_mut().settings.lock_pin = stored;
    app.run_steps(3);
    assert!(app.query_by_label("Nhập mã khóa").is_some(), "the lock screen follows the screen language");
    type_pin(&mut app, "1234");
    assert!(app.query_by_role_and_label(Role::Button, "Khóa").is_some(), "the top-bar lock button too");
}

// ---------------- saving (audit H-01) ----------------

/// The app on a writable temporary store (instead of the read-only demo one).
fn launch_writable(page: &str) -> (Harness<'static, DeskApp>, PathBuf) {
    let mut h = launch(page, "ko");
    let dir = std::env::temp_dir().join(format!("mdd-e2e-{}", new_id()));
    std::fs::create_dir_all(dir.join("files")).unwrap();
    h.state_mut().store = Store::at(dir.clone()).with_secrets(std::sync::Arc::new(crate::model::MemSecrets::default()));
    (h, dir)
}

/// Lets the 1-second save timer fire on the next frame.
fn save_tick(app: &mut Harness<'static, DeskApp>) {
    app.state_mut().last_save = Instant::now() - Duration::from_secs(2);
    app.run_steps(2);
}

#[test]
fn e2e_failed_save_warns_and_retries_until_it_works() {
    let _screen = lock_screen();
    let (mut app, dir) = launch_writable("home");
    std::fs::create_dir(dir.join("library.json.tmp")).unwrap(); // the disk refuses the write
    app.state_mut().lib.docs.push(Document { id: new_id(), title: "새 메모".into(), ..Default::default() });
    app.state_mut().lib_dirty = true;
    save_tick(&mut app);
    assert!(app.query_by_label_contains("저장하지 못했습니다").is_some(), "the failure is shown");
    assert!(app.state().lib_dirty, "the change is still waiting to be saved");

    std::fs::remove_dir(dir.join("library.json.tmp")).unwrap(); // the disk works again
    save_tick(&mut app);
    assert!(!app.state().lib_dirty);
    let saved: Library = serde_json::from_slice(&std::fs::read(dir.join("library.json")).unwrap()).unwrap();
    assert!(saved.docs.iter().any(|d| d.title == "새 메모"), "retried and saved");
}

#[test]
fn e2e_new_lock_number_that_failed_to_save_is_not_reported_as_done() {
    let _screen = lock_screen();
    let (mut app, dir) = launch_writable("home");
    app.state_mut().settings.lock_pin.clear();
    app.state_mut().lock = crate::lock::Lock::starting("");
    std::fs::create_dir(dir.join("settings.json.tmp")).unwrap();
    app.run_steps(2);
    type_pin(&mut app, "1357");
    type_pin(&mut app, "1357");
    assert!(app.query_by_label_contains("잠금 번호를 정했습니다").is_none(), "no success message when it wasn't saved");
    assert!(app.query_by_label_contains("저장하지 못했습니다").is_some());

    std::fs::remove_dir(dir.join("settings.json.tmp")).unwrap();
    save_tick(&mut app);
    let saved: Settings = serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap();
    assert!(crate::lock::verify_pin(&saved.lock_pin, "1357"), "the number is on disk after the retry");
}

// ---------------- importing (audit M-04, M-05) ----------------

fn drop_files(app: &mut Harness<'static, DeskApp>, paths: &[PathBuf]) {
    for p in paths {
        app.input_mut().dropped_files.push(egui::DroppedFile { path: Some(p.clone()), ..Default::default() });
    }
    app.run_steps(1);
}

/// Steps until every import (and anything queued behind it) is finished.
fn wait_imports(app: &mut Harness<'static, DeskApp>) {
    for _ in 0..300 {
        app.run_steps(1);
        if app.state().import_rx.is_none() && app.state().import_queue.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("imports did not finish");
}

#[test]
fn e2e_second_drop_while_importing_keeps_both() {
    let _screen = lock_screen();
    let (mut app, dir) = launch_writable("library");
    let (a, b) = (dir.join("first.txt"), dir.join("second.txt"));
    std::fs::write(&a, "FIRST DOCUMENT").unwrap();
    std::fs::write(&b, "SECOND DOCUMENT").unwrap();
    drop_files(&mut app, &[a]);
    drop_files(&mut app, &[b]); // while the first one is still being read
    wait_imports(&mut app);
    let names: Vec<String> = app.state().lib.live_docs().map(|(_, d)| d.file_name.clone()).collect();
    assert!(names.contains(&"first.txt".to_string()), "first kept: {names:?}");
    assert!(names.contains(&"second.txt".to_string()), "second kept: {names:?}");
}

#[test]
fn e2e_same_name_and_size_but_different_content_are_both_kept() {
    let _screen = lock_screen();
    let (mut app, dir) = launch_writable("library");
    for n in ["a", "b", "c"] {
        std::fs::create_dir(dir.join(n)).unwrap();
    }
    std::fs::write(dir.join("a/report.txt"), "AAAA").unwrap();
    std::fs::write(dir.join("b/report.txt"), "BBBB").unwrap();
    std::fs::write(dir.join("c/report.txt"), "AAAA").unwrap(); // a real duplicate of a/
    let before = app.state().lib.live_docs().count();
    for n in ["a", "b", "c"] {
        drop_files(&mut app, &[dir.join(n).join("report.txt")]);
        wait_imports(&mut app);
    }
    let reports: Vec<String> = app.state().lib.live_docs().filter(|(_, d)| d.file_name == "report.txt").map(|(_, d)| d.text.clone()).collect();
    assert_eq!(app.state().lib.live_docs().count(), before + 2);
    assert!(reports.contains(&"AAAA".to_string()) && reports.contains(&"BBBB".to_string()), "{reports:?}");
}

// ---------------- nothing starts before the lock opens (audit M-09) ----------------

#[test]
fn e2e_auto_collect_and_sync_wait_for_the_lock() {
    let _screen = lock_screen();
    let (mut app, dir) = launch_writable("home");
    let watch = dir.join("watch");
    std::fs::create_dir_all(&watch).unwrap();
    std::fs::write(watch.join("a.txt"), "내용").unwrap();
    let cloud = dir.join("cloud");
    std::fs::create_dir_all(&cloud).unwrap();
    {
        let st = app.state_mut();
        st.settings.watch_folders = vec![watch.display().to_string()];
        st.settings.auto_scan = true;
        st.settings.local_only = false;
        st.settings.sync_folder = cloud.display().to_string();
        st.settings.account_id = "me".into();
        st.settings.remember_pass = true;
        st.settings.saved_pass = "secret123".into();
        st.connect_after_unlock = true;
        st.settings.lock_pin = crate::lock::hash_pin("1234");
        st.lock = crate::lock::Lock::starting(&st.settings.lock_pin);
    }
    app.run_steps(5);
    assert!(app.state().pipe.last_scan.is_none(), "no auto-collect while locked");
    assert!(app.state().account.is_none(), "no sync account opened while locked");

    type_pin(&mut app, "1234");
    assert!(app.state().pipe.last_scan.is_some(), "auto-collect starts once opened");
    assert!(app.state().account.is_some(), "the remembered account connects once opened");
}

// ---------------- a background job that dies (audit 6) ----------------

#[test]
fn e2e_import_that_ends_without_a_result_is_not_left_running() {
    let _screen = lock_screen();
    let (mut app, _dir) = launch_writable("library");
    let (tx, rx) = channel::<ImportMsg>();
    drop(tx); // the reading thread is gone without sending its result
    app.state_mut().import_rx = Some(rx);
    app.state_mut().importing = 3;
    app.run_steps(3);
    assert!(app.state().import_rx.is_none());
    assert_eq!(app.state().importing, 0, "no longer 'importing 3 files'");
    assert!(app.query_by_label_contains("중단").is_some(), "the person is told");
}

// ---------------- secrets (audit 6) ----------------

#[test]
fn e2e_api_key_typed_on_the_ai_page_is_not_written_to_settings_file() {
    let _screen = lock_screen();
    let (mut app, dir) = launch_writable("ai");
    let mem = std::sync::Arc::new(crate::model::MemSecrets::default());
    app.state_mut().store = Store::at(dir.clone()).with_secrets(mem.clone());
    app.state_mut().aic.api_tab = Some(Provider::OpenAi);
    app.run_steps(3);
    app.get_by_role(Role::PasswordInput).type_text("sk-e2e-secret-1");
    app.run_steps(3);
    save_tick(&mut app);
    let file = std::fs::read_to_string(dir.join("settings.json")).unwrap();
    assert!(!file.contains("sk-e2e-secret-1"), "the key is not in settings.json");
    use crate::model::Secrets;
    assert_eq!(mem.get("api:openai").unwrap().as_deref(), Some("sk-e2e-secret-1"), "it is in the secret store");
    assert_eq!(app.state().settings.key_for(Provider::OpenAi), "sk-e2e-secret-1", "and the app still uses it");
}

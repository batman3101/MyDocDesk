//! The 4-digit lock that works as the app's login on a personal PC.
//!
//! First start: pick a number and type it again to confirm, then the app opens.
//! Later starts, and whenever the person locks the app: type the same 4 digits to open it.
//! Only a salted hash of the number is kept (in settings.json, this device only).
//! This is a screen lock, not encryption: the documents themselves are not scrambled by it.

use std::time::Instant;

pub const PIN_LEN: usize = 4;

/// What the lock screen is showing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Unlocked: the app is in use.
    Open,
    /// Type the number to open the app.
    Unlock,
    /// Pick a number (`first` holds the first entry while waiting for the confirmation).
    Setup { first: Option<String> },
    /// Changing the number: type the current one first.
    ChangeVerify,
}

/// What one complete 4-digit entry did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Unlocked,
    /// A new number was saved (the caller saves settings).
    PinSet,
    NeedConfirm,
    Mismatch,
    Wrong,
    Waiting,
    /// The current number was right; now pick the new one.
    ChangeAllowed,
}

pub struct Lock {
    pub screen: Screen,
    pub input: String,
    pub error: String,
    /// Picking a number from settings (can be cancelled), not the first start.
    pub changing: bool,
    fails: u32,
    wait_until: Option<Instant>,
}

const ITER: u32 = 60_000;
const MAX_FAILS: u32 = 5;
const WAIT_SECS: u64 = 30;

/// Keeps ASCII digits only, at most four.
pub fn clean_input(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).take(PIN_LEN).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn derive(pin: &str, salt: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(pin.as_bytes(), salt, ITER, &mut out);
    out
}

/// "salt$hash" in hex; a new random salt every time.
pub fn hash_pin(pin: &str) -> String {
    let mut salt = [0u8; 16];
    let _ = getrandom::getrandom(&mut salt);
    format!("{}${}", hex(&salt), hex(&derive(pin, &salt)))
}

pub fn verify_pin(stored: &str, pin: &str) -> bool {
    let Some((salt_hex, hash_hex)) = stored.split_once('$') else { return false };
    if salt_hex.len() % 2 != 0 || hash_hex.len() != 64 {
        return false;
    }
    let salt: Option<Vec<u8>> = (0..salt_hex.len()).step_by(2).map(|i| u8::from_str_radix(salt_hex.get(i..i + 2)?, 16).ok()).collect();
    let Some(salt) = salt else { return false };
    let got = hex(&derive(pin, &salt));
    // compare every byte so the time taken doesn't hint at the number
    got.len() == hash_hex.len() && got.bytes().zip(hash_hex.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

impl Lock {
    fn at(screen: Screen) -> Lock {
        Lock { screen, input: String::new(), error: String::new(), changing: false, fails: 0, wait_until: None }
    }
    /// How the app starts: pick a number when none is set, otherwise locked.
    pub fn starting(pin_hash: &str) -> Lock {
        Lock::at(if pin_hash.is_empty() { Screen::Setup { first: None } } else { Screen::Unlock })
    }
    pub fn open() -> Lock {
        Lock::at(Screen::Open)
    }
    pub fn is_locked(&self) -> bool {
        self.screen != Screen::Open
    }
    /// The person steps away.
    pub fn lock(&mut self) {
        self.screen = Screen::Unlock;
        self.input.clear();
        self.error.clear();
        self.changing = false;
    }
    pub fn begin_change(&mut self) {
        self.screen = Screen::ChangeVerify;
        self.input.clear();
        self.error.clear();
        self.changing = true;
    }
    /// Only changing the number can be cancelled.
    pub fn cancel(&mut self) {
        if self.changing {
            *self = Lock { fails: self.fails, wait_until: self.wait_until, ..Lock::open() };
        }
    }
    pub fn waiting_secs(&self, now: Instant) -> u64 {
        match self.wait_until {
            Some(t) if t > now => (t - now).as_secs_f32().ceil() as u64,
            _ => 0,
        }
    }
    /// Handles a complete entry in `input`.
    pub fn submit(&mut self, stored_hash: &mut String, now: Instant) -> Outcome {
        let pin = std::mem::take(&mut self.input);
        if self.waiting_secs(now) > 0 {
            self.error = crate::ft!("{}초 뒤에 다시 입력해 주세요", "Hãy nhập lại sau {} giây", self.waiting_secs(now));
            return Outcome::Waiting;
        }
        self.wait_until = None;
        match self.screen.clone() {
            Screen::Open => Outcome::Unlocked,
            Screen::Unlock | Screen::ChangeVerify => {
                if verify_pin(stored_hash, &pin) {
                    self.fails = 0;
                    self.error.clear();
                    if self.screen == Screen::ChangeVerify {
                        self.screen = Screen::Setup { first: None };
                        Outcome::ChangeAllowed
                    } else {
                        self.screen = Screen::Open;
                        Outcome::Unlocked
                    }
                } else {
                    self.fails += 1;
                    if self.fails >= MAX_FAILS {
                        self.fails = 0;
                        self.wait_until = Some(now + std::time::Duration::from_secs(WAIT_SECS));
                        self.error = crate::ft!("{}번 틀려서 {}초 동안 입력을 막습니다", "Sai {} lần nên tạm khóa nhập trong {} giây", MAX_FAILS, WAIT_SECS);
                    } else {
                        self.error = crate::ft!("번호가 맞지 않습니다 ({}/{})", "Mã số không đúng ({}/{})", self.fails, MAX_FAILS);
                    }
                    Outcome::Wrong
                }
            }
            Screen::Setup { first: None } => {
                self.error.clear();
                self.screen = Screen::Setup { first: Some(pin) };
                Outcome::NeedConfirm
            }
            Screen::Setup { first: Some(first) } => {
                if first == pin {
                    *stored_hash = hash_pin(&pin);
                    self.error.clear();
                    self.changing = false;
                    self.screen = Screen::Open;
                    Outcome::PinSet
                } else {
                    self.screen = Screen::Setup { first: None };
                    self.error = crate::i18n::tr("두 번 입력한 번호가 다릅니다. 처음부터 다시 정해 주세요").into();
                    Outcome::Mismatch
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn enter(l: &mut Lock, stored: &mut String, pin: &str, now: Instant) -> Outcome {
        l.input = pin.into();
        l.submit(stored, now)
    }

    #[test]
    fn input_keeps_only_four_digits() {
        assert_eq!(clean_input("12a3 45"), "1234");
        assert_eq!(clean_input("９"), ""); // full-width digits are not accepted
        assert_eq!(clean_input("07"), "07");
    }

    #[test]
    fn pin_is_stored_as_salted_hash() {
        let h = hash_pin("1234");
        assert!(verify_pin(&h, "1234"));
        assert!(!verify_pin(&h, "4321"));
        assert_ne!(h, hash_pin("1234"), "each hash gets its own salt");
        assert!(!verify_pin("", "1234"), "no number set: nothing opens it");
        assert!(!verify_pin("garbage", "1234"));
    }

    #[test]
    fn first_start_picks_and_confirms_then_opens() {
        let now = Instant::now();
        let mut stored = String::new();
        let mut l = Lock::starting(&stored);
        assert_eq!(l.screen, Screen::Setup { first: None });
        assert!(l.is_locked());
        assert_eq!(enter(&mut l, &mut stored, "1234", now), Outcome::NeedConfirm);
        assert!(l.is_locked());
        assert!(l.input.is_empty(), "the box is cleared for the confirmation");
        assert_eq!(enter(&mut l, &mut stored, "1234", now), Outcome::PinSet);
        assert!(!l.is_locked());
        assert!(verify_pin(&stored, "1234"));
    }

    #[test]
    fn confirmation_mismatch_starts_over() {
        let now = Instant::now();
        let mut stored = String::new();
        let mut l = Lock::starting(&stored);
        enter(&mut l, &mut stored, "1234", now);
        assert_eq!(enter(&mut l, &mut stored, "5678", now), Outcome::Mismatch);
        assert_eq!(l.screen, Screen::Setup { first: None });
        assert!(!l.error.is_empty());
        assert!(stored.is_empty(), "nothing is saved");
    }

    #[test]
    fn start_with_number_is_locked_and_opens_with_it() {
        let now = Instant::now();
        let mut stored = hash_pin("1234");
        let mut l = Lock::starting(&stored);
        assert_eq!(l.screen, Screen::Unlock);
        assert_eq!(enter(&mut l, &mut stored, "0000", now), Outcome::Wrong);
        assert!(l.is_locked());
        assert!(!l.error.is_empty());
        assert_eq!(enter(&mut l, &mut stored, "1234", now), Outcome::Unlocked);
        assert!(!l.is_locked());
        assert!(l.error.is_empty());
        l.lock();
        assert_eq!(l.screen, Screen::Unlock);
    }

    #[test]
    fn five_wrong_tries_wait_thirty_seconds() {
        let now = Instant::now();
        let mut stored = hash_pin("1234");
        let mut l = Lock::starting(&stored);
        for _ in 0..5 {
            enter(&mut l, &mut stored, "9999", now);
        }
        assert_eq!(enter(&mut l, &mut stored, "1234", now), Outcome::Waiting, "even the right number waits");
        assert!(l.waiting_secs(now) > 25);
        let later = now + Duration::from_secs(31);
        assert_eq!(l.waiting_secs(later), 0);
        assert_eq!(enter(&mut l, &mut stored, "1234", later), Outcome::Unlocked);
    }

    #[test]
    fn change_number_needs_the_current_one() {
        let now = Instant::now();
        let mut stored = hash_pin("1234");
        let mut l = Lock::open();
        l.begin_change();
        assert_eq!(l.screen, Screen::ChangeVerify);
        assert_eq!(enter(&mut l, &mut stored, "0000", now), Outcome::Wrong);
        assert_eq!(enter(&mut l, &mut stored, "1234", now), Outcome::ChangeAllowed);
        assert_eq!(l.screen, Screen::Setup { first: None });
        enter(&mut l, &mut stored, "2468", now);
        assert_eq!(enter(&mut l, &mut stored, "2468", now), Outcome::PinSet);
        assert!(!l.is_locked());
        assert!(verify_pin(&stored, "2468"));
        assert!(!verify_pin(&stored, "1234"));
    }

    #[test]
    fn changing_can_be_cancelled_but_unlocking_cannot() {
        let mut l = Lock::open();
        l.begin_change();
        l.cancel();
        assert!(!l.is_locked());
        let mut l = Lock::starting(&hash_pin("1234"));
        l.cancel();
        assert!(l.is_locked(), "cancel does not skip the lock");
        let mut l = Lock::starting("");
        l.cancel();
        assert!(l.is_locked(), "cancel does not skip picking the first number");
    }

    /// Every fixed text of the lock (screen, top-bar button, settings) has a Vietnamese version.
    pub(crate) const LOCK_TEXTS: &[&str] = &[
        "두 번 입력한 번호가 다릅니다. 처음부터 다시 정해 주세요",
        "잠금 번호 정하기",
        "이 PC에서 MyDocDesk를 열 때 쓸 숫자 4자리를 정하세요",
        "확인을 위해 같은 번호를 한 번 더 입력해 주세요",
        "지금 쓰는 잠금 번호를 입력하세요",
        "확인되면 새 번호를 정합니다",
        "잠금 번호를 입력하세요",
        "숫자 4자리를 넣으면 바로 열립니다",
        "숫자 4자리",
        "번호는 이 PC에만 저장됩니다",
        "취소",
        "잠금 번호를 정했습니다. 자리를 비울 땐 🔒 잠금 (Ctrl+L)",
        "🔒 잠금",
        "잠금",
        "자리를 비울 때 잠급니다 (Ctrl+L)",
        "잠금 (로그인)",
        "앱을 열 때와 잠금을 풀 때 숫자 4자리를 씁니다. 자리를 비울 땐 위쪽의 🔒 잠금 또는 Ctrl+L.",
        "잠금 번호 바꾸기",
    ];

    #[test]
    fn lock_texts_have_vietnamese() {
        let missing: Vec<&&str> = LOCK_TEXTS.iter().filter(|s| crate::i18n::tr_in(true, s) == **s).collect();
        assert!(missing.is_empty(), "no Vietnamese for: {missing:?}");
    }
}

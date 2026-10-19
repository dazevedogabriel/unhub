//! Executa macros (equivalente ao MacroPlayerActor do G HUB).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::macros::{Macro, MacroType, Step, WheelDir};

const MIN_LOOP_MS: u64 = 10; // evita inundar o sistema com um loop sem nenhum atraso

pub trait Output: Send + Sync {
    fn key(&self, code: u16, down: bool);
    fn mouse(&self, button: u8, down: bool);
    fn wheel(&self, dir: WheelDir, amount: u32);
    fn text(&self, s: &str);
    fn media(&self, code: u16);
    fn launch(&self, path: &str, args: &str);
}

/// Saída real via SendInput.
pub struct WinOutput;

impl Output for WinOutput {
    fn key(&self, code: u16, down: bool) {
        let _ = crate::winput::key(code, down);
    }
    fn mouse(&self, button: u8, down: bool) {
        let _ = crate::winput::mouse_button(button, down);
    }
    fn wheel(&self, dir: WheelDir, amount: u32) {
        let _ = crate::winput::wheel(dir, amount);
    }
    fn text(&self, s: &str) {
        let _ = crate::winput::text(s);
    }
    fn media(&self, code: u16) {
        let _ = crate::winput::media(code);
    }
    fn launch(&self, path: &str, args: &str) {
        // ShellExecute aceita .exe, .bat, atalhos .lnk e documentos
        use crate::util::wide;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let (verb, p, a) = (wide("open"), wide(path), wide(args));
        unsafe {
            ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), p.as_ptr(), a.as_ptr(), std::ptr::null(), SW_SHOWNORMAL);
        }
    }
}

/// Insere o atraso padrão entre ações consecutivas que não são atrasos (opção do G HUB).
pub fn with_default_delays(steps: &[Step], m: &Macro, looped: bool) -> Vec<Step> {
    if !m.use_default_delay || steps.is_empty() {
        return steps.to_vec();
    }
    let d = Step::Delay { ms: m.default_delay };
    let mut out = Vec::with_capacity(steps.len() * 2);
    for (i, s) in steps.iter().enumerate() {
        out.push(s.clone());
        let next = steps.get(i + 1).or(if looped { steps.first() } else { None });
        if let Some(n) = next {
            if !s.is_delay() && !n.is_delay() {
                out.push(d.clone());
            }
        }
    }
    out
}

#[derive(Default)]
struct Flags {
    stop: bool,     // interrompe tudo (toggle off, agente saindo)
    released: bool, // botão solto
}

#[derive(Clone, Copy, PartialEq)]
enum Until {
    Stop,
    Either,
}

#[derive(Default)]
struct RunCtl {
    flags: Mutex<Flags>,
    cv: Condvar,
}

impl RunCtl {
    fn fired(&self, until: Until) -> bool {
        let f = self.flags.lock().unwrap();
        f.stop || (until == Until::Either && f.released)
    }

    /// Espera `ms`; true se a condição disparou antes.
    fn wait(&self, ms: u64, until: Until) -> bool {
        let deadline = Instant::now() + Duration::from_millis(ms);
        let mut f = self.flags.lock().unwrap();
        loop {
            if f.stop || (until == Until::Either && f.released) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            f = self.cv.wait_timeout(f, deadline - now).unwrap().0;
        }
    }

    fn wait_forever(&self, until: Until) {
        let mut f = self.flags.lock().unwrap();
        while !(f.stop || (until == Until::Either && f.released)) {
            f = self.cv.wait(f).unwrap();
        }
    }

    fn set(&self, stop: bool, released: bool) {
        let mut f = self.flags.lock().unwrap();
        f.stop |= stop;
        f.released |= released;
        self.cv.notify_all();
    }
}

/// Uma execução em andamento de uma macro, disparada por um botão.
struct Run<'a> {
    m: &'a Macro,
    out: &'a dyn Output,
    ctl: &'a RunCtl,
    held_keys: HashSet<u16>,
    held_buttons: HashSet<u8>,
}

impl Run<'_> {
    /// Toca a lista; false se `until` disparou no meio.
    fn play(&mut self, steps: &[Step], until: Until) -> bool {
        for s in steps {
            if self.ctl.fired(until) {
                return false;
            }
            match s {
                Step::Delay { ms } => {
                    if self.ctl.wait(*ms as u64, until) {
                        return false;
                    }
                }
                Step::Key { code, down } => {
                    self.out.key(*code, *down);
                    if *down { self.held_keys.insert(*code); } else { self.held_keys.remove(code); }
                }
                Step::Mouse { button, down } => {
                    self.out.mouse(*button, *down);
                    if *down { self.held_buttons.insert(*button); } else { self.held_buttons.remove(button); }
                }
                Step::Wheel { dir, amount } => self.out.wheel(*dir, *amount),
                Step::Text { text } => self.out.text(text),
                Step::Media { code } => self.out.media(*code),
                Step::Launch { path, args } => self.out.launch(path, args),
            }
        }
        true
    }

    fn looped(&mut self, steps: &[Step], until: Until) {
        let steps = with_default_delays(steps, self.m, true);
        if steps.is_empty() {
            return;
        }
        let has_delay = steps.iter().any(|s| matches!(s, Step::Delay { ms } if *ms > 0));
        while !self.ctl.fired(until) {
            let start = Instant::now();
            if !self.play(&steps, until) {
                break;
            }
            if !has_delay {
                let el = start.elapsed().as_millis() as u64;
                if el < MIN_LOOP_MS && self.ctl.wait(MIN_LOOP_MS - el, until) {
                    break;
                }
            }
        }
    }

    fn main(&mut self) {
        let m = self.m;
        match m.kind {
            MacroType::Simple => {
                self.play(&with_default_delays(&m.simple, m, false), Until::Stop);
            }
            MacroType::Repeat => self.looped(&m.held, Until::Either),
            MacroType::Toggle => self.looped(&m.toggle, Until::Stop),
            MacroType::Sequence => {
                if self.play(&with_default_delays(&m.press, m, false), Until::Stop) {
                    self.looped(&m.held, Until::Either);
                    if !self.ctl.fired(Until::Stop) {
                        self.ctl.wait_forever(Until::Either);
                        self.play(&with_default_delays(&m.release, m, false), Until::Stop);
                    }
                }
            }
        }
        self.release_all();
    }

    /// Solta o que a macro deixou pressionado (nunca deixar Ctrl "preso").
    fn release_all(&mut self) {
        for code in self.held_keys.drain() {
            self.out.key(code, false);
        }
        for b in self.held_buttons.drain() {
            self.out.mouse(b, false);
        }
    }
}

struct Active {
    id: u64,
    kind: MacroType,
    ctl: Arc<RunCtl>,
}

/// Recebe pressionar/soltar de cada gatilho e controla as execuções.
pub struct Player {
    out: Arc<dyn Output>,
    runs: Arc<Mutex<HashMap<u32, Active>>>,
    next_id: AtomicU64,
}

impl Player {
    pub fn new(out: Arc<dyn Output>) -> Self {
        Self { out, runs: Arc::default(), next_id: AtomicU64::new(1) }
    }

    pub fn press(&self, trigger: u32, m: &Macro) {
        let mut runs = self.runs.lock().unwrap();
        if let Some(run) = runs.get(&trigger) {
            if run.kind == MacroType::Toggle {
                run.ctl.set(true, false); // 2º toque desliga
            }
            return; // simple/sequence em andamento: ignora repetição
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let ctl = Arc::new(RunCtl::default());
        runs.insert(trigger, Active { id, kind: m.kind, ctl: ctl.clone() });
        drop(runs);
        let m = m.clone();
        let out = self.out.clone();
        let all = self.runs.clone();
        std::thread::Builder::new()
            .name(format!("macro:{}", m.name))
            .spawn(move || {
                Run { m: &m, out: &*out, ctl: &ctl, held_keys: HashSet::new(), held_buttons: HashSet::new() }.main();
                let mut runs = all.lock().unwrap();
                if runs.get(&trigger).map(|a| a.id) == Some(id) {
                    runs.remove(&trigger);
                }
            })
            .ok();
    }

    pub fn release(&self, trigger: u32) {
        let runs = self.runs.lock().unwrap();
        if let Some(run) = runs.get(&trigger) {
            if run.kind != MacroType::Toggle {
                run.ctl.set(false, true);
            }
        }
    }

    pub fn stop_all(&self) {
        for run in self.runs.lock().unwrap().values() {
            run.ctl.set(true, true);
        }
        let end = Instant::now() + Duration::from_secs(1);
        while self.busy() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn busy(&self) -> bool {
        !self.runs.lock().unwrap().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeOut(Mutex<Vec<String>>);

    impl Output for FakeOut {
        fn key(&self, c: u16, d: bool) { self.0.lock().unwrap().push(format!("key {c} {d}")); }
        fn mouse(&self, b: u8, d: bool) { self.0.lock().unwrap().push(format!("mouse {b} {d}")); }
        fn wheel(&self, _: WheelDir, n: u32) { self.0.lock().unwrap().push(format!("wheel {n}")); }
        fn text(&self, s: &str) { self.0.lock().unwrap().push(format!("text {s}")); }
        fn media(&self, c: u16) { self.0.lock().unwrap().push(format!("media {c}")); }
        fn launch(&self, p: &str, _: &str) { self.0.lock().unwrap().push(format!("launch {p}")); }
    }

    fn k(code: u16, down: bool) -> Step {
        Step::Key { code, down }
    }

    fn wait_idle(p: &Player) {
        let end = Instant::now() + Duration::from_secs(2);
        while p.busy() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn setup() -> (Arc<FakeOut>, Player) {
        let out = Arc::new(FakeOut::default());
        (out.clone(), Player::new(out))
    }

    #[test]
    fn default_delays_inserted() {
        let mut m = Macro::new("x", MacroType::Simple);
        m.default_delay = 30;
        let s = with_default_delays(&[k(4, true), k(4, false), Step::Delay { ms: 5 }, k(5, true)], &m, false);
        assert_eq!(s, vec![k(4, true), Step::Delay { ms: 30 }, k(4, false), Step::Delay { ms: 5 }, k(5, true)]);
        let l = with_default_delays(&[k(4, true), k(4, false)], &m, true);
        assert_eq!(l.len(), 4);
    }

    #[test]
    fn simple_plays_once_and_releases_held() {
        let (out, p) = setup();
        let mut m = Macro::new("x", MacroType::Simple);
        m.use_default_delay = false;
        m.simple = vec![k(0xE0, true), k(6, true), k(6, false)]; // Ctrl fica "preso"
        p.press(1, &m);
        wait_idle(&p);
        let ev = out.0.lock().unwrap().clone();
        assert_eq!(ev, vec!["key 224 true", "key 6 true", "key 6 false", "key 224 false"]);
    }

    #[test]
    fn repeat_until_release() {
        let (out, p) = setup();
        let mut m = Macro::new("x", MacroType::Repeat);
        m.use_default_delay = false;
        m.held = vec![Step::Mouse { button: 1, down: true }, Step::Mouse { button: 1, down: false }];
        p.press(1, &m);
        std::thread::sleep(Duration::from_millis(80));
        p.release(1);
        wait_idle(&p);
        let n = out.0.lock().unwrap().len();
        assert!(n >= 6, "{n} eventos");
        assert!(!p.busy());
    }

    #[test]
    fn toggle_second_press_stops() {
        let (_out, p) = setup();
        let mut m = Macro::new("x", MacroType::Toggle);
        m.toggle = vec![Step::Delay { ms: 5 }];
        p.press(1, &m);
        p.release(1);
        std::thread::sleep(Duration::from_millis(30));
        assert!(p.busy());
        p.press(1, &m);
        wait_idle(&p);
        assert!(!p.busy());
    }

    #[test]
    fn sequence_press_held_release() {
        let (out, p) = setup();
        let mut m = Macro::new("x", MacroType::Sequence);
        m.use_default_delay = false;
        m.press = vec![Step::Text { text: "a".into() }];
        m.held = vec![Step::Delay { ms: 5 }];
        m.release = vec![Step::Text { text: "b".into() }];
        p.press(1, &m);
        std::thread::sleep(Duration::from_millis(30));
        p.release(1);
        wait_idle(&p);
        assert_eq!(out.0.lock().unwrap().clone(), vec!["text a", "text b"]);
    }
}

//! Thread que fala com o mouse, para a interface nunca travar esperando o HID.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use hidapi::HidApi;

use crate::mouse::{Mouse, Status};
use crate::onboard::Profile;
use crate::util;

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub pid: u16,
    pub name: String,
    pub button_count: usize,
    pub dpi_range: (u32, u32, u32),
}

pub enum Cmd {
    Reload,
    Apply(Profile),
    Restore,
    TestDpi(u32),
    Reconnect,
}

pub enum Event {
    Connected(DeviceInfo, Profile, Option<Status>),
    NoDevice(String),
    Profile(Profile),
    Status(Status),
    Applied(Profile),
    Restored(PathBuf, Profile),
    DpiTested(u32),
    Error(String),
}

pub struct Worker {
    pub tx: Sender<Cmd>,
    pub rx: Receiver<Event>,
}

impl Worker {
    pub fn spawn(ctx: eframe::egui::Context) -> Self {
        let (tx, cmd_rx) = mpsc::channel();
        let (ev_tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("unhub-device".into())
            .spawn(move || run(cmd_rx, ev_tx, ctx))
            .expect("thread do dispositivo");
        Self { tx, rx }
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

fn run(rx: Receiver<Cmd>, tx: Sender<Event>, ctx: eframe::egui::Context) {
    let emit = |e: Event| {
        let _ = tx.send(e);
        ctx.request_repaint();
    };
    let mut api: Option<HidApi> = None;
    let mut mouse: Option<Mouse> = None;
    let mut last_try = Instant::now() - Duration::from_secs(60);
    let mut last_err = String::new();

    loop {
        if mouse.is_none() && last_try.elapsed() >= Duration::from_secs(3) {
            last_try = Instant::now();
            match connect(&mut api) {
                Ok((mut m, profile)) => {
                    let info = DeviceInfo {
                        pid: m.pid,
                        name: m.name.clone(),
                        button_count: m.memory.button_count.min(crate::onboard::BUTTON_SLOTS),
                        dpi_range: m.dpi_range(),
                    };
                    let st = m.status().ok();
                    mouse = Some(m);
                    last_err.clear();
                    emit(Event::Connected(info, profile, st));
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg != last_err {
                        last_err = msg.clone();
                        emit(Event::NoDevice(msg));
                    }
                }
            }
        }

        let cmd = match rx.recv_timeout(Duration::from_millis(1500)) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };

        let Some(m) = mouse.as_mut() else {
            if let Some(Cmd::Reconnect) = cmd {
                last_try = Instant::now() - Duration::from_secs(60);
                last_err.clear();
            } else if cmd.is_some() {
                emit(Event::Error("mouse desconectado".into()));
            }
            continue;
        };

        let was_poll = cmd.is_none();
        let result: anyhow::Result<Option<Event>> = match cmd {
            None => m.status().map(|s| Some(Event::Status(s))),
            Some(Cmd::Reconnect) => Ok(None),
            Some(Cmd::Reload) => m.read_profile().map(|p| Some(Event::Profile(p))),
            Some(Cmd::Apply(p)) => m.apply(&p).and_then(|_| m.read_profile()).map(|p| Some(Event::Applied(p))),
            Some(Cmd::Restore) => m
                .restore_original()
                .and_then(|path| Ok((path, m.read_profile()?)))
                .map(|(path, p)| Some(Event::Restored(path, p))),
            Some(Cmd::TestDpi(v)) => m.set_dpi_now(v).map(|_| Some(Event::DpiTested(v))),
        };
        match result {
            Ok(Some(ev)) => emit(ev),
            Ok(None) => {}
            Err(e) => {
                // perdeu o mouse? tenta reabrir no próximo ciclo
                let gone = was_poll || m.status().is_err();
                if !was_poll {
                    emit(Event::Error(e.to_string()));
                }
                if gone {
                    mouse = None;
                    last_err = "mouse desconectado".into();
                    emit(Event::NoDevice(last_err.clone()));
                    last_try = Instant::now() - Duration::from_secs(60);
                }
            }
        }
    }
}

fn connect(api: &mut Option<HidApi>) -> anyhow::Result<(Mouse, Profile)> {
    match api {
        Some(a) => a.refresh_devices()?,
        None => *api = Some(HidApi::new()?),
    }
    let mut m = Mouse::open_first(api.as_ref().unwrap())?;
    let p = m.read_profile()?;
    let _ = util::data_dir();
    Ok((m, p))
}

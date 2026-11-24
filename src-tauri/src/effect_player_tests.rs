//! The effect player bridge against a fake helper that speaks the protocol over pipes.
use super::*;
use std::sync::atomic::AtomicUsize;

#[derive(Clone, Default)]
struct Script {
    /// Exit with code 3 after an `opengl` error status, before `ready`.
    opengl: bool,
    /// Exit (without a reply) at these load numbers, counted over every fake started.
    die_at_load: Vec<usize>,
    /// `load` of this effect is refused.
    refuse: Option<String>,
    /// `fps` above this is refused (an older helper).
    max_fps: Option<u64>,
    /// The private memory `stats` reports (MB).
    private_mb: Option<f64>,
}
struct Fake {
    code: Arc<Mutex<Option<i32>>>,
    pid: u32,
}
impl ProcessControl for Fake {
    fn pid(&self) -> u32 {
        self.pid
    }
    fn try_wait(&mut self) -> Option<i32> {
        *lock(&self.code)
    }
    fn kill(&mut self) {
        lock(&self.code).get_or_insert(-1);
    }
}
fn status(out: &mut impl Write, value: Value) {
    let _ = write_message(out, 1, &serde_json::to_vec(&value).unwrap());
}
fn frame(out: &mut impl Write, sequence: u32, generation: u32) {
    let mut payload = vec![0u8; 32];
    payload[..4].copy_from_slice(&sequence.to_le_bytes());
    payload[4..8].copy_from_slice(&generation.to_le_bytes());
    payload[16..18].copy_from_slice(&64u16.to_le_bytes());
    payload[18..20].copy_from_slice(&48u16.to_le_bytes());
    payload.extend_from_slice(b"\xff\xd8jpeg\xff\xd9");
    let _ = write_message(out, 2, &payload);
}
type Spawned = (Spawner, Arc<AtomicUsize>, Arc<Mutex<Vec<Value>>>);
/// A spawner of fakes, the number of fakes started and every command they got.
fn fakes(script: Script) -> Spawned {
    let started = Arc::new(AtomicUsize::new(0));
    let loads = Arc::new(AtomicUsize::new(0));
    let commands = Arc::new(Mutex::new(vec![]));
    let (s, c) = (started.clone(), commands.clone());
    let spawner: Spawner = Arc::new(move |_launch: &Launch| {
        let n = s.fetch_add(1, Ordering::SeqCst) + 1;
        let (stdin_r, stdin_w) = std::io::pipe()?;
        let (stdout_r, mut stdout_w) = std::io::pipe()?;
        let (stderr_r, stderr_w) = std::io::pipe()?;
        let code = Arc::new(Mutex::new(None));
        let (script, loads, commands, exit) = (script.clone(), loads.clone(), c.clone(), code.clone());
        std::thread::spawn(move || {
            let _stderr = stderr_w;
            if script.opengl {
                status(&mut stdout_w, json!({"type": "error", "fatal": true, "code": "opengl", "message": "OpenGL 4.6 is not available on this computer."}));
                *lock(&exit) = Some(3);
                return;
            }
            status(&mut stdout_w, json!({"type": "ready", "protocol": 1, "gl": {"renderer": "Fake GPU", "version": "4.6"}}));
            let (mut generation, mut sequence) = (0u32, 0u32);
            for line in std::io::BufReader::new(stdin_r).lines() {
                let Ok(line) = line else { break };
                let command: Value = serde_json::from_str(&line).unwrap();
                lock(&commands).push(command.clone());
                let id = command["id"].clone();
                let cmd = command["cmd"].as_str().unwrap_or_default().to_owned();
                if cmd == "load" {
                    let k = loads.fetch_add(1, Ordering::SeqCst) + 1;
                    if script.die_at_load.contains(&k) {
                        *lock(&exit) = Some(1);
                        return;
                    }
                    if script.refuse.as_deref() == command["effect"].as_str() {
                        status(&mut stdout_w, json!({"type": "reply", "id": id, "command": cmd, "ok": false, "error": "The effect was not found.", "generation": generation}));
                        continue;
                    }
                }
                if cmd == "fps" && script.max_fps.is_some_and(|m| command["value"].as_u64().unwrap() > m) {
                    status(&mut stdout_w, json!({"type": "reply", "id": id, "command": cmd, "ok": false, "error": "The frame rate must be between 1 and 120.", "generation": generation}));
                    continue;
                }
                if ["load", "overlay", "compare", "restart"].contains(&cmd.as_str()) {
                    generation += 1;
                }
                let loaded = if ["load", "overlay", "compare"].contains(&cmd.as_str()) { json!({"effect": command["effect"], "overlay": command["overlay"], "notes": {}}) } else { Value::Null };
                status(&mut stdout_w, json!({"type": "reply", "id": id, "command": cmd, "ok": true, "generation": generation, "loaded": loaded, "stats": {"privateMB": script.private_mb.unwrap_or(100.)}}));
                for _ in 0..3 {
                    frame(&mut stdout_w, sequence, generation);
                    sequence += 1;
                }
            }
            *lock(&exit) = Some(0);
        });
        Ok(Process { stdin: Box::new(stdin_w), stdout: Box::new(stdout_r), stderr: Box::new(stderr_r), control: Box::new(Fake { code, pid: 90000 + n as u32 }) })
    });
    (spawner, started, commands)
}
fn launch() -> Launch {
    Launch { exe: PathBuf::from("fake.exe"), pak: "pak01_dir.vpk".into(), args: vec![] }
}
fn events(player: &EffectPlayer) -> Arc<Mutex<Vec<Event>>> {
    let seen = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    player.set_sink(Arc::new(move |e| lock(&s).push(e)));
    seen
}
const A: &str = "particles/a.vpcf_c";

#[test]
fn the_frame_layout_is_read_and_only_the_newest_frame_is_kept() {
    let mut bytes = vec![];
    status(&mut bytes, json!({"type": "ready"}));
    frame(&mut bytes, 7, 2);
    write_message(&mut bytes, 9, b"later kinds are skipped").unwrap();
    let mut reader = bytes.as_slice();
    assert!(matches!(read_message(&mut reader).unwrap(), Some(Message::Status(s)) if s["type"] == "ready"));
    let Some(Message::Frame(payload)) = read_message(&mut reader).unwrap() else { panic!() };
    let info = frame_info(&payload).unwrap();
    assert_eq!((info.sequence, info.generation, info.width, info.height), (7, 2, 64, 48));
    assert_eq!(read_message(&mut reader).unwrap(), Some(Message::Other));
    assert_eq!(read_message(&mut reader).unwrap(), None);
    assert!(read_message(&mut &b"XXXX\x01\0\0\0\0\0\0\0"[..]).is_err(), "a broken stream stops");

    let slot = FrameSlot::new();
    assert!(slot.after(0, Duration::from_millis(5)).is_none());
    for g in 1..=5u32 {
        let mut p = vec![0u8; 32];
        p[4..8].copy_from_slice(&g.to_le_bytes());
        slot.put(&p);
    }
    // A burst: the page gets the last one only.
    let newest = slot.after(0, Duration::ZERO).unwrap();
    assert_eq!(&newest[..4], FRAME_MAGIC);
    assert_eq!(u32::from_le_bytes(newest[4..8].try_into().unwrap()), 5);
    assert_eq!(frame_info(&newest[FRAME_PREFIX..]).unwrap().generation, 5);
    assert!(slot.after(5, Duration::from_millis(5)).is_none(), "nothing newer");
}

#[test]
fn no_helper_file_is_a_quiet_state_and_opengl_missing_is_remembered() {
    let (spawner, started, _) = fakes(Script::default());
    let player = EffectPlayer::with_spawner(spawner);
    let seen = events(&player);
    let state = player.shared.ensure(None).unwrap().unwrap();
    assert!(matches!(state, PlayerState::Unavailable { reason: Unavailable::Missing, .. }), "{state:?}");
    assert_eq!(started.load(Ordering::SeqCst), 0);
    assert!(matches!(lock(&seen)[0], Event::Unavailable { reason: Unavailable::Missing, .. }));

    let (spawner, started, _) = fakes(Script { opengl: true, ..Default::default() });
    let player = EffectPlayer::with_spawner(spawner);
    let state = player.shared.ensure(Some(&launch())).unwrap().unwrap();
    let PlayerState::Unavailable { reason: Unavailable::Opengl, message } = state else { panic!("{state:?}") };
    assert_eq!(message, "OpenGL 4.6 is not available on this computer.", "the helper's sentence");
    // Every later effect: not tried again this session.
    for _ in 0..3 {
        assert!(matches!(player.shared.ensure(Some(&launch())).unwrap(), Some(PlayerState::Unavailable { reason: Unavailable::Opengl, .. })));
    }
    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert_eq!(player.pid(), None);
}

#[test]
fn a_helper_that_dies_is_started_once_more_then_the_page_falls_back() {
    // Load 1 plays; the helper dies at load 2; its restart plays the last
    // effect (load 3) and then the effect asked for (load 4); it dies again at 5.
    let (spawner, started, _) = fakes(Script { die_at_load: vec![2, 5], ..Default::default() });
    let player = EffectPlayer::with_spawner(spawner);
    let seen = events(&player);
    let shared = player.shared.clone();
    *lock(&shared.exe) = Some(PathBuf::from("fake.exe"));
    assert_eq!(shared.ensure(Some(&launch())).unwrap(), None);
    let first = shared.send_effect(A, &BTreeMap::new(), None, false).unwrap();
    assert_eq!(first.generation, Some(1));
    lock(&shared.inner).last = Some((A.into(), BTreeMap::new()));
    let answer = {
        let _work = lock(&shared.work);
        shared.send_effect("particles/b.vpcf_c", &BTreeMap::new(), None, false).unwrap()
    };
    assert!(matches!(answer.state, PlayerState::Ready { .. }), "{answer:?}");
    assert!(answer.generation.is_some(), "the effect plays after the restart");
    assert_eq!(started.load(Ordering::SeqCst), 2);
    assert!(lock(&seen).iter().any(|e| matches!(e, Event::Restarted { .. })));
    // It dies again: no third helper; the page shows the strips with one line.
    let answer = {
        let _work = lock(&shared.work);
        shared.send_effect("particles/c.vpcf_c", &BTreeMap::new(), None, false).unwrap()
    };
    let PlayerState::Failed { message } = &answer.state else { panic!("{answer:?}") };
    assert_eq!(message, HELPER_STOPPED_AGAIN);
    assert_eq!(started.load(Ordering::SeqCst), 2);
    assert!(lock(&seen).iter().any(|e| matches!(e, Event::Failed { .. })));
    assert_eq!(player.pid(), None);
}

#[test]
fn a_refused_command_keeps_the_picture_and_controls_reach_the_helper() {
    let (spawner, _, commands) = fakes(Script { refuse: Some("particles/missing.vpcf_c".into()), ..Default::default() });
    let player = EffectPlayer::with_spawner(spawner);
    let shared = player.shared.clone();
    shared.ensure(Some(&launch())).unwrap();
    let played = shared.send_effect(A, &BTreeMap::new(), None, false).unwrap();
    assert!(player.frame_after(0, Duration::from_secs(2)).is_some(), "frames arrive");
    let refused = shared.send_effect("particles/missing.vpcf_c", &BTreeMap::new(), None, false).unwrap();
    assert_eq!(refused.error.as_deref(), Some("The effect was not found."));
    assert!(matches!(refused.state, PlayerState::Ready { .. }), "one line; the player stays");
    assert_eq!(lock(&shared.inner).current.as_ref().unwrap().target, A, "the previous effect stays loaded");
    assert_eq!(lock(&shared.inner).current.as_ref().unwrap().generation, played.generation.unwrap());

    for control in [
        Control::Compare { on: true },
        Control::Pause,
        Control::Speed { value: 0.25 },
        Control::Background { color: "FFFFFF".into() },
        Control::Brightness { stops: 9. },
        Control::Orbit { dx: 5., dy: -2. },
        Control::Zoom { steps: 1.5 },
        Control::ResetView,
    ] {
        let answer = player.control(control).unwrap();
        assert!(answer.error.is_none(), "{answer:?}");
    }
    let view = player.view();
    assert!(view.compare && view.paused && view.speed == 0.25);
    assert_eq!(view.background, BACKGROUNDS[0], "only the dark backgrounds are offered");
    assert_eq!(view.brightness, 4., "view brightness stays within 0 to +4 stops");
    let sent: Vec<String> = lock(&commands).iter().map(|c| c["cmd"].as_str().unwrap().to_owned()).collect();
    assert_eq!(sent, ["load", "load", "compare", "pause", "speed", "background", "brightness", "orbit", "zoom", "resetView"]);
    // Stop closes stdin; the fake sees the end of its input and exits.
    player.stop();
    assert_eq!(player.pid(), None);
    assert_eq!(player.state(), PlayerState::Idle);
}

#[test]
fn page_commands_deserialise_as_the_page_sends_them() {
    let c: Control = serde_json::from_value(json!({"command": "orbit", "dx": 3, "dy": -1})).unwrap();
    assert!(matches!(c, Control::Orbit { dx, dy } if dx == 3. && dy == -1.));
    let c: Control = serde_json::from_value(json!({"command": "reset_view"})).unwrap();
    assert!(matches!(c, Control::ResetView));
    let v: View = serde_json::from_value(json!({"compare": true, "loop": false, "speed": 0.5})).unwrap();
    assert!(v.compare && !v.looping && v.speed == 0.5 && v.width == 640);
    let e = serde_json::to_value(Event::Unavailable { reason: Unavailable::Opengl, message: "x".into() }).unwrap();
    assert_eq!(e, json!({"type": "unavailable", "reason": "opengl", "message": "x"}));
    let s = serde_json::to_value(PlayerState::Ready { driver: "d".into() }).unwrap();
    assert_eq!(s, json!({"state": "ready", "driver": "d"}));
}

#[test]
fn a_saved_frame_rate_is_set_before_the_first_load_and_a_refused_one_degrades() {
    let (spawner, _, commands) = fakes(Script { max_fps: Some(120), ..Default::default() });
    let player = EffectPlayer::with_spawner(spawner);
    let shared = player.shared.clone();
    // The page's saved rate comes with the view of the first play.
    lock(&shared.inner).view.fps = 144;
    shared.ensure(Some(&launch())).unwrap();
    let notice = shared.sync_view();
    let played = shared.send_effect(A, &BTreeMap::new(), None, false).unwrap();
    assert!(played.generation.is_some());
    assert_eq!(notice.as_deref(), Some("This effect player cannot play at 144 frames per second here, so it plays at 120."));
    let sent: Vec<String> = lock(&commands).iter().map(|c| format!("{} {}", c["cmd"].as_str().unwrap(), c["value"])).collect();
    assert_eq!(sent, ["fps 144", "fps 120", "load null"], "the rate is set before the first load (no start at 30)");
    assert_eq!(player.view().fps, 144, "the preference is kept for a later helper");
    assert_eq!(lock(&shared.inner).helper.as_ref().unwrap().view.fps, 120);
    // The page asks for 60, then 144 again: 144 is not tried again on this helper.
    assert!(player.control(Control::Fps { value: 60 }).unwrap().notice.is_none());
    let again = player.control(Control::Fps { value: 144 }).unwrap();
    assert!(again.notice.unwrap().contains("plays at 120"));
    let last: Vec<String> = lock(&commands).iter().skip(3).map(|c| format!("{} {}", c["cmd"].as_str().unwrap(), c["value"])).collect();
    assert_eq!(last, ["fps 60", "fps 120"]);
    // A rate the helper takes needs no sentence and no second try.
    let (spawner, _, commands) = fakes(Script::default());
    let player = EffectPlayer::with_spawner(spawner);
    lock(&player.shared.inner).view.fps = 144;
    player.shared.ensure(Some(&launch())).unwrap();
    assert!(player.shared.sync_view().is_none());
    assert_eq!(lock(&commands).len(), 1);
    // Thirty, the helper's own default, needs no command at all.
    let (spawner, _, commands) = fakes(Script::default());
    let player = EffectPlayer::with_spawner(spawner);
    player.shared.ensure(Some(&launch())).unwrap();
    assert!(player.shared.sync_view().is_none());
    assert!(lock(&commands).is_empty());
}

#[test]
fn a_helper_past_the_memory_limit_is_started_again_between_effects() {
    let (spawner, started, _) = fakes(Script { private_mb: Some(MEMORY_LIMIT_MB + 1.), ..Default::default() });
    let player = EffectPlayer::with_spawner(spawner);
    let shared = player.shared.clone();
    shared.ensure(Some(&launch())).unwrap();
    shared.send_effect(A, &BTreeMap::new(), None, false).unwrap();
    let first = player.pid().unwrap();
    assert!(shared.restart_if_large(), "too large: stopped before the next effect");
    assert_eq!(player.pid(), None);
    assert_eq!(shared.ensure(Some(&launch())).unwrap(), None, "the next effect starts a fresh helper");
    assert_ne!(player.pid().unwrap(), first);
    assert_eq!(started.load(Ordering::SeqCst), 2);
    let (spawner, _, _) = fakes(Script::default());
    let small = EffectPlayer::with_spawner(spawner);
    small.shared.ensure(Some(&launch())).unwrap();
    assert!(!small.shared.restart_if_large(), "a small helper keeps running");
    assert!(small.pid().is_some());
}

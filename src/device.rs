//! Web Audio samples and device sensors, ported from sound.js and app.js:
//! 2D shake to roll (devicemotion) and 3D table tilt (deviceorientation).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use js_sys::{Array, Function, Promise, Reflect};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    AddEventListenerOptions, AudioBuffer, AudioContext, DeviceMotionEvent, DeviceOrientationEvent,
    Document, Element, Event, Response,
};

use crate::model::State;

// Web Audio instead of <audio>: iOS starts <audio> noticeably late. Buffers are decoded up front
// so a sound plays the moment it is triggered; a sound that failed to load stays silent.
// click (UI) is the die-on-die contact sample at half volume.
const FILES: [(&str, &str); 5] = [
    ("throw", "throw.mp3"),
    ("shuffle", "shuffle.mp3"),
    ("click", "contact-die.wav"),
    ("table", "contact-table.wav"),
    ("die", "contact-die.wav"),
];
// Calibration (m/s², including gravity ≈ 9.8): START = jolt to begin, KEEP = still shaking, STILL_MS = rest before the throw.
const SHAKE_START: f64 = 22.0;
const SHAKE_KEEP: f64 = 15.0;
const SHAKE_STILL_MS: i32 = 350;
const TILT_DEADBAND: f64 = 0.01;
const FLAT: [f64; 3] = [0.0, -1.0, 0.0];

#[derive(Clone, Copy, PartialEq)]
enum Permission {
    Pending,
    Granted,
    Denied,
}

#[derive(Default)]
struct Device {
    // Last configured state.
    view: String,
    tilt: bool,
    shake: bool,
    sound: bool,
    blocked: bool,
    rollable: Vec<u32>,
    // Platform callbacks and the retained listener closures (created once by init).
    on_shake: Option<Rc<dyn Fn()>>,
    on_tilt: Option<Rc<dyn Fn(f32, f32)>>,
    motion: Option<Closure<dyn FnMut(Event)>>,
    orientation: Option<Closure<dyn FnMut(Event)>>,
    shake_done: Option<Closure<dyn FnMut()>>,
    motion_on: bool,
    orientation_on: bool,
    shake_timer: Option<i32>,
    last_mag: f64,
    gravity: [f64; 3],
    // iOS prompts, cached per sensor so the toggle and its bubbling click do not ask twice: [shake, tilt].
    permissions: [Option<Permission>; 2],
    audio: Option<AudioContext>,
    buffers: HashMap<&'static str, AudioBuffer>,
    voices: u32,
    window_start: f64,
}

thread_local! {
    static DEVICE: RefCell<Device> = RefCell::new(Device { gravity: FLAT, ..Default::default() });
}

/// Never call platform callbacks inside: they may re-enter configure.
fn with<R>(f: impl FnOnce(&mut Device) -> R) -> R {
    DEVICE.with(|d| f(&mut d.borrow_mut()))
}

fn shake_active(d: &Device) -> bool {
    d.view == "2d" && d.shake
}

fn tilt_active(d: &Device) -> bool {
    d.view == "3d" && d.tilt
}

pub fn init(on_shake: Rc<dyn Fn()>, on_tilt: Rc<dyn Fn(f32, f32)>) {
    with(|d| {
        if d.motion.is_some() {
            return;
        }
        d.on_shake = Some(on_shake);
        d.on_tilt = Some(on_tilt);
        d.motion = Some(Closure::<dyn FnMut(Event)>::new(motion));
        d.orientation = Some(Closure::<dyn FnMut(Event)>::new(orientation));
        d.shake_done = Some(Closure::<dyn FnMut()>::new(shake_done));
        listen(d);
    });
}

/// Called on every state change and around rolls. blocked: rolling, dragging or a die selected.
pub fn configure(state: &State, blocked: bool) {
    // Dice of the next roll: the table dice, or all of them once every die is in a tray.
    let all = state.dice.iter().all(|die| die.tray.is_some());
    let rollable = state
        .dice
        .iter()
        .filter(|die| all || die.tray.is_none())
        .map(|die| die.id)
        .collect();
    let (cancel, reset) = with(|d| {
        d.view.clone_from(&state.view);
        d.tilt = state.tilt;
        d.shake = state.shake;
        d.sound = state.sound;
        d.blocked = blocked;
        d.rollable = rollable;
        if d.sound {
            load_sounds(d);
        }
        listen(d);
        (blocked || !shake_active(d), !tilt_active(d))
    });
    if cancel {
        cancel_shake();
    }
    if reset {
        reset_tilt();
    }
}

/// Only the sensor of the active view, only when opted in, only from a user gesture (iOS).
/// Reads the state last passed to configure.
pub fn request_permission() {
    let Some(tilt) = with(|d| {
        let tilt = d.view == "3d";
        (if tilt { d.tilt } else { d.shake }).then_some(tilt)
    }) else {
        return;
    };
    let Some(window) = web_sys::window() else {
        return;
    };
    let name = if tilt {
        "DeviceOrientationEvent"
    } else {
        "DeviceMotionEvent"
    };
    let sensor = Reflect::get(&window, &name.into()).unwrap_or(JsValue::UNDEFINED);
    let Some(request) = Reflect::get(&sensor, &"requestPermission".into())
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
    else {
        return;
    };
    let key = tilt as usize;
    let cached = with(|d| {
        let cached = d.permissions[key];
        if cached.is_none() {
            d.permissions[key] = Some(Permission::Pending);
        }
        cached
    });
    match cached {
        Some(Permission::Denied) => spawn_local(async move { deny(tilt) }),
        // A pending prompt applies its own answer to the then current state.
        Some(_) => {}
        None => {
            // Synchronous call: iOS only prompts inside the gesture.
            let result = request.call0(&sensor);
            spawn_local(async move {
                let granted = match result {
                    Ok(result) => JsFuture::from(Promise::resolve(&result))
                        .await
                        .is_ok_and(|r| r.as_string().as_deref() == Some("granted")),
                    Err(_) => false,
                };
                with(|d| {
                    d.permissions[key] = Some(if granted {
                        Permission::Granted
                    } else {
                        Permission::Denied
                    })
                });
                if !granted {
                    deny(tilt);
                }
            });
        }
    }
}

/// A refused sensor switches its option off for good (persisted through the platform).
fn deny(tilt: bool) {
    if !with(|d| if tilt { d.tilt } else { d.shake }) {
        return;
    }
    if tilt {
        reset_tilt();
    } else {
        cancel_shake();
    }
    crate::platform::change(move |s| {
        if tilt {
            s.tilt = false;
        } else {
            s.shake = false;
        }
    });
}

// Sensors

/// Attaches only the listener of the active, opted-in sensor.
fn listen(d: &mut Device) {
    let (shake, tilt) = (shake_active(d), tilt_active(d));
    toggle(&mut d.motion_on, shake, "devicemotion", d.motion.as_ref());
    toggle(
        &mut d.orientation_on,
        tilt,
        "deviceorientation",
        d.orientation.as_ref(),
    );
}

fn toggle(on: &mut bool, want: bool, kind: &str, closure: Option<&Closure<dyn FnMut(Event)>>) {
    let (Some(window), Some(closure)) = (web_sys::window(), closure) else {
        return;
    };
    if *on == want {
        return;
    }
    let f: &Function = closure.as_ref().unchecked_ref();
    let done = if want {
        let options = AddEventListenerOptions::new();
        options.set_passive(true);
        window.add_event_listener_with_callback_and_add_event_listener_options(kind, f, &options)
    } else {
        window.remove_event_listener_with_callback(kind, f)
    };
    if done.is_ok() {
        *on = want;
    }
}

fn document() -> Option<Document> {
    web_sys::window()?.document()
}

fn paused() -> bool {
    document().map_or(true, |d| {
        d.hidden() || d.query_selector("dialog[open]").ok().flatten().is_some()
    })
}

fn each(selector: &str, mut f: impl FnMut(Element)) {
    let Some(list) = document().and_then(|d| d.query_selector_all(selector).ok()) else {
        return;
    };
    for i in 0..list.length() {
        if let Some(el) = list.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
            f(el);
        }
    }
}

fn clear_rattle() {
    each(".rattle", |el| {
        let _ = el.class_list().remove_1("rattle");
    });
}

fn reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| {
            w.match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
        })
        .is_some_and(|m| m.matches())
}

/// navigator.vibrate where it exists (not on iOS).
pub fn vibrate(pattern: &[u32]) {
    let Some(navigator) = web_sys::window().map(|w| w.navigator()) else {
        return;
    };
    if let Ok(f) =
        Reflect::get(&navigator, &"vibrate".into()).and_then(|f| f.dyn_into::<Function>())
    {
        let pattern: Array = pattern.iter().map(|&v| JsValue::from(v)).collect();
        let _ = f.call1(&navigator, &pattern);
    }
}

// 2D shake to roll. In 3D the orientation sensor only tilts the physical table.
fn motion(event: Event) {
    let Some(a) = event
        .dyn_ref::<DeviceMotionEvent>()
        .and_then(|e| e.acceleration_including_gravity())
    else {
        return;
    };
    if paused() {
        return;
    }
    let (x, y, z) = (
        a.x().unwrap_or(0.0),
        a.y().unwrap_or(0.0),
        a.z().unwrap_or(0.0),
    );
    let m = (x * x + y * y + z * z).sqrt();
    let start = with(|d| {
        if !shake_active(d) || d.blocked || d.rollable.is_empty() {
            return None;
        }
        let jolt = m > SHAKE_START && m - d.last_mag > 15.0;
        d.last_mag = m;
        if !jolt && !(d.shake_timer.is_some() && m > SHAKE_KEEP) {
            return None;
        }
        let window = web_sys::window()?;
        let start = d.shake_timer.is_none();
        if let Some(id) = d.shake_timer.take() {
            window.clear_timeout_with_handle(id);
        }
        let done: &Function = d.shake_done.as_ref()?.as_ref().unchecked_ref();
        d.shake_timer = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(done, SHAKE_STILL_MS)
            .ok();
        start.then(|| d.rollable.clone())
    });
    let Some(rollable) = start else { return };
    each("#dice .die, .tray .die", |el| {
        if el
            .get_attribute("data-id")
            .and_then(|v| v.parse::<u32>().ok())
            .is_some_and(|id| rollable.contains(&id))
        {
            let _ = el.class_list().add_1("rattle");
        }
    });
    // The rattle starts with the shaking, not with the throw; with reduced motion nothing wobbles.
    if !reduced_motion() {
        play("shuffle");
    }
    vibrate(&[15, 50, 15, 50, 15]);
}

fn shake_done() {
    let on_shake = with(|d| {
        d.shake_timer = None;
        d.on_shake.clone()
    });
    clear_rattle();
    if let Some(f) = on_shake {
        f();
    }
}

fn cancel_shake() {
    let timer = with(|d| {
        d.last_mag = 0.0;
        d.shake_timer.take()
    });
    if let Some(id) = timer {
        if let Some(window) = web_sys::window() {
            window.clear_timeout_with_handle(id);
        }
        clear_rattle();
    }
}

fn orientation(event: Event) {
    let Some(e) = event.dyn_ref::<DeviceOrientationEvent>() else {
        return;
    };
    let (Some(beta), Some(gamma)) = (e.beta(), e.gamma()) else {
        return;
    };
    if paused() {
        return;
    }
    let Some(g) = gravity(beta, gamma, screen_angle()) else {
        return;
    };
    let on_tilt = with(|d| {
        if !tilt_active(d) || d.blocked {
            return None;
        }
        let [x, y, z] = d.gravity;
        if ((g[0] - x).powi(2) + (g[1] - y).powi(2) + (g[2] - z).powi(2)).sqrt() < TILT_DEADBAND {
            return None;
        }
        d.gravity = g;
        d.on_tilt.clone()
    });
    if let Some(f) = on_tilt {
        f(g[0] as f32, g[2] as f32);
    }
}

fn reset_tilt() {
    let on_tilt = with(|d| {
        if d.gravity == FLAT {
            return None;
        }
        d.gravity = FLAT;
        d.on_tilt.clone()
    });
    if let Some(f) = on_tilt {
        f(0.0, 0.0);
    }
}

/// screen.orientation.angle ?? window.orientation ?? 0 (older iOS only has window.orientation).
fn screen_angle() -> f64 {
    let Some(window) = web_sys::window() else {
        return 0.0;
    };
    Reflect::get(&window, &"screen".into())
        .and_then(|s| Reflect::get(&s, &"orientation".into()))
        .and_then(|o| Reflect::get(&o, &"angle".into()))
        .ok()
        .and_then(|a| a.as_f64())
        .or_else(|| {
            Reflect::get(&window, &"orientation".into())
                .ok()
                .and_then(|a| a.as_f64())
        })
        .unwrap_or(0.0)
}

/// Unit gravity direction (X right, Y up, Z screen down) for a device orientation in degrees.
fn gravity(beta: f64, gamma: f64, screen_angle: f64) -> Option<[f64; 3]> {
    if !(beta.is_finite() && gamma.is_finite() && screen_angle.is_finite()) {
        return None;
    }
    let (b, g, a) = (
        beta.to_radians(),
        gamma.to_radians(),
        screen_angle.to_radians(),
    );
    let min_normal = 75f64.to_radians().cos();
    let (mut x, mut z) = (g.sin() * b.cos(), b.sin());
    // ponytail: cap tilt at 75°; off-table falls would need a catch/return model.
    let mut normal = b.cos() * g.cos();
    if normal < min_normal {
        let horizontal = x.hypot(z);
        if horizontal < 1e-9 {
            x = 0.0;
            z = 0.0;
            normal = 1.0;
        } else {
            let scale = (1.0 - min_normal * min_normal).sqrt() / horizontal;
            x *= scale;
            z *= scale;
            normal = min_normal;
        }
    }
    let length = (x * x + normal * normal + z * z).sqrt();
    x /= length;
    z /= length;
    Some([
        x * a.cos() + z * a.sin(),
        -normal / length,
        z * a.cos() - x * a.sin(),
    ])
}

// Audio

fn load_sounds(d: &mut Device) {
    if d.audio.is_some() {
        return;
    }
    let Ok(context) = AudioContext::new() else {
        return;
    };
    for (name, file) in FILES {
        let context = context.clone();
        spawn_local(async move {
            if let Ok(buffer) = decode(&context, file).await {
                with(|d| {
                    d.buffers.insert(name, buffer);
                });
            }
        });
    }
    d.audio = Some(context);
}

async fn decode(context: &AudioContext, file: &str) -> Result<AudioBuffer, JsValue> {
    let window = web_sys::window().ok_or(JsValue::NULL)?;
    let response: Response = JsFuture::from(window.fetch_with_str(&format!("sounds/{file}")))
        .await?
        .dyn_into()?;
    let bytes: js_sys::ArrayBuffer = JsFuture::from(response.array_buffer()?).await?.dyn_into()?;
    JsFuture::from(context.decode_audio_data(&bytes)?)
        .await?
        .dyn_into()
}

/// iOS only releases audio after a user gesture.
pub fn resume() {
    with(|d| {
        if let Some(context) = &d.audio {
            let _ = context.resume();
        }
    });
}

fn play_buffer(d: &Device, name: &str, gain: f32, rate: f32) {
    let (Some(context), Some(buffer)) = (&d.audio, d.buffers.get(name)) else {
        return;
    };
    let _ = context.resume();
    let (Ok(source), Ok(node)) = (context.create_buffer_source(), context.create_gain()) else {
        return;
    };
    source.set_buffer(Some(buffer));
    source.playback_rate().set_value(rate);
    node.gain().set_value(gain);
    let _ = source
        .connect_with_audio_node(&node)
        .and_then(|_| node.connect_with_audio_node(&context.destination()))
        .and_then(|_| source.start());
}

/// UI and roll samples: throw, shuffle, click. Silent while sound is off.
pub fn play(name: &str) {
    with(|d| {
        if d.sound {
            play_buffer(d, name, if name == "click" { 0.5 } else { 1.0 }, 1.0);
        }
    });
}

/// 3D contacts from physics: die-on-die or table/wall, speed = closing velocity in world units/s.
/// Loudness rises with speed³: a die dropped from the hand (~8.5) is quiet, only fast and cup throws (16+)
/// reach full volume. A slight pitch wobble keeps repeats from sounding identical.
pub fn impact(die: bool, speed: f32) {
    let now = web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now());
    with(|d| {
        if !d.sound {
            return;
        }
        if now - d.window_start > 25.0 {
            d.window_start = now;
            d.voices = 0;
        }
        if d.voices >= 8 {
            return; // pileups stay a thud instead of a crackle
        }
        d.voices += 1;
        let k = (speed / 16.0).min(1.0).powi(3);
        let gain = if die { (3.4 * k).min(1.2) } else { 0.95 * k }; // die sample is ~13 dB quieter
        let rate = 1.0 + (js_sys::Math::random() as f32 - 0.5) * 0.08;
        play_buffer(d, if die { "die" } else { "table" }, gain, rate);
    });
}

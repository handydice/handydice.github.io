use crate::{
    device,
    gpu::{self, Bounds, Preview, Rect, Renderer},
    model::{self, State},
    physics::{PhysicsWorld, Pose},
};
use dioxus::prelude::*;
use js_sys::{Function, Reflect};
use std::{cell::RefCell, future::Future, rc::Rc};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Element, Event, HtmlCanvasElement, HtmlDialogElement, HtmlElement, KeyboardEvent, PointerEvent,
};

thread_local! { static APP: RefCell<Option<Runtime>> = const { RefCell::new(None) }; }
struct Listener {
    target: web_sys::EventTarget,
    name: &'static str,
    callback: Closure<dyn FnMut(Event)>,
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback_and_bool(
            self.name,
            self.callback.as_ref().unchecked_ref(),
            matches!(
                self.name,
                "scroll" | "webglcontextlost" | "webglcontextrestored"
            ),
        );
    }
}
struct Drag {
    id: u32,
    pointer: i32,
    button: i16,
    start_x: f64,
    start_y: f64,
    x: f64,
    y: f64,
    started: f64,
    dragged_at: f64,
    moved: bool,
    rotate: bool,
    second: Option<(i32, f64, f64)>,
    keyboard: bool,
    target: Option<String>,
    original_tray: Option<String>,
}
struct Runtime {
    state: Signal<State>,
    rolling: Signal<bool>,
    selected: Signal<Option<u32>>,
    error: Signal<String>,
    backend: Signal<String>,
    world: PhysicsWorld,
    renderer: Option<Renderer>,
    dirty: bool,
    assets_dirty: bool,
    pumping: bool,
    last: f64,
    table: Rect,
    bounds: Bounds,
    drag: Option<Drag>,
    listeners: Vec<Listener>,
    install_event: Option<Event>,
    generation: u64,
    initial: bool,
    poses: Vec<Pose>,
    last_map: Vec<(u32, Vec<u32>)>,
    last_flick: Option<(u32, bool)>,
}
fn window() -> web_sys::Window {
    web_sys::window().expect("browser window")
}
fn document() -> web_sys::Document {
    window().document().expect("browser document")
}
fn element(id: &str) -> Option<Element> {
    document().get_element_by_id(id)
}
fn now() -> f64 {
    window().performance().map(|p| p.now()).unwrap_or(0.)
}
fn app_height() -> f64 {
    let w = window();
    let mut h = w.visual_viewport().map(|v| v.height()).unwrap_or_else(|| {
        w.inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(844.)
    });
    if Reflect::get(&w.navigator(), &"standalone".into())
        .ok()
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        if let Ok(screen) = w.screen() {
            let sh = screen.height().unwrap_or(h as i32) as f64;
            let sw = screen.width().unwrap_or(390) as f64;
            let iw = w
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(390.);
            let ih = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(h);
            let actual = if ih > iw { sh.max(sw) } else { sh.min(sw) };
            if actual - h > 15. {
                h = actual;
            }
        }
    }
    h.round()
}
pub fn stored(key: &str) -> Option<String> {
    window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item(key).ok().flatten())
}
pub fn store(key: &str, value: &str) {
    if let Some(s) = window().local_storage().ok().flatten() {
        if let Err(e) = s.set_item(key, value) {
            web_sys::console::warn_2(&"Local storage unavailable".into(), &e);
        }
    }
}
fn save(state: &State) {
    if let Ok(json) = serde_json::to_string(state) {
        store("handydice-state", &json);
    }
}
fn listen(
    target: &web_sys::EventTarget,
    name: &'static str,
    mut f: impl FnMut(Event) + 'static,
) -> Listener {
    let callback = Closure::wrap(Box::new(move |e| f(e)) as Box<dyn FnMut(Event)>);
    let _ = target.add_event_listener_with_callback_and_bool(
        name,
        callback.as_ref().unchecked_ref(),
        matches!(name, "scroll" | "webglcontextlost" | "webglcontextrestored"),
    );
    Listener {
        target: target.clone(),
        name,
        callback,
    }
}
pub fn init(
    state: Signal<State>,
    rolling: Signal<bool>,
    selected: Signal<Option<u32>>,
    error: Signal<String>,
    backend: Signal<String>,
) {
    APP.with_borrow_mut(|slot| {
        *slot = Some(Runtime {
            state,
            rolling,
            selected,
            error,
            backend,
            world: PhysicsWorld::new(),
            renderer: None,
            dirty: true,
            assets_dirty: true,
            pumping: false,
            last: 0.,
            table: Rect::default(),
            bounds: Bounds::default(),
            drag: None,
            listeners: Vec::new(),
            install_event: None,
            generation: 0,
            initial: !State::load_restored(stored("handydice-state").as_deref()).1,
            poses: Vec::with_capacity(12),
            last_map: Vec::with_capacity(12),
            last_flick: None,
        })
    });
    device::init(
        Rc::new(|| roll_from(true)),
        Rc::new(|x, z| {
            APP.with_borrow_mut(|app| {
                if let Some(a) = app {
                    a.world.set_tilt(x, z);
                    a.dirty = true;
                }
            });
            invalidate();
        }),
    );
    let w: web_sys::EventTarget = window().into();
    let d: web_sys::EventTarget = document().into();
    let listeners = vec![
        listen(&w, "resize", |_| {
            cancel();
            relayout();
        }),
        listen(&w, "orientationchange", |_| {
            spawn_local(async {
                gloo_timers::future::TimeoutFuture::new(100).await;
                relayout();
                gloo_timers::future::TimeoutFuture::new(200).await;
                relayout();
            })
        }),
        listen(&w, "pointermove", |e| {
            if let Ok(e) = e.dyn_into::<PointerEvent>() {
                pointer_move(e)
            }
        }),
        listen(&w, "pointerup", |e| {
            if let Ok(e) = e.dyn_into::<PointerEvent>() {
                pointer_end(e, false)
            }
        }),
        listen(&w, "pointercancel", |e| {
            if let Ok(e) = e.dyn_into::<PointerEvent>() {
                pointer_end(e, true)
            }
        }),
        listen(&w, "pointerdown", |e| {
            if let Ok(e) = e.dyn_into::<PointerEvent>() {
                APP.with_borrow_mut(|app| {
                    if let Some(a) = app {
                        if let Some(d) = &mut a.drag {
                            if d.pointer != e.pointer_id() {
                                d.second = Some((
                                    e.pointer_id(),
                                    e.client_x() as f64,
                                    e.client_y() as f64,
                                ));
                            }
                        }
                    }
                });
            }
        }),
        listen(&w, "blur", |_| cancel()),
        listen(&w, "keydown", |e| {
            if let Ok(e) = e.dyn_into::<KeyboardEvent>() {
                if e.key() == "Escape" {
                    cancel();
                }
            }
        }),
        listen(&d, "visibilitychange", |_| {
            cancel();
            if !document().hidden() {
                spawn_local(async {
                    gloo_timers::future::TimeoutFuture::new(50).await;
                    relayout();
                });
            }
        }),
        listen(&w, "beforeinstallprompt", |e| {
            e.prevent_default();
            APP.with_borrow_mut(|app| {
                if let Some(a) = app {
                    a.install_event = Some(e)
                }
            });
        }),
        listen(&w, "appinstalled", |_| {
            APP.with_borrow_mut(|app| {
                if let Some(a) = app {
                    a.install_event = None
                }
            })
        }),
        listen(&d, "click", |_| {
            device::resume();
            device::request_permission();
        }),
        listen(&d, "scroll", |_| invalidate()),
        listen(&w, "contextmenu", |e| {
            if APP.with_borrow(|app| app.as_ref().is_some_and(|a| a.drag.is_some())) {
                e.prevent_default();
            }
        }),
        listen(&d, "webglcontextlost", |e| {
            if !e
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|el| el.id() == "dice-canvas")
            {
                return;
            }
            e.prevent_default();
            clear_hints();
            APP.with_borrow_mut(|app| {
                if let Some(a) = app {
                    a.generation += 1;
                    a.renderer = None;
                    a.world = PhysicsWorld::new();
                    a.drag = None;
                    a.rolling.set(false);
                    a.error.set("WebGL context lost".into());
                    a.assets_dirty = true;
                }
            });
            if let Some(b) = document().body() {
                let _ = b.class_list().remove_2("rolling", "sum-pending");
            }
        }),
        listen(&d, "webglcontextrestored", |e| {
            if !e
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|el| el.id() == "dice-canvas")
            {
                return;
            }
            if let Some(canvas) = element("dice-canvas") {
                canvas.remove();
            }
            APP.with_borrow_mut(|app| {
                if let Some(a) = app {
                    a.error.set(String::new());
                    a.renderer = None;
                    a.assets_dirty = true;
                    a.dirty = true;
                }
            });
            relayout();
        }),
    ];
    APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            a.listeners = listeners;
        }
    });
    if let Some(v) = window().visual_viewport() {
        let t: web_sys::EventTarget = v.into();
        let l = listen(&t, "resize", |_| relayout());
        APP.with_borrow_mut(|app| {
            if let Some(a) = app {
                a.listeners.push(l)
            }
        });
    }
    let theme = stored("handydice-theme").unwrap_or_else(|| "auto".into());
    set_theme(&theme);
    for delay in [50, 150, 300, 500, 800, 1200] {
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(delay).await;
            relayout();
        });
    }
    if let Ok(Some(query)) = window().match_media("(prefers-color-scheme: light)") {
        let target: web_sys::EventTarget = query.into();
        let listener = listen(&target, "change", |_| {
            set_theme(&stored("handydice-theme").unwrap_or_else(|| "auto".into()))
        });
        APP.with_borrow_mut(|app| {
            if let Some(a) = app {
                a.listeners.push(listener)
            }
        });
    }
}
pub fn change(f: impl FnOnce(&mut State)) {
    let config = APP.with_borrow_mut(|app| {
        let a = app.as_mut()?;
        let state = {
            let mut state = a.state.write();
            f(&mut state);
            state.assign_slots();
            save(&state);
            state.clone()
        };
        a.dirty = true;
        a.assets_dirty = true;
        Some((
            state,
            *a.rolling.peek() || a.drag.is_some() || a.selected.peek().is_some(),
        ))
    });
    if let Some((state, blocked)) = config {
        device::configure(&state, blocked);
    }
    invalidate();
}
pub fn sync(state: State) {
    let root = document().document_element().unwrap();
    let _ = root.set_attribute("data-view", &state.view);
    let _ = root.set_attribute("data-surface", &state.surface);
    let _ = root.class_list().add_1("ready");
    let blocked = APP.with_borrow(|app| {
        app.as_ref()
            .is_some_and(|a| *a.rolling.peek() || a.drag.is_some() || a.selected.peek().is_some())
    });
    device::configure(&state, blocked);
    APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            a.dirty = true;
            a.assets_dirty = true;
            if state.view == "2d" {
                a.renderer = None;
                a.world = PhysicsWorld::new();
                a.last_map.clear();
                a.last_flick = None;
                a.error.set(String::new());
                a.generation += 1;
                if let Some(canvas) = element("dice-canvas") {
                    canvas.remove();
                }
            }
        }
    });
    relayout();
    if let Some(bag) = element("bag") {
        if bag.get_attribute("data-close-bound").is_none() {
            let _ = bag.set_attribute("data-close-bound", "true");
            let target: web_sys::EventTarget = bag.into();
            let listener = listen(&target, "close", |_| {
                if !element("bag").is_some_and(|e| e.has_attribute("open")) {
                    if let Some(el) =
                        element("bag-open").and_then(|e| e.dyn_into::<HtmlElement>().ok())
                    {
                        let _ = el.focus();
                    }
                }
            });
            APP.with_borrow_mut(|app| {
                if let Some(a) = app {
                    a.listeners.push(listener)
                }
            });
        }
    }
    let first = APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            let f = a.initial && state.view == "2d";
            if f {
                a.initial = false;
            }
            f
        } else {
            false
        }
    });
    if first {
        spawn_local(async {
            next_frame().await;
            roll();
        });
    }
}
fn rect(el: &Element) -> Rect {
    let r = el.get_bounding_client_rect();
    Rect {
        left: r.x() as f32,
        top: r.y() as f32,
        width: r.width() as f32,
        height: r.height() as f32,
    }
}
fn inside(r: Rect, x: f64, y: f64) -> bool {
    x >= r.left as f64
        && x <= (r.left + r.width) as f64
        && y >= r.top as f64
        && y <= (r.top + r.height) as f64
}
fn target_at(x: f64, y: f64) -> Option<String> {
    for (id, tray) in [("toptrays", "t1"), ("bottomtrays", "b1"), ("dice", "")] {
        let el = if tray.is_empty() {
            element(id)
        } else {
            document()
                .query_selector(&format!(".tray[data-tray='{tray}']"))
                .ok()
                .flatten()
        };
        if el.is_some_and(|el| inside(rect(&el), x, y)) {
            return Some(tray.into());
        }
    }
    None
}
fn before_at(tray: &str, id: u32, x: f64, y: f64) -> Option<u32> {
    let nodes = document()
        .query_selector_all(&format!(".tray[data-tray='{tray}'] .die[data-id]"))
        .ok()?;
    for n in 0..nodes.length() {
        let el = nodes.item(n)?.dyn_into::<Element>().ok()?;
        let other = el.get_attribute("data-id")?.parse().ok()?;
        let r = rect(&el);
        if other != id
            && (y < (r.top as f64)
                || (y <= (r.top + r.height) as f64 && x < (r.left + r.width / 2.) as f64))
        {
            return Some(other);
        }
    }
    None
}
fn relayout() {
    if let Some(root) = document()
        .document_element()
        .and_then(|e| e.dyn_into::<HtmlElement>().ok())
    {
        let _ = root
            .style()
            .set_property("--app-height", &format!("{}px", app_height()));
    }
    APP.with_borrow_mut(|app| {
        let Some(a) = app else { return };
        let Some(table) = element("dice") else { return };
        a.table = rect(&table);
        let w = a.table.width.max(1.);
        let h = a.table.height.max(1.);
        let ratio = (w / h).max(0.4);
        let half = 2.7 * (w.min(h) / 512.).max(1.);
        a.bounds = Bounds {
            x: half * ratio.max(1.),
            z: half * (1. / ratio).max(1.),
            min_z: -half * (1. / ratio).max(1.),
            max_z: half * (1. / ratio).max(1.),
        };
        let state = a.state.peek().clone();
        for (tray, top) in [("t1", true), ("b1", false)] {
            if state.dice.iter().all(|d| d.tray.as_deref() != Some(tray)) {
                if let Ok(Some(el)) =
                    document().query_selector(&format!(".tray[data-tray='{tray}']"))
                {
                    let r = rect(&el);
                    let p = gpu::unproject(
                        a.table.left,
                        if top { r.top } else { r.top + r.height },
                        a.table,
                        a.bounds,
                        0.,
                    );
                    if top {
                        a.bounds.min_z = a.bounds.min_z.min(p.z)
                    } else {
                        a.bounds.max_z = a.bounds.max_z.max(p.z)
                    }
                }
            }
        }
        if state.view == "2d" {
            fit_flat(&state, a.table);
        } else {
            a.world.sync(&state.dice, a.bounds.x * 2., a.bounds.z * 2.);
            a.world.set_play_bounds(a.bounds.min_z, a.bounds.max_z);
        }
        a.dirty = true;
    });
    invalidate();
}
fn fit_flat(state: &State, r: Rect) {
    let count = state
        .dice
        .iter()
        .filter_map(|d| d.slot)
        .max()
        .map_or(0, |x| x + 1);
    let gap = if r.width > r.height { 12. } else { 16. };
    let max_size = if r.width > r.height {
        128.
    } else {
        (r.width - 2. * gap) / 3.
    };
    let mut columns = 1;
    let mut size = 0f32;
    for c in 1..=count {
        let rows = count.div_ceil(c);
        let candidate = max_size
            .min((r.width - 8. - (c - 1) as f32 * gap) / c as f32)
            .min((r.height - 20. - (rows - 1) as f32 * gap) / rows as f32);
        if candidate > size {
            columns = c;
            size = candidate;
        }
    }
    if let Some(el) = element("dice").and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
        let _ = el
            .style()
            .set_property("--dice-columns", &columns.to_string());
        let _ = el
            .style()
            .set_property("--dice-size", &format!("{}px", size.max(24.)));
    }
}
async fn next_frame() -> f64 {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().request_animation_frame(resolve.unchecked_ref());
    });
    JsFuture::from(promise)
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or_else(now)
}
fn invalidate() {
    let start = APP.with_borrow_mut(|app| {
        let Some(a) = app else { return false };
        a.dirty = true;
        if a.pumping {
            return false;
        }
        a.pumping = true;
        true
    });
    if start {
        spawn_local(pump());
    }
}
async fn pump() {
    let mut state = APP.with_borrow(|app| app.as_ref().unwrap().state.peek().clone());
    let mut preview_items = Vec::with_capacity(model::MAX_DICE);
    loop {
        let time = next_frame().await;
        if document().hidden() {
            APP.with_borrow_mut(|app| {
                if let Some(a) = app {
                    a.pumping = false;
                    a.last = 0.;
                }
            });
            return;
        }
        let snapshot = APP.with_borrow_mut(|app| {
            let a = app.as_mut()?;
            if a.state.peek().view != "3d" || !a.error.peek().is_empty() {
                a.pumping = false;
                a.last = 0.;
                return None;
            }
            let dt = if a.last == 0. {
                1. / 60.
            } else {
                ((time - a.last) / 1000.).clamp(0., 0.05) as f32
            };
            a.last = time;
            let moving = a.world.step(dt);
            a.world.write_poses(&mut a.poses);
            if a.state.peek().sound {
                for (die, speed) in a.world.impacts.drain(..) {
                    device::impact(die, speed);
                }
            } else {
                a.world.impacts.clear();
            }
            for (id, triggered) in a.world.randomized.drain(..) {
                if !triggered {
                    a.last_flick = Some((id, false));
                } else {
                    a.assets_dirty = true;
                    a.rolling.set(true);
                    if a.last_flick.is_some_and(|f| f.0 == id) {
                        a.last_flick = Some((id, true));
                    }
                }
            }
            let assets = if a.assets_dirty {
                let s = a.state.peek();
                a.last_map
                    .retain(|(id, _)| s.dice.iter().any(|d| d.id == *id));
                for d in &s.dice {
                    if !a.last_map.iter().any(|m| m.0 == d.id) {
                        a.last_map
                            .push((d.id, Vec::with_capacity(d.sides as usize)));
                    }
                    let map = &mut a.last_map.iter_mut().find(|m| m.0 == d.id).unwrap().1;
                    map.clear();
                    if let Some(values) = a.world.face_values(d.id) {
                        map.extend_from_slice(values);
                    } else {
                        map.extend(1..=d.sides);
                    }
                }
                a.assets_dirty = false;
                Some((s.clone(), std::mem::take(&mut a.last_map)))
            } else {
                None
            };
            a.dirty = false;
            Some((
                a.renderer.take(),
                std::mem::take(&mut a.poses),
                assets,
                moving,
                a.table,
                a.bounds,
                a.generation,
                *a.selected.peek(),
                a.last_flick,
            ))
        });
        let Some((
            renderer,
            poses,
            assets,
            moving,
            table,
            bounds,
            generation,
            selected,
            last_flick,
        )) = snapshot
        else {
            return;
        };
        let mut renderer = match renderer {
            Some(r) => r,
            None => {
                let canvas = if let Some(c) = element("dice-canvas") {
                    c.dyn_into::<HtmlCanvasElement>().unwrap()
                } else {
                    let c = document()
                        .create_element("canvas")
                        .unwrap()
                        .dyn_into::<HtmlCanvasElement>()
                        .unwrap();
                    c.set_id("dice-canvas");
                    let _ = c.set_attribute("aria-hidden", "true");
                    let _ = document().body().unwrap().prepend_with_node_1(&c);
                    c
                };
                match Renderer::new(canvas).await {
                    Ok(r) => r,
                    Err(e) => {
                        fail(e);
                        return;
                    }
                }
            }
        };
        let w = window()
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(390.) as f32;
        let h = app_height() as f32;
        let dpr = (window().device_pixel_ratio() as f32).min(if moving { 1.25 } else { 2. });
        renderer.set_view(
            Rect {
                left: 0.,
                top: 0.,
                width: w,
                height: h,
            },
            dpr,
        );
        if let Some((new_state, mappings)) = assets {
            state = new_state;
            if let Err(e) = renderer.sync(&state.dice, &mappings).await {
                fail(e);
                return;
            }
            let theme = stored("handydice-theme");
            let light = theme.as_deref() == Some("light")
                || (theme.as_deref() != Some("dark")
                    && window()
                        .match_media("(prefers-color-scheme: light)")
                        .ok()
                        .flatten()
                        .is_some_and(|q| q.matches()));
            if let Err(e) = renderer.set_surface(&state.surface, light).await {
                fail(e);
                return;
            }
            APP.with_borrow_mut(|app| {
                if let Some(a) = app {
                    a.last_map = mappings;
                }
            });
        }
        renderer.set_selected(selected.as_slice());
        previews(&state, &poses, &mut preview_items);
        if let Err(e) = renderer.render(table, bounds, &poses, &preview_items) {
            fail(e);
            return;
        }
        place_targets(&state, &poses, table, bounds);
        if let Some(el) = element("flick-debug") {
            let debug = window()
                .location()
                .search()
                .ok()
                .is_some_and(|q| q.split(['?', '&']).any(|p| p == "debug"));
            if let Some((id, triggered)) = last_flick.filter(|_| debug) {
                if let Some(d) = state.dice.iter().find(|d| d.id == id) {
                    el.set_text_content(Some(&format!(
                        "Last flick · D{} · Crypto: {}",
                        d.sides,
                        if triggered { "YES" } else { "NO" }
                    )));
                    let _ = el
                        .set_attribute("data-triggered", if triggered { "true" } else { "false" });
                    let _ = el.remove_attribute("hidden");
                }
            } else {
                let _ = el.set_attribute("hidden", "");
            }
        }
        let (again, first, configure) = APP.with_borrow_mut(|app| {
            let Some(a) = app else {
                return (false, false, None);
            };
            if a.generation != generation {
                return (true, false, None);
            }
            a.poses = poses;
            if a.backend.peek().as_str() != renderer.backend() {
                a.backend.set(renderer.backend().to_string());
            }
            a.renderer = Some(renderer);
            let mut configure = None;
            if !moving && a.drag.is_none() {
                let values = a.world.values();
                let changed = values.iter().any(|(id, value)| {
                    a.state
                        .peek()
                        .dice
                        .iter()
                        .any(|d| d.id == *id && d.value != *value)
                });
                if changed {
                    let mut s = a.state.write();
                    for (id, value) in values {
                        s.set_value(id, value);
                    }
                    save(&s);
                } else if *a.rolling.peek() {
                    save(&a.state.peek());
                }
                if *a.rolling.peek() {
                    a.rolling.set(false);
                    configure = Some(a.state.peek().clone());
                    if let Some(b) = document().body() {
                        let _ = b.class_list().remove_2("rolling", "sum-pending");
                    }
                    vibrate(&[25]);
                }
            }
            let first = a.initial;
            a.initial = false;
            let active = moving || a.dirty;
            if !active {
                a.pumping = false;
                a.last = 0.;
            }
            (active, first, configure)
        });
        if let Some(state) = configure {
            device::configure(&state, false);
        }
        if first {
            roll();
        }
        if !again {
            return;
        }
    }
}
fn fail(message: String) {
    APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            a.error.set(message);
            a.rolling.set(false);
            a.pumping = false;
            a.renderer = None;
            a.last = 0.;
        }
    });
    if let Some(b) = document().body() {
        let _ = b.class_list().remove_2("rolling", "sum-pending");
    }
}
fn previews(state: &State, poses: &[Pose], items: &mut Vec<Preview>) {
    items.clear();
    let held = APP.with_borrow(|app| {
        app.as_ref().and_then(|a| {
            let drag = a.drag.as_ref()?;
            if !drag.moved {
                return None;
            }
            let pose = a.world.grip_pose()?;
            Some((pose, drag.x as f32, drag.y as f32))
        })
    });
    for d in state.dice.iter().filter(|d| d.tray.is_some()) {
        if held.is_some_and(|(pose, _, _)| pose.id == d.id) {
            continue;
        }
        let Some(el) = element(&format!("die-{}", d.id)) else {
            continue;
        };
        let r = rect(&el);
        let clip = el.parent_element().map(|p| rect(&p));
        if clip.is_some_and(|c| {
            r.top + r.height < c.top
                || r.top > c.top + c.height
                || r.left + r.width < c.left
                || r.left > c.left + c.width
        }) {
            continue;
        }
        items.push(Preview {
            id: d.id,
            held: false,
            rect: r,
            clip,
            rotation: None,
        });
    }
    if let Some((pose, x, y)) = held {
        if !poses.iter().any(|p| p.id == pose.id) {
            items.push(Preview {
                id: pose.id,
                held: true,
                rect: Rect {
                    left: x - 42.,
                    top: y - 42.,
                    width: 84.,
                    height: 84.,
                },
                clip: None,
                rotation: Some(pose.rotation),
            });
        }
    }
}
fn place_targets(state: &State, poses: &[Pose], table: Rect, bounds: Bounds) {
    for pose in poses {
        let Some(d) = state
            .dice
            .iter()
            .find(|d| d.id == pose.id && d.tray.is_none())
        else {
            continue;
        };
        let radius = match d.sides {
            4 => 0.68,
            6 => 0.78,
            8 | 10 => 0.66,
            12 => 0.68,
            20 => 0.7,
            _ => 0.8,
        };
        let (x, y, size) = gpu::project(table, bounds, pose.position, radius);
        if let Some(el) =
            element(&format!("die-{}", pose.id)).and_then(|e| e.dyn_into::<HtmlElement>().ok())
        {
            let size = size.max(44.);
            let s = el.style();
            let _ = s.set_property("left", &format!("{}px", x - table.left - size / 2.));
            let _ = s.set_property("top", &format!("{}px", y - table.top - size / 2.));
            let _ = s.set_property("width", &format!("{size}px"));
            let _ = s.set_property("height", &format!("{size}px"));
            let _ = s.set_property(
                "z-index",
                &format!("{}", (pose.position.y * 10. + 10.) as i32),
            );
        }
    }
}
fn calm() -> bool {
    window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
}
fn dialog_open() -> bool {
    document()
        .query_selector("dialog[open]")
        .ok()
        .flatten()
        .is_some()
}
pub fn roll() {
    roll_from(false);
}
fn roll_from(shaken: bool) {
    if dialog_open() {
        return;
    }
    if APP.with_borrow(|app| {
        app.as_ref().is_some_and(|a| {
            *a.rolling.peek() || a.drag.is_some() || (shaken && a.selected.peek().is_some())
        })
    }) {
        return;
    }
    device::resume();
    cancel();
    let action = APP.with_borrow_mut(|app| {
        let a = app.as_mut()?;
        if a.state.peek().dice.is_empty() {
            return Some(false);
        }
        if a.state.peek().view == "3d" && (!a.error.peek().is_empty() || a.renderer.is_none()) {
            return None;
        }
        a.rolling.set(true);
        a.last_flick = None;
        Some(true)
    });
    match action {
        Some(false) => {
            show_dialog("bag");
            return;
        }
        Some(true) => {}
        None => return,
    }
    let state = APP.with_borrow(|app| app.as_ref().unwrap().state.peek().clone());
    device::configure(&state, true);
    if let Some(b) = document().body() {
        let _ = b.class_list().add_2("rolling", "sum-pending");
    }
    if state.view == "2d" {
        spawn_local(async move {
            roll_flat(shaken).await;
        });
        return;
    }
    APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            let ids = {
                let mut s = a.state.write();
                let ids = s.roll();
                save(&s);
                ids
            };
            let s = a.state.peek().clone();
            a.world.sync(&s.dice, a.bounds.x * 2., a.bounds.z * 2.);
            a.world.throw(&ids, calm());
            a.assets_dirty = true;
            a.dirty = true;
        }
    });
    invalidate();
}
async fn animate(el: Element, frames: JsValue, duration: f64, delay: f64, iterations: f64) {
    let opts = js_sys::Object::new();
    for (k, v) in [
        ("duration", duration),
        ("delay", delay),
        ("iterations", iterations),
    ] {
        let _ = Reflect::set(&opts, &k.into(), &v.into());
    }
    let _ = Reflect::set(&opts, &"easing".into(), &"ease-in-out".into());
    if let Ok(f) = Reflect::get(&el, &"animate".into()).and_then(|f| f.dyn_into::<Function>()) {
        if let Ok(animation) = f.call2(&el, &frames, &opts) {
            if let Ok(p) = Reflect::get(&animation, &"finished".into())
                .and_then(|p| p.dyn_into::<js_sys::Promise>())
            {
                let _ = JsFuture::from(p).await;
            }
        }
    }
}
fn frames(json: serde_json::Value) -> JsValue {
    js_sys::JSON::parse(&json.to_string()).unwrap_or(JsValue::NULL)
}
async fn together(tasks: Vec<impl Future<Output = ()> + 'static>) {
    let promises = js_sys::Array::new();
    for task in tasks {
        promises.push(&wasm_bindgen_futures::future_to_promise(async move {
            task.await;
            Ok(JsValue::UNDEFINED)
        }));
    }
    let _ = JsFuture::from(js_sys::Promise::all(&promises)).await;
}
async fn roll_flat(shaken: bool) {
    let reduced = calm();
    let ids = APP.with_borrow(|app| app.as_ref().unwrap().state.peek().rollable_ids());
    let body = document().body().unwrap();
    let _ = body.class_list().add_2("rolling", "sum-pending");
    if !reduced && !shaken {
        device::play("throw");
        let button = element("roll").map(|e| rect(&e)).unwrap_or_default();
        let mut tasks = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            if let Some(el) = element(&format!("die-{id}")) {
                let r = rect(&el);
                tasks.push(animate(el,frames(serde_json::json!([{"transform":"none","opacity":1},{"transform":format!("translate({}px,{}px) scale(.25) rotate(180deg)",button.left+button.width/2.-r.left-r.width/2.,button.top+button.height/2.-r.top-r.height/2.),"opacity":0}])),200.,i as f64*20.,1.));
            }
        }
        together(tasks).await;
        if let Some(el) = element("roll") {
            vibrate(&[15, 50, 15, 50, 15, 50, 15]);
            animate(el,frames(serde_json::json!([{"transform":"translateX(-7px) rotate(-2.5deg)"},{"transform":"translateX(7px) rotate(2.5deg)"}])),85.,0.,4.).await;
        }
    } else if !shaken {
        device::play("shuffle");
    }
    APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            let mut s = a.state.write();
            s.roll();
            save(&s);
        }
    });
    next_frame().await;
    let _ = body.class_list().remove_1("sum-pending");
    let button = element("roll").map(|e| rect(&e)).unwrap_or_default();
    let mut tasks = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        if let Some(el) = element(&format!("die-{id}")) {
            let r = rect(&el);
            let start = if shaken {
                "translateY(-2.5rem) scale(1.25) rotate(-90deg)".to_string()
            } else {
                format!(
                    "translate({}px,{}px) scale(.3) rotate(-90deg)",
                    button.left + button.width / 2. - r.left - r.width / 2.,
                    button.top + button.height / 2. - r.top - r.height / 2.
                )
            };
            tasks.push(animate(el,frames(if reduced{serde_json::json!([{"opacity":0},{"opacity":1}])}else{serde_json::json!([{"transform":start,"opacity":0},{"transform":"scale(1.1)","opacity":1,"offset":0.7},{"transform":"none","opacity":1}])}),if reduced{150.}else{420.},i as f64*70.,1.));
        }
    }
    together(tasks).await;
    let _ = body.class_list().remove_2("rolling", "sum-pending");
    let state = APP.with_borrow_mut(|app| {
        let a = app.as_mut()?;
        a.rolling.set(false);
        save(&a.state.peek());
        Some(a.state.peek().clone())
    });
    if let Some(state) = state {
        device::configure(&state, false);
    }
    vibrate(&[25]);
}
fn vibrate(pattern: &[u32]) {
    let nav = window().navigator();
    if let Ok(f) = Reflect::get(&nav, &"vibrate".into()).and_then(|f| f.dyn_into::<Function>()) {
        let arr = js_sys::Array::new();
        for p in pattern {
            arr.push(&(*p).into());
        }
        let _ = f.call1(&nav, &arr);
    }
}
pub fn return_all() {
    cancel();
    change(|s| s.return_all());
}
pub fn tap(id: u32) {
    device::resume();
    device::play("click");
    let action = APP.with_borrow_mut(|app| {
        let a = app.as_mut()?;
        if *a.rolling.peek() {
            return None;
        }
        let state = a.state.peek();
        let d = state.dice.iter().find(|d| d.id == id)?;
        if state.click_mode == "direct" {
            return Some((
                if d.tray.is_some() {
                    None
                } else {
                    Some(state.next_tray().to_string())
                },
                None,
            ));
        }
        if *a.selected.peek() == Some(id) {
            a.selected.set(None)
        } else if a.selected.peek().is_some() && d.tray.is_some() {
            return Some((d.tray.clone(), Some(id)));
        } else {
            a.selected.set(Some(id))
        }
        None
    });
    if let Some((tray, before)) = action {
        let selected = APP.with_borrow(|app| {
            let a = app.as_ref().unwrap();
            if a.state.peek().click_mode == "direct" {
                id
            } else {
                a.selected.peek().unwrap_or(id)
            }
        });
        change(|s| s.move_die(selected, tray.as_deref(), before));
        APP.with_borrow_mut(|app| {
            if let Some(a) = app {
                a.selected.set(None);
            }
        });
    }
    choosing();
    invalidate();
}
pub fn drop_selected(tray: Option<&str>, before: Option<u32>) {
    let id = APP.with_borrow_mut(|app| app.as_mut().and_then(|a| a.selected.take()));
    if let Some(id) = id {
        change(|s| s.move_die(id, tray, before));
    }
    choosing();
}
fn choosing() {
    let config = APP.with_borrow(|app| {
        app.as_ref().map(|a| {
            (
                a.state.peek().clone(),
                a.selected.peek().is_some(),
                *a.rolling.peek() || a.drag.is_some(),
            )
        })
    });
    if let Some((state, selected, blocked)) = config {
        if let Some(b) = document().body() {
            let _ = b.class_list().toggle_with_force("choosing", selected);
        }
        device::configure(&state, blocked || selected);
    }
}
pub fn cancel() {
    APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            if let Some(d) = a.drag.take() {
                a.world.end_grip(true);
                if let Some(el) = element(&format!("die-{}", d.id)) {
                    let _ = el.remove_attribute("style");
                }
            }
            a.selected.set(None);
            a.dirty = true;
        }
    });
    clear_hints();
    choosing();
    invalidate();
}
fn clear_hints() {
    if let Some(b) = document().body() {
        let _ = b.class_list().remove_1("dragging");
    }
    if let Some(m) = element("insertion-marker") {
        m.remove();
    }
    if let Ok(nodes) = document().query_selector_all(".over,.held") {
        for i in 0..nodes.length() {
            if let Some(el) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                let _ = el.class_list().remove_2("over", "held");
            }
        }
    }
}
fn hints(x: f64, y: f64, id: u32) {
    clear_hints();
    if let Some(b) = document().body() {
        let _ = b.class_list().add_1("dragging");
    }
    let Some(tray) = target_at(x, y) else { return };
    let el = if tray.is_empty() {
        element("dice")
    } else {
        document()
            .query_selector(&format!(".tray[data-tray='{tray}']"))
            .ok()
            .flatten()
    };
    if let Some(el) = el {
        let _ = el.class_list().add_1("over");
        if !tray.is_empty() {
            if let Some(container) = el.first_element_child() {
                let marker = document().create_element("div").unwrap();
                marker.set_id("insertion-marker");
                marker.set_class_name("ins");
                let before =
                    before_at(&tray, id, x, y).and_then(|id| element(&format!("die-{id}")));
                let _ = container.insert_before(&marker, before.as_ref().map(|e| e.as_ref()));
            }
        }
    }
}
pub fn pointer_start(
    mut id: u32,
    x: f64,
    y: f64,
    pointer_id: i32,
    button: i16,
    shift: bool,
) -> bool {
    device::resume();
    APP.with_borrow_mut(|app| {
        let Some(a) = app else { return false };
        if *a.rolling.peek() || a.drag.is_some() {
            return false;
        }
        let state = a.state.peek();
        if button > if state.view == "2d" { 0 } else { 2 } {
            return false;
        }
        let Some(d) = state.dice.iter().find(|d| d.id == id) else {
            return false;
        };
        let tray = d.tray.clone();
        if state.view == "3d" && tray.is_none() {
            if let Some(renderer) = &a.renderer {
                if let Some(picked) =
                    renderer.raycast(x as f32, y as f32, a.table, a.bounds, &a.poses)
                {
                    id = picked;
                } else {
                    return false;
                }
            }
        }
        a.drag = Some(Drag {
            id,
            pointer: pointer_id,
            button,
            start_x: x,
            start_y: y,
            x,
            y,
            started: now(),
            dragged_at: 0.,
            moved: false,
            rotate: shift || button == 2,
            second: None,
            keyboard: false,
            target: None,
            original_tray: tray,
        });
        if let Some(el) = element(&format!("die-{id}")) {
            let _ = el.class_list().add_1("held");
        }
        true
    })
}
fn pointer_move(e: PointerEvent) {
    let x = e.client_x() as f64;
    let y = e.client_y() as f64;
    let mut began = false;
    let hint = APP.with_borrow_mut(|app| {
        let a = app.as_mut()?;
        let d = a.drag.as_mut()?;
        let second = d.second.is_some_and(|p| p.0 == e.pointer_id());
        if e.pointer_id() != d.pointer && !second {
            return None;
        }
        let flat = a.state.peek().view == "2d";
        if !d.moved {
            let threshold = if now() - d.started < 150. { 30. } else { 12. };
            if (x - d.start_x).hypot(y - d.start_y) < threshold && !second {
                return None;
            }
            d.moved = true;
            d.dragged_at = now();
            began = true;
            a.selected.set(None);
            if !flat {
                let p = gpu::unproject(d.start_x as f32, d.start_y as f32, a.table, a.bounds, 1.6);
                a.world.begin_grip(d.id, p.x, p.z);
            }
        }
        if flat {
            if let Some(el) =
                element(&format!("die-{}", d.id)).and_then(|e| e.dyn_into::<HtmlElement>().ok())
            {
                let r = el.get_bounding_client_rect();
                let s = el.style();
                let _ = s.set_property("position", "fixed");
                let _ = s.set_property("left", &format!("{}px", d.start_x - r.width() / 2.));
                let _ = s.set_property("top", &format!("{}px", d.start_y - r.height() / 2.));
                let _ = s.set_property("width", &format!("{}px", r.width()));
                let _ = s.set_property("z-index", "30");
                let _ = s.set_property("pointer-events", "none");
                let _ = s.set_property(
                    "transform",
                    &format!(
                        "translate({}px,{}px) scale(1.08)",
                        x - d.start_x,
                        y - d.start_y
                    ),
                );
            }
        } else {
            let (dx, dy) = if second {
                let (_, sx, sy) = d.second.unwrap();
                d.second = Some((e.pointer_id(), x, y));
                ((x - sx) as f32, (y - sy) as f32)
            } else {
                ((x - d.x) as f32, (y - d.y) as f32)
            };
            let p = gpu::unproject(
                if second { d.x } else { x } as f32,
                if second { d.y } else { y } as f32,
                a.table,
                a.bounds,
                1.6,
            );
            a.world
                .move_grip(p.x, p.z, dx, dy, second || d.rotate || e.shift_key());
        }
        if !second {
            d.x = x;
            d.y = y;
        }
        a.dirty = true;
        Some((d.x, d.y, d.id, flat || now() - d.dragged_at >= 200.))
    });
    if let Some((x, y, id, show)) = hint {
        e.prevent_default();
        if began {
            choosing();
            let pointer = e.pointer_id();
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(200).await;
                let p = APP.with_borrow(|app| {
                    app.as_ref().and_then(|a| {
                        a.drag
                            .as_ref()
                            .filter(|d| d.pointer == pointer && d.id == id && d.moved)
                            .map(|d| (d.x, d.y))
                    })
                });
                if let Some((x, y)) = p {
                    hints(x, y, id);
                }
            });
        }
        if show {
            hints(x, y, id);
        }
        invalidate();
    }
}
fn pointer_end(e: PointerEvent, cancelled: bool) {
    let drag = APP.with_borrow_mut(|app| {
        let a = app.as_mut()?;
        let d = a.drag.as_mut()?;
        if d.second.is_some_and(|p| p.0 == e.pointer_id()) {
            d.second = None;
            return None;
        }
        if d.pointer != e.pointer_id() {
            return None;
        }
        a.drag.take()
    });
    let Some(d) = drag else { return };
    clear_hints();
    if !d.moved {
        if !cancelled && d.button == 0 {
            tap(d.id);
        }
        return;
    }
    if let Some(el) = element(&format!("die-{}", d.id)) {
        let _ = el.remove_attribute("style");
    }
    let target = if cancelled {
        None
    } else {
        target_at(e.client_x() as f64, e.client_y() as f64)
    };
    APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            a.world.end_grip(cancelled || target.is_none());
            a.dirty = true;
        }
    });
    if let Some(tray) = target {
        let before = if tray.is_empty() {
            None
        } else {
            before_at(&tray, d.id, e.client_x() as f64, e.client_y() as f64)
        };
        change(|s| {
            s.move_die(
                d.id,
                if tray.is_empty() { None } else { Some(&tray) },
                before,
            )
        });
    }
    relayout();
}
pub fn blur(id: u32) {
    let grabbed = APP.with_borrow(|app| {
        app.as_ref()
            .is_some_and(|a| a.drag.as_ref().is_some_and(|d| d.keyboard && d.id == id))
    });
    if grabbed {
        cancel();
    }
}
pub fn key(id: u32, key: &str, shift: bool) -> bool {
    let handled = APP.with_borrow(|app| {
        app.as_ref().is_some_and(|a| {
            !*a.rolling.peek()
                && if a.drag.is_some() {
                    matches!(
                        key,
                        "Enter"
                            | " "
                            | "Space"
                            | "Escape"
                            | "ArrowUp"
                            | "ArrowDown"
                            | "ArrowLeft"
                            | "ArrowRight"
                    )
                } else {
                    matches!(key, "Enter" | " " | "Space")
                }
        })
    });
    if handled {
        key_inner(id, key, shift);
    }
    handled
}
fn key_inner(id: u32, key: &str, shift: bool) {
    if key == "Escape" {
        cancel();
        return;
    }
    if !matches!(
        key,
        "Enter" | " " | "Space" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
    ) {
        return;
    }
    let mut drop = None;
    APP.with_borrow_mut(|app| {
        let Some(a) = app else { return };
        if *a.rolling.peek() {
            return;
        }
        let confirm = matches!(key, "Enter" | " " | "Space");
        if a.drag.is_none() {
            if !confirm {
                return;
            }
            let Some(el) = element(&format!("die-{id}")) else {
                return;
            };
            let r = rect(&el);
            let x = (r.left + r.width / 2.) as f64;
            let y = (r.top + r.height / 2.) as f64;
            let tray = a
                .state
                .peek()
                .dice
                .iter()
                .find(|d| d.id == id)
                .and_then(|d| d.tray.clone());
            let p = gpu::unproject(x as f32, y as f32, a.table, a.bounds, 1.6);
            if a.state.peek().view == "3d" {
                a.world.begin_grip(id, p.x, p.z);
            }
            a.drag = Some(Drag {
                id,
                pointer: -1,
                button: 0,
                start_x: x,
                start_y: y,
                x,
                y,
                started: now(),
                dragged_at: now(),
                moved: true,
                rotate: false,
                second: None,
                keyboard: true,
                target: Some(tray.clone().unwrap_or_default()),
                original_tray: tray,
            });
            a.selected.set(None);
            return;
        }
        let d = a.drag.as_mut().unwrap();
        if !d.keyboard || d.id != id {
            return;
        }
        if confirm {
            drop = Some((d.id, d.target.clone()));
            return;
        }
        if shift && a.state.peek().view == "3d" {
            let (dx, dy) = match key {
                "ArrowLeft" => (-15., 0.),
                "ArrowRight" => (15., 0.),
                "ArrowUp" => (0., -15.),
                _ => (0., 15.),
            };
            let p = gpu::unproject(d.x as f32, d.y as f32, a.table, a.bounds, 1.6);
            a.world.move_grip(p.x, p.z, dx, dy, true);
        } else {
            let tray = match key {
                "ArrowUp" => "t1",
                "ArrowDown" => "b1",
                _ => "",
            };
            d.target = Some(tray.into());
            let el = if tray.is_empty() {
                element("dice")
            } else {
                document()
                    .query_selector(&format!(".tray[data-tray='{tray}']"))
                    .ok()
                    .flatten()
            };
            if let Some(el) = el {
                let r = rect(&el);
                d.x = (r.left + r.width / 2.) as f64;
                d.y = (r.top + r.height / 2.) as f64;
                let p = gpu::unproject(d.x as f32, d.y as f32, a.table, a.bounds, 1.6);
                a.world.move_grip(p.x, p.z, 0., 0., false);
            }
        }
        a.dirty = true;
    });
    if let Some((id, tray)) = drop {
        APP.with_borrow_mut(|app| {
            if let Some(a) = app {
                let auto_place = a.drag.as_ref().is_some_and(|d| d.original_tray.is_some())
                    && tray.as_deref() == Some("");
                a.world.end_grip(auto_place);
                a.drag = None;
            }
        });
        change(|s| s.move_die(id, tray.as_deref().filter(|v| !v.is_empty()), None));
        clear_hints();
        relayout();
        if let Some(el) =
            element(&format!("die-{id}")).and_then(|e| e.dyn_into::<HtmlElement>().ok())
        {
            let _ = el.focus();
        }
    } else {
        let p = APP.with_borrow(|app| {
            app.as_ref()
                .and_then(|a| a.drag.as_ref().map(|d| (d.x, d.y, d.id)))
        });
        if let Some((x, y, id)) = p {
            hints(x, y, id);
        }
        invalidate();
    }
}
pub fn show_dialog(id: &str) {
    cancel();
    if let Some(d) = element(id).and_then(|e| e.dyn_into::<HtmlDialogElement>().ok()) {
        let _ = d.show_modal();
        d.set_scroll_top(0);
    }
}
pub fn close_dialog(id: &str) {
    if let Some(d) = element(id).and_then(|e| e.dyn_into::<HtmlDialogElement>().ok()) {
        d.close();
    }
    let focus = if id == "bag" { "bag-open" } else { "open" };
    if let Some(el) = element(focus).and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
        let _ = el.focus();
    }
}
pub fn set_theme(theme: &str) {
    let theme = if ["auto", "dark", "light"].contains(&theme) {
        theme
    } else {
        "auto"
    };
    store("handydice-theme", theme);
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("data-theme", theme);
        if let Ok(Some(style)) = window().get_computed_style(&root) {
            if let Ok(bg) = style.get_property_value("--bg") {
                if let Ok(Some(meta)) = document().query_selector("meta[name='theme-color']") {
                    let _ = meta.set_attribute("content", bg.trim());
                }
            }
        }
    }
    APP.with_borrow_mut(|app| {
        if let Some(a) = app {
            a.assets_dirty = true;
        }
    });
    invalidate();
}
pub fn set_language(lang: &str) {
    store("handydice-language", lang);
    let _ = window().location().reload();
}
pub fn request_sensor() {
    device::request_permission();
}
pub fn install_available() -> bool {
    APP.with_borrow(|app| app.as_ref().is_some_and(|a| a.install_event.is_some()))
}
pub fn install() {
    let event = APP.with_borrow_mut(|app| app.as_mut().and_then(|a| a.install_event.take()));
    if let Some(e) = event {
        if let Ok(f) = Reflect::get(&e, &"prompt".into()).and_then(|f| f.dyn_into::<Function>()) {
            let _ = f.call0(&e);
        }
    } else {
        let lang = stored("handydice-language").unwrap_or_else(|| "en".into());
        let _=window().alert_with_message(&model::t(&lang,"Install: Android: “Install app” (appears when Chrome offers it); iOS: Share → “Add to Home Screen”."));
    }
}

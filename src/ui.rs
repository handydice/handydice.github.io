//! Dioxus port of index.html + the DOM half of app.js: same markup, ids and classes, so style.css applies unchanged.
//! Runtime behavior (rolls, drags, physics, GPU, dialogs, sensors) lives in platform.rs.
use crate::model::{self, t, DiceSet, Die, State, MAX_DICE};
use crate::{device, platform};
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen::JsCast;

// Pip positions in a 3×3 grid (0 = top left … 8 = bottom right).
const PIPS: [&[usize]; 7] = [
    &[],
    &[4],
    &[0, 8],
    &[0, 4, 8],
    &[0, 2, 6, 8],
    &[0, 2, 4, 6, 8],
    &[0, 2, 3, 5, 6, 8],
];
const TYPE_OPTIONS: [(u32, bool); 8] = [
    (4, false),
    (6, false),
    (6, true),
    (8, false),
    (10, false),
    (12, false),
    (20, false),
    (100, false),
];
const SURFACE_NAMES: [&str; 6] = ["None", "Casino", "Oak", "Crafts", "Blue wood", "Terracotta"];
// (preference, lang attribute, flag, name)
const LANGUAGE_NAMES: [(&str, &str, &str, &str); 9] = [
    ("auto", "", "🌐", "Auto"),
    ("en", "en", "🇬🇧", "English"),
    ("de", "de", "🇩🇪", "Deutsch"),
    ("fr", "fr", "🇫🇷", "Français"),
    ("es", "es", "🇪🇸", "Español"),
    ("pt", "pt-BR", "🇧🇷", "Português"),
    ("it", "it", "🇮🇹", "Italiano"),
    ("nl", "nl", "🇳🇱", "Nederlands"),
    ("pl", "pl", "🇵🇱", "Polski"),
];
const THEMES: [(&str, &str); 3] = [("auto", "Auto"), ("light", "Light"), ("dark", "Dark")];
const SETTINGS_SVG: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"#;

/// Render inputs shared by every die target.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    lang: &'a str,
    flat: bool,
    select_mode: bool,
    selected: Option<u32>,
    rolling: Signal<bool>,
}

fn pressed(on: bool) -> &'static str {
    if on {
        "true"
    } else {
        "false"
    }
}

/// Signal::set needs `&mut`; copying the handle keeps handler closures `Fn` and `Copy`.
fn set<T: 'static>(mut signal: Signal<T>, value: T) {
    signal.set(value);
}

fn story_of(d: &Die) -> Option<&str> {
    d.story.as_deref().filter(|s| model::is_story(s))
}

fn die_label(lang: &str, sides: u32, color: &str, numbered: bool, story: Option<&str>) -> String {
    if let Some(story) = story {
        return format!(
            "{} {}",
            t(lang, "Story die"),
            story.trim_start_matches("classic-")
        );
    }
    let mut label = String::new();
    if !color.is_empty() {
        label.push_str(&t(lang, model::color_name(color)));
        label.push(' ');
    }
    label.push_str(&model::die_type(lang, sides));
    if sides == 6 {
        label.push(' ');
        label.push_str(&t(lang, if numbered { "Number" } else { "Pips" }));
    }
    label
}

fn sum_label(dice: &[Die]) -> String {
    if dice.iter().any(|d| d.story.is_none()) {
        format!("Σ {}", model::total(dice))
    } else {
        String::new()
    }
}

fn face(lang: &str, sides: u32, value: u32, numbered: bool, story: Option<&str>) -> Element {
    if let Some((motif, _)) = story.and_then(|s| model::story_face(s, value)) {
        let src = format!("icons/story/{motif}.svg");
        return rsx! { img { class: "story-face", src, alt: "", draggable: "false" } };
    }
    if sides == 6 && !numbered {
        let on = PIPS.get(value as usize).copied().unwrap_or(&[]);
        return rsx! {
            span { class: "pips", "data-value": "{value}",
                for n in 0..9usize {
                    i { class: on.contains(&n).then_some("on") }
                }
            }
        };
    }
    let kind = model::die_type(lang, sides);
    rsx! { span { class: "num", "{value}" } small { "{kind}" } }
}

/// Die type without a roll: classic D6 shows five pips; numeric dice show their side count.
fn type_face(lang: &str, sides: u32, numbered: bool, story: Option<&str>) -> Element {
    if story.is_some() {
        return face(lang, sides, 1, false, story);
    }
    if sides == 6 && !numbered {
        return face(lang, 6, 5, false, None);
    }
    let kind = model::die_type(lang, sides);
    rsx! { span { class: "num", "{sides}" } small { "{kind}" } }
}

fn sides_face(lang: &str, sides: u32, numbered: bool) -> Element {
    if sides == 6 && !numbered {
        return type_face(lang, sides, numbered, None);
    }
    rsx! { span { class: "num", "{sides}" } }
}

fn mini_die(lang: &str, sides: u32, color: &str, numbered: bool, story: Option<&str>) -> Element {
    let color = if story.is_some() { "white" } else { color };
    rsx! {
        span { class: "die", "data-shape": "d{sides}", "data-color": "{color}", "aria-hidden": "true",
            {type_face(lang, sides, numbered, story)}
        }
    }
}

fn die_el(d: &Die, c: Ctx) -> Element {
    let id = d.id;
    let story = story_of(d);
    let shown = story
        .and_then(|s| model::story_face(s, d.value))
        .map_or_else(|| d.value.to_string(), |(_, label)| t(c.lang, label));
    let action = t(
        c.lang,
        if c.select_mode {
            "select"
        } else if d.tray.is_some() {
            "return to table"
        } else {
            "move to tray"
        },
    );
    let label = format!(
        "{}: {shown}, {action}",
        die_label(c.lang, d.sides, &d.color, d.numbered, story)
    );
    let selected = c.selected == Some(id);
    // 3D: a transparent focus and drop target over the rendered mesh; 2D draws the die itself.
    let class = match (c.flat, selected) {
        (true, false) => "die",
        (true, true) => "die selected",
        (false, false) => "die rendered-die",
        (false, true) => "die rendered-die selected",
    };
    let color = if story.is_some() {
        "white"
    } else {
        d.color.as_str()
    };
    let shape = c.flat.then(|| format!("d{}", d.sides));
    let described = if c.flat {
        "drag-help"
    } else {
        "drag-help rotate-help"
    };
    let rolling = c.rolling;
    rsx! {
        div {
            key: "{id}",
            id: "die-{id}",
            class,
            "data-id": "{id}",
            role: "button",
            tabindex: "0",
            "data-color": "{color}",
            "data-shape": shape,
            "aria-label": "{label}",
            "aria-describedby": described,
            "aria-pressed": pressed(selected),
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                // Screen readers click without a pointer sequence; pointer taps are handled by pointer_start.
                if e.try_as_web_event().is_some_and(|m| m.detail() == 0) && !rolling() {
                    platform::tap(id);
                }
            },
            oncontextmenu: move |e: MouseEvent| e.prevent_default(),
            onpointerdown: move |e: PointerEvent| {
                let at = e.client_coordinates();
                let button = e.try_as_web_event().map_or(0, |p| p.button());
                let shift = e.modifiers().contains(Modifiers::SHIFT);
                if platform::pointer_start(id, at.x, at.y, e.pointer_id(), button, shift) {
                    e.prevent_default();
                }
            },
            onkeydown: move |e: KeyboardEvent| {
                if platform::key(id, &e.key().to_string(), e.modifiers().contains(Modifiers::SHIFT)) {
                    e.prevent_default();
                }
            },
            onblur: move |_| platform::blur(id),
            if c.flat {
                {face(c.lang, d.sides, d.value, d.numbered, story)}
            }
        }
    }
}

fn ghost(id: u32) -> Element {
    rsx! { div { key: "g{id}", class: "die ghost", "aria-hidden": "true" } }
}

/// A set card: a tap replaces the table with the set and rolls.
fn preset_el(set: DiceSet, lang: &str, key: String) -> Element {
    let minis: Vec<Element> = set
        .dice
        .iter()
        .enumerate()
        .map(|(i, &sides)| {
            let color = set.colors.get(i).cloned().flatten().unwrap_or_default();
            let numbered = set.numbered.get(i).copied().flatten() == Some(true);
            let story = set
                .stories
                .get(i)
                .cloned()
                .flatten()
                .filter(|s| model::is_story(s));
            mini_die(lang, sides, &color, numbered, story.as_deref())
        })
        .collect();
    let name = set.name.clone();
    rsx! {
        button { key: "{key}", r#type: "button", class: "preset",
            onclick: move |_| {
                platform::close_dialog("bag");
                platform::change(|s| s.use_set(&set));
                platform::roll();
            },
            span { class: "minis", {minis.into_iter()} }
            span { class: "name", "{name}" }
        }
    }
}

fn tray_el(tray: &'static str, label: &str, total_label: &str, dice: &[Die], c: Ctx) -> Element {
    let sum = sum_label(dice);
    let class = if dice.is_empty() {
        "tray"
    } else {
        "tray occupied"
    };
    let rolling = c.rolling;
    rsx! {
        section { class, "data-tray": tray, "aria-label": "{label}",
            onclick: move |_| if !rolling() { platform::drop_selected(Some(tray), None) },
            div { class: "tray-dice", "data-label": "{label}",
                {dice.iter().map(|d| die_el(d, c))}
            }
            output { class: "tray-total", "aria-label": "{total_label}", hidden: sum.is_empty(), "{sum}" }
        }
    }
}

fn document() -> Option<web_sys::Document> {
    web_sys::window().and_then(|w| w.document())
}

/// Runs after Dioxus applied the pending render (it renders in a microtask, this waits for the next task).
fn after_render(f: impl FnOnce(&web_sys::Document) + 'static) {
    wasm_bindgen_futures::spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(0).await;
        if let Some(doc) = document() {
            f(&doc);
        }
    });
}

/// Focuses the first match; text inputs get their content selected.
fn focus_first(doc: &web_sys::Document, selectors: &[String]) {
    for selector in selectors {
        let Some(el) = doc
            .query_selector(selector)
            .ok()
            .flatten()
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        else {
            continue;
        };
        let _ = el.focus();
        if let Some(input) = el.dyn_ref::<web_sys::HtmlInputElement>() {
            input.select();
        }
        return;
    }
}

/// i18n.js localize(): document language, translated install manifest and description.
fn localize_document(lang: &str) {
    let Some(doc) = document() else { return };
    if let Some(root) = doc.document_element() {
        let _ = root.set_attribute("lang", lang);
    }
    if lang == "en" {
        return;
    }
    if let Ok(Some(link)) = doc.query_selector("link[rel=\"manifest\"]") {
        let _ = link.set_attribute("href", &format!("manifest.{lang}.webmanifest"));
    }
    if let Ok(Some(meta)) = doc.query_selector("meta[name=\"description\"]") {
        if let Some(content) = meta.get_attribute("content") {
            let _ = meta.set_attribute("content", &t(lang, &content));
        }
    }
}

fn browser_languages() -> Vec<String> {
    let Some(navigator) = web_sys::window().map(|w| w.navigator()) else {
        return vec!["en".into()];
    };
    let list: Vec<String> = navigator
        .languages()
        .iter()
        .filter_map(|v| v.as_string())
        .collect();
    if list.is_empty() {
        navigator.language().into_iter().collect()
    } else {
        list
    }
}

#[allow(non_snake_case)]
pub fn App() -> Element {
    let state = use_signal(|| State::load(platform::stored(model::STATE_KEY).as_deref()));
    let rolling = use_signal(|| false);
    let selected = use_signal(|| None::<u32>);
    let error = use_signal(String::new);
    let backend = use_signal(String::new);
    use_hook(move || platform::init(state, rolling, selected, error, backend));
    use_effect(move || {
        let current = state.read().clone();
        platform::sync(current);
    });

    // Language: a manual choice wins, Auto follows the browser. Changing it reloads the app.
    let language = use_signal(|| {
        let preference = platform::stored(model::LANGUAGE_KEY)
            .filter(|l| model::LANGUAGES.contains(&l.as_str()))
            .unwrap_or_else(|| "auto".into());
        let lang = model::choose_language(&preference, &browser_languages());
        localize_document(&lang);
        (preference, lang)
    });
    let theme = use_signal(|| {
        platform::stored(model::THEME_KEY)
            .filter(|v| THEMES.iter().any(|(id, _)| id == v))
            .unwrap_or_else(|| "auto".into())
    });
    let bag_color = use_signal(|| "white".to_string());
    let just_added = use_signal(|| None::<u32>);
    let editing = use_signal(|| false);
    let saving = use_signal(|| false);
    let set_name = use_signal(String::new);
    let install_ready = use_signal(|| false);
    let roll_press = use_signal(|| None::<i32>);
    let presets = use_hook(|| model::presets(&language.peek().1));

    let (preference, lang) = language();
    let lang = lang.as_str();
    let tr = |text: &str| t(lang, text);
    let s = state.read().clone();
    let is_rolling = rolling();
    let flat = s.view == "2d";
    let c = Ctx {
        lang,
        flat,
        select_mode: s.click_mode == "select",
        selected: selected(),
        rolling,
    };

    let on_table: Vec<Die> = s
        .dice
        .iter()
        .filter(|d| d.tray.is_none())
        .cloned()
        .collect();
    let in_tray = |tray: &str| -> Vec<Die> {
        s.dice
            .iter()
            .filter(|d| d.tray.as_deref() == Some(tray))
            .cloned()
            .collect()
    };
    let (top, bottom) = (in_tray("t1"), in_tray("b1"));
    // 2D keeps tray dice's table slots free (ghosts) so nothing shifts; 3D places dice physically.
    let table: Vec<Element> = if flat {
        let mut slotted: Vec<&Die> = s.dice.iter().filter(|d| d.slot.is_some()).collect();
        slotted.sort_by_key(|d| d.slot);
        slotted
            .into_iter()
            .map(|d| {
                if d.tray.is_some() {
                    ghost(d.id)
                } else {
                    die_el(d, c)
                }
            })
            .collect()
    } else {
        on_table.iter().map(|d| die_el(d, c)).collect()
    };
    // Roll number of the turn: the full roll counts as #1, so the first reroll shows #2.
    let total = if on_table.is_empty() {
        String::new()
    } else {
        let reroll = if s.rerolls > 0 {
            format!(" #{}", s.rerolls + 1)
        } else {
            String::new()
        };
        format!("{}{reroll}", sum_label(&on_table))
            .trim()
            .to_string()
    };
    let roll_label = tr(if !s.dice.is_empty() && on_table.is_empty() {
        "Reroll all dice"
    } else {
        "Roll"
    });
    let err = error();
    let error_text = if err.is_empty() || flat {
        String::new()
    } else if err.to_lowercase().contains("image") {
        tr("Dice images could not be loaded. Reload the app and try again.")
    } else {
        tr("3D rendering needs WebGL2. Choose 2D in Settings, or enable graphics acceleration and reload.")
    };
    let current_theme = theme();
    let color = bag_color();
    let full = s.dice.len() >= MAX_DICE;
    let added = just_added();
    let is_editing = editing();
    let count = format!("{}/{MAX_DICE}", s.dice.len());

    let open_bag = move || {
        if rolling() {
            return;
        }
        set(editing, false);
        set(saving, false);
        set(just_added, None);
        platform::show_dialog("bag");
    };
    // No dice at all: the button opens the dice bag instead, so an empty board is one tap from dice.
    let press_roll = move || {
        if state.peek().dice.is_empty() {
            open_bag()
        } else {
            platform::roll()
        }
    };

    rsx! {
        main {
            header {
                button { id: "bag-open", "aria-haspopup": "dialog", disabled: is_rolling,
                    onclick: move |_| open_bag(),
                    i { class: "ico d6", "aria-hidden": "true" }
                    {tr("Dice")}
                }
                button { id: "open", "aria-haspopup": "dialog", "aria-label": tr("Settings"),
                    dangerous_inner_html: SETTINGS_SVG,
                    onclick: move |_| {
                        set(install_ready, platform::install_available());
                        platform::show_dialog("settings");
                    },
                }
            }
            section { id: "dice", "data-tray": "", "aria-label": tr("Dice"), "aria-live": "polite", "data-label": tr("Table"),
                onclick: move |_| if !rolling() { platform::drop_selected(None, None) },
                {table.into_iter()}
            }
            div { id: "controls",
                div { id: "trayarea",
                    div { class: "trays", id: "toptrays",
                        {tray_el("t1", &tr("Top tray"), &tr("Top tray total"), &top, c)}
                    }
                    div { class: "trays", id: "bottomtrays",
                        div { id: "total", "{total}" }
                        {tray_el("b1", &tr("Bottom tray"), &tr("Bottom tray total"), &bottom, c)}
                    }
                }
                div { id: "actions",
                    button { id: "reset", hidden: on_table.len() == s.dice.len(),
                        onclick: move |_| if !rolling() { platform::return_all() },
                        {tr("↺ Return")}
                    }
                    // Roll on release, not only on click: after a long press the browser fires contextmenu and
                    // drops the click. Only a press that started on the button counts. click is for keyboards.
                    button { id: "roll", disabled: is_rolling,
                        onpointerdown: move |e: PointerEvent| {
                            let button = e.try_as_web_event().map_or(0, |p| p.button());
                            if !e.is_primary() || button != 0 || rolling() {
                                return;
                            }
                            set(roll_press, Some(e.pointer_id()));
                            if let Some(el) = document().and_then(|d| d.get_element_by_id("roll")) {
                                let _ = el.set_pointer_capture(e.pointer_id());
                            }
                        },
                        onpointerup: move |e: PointerEvent| {
                            if *roll_press.peek() == Some(e.pointer_id()) {
                                set(roll_press, None);
                                press_roll();
                            }
                        },
                        onpointercancel: move |_| set(roll_press, None),
                        onlostpointercapture: move |_| set(roll_press, None),
                        onclick: move |e: MouseEvent| {
                            if e.try_as_web_event().is_some_and(|m| m.detail() == 0) {
                                press_roll();
                            }
                        },
                        oncontextmenu: move |e: MouseEvent| e.prevent_default(),
                        i { class: "ico cup", "aria-hidden": "true" }
                        span { id: "roll-label", "{roll_label}" }
                    }
                }
            }
            output { id: "flick-debug", hidden: true, "aria-live": "polite" }
        }

        dialog { id: "bag", class: "screen", "aria-labelledby": "bag-title",
            div {
                div { class: "bag-top",
                    div { class: "head",
                        form { method: "dialog",
                            button { class: "back", "aria-label": tr("Back"), "←" }
                        }
                        h2 { id: "bag-title", {tr("Dice")} }
                    }
                    h3 {
                        span { {tr("On the table")} }
                        " "
                        small { id: "table-count", "{count}" }
                        button { id: "clear", class: "link", r#type: "button", hidden: s.dice.is_empty(),
                            onclick: move |_| {
                                platform::change(|s| s.dice.clear());
                                device::play("click");
                                after_render(|d| focus_first(d, &[".die-types button".into()]));
                            },
                            {tr("Clear")}
                        }
                    }
                    div { id: "table-dice", class: "minis",
                        for (i, d) in s.dice.iter().enumerate() {
                            button {
                                key: "{d.id}",
                                r#type: "button",
                                class: if added == Some(d.id) { "mini new" } else { "mini" },
                                "aria-label": format!("{}: {}", die_label(lang, d.sides, &d.color, d.numbered, story_of(d)), tr("remove")),
                                onclick: {
                                    let id = d.id;
                                    move |_| {
                                        platform::change(|s| s.remove(id));
                                        device::play("click");
                                        // Keep the focus in the row: the next die, else the previous, else the first type.
                                        after_render(move |doc| focus_first(doc, &[
                                            format!("#table-dice > :nth-child({})", i + 1),
                                            format!("#table-dice > :nth-child({})", i),
                                            ".die-types button".into(),
                                        ]));
                                    }
                                },
                                {mini_die(lang, d.sides, &d.color, d.numbered, story_of(d))}
                            }
                        }
                    }
                    p { id: "table-empty", class: "hint", hidden: !s.dice.is_empty(), {tr("Tap a die below to put it on the table.")} }
                }
                h3 { {tr("Add")} }
                div { class: "palette scroller", role: "group", "aria-label": tr("Color"),
                    for &option in model::COLORS {
                        button {
                            key: "{option}",
                            r#type: "button",
                            class: "die",
                            "data-color": option,
                            "aria-label": tr(model::color_name(option)),
                            title: tr(model::color_name(option)),
                            "aria-pressed": pressed(option == color),
                            onclick: move |_| set(bag_color, option.to_string()),
                        }
                    }
                }
                div { class: "die-types", role: "group", "aria-label": tr("Add"),
                    for (sides, numbered) in TYPE_OPTIONS {
                        button {
                            key: "{sides}-{numbered}",
                            r#type: "button",
                            "aria-label": format!("{}: {}", tr("Add"), die_label(lang, sides, "", numbered, None)),
                            disabled: full,
                            onclick: move |_| {
                                if state.peek().dice.len() >= MAX_DICE {
                                    return;
                                }
                                let color = bag_color.peek().clone();
                                let mut id = None;
                                platform::change(|s| id = s.add(sides, &color, numbered, None));
                                set(just_added, id);
                                device::play("click");
                            },
                            span { class: "die", "data-shape": "d{sides}", "data-color": "{color}", "aria-hidden": "true",
                                {sides_face(lang, sides, numbered)}
                            }
                        }
                    }
                }
                h3 {
                    span { {tr("Sets")} }
                    button { id: "edit-sets", class: "link", r#type: "button", "aria-pressed": pressed(is_editing), hidden: s.sets.is_empty(),
                        onclick: move |_| set(editing, !editing()),
                        {tr(if is_editing { "Done" } else { "Edit" })}
                    }
                }
                // Own sets come first; edit mode shows their remove buttons.
                div { id: "sets", class: "scroller",
                    button { r#type: "button", class: "preset save", hidden: s.dice.is_empty(), "aria-label": tr("Save current dice"),
                        onclick: {
                            let lang = lang.to_string();
                            move |_| {
                                set(saving, true);
                                set(set_name, state.peek().current_name(&lang));
                                after_render(|d| focus_first(d, &["#set-name".into()]));
                            }
                        },
                        span { class: "plus", "aria-hidden": "true", "+" }
                        span { class: "name", {tr("Save")} }
                    }
                    for (i, own) in s.sets.iter().enumerate() {
                        div { key: "own-{i}-{own.name}", class: "own",
                            {preset_el(own.clone(), lang, format!("own-{i}"))}
                            button { r#type: "button", class: "remove", "aria-label": format!("{}: {}", own.name, tr("remove")), hidden: !is_editing,
                                onclick: move |_| {
                                    let mut left = 0;
                                    platform::change(|s| {
                                        if i < s.sets.len() {
                                            s.sets.remove(i);
                                        }
                                        left = s.sets.len();
                                    });
                                    if left == 0 {
                                        set(editing, false);
                                    }
                                    let target = if left == 0 { ".preset.save" } else { "#edit-sets" };
                                    after_render(move |d| focus_first(d, &[target.into()]));
                                },
                                "×"
                            }
                        }
                    }
                    {presets.iter().enumerate().map(|(i, preset)| preset_el(preset.clone(), lang, format!("preset-{i}")))}
                }
                form { id: "save-set", class: "line", hidden: !saving(),
                    onsubmit: {
                        let lang = lang.to_string();
                        move |e: FormEvent| {
                            e.prevent_default();
                            if state.peek().dice.is_empty() {
                                return;
                            }
                            let name = set_name.peek().clone();
                            platform::change(|s| s.save_set(&name, &lang));
                            set(saving, false);
                            after_render(|d| {
                                if let Some(sets) = d.get_element_by_id("sets") {
                                    sets.set_scroll_left(0);
                                }
                            });
                        }
                    },
                    input { id: "set-name", maxlength: "40", autocomplete: "off", "aria-label": tr("Save current dice"),
                        value: set_name(),
                        oninput: move |e: FormEvent| set(set_name, e.value()),
                    }
                    button { class: "primary", {tr("Save")} }
                }
            }
        }

        dialog { id: "settings", class: "screen",
            div {
                div { class: "head",
                    form { method: "dialog",
                        button { class: "back", "aria-label": tr("Back"), "←" }
                    }
                    h2 { {tr("Settings")} }
                }

                h3 { {tr("Sound")} }
                div { class: "grid", id: "sound",
                    for (on, value, icon, label) in [(true, "on", "ico volume", "On"), (false, "off", "ico volume-off", "Off")] {
                        button { key: "{value}", "data-sound": value, "aria-pressed": pressed(s.sound == on),
                            onclick: move |_| {
                                platform::change(|s| s.sound = on);
                                if on {
                                    device::resume();
                                }
                            },
                            i { class: icon, "aria-hidden": "true" }
                            {tr(label)}
                        }
                    }
                }

                h3 { {tr("Appearance")} }
                div { class: "grid", id: "theme",
                    for (value, label) in THEMES {
                        button { key: "{value}", "data-theme": value, "aria-pressed": pressed(current_theme == value),
                            onclick: move |_| {
                                platform::set_theme(value);
                                set(theme, value.to_string());
                            },
                            {tr(label)}
                        }
                    }
                }

                h3 { {tr("Table")} }
                div { class: "grid", id: "surface",
                    for (&surface, name) in model::SURFACES.iter().zip(SURFACE_NAMES) {
                        button { key: "{surface}", "data-surface": surface, "aria-pressed": pressed(s.surface == surface),
                            onclick: move |_| platform::change(|s| s.surface = surface.to_string()),
                            if surface.is_empty() {
                                span { class: "thumb" }
                            } else {
                                img { class: "thumb", src: "backgrounds/thumbs/{surface}.webp", alt: "", loading: "lazy" }
                            }
                            {tr(name)}
                        }
                    }
                }

                h3 { {tr("Language")} }
                div { class: "grid", id: "language",
                    for (value, code, flag, name) in LANGUAGE_NAMES {
                        button { key: "{value}", "data-language": value, lang: (!code.is_empty()).then_some(code), "aria-pressed": pressed(preference == value),
                            onclick: move |_| platform::set_language(value),
                            span { class: "flag", "aria-hidden": "true", "{flag}" }
                            {tr(name)}
                        }
                    }
                }

                h3 { {tr("Dice display")} }
                div { class: "grid", id: "view",
                    for (value, label) in [("2d", "2D"), ("3d", "3D")] {
                        button { key: "{value}", "data-view": value, "aria-pressed": pressed(s.view == value),
                            // Switching keeps the round as it is; only the presentation changes. Not during a roll.
                            onclick: move |_| {
                                if rolling() || state.peek().view == value {
                                    return;
                                }
                                platform::cancel();
                                set(error, String::new());
                                platform::change(|s| s.view = value.to_string());
                                platform::request_sensor();
                            },
                            {tr(label)}
                        }
                    }
                }
                p { class: "hint", {tr("2D: lightweight, ideal for older devices. 3D: physical dice, requires WebGL2.")} }
                p { class: "hint", {tr("Landscape: dice on the left, trays and cup on the right. Rotating keeps your round intact. Full trays can be scrolled.")} }

                h3 { "data-view-only": "3d", {tr("Table tilt")} }
                div { class: "grid", id: "tilt", "data-view-only": "3d",
                    for (on, value, label) in [(true, "on", "On"), (false, "off", "Off")] {
                        button { key: "{value}", "data-tilt": value, "aria-pressed": pressed(s.tilt == on),
                            onclick: move |_| {
                                platform::change(|s| s.tilt = on);
                                if on {
                                    platform::request_sensor();
                                }
                            },
                            {tr(label)}
                        }
                    }
                }
                p { class: "hint", "data-view-only": "3d", {tr("Tilt your phone to move table dice. Off keeps the table level.")} }

                h3 { "data-view-only": "2d", {tr("Shake to roll")} }
                div { class: "grid", id: "shake", "data-view-only": "2d",
                    for (on, value, label) in [(true, "on", "On"), (false, "off", "Off")] {
                        button { key: "{value}", "data-shake": value, "aria-pressed": pressed(s.shake == on),
                            onclick: move |_| {
                                platform::change(|s| s.shake = on);
                                if on {
                                    platform::request_sensor();
                                }
                            },
                            {tr(label)}
                        }
                    }
                }
                p { class: "hint", "data-view-only": "2d", {tr("Shake your phone to roll dice. Off needs no motion sensor access.")} }

                h3 { {tr("Accessibility")} }
                p { class: "hint", {tr("Tap to move dice to the last used tray, initially the bottom tray. Tap again to return them. Drag to choose the top or bottom tray.")} }
                label { r#for: "click-mode", {tr("When tapping a die")} }
                select { id: "click-mode", value: s.click_mode.clone(),
                    onchange: move |e: FormEvent| {
                        platform::cancel();
                        let mode = if e.value() == "select" { "select" } else { "direct" };
                        platform::change(|s| s.click_mode = mode.to_string());
                    },
                    option { value: "direct", selected: s.click_mode != "select", {tr("Move / return immediately")} }
                    option { value: "select", selected: s.click_mode == "select", {tr("Select, then tap the destination")} }
                }
                p { class: "hint", {tr("Two taps: select a die, then tap the top tray, bottom tray or table. Tap a tray die to insert before it. Tap the selected die again or press Esc to cancel. Drag & drop remains available in both modes.")} }
                p { class: "hint", "data-view-only": "3d", {tr("3D: drag gently to move, flick to throw. Fast spins randomize the numbers. Enable table tilt in Settings to move dice by tilting your phone. Shift-drag or use a second finger to rotate.")} }
                p { class: "hint", id: "drag-help", {tr("Keyboard: Enter or Space to pick up, ↑ for top, ↓ for bottom, ← for the table. Enter to drop, Esc to cancel.")} }
                p { class: "hint", id: "rotate-help", "data-view-only": "3d", {tr("Shift + arrow keys rotate picked-up dice.")} }

                button { id: "install", class: "wide", hidden: !install_ready(),
                    onclick: move |_| {
                        platform::install();
                        set(install_ready, false);
                    },
                    {tr("Install app")}
                }
                p { class: "hint",
                    {tr("Works offline · no trackers · no sign-up.")}
                    br {}
                    {tr("Language: Auto follows your browser. A manual choice is saved on this device.")}
                    br {}
                    {tr("Install: Android: “Install app” (appears when Chrome offers it); iOS: Share → “Add to Home Screen”.")}
                }

                h3 { {tr("License")} }
                p { class: "hint",
                    {tr("MIT license. The source code is on GitHub.")}
                    " "
                    a { href: "https://github.com/handydice/handydice.github.io", target: "_blank", rel: "noreferrer", {tr("View source code")} }
                }
            }
        }

        p { id: "render-error", role: "alert", title: "{err}", hidden: error_text.is_empty(), "{error_text}" }
    }
}

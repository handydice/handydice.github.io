//! Dice, round state, story dice and translations. Port of dice.js, state.js, story.js and i18n.js.
//! The serialized `State` is the legacy `handydice-state` localStorage format, so saved rounds survive.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

pub const STATE_KEY: &str = "handydice-state";
pub const THEME_KEY: &str = "handydice-theme";
pub const LANGUAGE_KEY: &str = "handydice-language";

pub const MAX_DICE: usize = 12;
pub const SIDES: &[u32] = &[4, 6, 8, 10, 12, 20, 100];
/// skin-* are painted D6 faces from skins/<name>/; on other dice they fall back to a plain color.
pub const COLORS: &[&str] = &[
    "white",
    "red",
    "blue",
    "yellow",
    "green",
    "orange",
    "brown",
    "purple",
    "turquoise",
    "pink",
    "gray",
    "skin-opal",
    "skin-wood",
    "skin-galaxy",
    "skin-jade",
    "skin-amber",
    "skin-terrazzo",
    "skin-ruby",
    "skin-steel",
    "skin-prism",
    "skin-marble",
    "skin-ice",
    "skin-walnut",
];
/// Selectable table backgrounds; "" is none.
pub const SURFACES: &[&str] = &["", "casino", "oak", "crafts", "bluewood", "terracotta"];
pub const LANGUAGES: &[&str] = &["en", "de", "fr", "es", "pt", "it", "nl", "pl"];
const TRAYS: &[&str] = &["t1", "b1"];
const DEFAULT_DICE: [u32; 5] = [6; 5];

/// English color names (t() keys), same order as COLORS.
pub fn color_name(color: &str) -> &'static str {
    match color {
        "white" => "White",
        "red" => "Red",
        "blue" => "Blue",
        "yellow" => "Yellow",
        "green" => "Green",
        "orange" => "Orange",
        "brown" => "Brown",
        "purple" => "Purple",
        "turquoise" => "Turquoise",
        "pink" => "Pink",
        "gray" => "Dark gray",
        "skin-opal" => "Opal",
        "skin-wood" => "Wood",
        "skin-galaxy" => "Galaxy",
        "skin-jade" => "Jade",
        "skin-amber" => "Amber",
        "skin-terrazzo" => "Terrazzo",
        "skin-ruby" => "Ruby",
        "skin-steel" => "Steel",
        "skin-prism" => "Prism",
        "skin-marble" => "Black marble",
        "skin-ice" => "Ice",
        "skin-walnut" => "Walnut",
        _ => "",
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Die {
    /// Runtime identity only; reassigned on load, never stored.
    #[serde(skip)]
    pub id: u32,
    pub sides: u32,
    pub value: u32,
    /// "" = no color chosen (legacy `undefined`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub numbered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray: Option<String>,
    /// 2D table slot; a die in a tray keeps it free until the next roll.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<usize>,
}

/// A saved or preset set. Per-die vectors may be shorter than `dice` (missing = None).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiceSet {
    pub name: String,
    pub dice: Vec<u32>,
    #[serde(default)]
    pub colors: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "all_none")]
    pub numbered: Vec<Option<bool>>,
    #[serde(default, skip_serializing_if = "all_none")]
    pub stories: Vec<Option<String>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub dice: Vec<Die>,
    pub sets: Vec<DiceSet>,
    /// "" until a die was put aside (legacy `undefined`); see `next_tray`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_tray: String,
    pub surface: String,
    pub sound: bool,
    pub click_mode: String,
    pub rerolls: u32,
    pub view: String,
    pub tilt: bool,
    pub shake: bool,
    #[serde(skip)]
    next_id: u32,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn all_none<T>(v: &[Option<T>]) -> bool {
    v.iter().all(Option::is_none)
}

fn is_sides(n: u32) -> bool {
    SIDES.contains(&n)
}

fn is_tray(tray: &str) -> bool {
    TRAYS.contains(&tray)
}

/// JS `Number.isInteger`: 3 and 3.0 count, 3.5, "3" and NaN do not.
fn int(v: Option<&Value>) -> Option<i64> {
    let f = v?.as_f64()?;
    (f.is_finite() && f.fract() == 0.0).then_some(f as i64)
}

fn sides_of(v: Option<&Value>) -> Option<u32> {
    int(v)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| is_sides(n))
}

fn str_of(v: Option<&Value>) -> Option<&str> {
    v?.as_str()
}

impl Default for State {
    fn default() -> Self {
        Self::load(None)
    }
}

impl State {
    pub fn load(json: Option<&str>) -> Self {
        Self::load_restored(json).0
    }

    /// Parses a stored round and repairs everything the UI relies on. Missing or broken data starts a
    /// fresh round of five D6 (`false`: the caller rolls it).
    pub fn load_restored(json: Option<&str>) -> (Self, bool) {
        let saved: Value = json
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or(Value::Null);
        let restored = saved
            .get("dice")
            .and_then(Value::as_array)
            .is_some_and(|d| d.len() <= MAX_DICE);
        let s = if restored {
            saved
        } else {
            serde_json::json!({ "dice": DEFAULT_DICE.map(|sides| serde_json::json!({ "sides": sides })) })
        };
        let dice: Vec<Die> = s["dice"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(load_die)
            .collect();
        let sets = s
            .get("sets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(load_set)
            .collect();
        let mut state = State {
            sets,
            last_tray: str_of(s.get("lastTray"))
                .filter(|t| is_tray(t))
                .unwrap_or_default()
                .to_owned(),
            surface: str_of(s.get("surface"))
                .filter(|x| SURFACES.contains(x))
                .unwrap_or_default()
                .to_owned(),
            sound: s.get("sound") != Some(&Value::Bool(false)),
            click_mode: if str_of(s.get("clickMode")) == Some("select") {
                "select"
            } else {
                "direct"
            }
            .to_owned(),
            rerolls: int(s.get("rerolls"))
                .filter(|&n| n > 0)
                .map_or(0, |n| n.min(u32::MAX as i64) as u32),
            view: if str_of(s.get("view")) == Some("2d") {
                "2d"
            } else {
                "3d"
            }
            .to_owned(),
            tilt: s.get("tilt") == Some(&Value::Bool(true)),
            shake: s.get("shake") == Some(&Value::Bool(true)),
            dice,
            next_id: 1,
        };
        for d in &mut state.dice {
            d.id = state.next_id;
            state.next_id += 1;
        }
        state.assign_slots();
        (state, restored)
    }

    fn new_die(
        &mut self,
        sides: u32,
        color: Option<&str>,
        numbered: bool,
        story: Option<&str>,
    ) -> Die {
        let id = self.next_id;
        self.next_id += 1;
        let mut d = Die {
            id,
            sides,
            value: random(sides),
            color: color
                .filter(|c| COLORS.contains(c))
                .unwrap_or_default()
                .to_owned(),
            numbered: false,
            story: None,
            tray: None,
            slot: None,
        };
        match story.filter(|s| sides == 6 && is_story(s)) {
            Some(s) => {
                d.story = Some(s.to_owned());
                d.color = "white".to_owned();
            }
            None => d.numbered = sides == 6 && numbered,
        }
        d
    }

    /// Puts a fresh die on the table; returns its id, None if the table is full or the die unknown.
    pub fn add(
        &mut self,
        sides: u32,
        color: &str,
        numbered: bool,
        story: Option<&str>,
    ) -> Option<u32> {
        if self.dice.len() >= MAX_DICE || !is_sides(sides) {
            return None;
        }
        let d = self.new_die(sides, Some(color), numbered, story);
        let id = d.id;
        self.dice.push(d);
        self.assign_slots();
        Some(id)
    }

    pub fn remove(&mut self, id: u32) {
        self.dice.retain(|d| d.id != id);
    }

    /// Tray `Some("t1"|"b1")`: moves the die there, right before `before` or at the end (tray order is
    /// the order in `dice`) and remembers the tray. `None`/"" returns it to the table.
    pub fn move_die(&mut self, id: u32, tray: Option<&str>, before: Option<u32>) {
        let Some(i) = self.dice.iter().position(|d| d.id == id) else {
            return;
        };
        match tray.filter(|t| !t.is_empty()) {
            None => self.dice[i].tray = None,
            Some(t) if is_tray(t) => {
                let mut d = self.dice.remove(i);
                d.tray = Some(t.to_owned());
                let at = before
                    .and_then(|b| self.dice.iter().position(|x| x.id == b))
                    .unwrap_or(self.dice.len());
                self.dice.insert(at, d);
                self.last_tray = t.to_owned();
            }
            Some(_) => return,
        }
        self.assign_slots();
    }

    /// Tray for a direct tap: the last used one, initially the bottom tray.
    pub fn next_tray(&self) -> &str {
        if self.last_tray.is_empty() {
            "b1"
        } else {
            &self.last_tray
        }
    }

    /// Dice of the next roll: the table dice, or all of them once every die is in a tray.
    pub fn rollable_ids(&self) -> Vec<u32> {
        let all = self.dice.iter().all(|d| d.tray.is_some());
        self.dice
            .iter()
            .filter(|d| all || d.tray.is_none())
            .map(|d| d.id)
            .collect()
    }

    /// Starts a roll and returns its ids in table order. 2D draws fresh crypto values; 3D keeps
    /// the committed results until physics lands. All dice in trays starts a new round.
    /// Rolled dice get slots 0..n-1 (shuffled in 2D), tray dice lose theirs.
    pub fn roll(&mut self) -> Vec<u32> {
        let mut ids = self.rollable_ids();
        if self.dice.iter().all(|d| d.tray.is_some()) {
            self.dice.iter_mut().for_each(|d| d.tray = None);
        }
        // Dice left in a tray → reroll since the last roll with all dice.
        self.rerolls = if self.dice.iter().any(|d| d.tray.is_some()) {
            self.rerolls.saturating_add(1)
        } else {
            0
        };
        if self.view == "2d" {
            shuffle(&mut ids);
        }
        for d in &mut self.dice {
            match ids.iter().position(|&id| id == d.id) {
                Some(slot) => {
                    d.slot = Some(slot);
                    if self.view == "2d" {
                        d.value = random(d.sides);
                    }
                }
                None => d.slot = None,
            }
        }
        ids
    }

    pub fn set_value(&mut self, id: u32, value: u32) {
        if let Some(d) = self
            .dice
            .iter_mut()
            .find(|d| d.id == id && (1..=d.sides).contains(&value))
        {
            d.value = value;
        }
    }

    pub fn return_all(&mut self) {
        self.dice.iter_mut().for_each(|d| d.tray = None);
        self.assign_slots();
    }

    /// Replaces the table with fresh dice of the set (the caller rolls next).
    pub fn use_set(&mut self, set: &DiceSet) {
        let dice: Vec<Die> = set
            .dice
            .iter()
            .take(MAX_DICE)
            .enumerate()
            .filter(|(_, s)| is_sides(**s))
            .map(|(i, &sides)| {
                let color = set.colors.get(i).cloned().flatten();
                let numbered = set.numbered.get(i).copied().flatten() == Some(true);
                let story = set.stories.get(i).cloned().flatten();
                self.new_die(sides, color.as_deref(), numbered, story.as_deref())
            })
            .collect();
        self.dice = dice;
        self.assign_slots();
    }

    /// Saves the table as the first own set; an empty name falls back to `current_name`.
    pub fn save_set(&mut self, name: &str, lang: &str) {
        if self.dice.is_empty() {
            return;
        }
        let name = name.trim();
        let mut numbered: Vec<Option<bool>> = self
            .dice
            .iter()
            .map(|d| (d.sides == 6 && d.numbered).then_some(true))
            .collect();
        let mut stories: Vec<Option<String>> = self.dice.iter().map(|d| d.story.clone()).collect();
        if all_none(&numbered) {
            numbered.clear();
        }
        if all_none(&stories) {
            stories.clear();
        }
        let set = DiceSet {
            name: if name.is_empty() {
                self.current_name(lang)
            } else {
                name.to_owned()
            },
            dice: self.dice.iter().map(|d| d.sides).collect(),
            colors: self
                .dice
                .iter()
                .map(|d| (!d.color.is_empty()).then(|| d.color.clone()))
                .collect(),
            numbered,
            stories,
        };
        self.sets.insert(0, set);
    }

    /// Name from the table, e.g. "Stories · Classic · 3×D6 · D20".
    pub fn current_name(&self, lang: &str) -> String {
        let story = if self.dice.iter().any(|d| d.story.is_some()) {
            t(lang, "Stories · Classic")
        } else {
            String::new()
        };
        let sides: Vec<u32> = self
            .dice
            .iter()
            .filter(|d| d.story.is_none())
            .map(|d| d.sides)
            .collect();
        [story, dice_name(lang, &sides)]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Table slots (2D): dice on the table without a slot (new, or back from a tray after a roll) are
    /// appended behind the highest slot; tray dice keep theirs free until the next roll.
    pub fn assign_slots(&mut self) {
        let mut next = self
            .dice
            .iter()
            .filter_map(|d| d.slot)
            .max()
            .map_or(0, |s| s + 1);
        for d in self
            .dice
            .iter_mut()
            .filter(|d| d.tray.is_none() && d.slot.is_none())
        {
            d.slot = Some(next);
            next += 1;
        }
    }
}

fn load_die(v: &Value) -> Option<Die> {
    let sides = sides_of(v.get("sides"))?;
    let value = int(v.get("value"))
        .filter(|&n| n >= 1 && n <= sides as i64)
        .map_or_else(|| random(sides), |n| n as u32);
    let story = str_of(v.get("story"))
        .filter(|s| sides == 6 && is_story(s))
        .map(str::to_owned);
    Some(Die {
        id: 0,
        sides,
        value,
        color: str_of(v.get("color"))
            .filter(|c| COLORS.contains(c))
            .unwrap_or_default()
            .to_owned(),
        numbered: sides == 6 && story.is_none() && v.get("numbered") == Some(&Value::Bool(true)),
        story,
        tray: str_of(v.get("tray"))
            .filter(|t| is_tray(t))
            .map(str::to_owned),
        slot: int(v.get("slot")).filter(|&n| n >= 0).map(|n| n as usize),
    })
}

fn load_set(v: &Value) -> Option<DiceSet> {
    let name = str_of(v.get("name"))?.to_owned();
    let raw = v.get("dice")?.as_array()?;
    if raw.is_empty() || raw.len() > MAX_DICE {
        return None;
    }
    let dice: Vec<u32> = raw
        .iter()
        .map(|s| sides_of(Some(s)))
        .collect::<Option<_>>()?;
    let at = |key: &str, i: usize| v.get(key).and_then(|a| a.get(i));
    let mut stories: Vec<Option<String>> = dice
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            str_of(at("stories", i))
                .filter(|x| s == 6 && is_story(x))
                .map(str::to_owned)
        })
        .collect();
    let mut numbered: Vec<Option<bool>> = dice
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            (s == 6 && stories[i].is_none() && at("numbered", i) == Some(&Value::Bool(true)))
                .then_some(true)
        })
        .collect();
    let colors = (0..dice.len())
        .map(|i| {
            str_of(at("colors", i))
                .filter(|c| COLORS.contains(c))
                .map(str::to_owned)
        })
        .collect();
    if all_none(&numbered) {
        numbered.clear();
    }
    if all_none(&stories) {
        stories.clear();
    }
    Some(DiceSet {
        name,
        dice,
        colors,
        numbered,
        stories,
    })
}

/// Unbiased crypto random number 1..=sides (rejection sampling avoids modulo bias).
pub fn random(sides: u32) -> u32 {
    assert!(sides > 0, "a die needs sides");
    let range = 1u64 << 32;
    let limit = range - range % sides as u64;
    loop {
        let mut buf = [0u8; 4];
        getrandom::fill(&mut buf).expect("crypto random source unavailable");
        let x = u32::from_le_bytes(buf) as u64;
        if x < limit {
            return (x % sides as u64) as u32 + 1;
        }
    }
}

/// Fresh numbering for a throw: geometric face k (index k-1) shows map[k-1]. Opposite-number pairs
/// {v, sides+1-v} are Fisher-Yates shuffled onto geometric pairs {k, sides+1-k} and each flipped by a
/// fair coin, so every face is uniform over 1..=sides independent of how physics lands the die.
pub fn random_face_values(sides: u32) -> Vec<u32> {
    assert!(is_sides(sides), "unsupported die D{sides}");
    let half = (sides / 2) as usize;
    let mut pairs: Vec<u32> = (1..=half as u32).collect();
    for i in (1..half).rev() {
        pairs.swap(i, random(i as u32 + 1) as usize - 1);
    }
    let mut map = vec![0; sides as usize];
    for k in 1..=half {
        let p = pairs[k - 1];
        let v = if random(2) == 1 { p } else { sides + 1 - p };
        map[k - 1] = v;
        map[sides as usize - k] = sides + 1 - v;
    }
    map
}

/// Fisher–Yates, in place, crypto random.
pub fn shuffle<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        items.swap(i, random(i as u32 + 1) as usize - 1);
    }
}

/// Sum of the numeric dice; story dice show pictures and count nothing.
pub fn total(dice: &[Die]) -> u32 {
    dice.iter()
        .filter(|d| !d.story.as_deref().is_some_and(is_story))
        .map(|d| d.value)
        .sum()
}

/// Built-in sets, names localized.
pub fn presets(lang: &str) -> Vec<DiceSet> {
    let d6 = die_type(lang, 6);
    let colors =
        |c: [&str; 6]| -> Vec<Option<String>> { c.iter().map(|c| Some(c.to_string())).collect() };
    let set = |name: String, dice: Vec<u32>| DiceSet {
        name,
        dice,
        colors: vec![],
        numbered: vec![],
        stories: vec![],
    };
    vec![
        set(format!("{} · 5×{d6}", t(lang, "Yahtzee")), vec![6; 5]),
        DiceSet {
            colors: colors(["blue", "yellow", "green", "orange", "purple", "white"]),
            ..set(
                format!("{} · 6×{d6}", t(lang, "That's Pretty Clever")),
                vec![6; 6],
            )
        },
        DiceSet {
            colors: colors(["white", "brown", "blue", "turquoise", "pink", "yellow"]),
            // Game title, the same in every language.
            ..set(format!("Clever³ · 6×{d6}"), vec![6; 6])
        },
        DiceSet {
            numbered: vec![None, Some(true)],
            ..set(
                format!(
                    "{} · {}–{}",
                    t(lang, "Roleplaying"),
                    die_type(lang, 4),
                    die_type(lang, 20)
                ),
                vec![4, 6, 8, 10, 12, 20],
            )
        },
        DiceSet {
            stories: STORY_DICE
                .iter()
                .map(|(id, _)| Some(id.to_string()))
                .collect(),
            ..set(t(lang, "Stories · Classic"), vec![6; 9])
        },
    ]
}

/// Story dice inspired by the classic nine-die picture set: six fixed motifs each (art in icons/story/<motif>.svg).
pub const STORY_DICE: &[(&str, [&str; 6])] = &[
    (
        "classic-1",
        ["house", "lightbulb", "sleep", "speech", "clock", "arrow"],
    ),
    (
        "classic-2",
        ["lock", "footprint", "flame", "sheep", "magnet", "learner"],
    ),
    (
        "classic-3",
        ["message", "pyramid", "tower", "rainbow", "tree", "eye"],
    ),
    (
        "classic-4",
        ["earth", "plane", "smile", "building", "flashlight", "apple"],
    ),
    (
        "classic-5",
        [
            "key",
            "shooting-star",
            "question",
            "tent",
            "spinning-object",
            "frown",
        ],
    ),
    (
        "classic-6",
        ["beetle", "die", "magnifier", "shadow", "hand", "turtle"],
    ),
    (
        "classic-7",
        [
            "masks",
            "fish",
            "parachute",
            "keyhole",
            "sunflower",
            "arrows",
        ],
    ),
    (
        "classic-8",
        ["book", "bridge", "abacus", "moon", "wand", "bee"],
    ),
    (
        "classic-9",
        ["alien", "lightning", "phone", "scales", "direction", "cane"],
    ),
];

/// English motif labels (t() keys).
const STORY_LABELS: &[(&str, &str)] = &[
    ("house", "House"),
    ("lightbulb", "Light bulb"),
    ("sleep", "Sleeping face"),
    ("speech", "Speech bubble"),
    ("clock", "Clock"),
    ("arrow", "Feathered arrow"),
    ("lock", "Padlock"),
    ("footprint", "Footprint"),
    ("flame", "Flame"),
    ("sheep", "Sheep"),
    ("magnet", "Magnet"),
    ("learner", "Learner plate"),
    ("message", "Message card"),
    ("pyramid", "Pyramid"),
    ("tower", "Tower"),
    ("rainbow", "Rainbow"),
    ("tree", "Tree"),
    ("eye", "Eye"),
    ("earth", "Globe"),
    ("plane", "Airplane"),
    ("smile", "Smiling face"),
    ("building", "Skyscraper"),
    ("flashlight", "Flashlight"),
    ("apple", "Apple"),
    ("key", "Key"),
    ("shooting-star", "Shooting star"),
    ("question", "Question mark"),
    ("tent", "Tent"),
    ("spinning-object", "Spinning object"),
    ("frown", "Sad face"),
    ("beetle", "Beetle"),
    ("die", "Die"),
    ("magnifier", "Magnifying glass"),
    ("shadow", "Shadow"),
    ("hand", "Hand"),
    ("turtle", "Turtle"),
    ("masks", "Theatre masks"),
    ("fish", "Fish"),
    ("parachute", "Parachute"),
    ("keyhole", "Keyhole"),
    ("sunflower", "Sunflower"),
    ("arrows", "Outward arrows"),
    ("book", "Book"),
    ("bridge", "Bridge"),
    ("abacus", "Abacus"),
    ("moon", "Crescent moon"),
    ("wand", "Magic wand"),
    ("bee", "Bee"),
    ("alien", "Alien"),
    ("lightning", "Lightning"),
    ("phone", "Mobile phone"),
    ("scales", "Scales"),
    ("direction", "Direction arrow"),
    ("cane", "Walking stick"),
];

pub fn is_story(id: &str) -> bool {
    STORY_DICE.iter().any(|(s, _)| *s == id)
}

/// Motif id and English label on side `value` (1..=6) of story die `story`; None for anything invalid.
pub fn story_face(story: &str, value: u32) -> Option<(&'static str, &'static str)> {
    let (_, faces) = STORY_DICE.iter().find(|(s, _)| *s == story)?;
    let motif = *faces.get((value as usize).checked_sub(1)?)?;
    STORY_LABELS.iter().find(|(id, _)| *id == motif).copied()
}

/// English is the source language; translations.json (converted from i18n.js) translates it.
static TRANSLATIONS: LazyLock<HashMap<String, HashMap<String, String>>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("translations.json")).expect("valid translations.json")
});

pub fn t(lang: &str, text: &str) -> String {
    TRANSLATIONS
        .get(lang)
        .and_then(|table| table.get(text))
        .cloned()
        .unwrap_or_else(|| text.to_owned())
}

/// A manual choice wins; "auto" uses the first supported browser preference, otherwise English.
pub fn choose_language(preference: &str, languages: &[String]) -> String {
    if LANGUAGES.contains(&preference) {
        return preference.to_owned();
    }
    languages
        .iter()
        .map(|l| {
            l.to_lowercase()
                .split('-')
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .find(|l| LANGUAGES.contains(&l.as_str()))
        .unwrap_or_else(|| "en".to_owned())
}

pub fn die_type(lang: &str, sides: u32) -> String {
    format!("{}{sides}", t(lang, "D"))
}

/// Name from the die types in dice order, e.g. "3×D6 · D20".
pub fn dice_name(lang: &str, sides: &[u32]) -> String {
    let mut counts: Vec<(u32, usize)> = Vec::new();
    for &s in sides {
        match counts.iter_mut().find(|(x, _)| *x == s) {
            Some((_, n)) => *n += 1,
            None => counts.push((s, 1)),
        }
    }
    counts
        .iter()
        .map(|&(s, n)| {
            if n > 1 {
                format!("{n}×{}", die_type(lang, s))
            } else {
                die_type(lang, s)
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_state_roll_and_tray_invariants() {
        // A legacy round (as app.js saved it) survives load → save unchanged.
        let round = json!({
            "dice": [{ "sides": 20, "value": 17, "tray": "t1", "color": "red", "slot": 2 }, { "sides": 6, "value": 3, "slot": 0 }],
            "sets": [{ "name": "Mine", "dice": [6, 20], "colors": ["blue", null] }],
            "lastTray": "t1", "surface": "oak", "sound": false, "clickMode": "select", "rerolls": 2, "view": "3d", "tilt": true, "shake": true,
        });
        let (state, restored) = State::load_restored(Some(&round.to_string()));
        assert!(restored);
        assert_eq!(serde_json::to_value(&state).unwrap(), round);

        // Corrupted data is repaired or dropped, never fatal; too many dice start a fresh round.
        let broken = json!({ "dice": [{ "sides": 6, "value": 9 }, { "sides": 0 }, { "sides": 6, "value": 2, "color": "\"><img>", "tray": "x", "slot": -1 }, null],
            "sets": [{ "name": "ok", "dice": [6], "colors": ["nope"] }, { "name": "bad", "dice": [1] }], "view": "webgl", "tilt": "true" });
        let repaired = State::load(Some(&broken.to_string()));
        assert_eq!(repaired.dice.len(), 2);
        assert!((1..=6).contains(&repaired.dice[0].value));
        assert_eq!(
            (
                repaired.dice[1].value,
                repaired.dice[1].color.as_str(),
                repaired.dice[1].tray.as_deref()
            ),
            (2, "", None)
        );
        assert_eq!(
            repaired.sets,
            vec![DiceSet {
                name: "ok".into(),
                dice: vec![6],
                colors: vec![None],
                numbered: vec![],
                stories: vec![]
            }]
        );
        assert_eq!((repaired.view.as_str(), repaired.tilt), ("3d", false));
        for json in [
            None,
            Some("{"),
            Some(r#"{"dice":[1,2,3,4,5,6,7,8,9,10,11,12,13]}"#),
        ] {
            let (fresh, restored) = State::load_restored(json);
            assert!(!restored && fresh.dice.len() == 5 && fresh.sound);
        }

        // Tray moves keep values and slots; a roll rolls only table dice and counts rerolls.
        let mut s = State::load(Some(r#"{"dice":[]}"#));
        let a = s.add(6, "blue", false, None).unwrap();
        let b = s.add(20, "", false, None).unwrap();
        let c = s.add(6, "", false, Some("classic-9")).unwrap();
        s.move_die(b, Some("t1"), None);
        s.move_die(a, Some("t1"), Some(b));
        let kept = s.dice.iter().find(|d| d.id == b).unwrap().clone();
        assert_eq!(
            s.dice.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![c, a, b]
        );
        assert_eq!((s.next_tray(), kept.slot), ("t1", Some(1)));
        assert_eq!(s.roll(), vec![c]);
        assert_eq!(s.rerolls, 1);
        let rolled = s.dice.iter().find(|d| d.id == c).unwrap();
        assert!((1..=6).contains(&rolled.value) && rolled.slot == Some(0));
        assert!(s
            .dice
            .iter()
            .filter(|d| d.tray.is_some())
            .all(|d| d.slot.is_none()));
        assert_eq!(s.dice.iter().find(|d| d.id == b).unwrap().value, kept.value);
        // Story dice show pictures: excluded from the total.
        assert_eq!(
            total(&s.dice),
            s.dice
                .iter()
                .filter(|d| d.id != c)
                .map(|d| d.value)
                .sum::<u32>()
        );
        // Everything in trays → the next roll is a new round with all dice.
        s.move_die(c, Some("b1"), None);
        let mut all = s.roll();
        all.sort();
        assert_eq!((all, s.rerolls), (vec![a, b, c], 0));
        assert!(s.dice.iter().all(|d| d.tray.is_none() && d.slot.is_some()));

        // Own sets round-trip through storage, including colors and stories.
        s.save_set("  ", "de");
        assert_eq!(s.sets[0].name, "Geschichten · Classic · W6 · W20");
        let reloaded = State::load(Some(&serde_json::to_string(&s).unwrap()));
        assert_eq!(reloaded.sets, s.sets);
        let mut used = State::load(Some(r#"{"dice":[]}"#));
        used.use_set(&reloaded.sets[0]);
        let shape = |s: &State| {
            s.dice
                .iter()
                .map(|d| (d.sides, d.color.clone(), d.story.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(&used), shape(&s));

        // Throw numberings put opposite numbers on opposite faces, each value exactly once.
        for &sides in SIDES {
            let map = random_face_values(sides);
            let mut sorted = map.clone();
            sorted.sort();
            assert_eq!(sorted, (1..=sides).collect::<Vec<_>>());
            assert!((0..sides as usize).all(|k| map[k] + map[sides as usize - 1 - k] == sides + 1));
        }
    }
}

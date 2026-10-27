#[cfg(target_arch = "wasm32")]
mod device;
pub mod geometry;
pub mod gpu;
pub mod model;
pub mod physics;
#[cfg(target_arch = "wasm32")]
mod platform;
#[cfg(target_arch = "wasm32")]
mod ui;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&info.to_string().into())
    }));
    dioxus::launch(ui::App);
}

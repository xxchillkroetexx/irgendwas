//! WASM entry point: hydrates the server-rendered HTML.

#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
    // Marker for end-to-end tests (`waitForHydration`): event handlers are attached now.
    // `<html>` is rendered by the server shell only, so this cannot cause a mismatch.
    if let Some(root) = leptos::prelude::document().document_element() {
        let _ = root.set_attribute("data-hydrated", "");
    }
}

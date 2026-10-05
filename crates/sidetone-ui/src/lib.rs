//! Dear ImGui inside X-Plane.
//!
//! X-Plane 12 on macOS renders with Metal and offers plugins only a legacy (2.1-class) OpenGL
//! compatibility context, so the renderer uses fixed-function client arrays — the same approach
//! as Dear ImGui's own `imgui_impl_opengl2`.

mod gl;
pub mod host;
mod renderer;
pub mod theme;

pub use dear_imgui_rs as imgui;
pub use host::{HostFrame, ImguiWindow, WindowKind};
pub use theme::Fonts;

/// Copies text to the macOS clipboard (for "Copy" buttons).
pub fn copy_to_clipboard(text: &str) {
    if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())) {
        log::warn!("Clipboard write failed: {e}");
    }
}

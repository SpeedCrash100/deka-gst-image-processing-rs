//!
//! Gstreamer WGPU interop structures
//!

mod gst_wgpu_context;
mod gst_wgpu_memory;

use std::sync::LazyLock;

pub use gst_wgpu_context::WgpuContext;
pub use gst_wgpu_context::GST_CONTEXT_WGPU_TYPE;

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "gstwgpu",
        gst::DebugColorFlags::empty(),
        Some("Gstreamer WGPU interop structures"),
    )
});

macro_rules! skip_assert_initialized {
    () => {};
}

use skip_assert_initialized;

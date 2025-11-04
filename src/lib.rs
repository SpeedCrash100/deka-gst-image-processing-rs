extern crate gstreamer as gst;
extern crate gstreamer_base as gst_base;
extern crate gstreamer_video as gst_video;

mod cpu_sobel;
mod wgpu_copy;
mod wgpu_env;
mod wgpu_sobel_simple;

use gst::glib;

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    cpu_sobel::register(plugin)?;
    wgpu_copy::register(plugin)?;
    wgpu_sobel_simple::register(plugin)?;
    Ok(())
}

gst::plugin_define!(
    deka_image_processing_rs,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    concat!(env!("CARGO_PKG_VERSION"), "-", env!("COMMIT_ID")),
    "MIT/X11",
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_REPOSITORY"),
    env!("BUILD_REL_DATE")
);

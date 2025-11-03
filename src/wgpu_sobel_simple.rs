mod imp;

use gst::glib;
use gst::prelude::*;

glib::wrapper! {
    pub struct WgpuSobelSimple(ObjectSubclass<imp::WgpuSobelSimple>) @extends gst_video::VideoFilter, gst_base::BaseTransform, gst::Element, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "dekawgpusobelsimple",
        gst::Rank::NONE,
        WgpuSobelSimple::static_type(),
    )
}

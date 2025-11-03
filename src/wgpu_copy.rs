mod imp;

use gst::glib;
use gst::prelude::*;

glib::wrapper! {
    pub struct WgpuCopy(ObjectSubclass<imp::WgpuCopy>) @extends gst_video::VideoFilter, gst_base::BaseTransform, gst::Element, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "dekawgpucopy",
        gst::Rank::NONE,
        WgpuCopy::static_type(),
    )
}

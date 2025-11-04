mod imp;

use gst::glib;
use gst::prelude::*;

glib::wrapper! {
    pub struct WgpuTextureCopy(ObjectSubclass<imp::WgpuTextureCopy>) @extends gst_video::VideoFilter, gst_base::BaseTransform, gst::Element, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "dekawgputexturecopy",
        gst::Rank::NONE,
        WgpuTextureCopy::static_type(),
    )
}

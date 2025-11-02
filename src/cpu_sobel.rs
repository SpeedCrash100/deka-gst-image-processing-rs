mod imp;

use gst::glib;
use gst::prelude::*;

glib::wrapper! {
    pub struct CpuSobel(ObjectSubclass<imp::CpuSobel>) @extends gst_video::VideoFilter, gst_base::BaseTransform, gst::Element, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "dekacpusobel",
        gst::Rank::NONE,
        CpuSobel::static_type(),
    )
}

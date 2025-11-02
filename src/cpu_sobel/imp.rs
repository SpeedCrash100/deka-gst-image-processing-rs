use std::sync::LazyLock;
use std::time::Instant;

use crate::glib;
use glib::prelude::*;
use gst::glib::subclass::prelude::*;
use gst::glib::subclass::{object::ObjectImpl, types::ObjectSubclass};
use gst::subclass::prelude::*;
use gst_base::subclass::prelude::BaseTransformImpl;
use gst_base::subclass::BaseTransformMode;
use gst_video::subclass::prelude::VideoFilterImpl;

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "dekacpusobel",
        gst::DebugColorFlags::empty(),
        Some("Deka's Sobel Filter on CPU"),
    )
});

#[derive(Debug)]
pub struct CpuSobel {}

impl CpuSobel {}

#[glib::object_subclass]
impl ObjectSubclass for CpuSobel {
    const NAME: &'static str = "GstCpuSobel";
    type Type = super::CpuSobel;
    type ParentType = gst_video::VideoFilter;

    fn with_class(_klass: &Self::Class) -> Self {
        Self {}
    }
}

impl ObjectImpl for CpuSobel {}
impl GstObjectImpl for CpuSobel {}
impl ElementImpl for CpuSobel {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
            gst::subclass::ElementMetadata::new(
                "Deka's Sobel Filter on CPU",
                "Filter/Effect/Video",
                "Applies a sobel filter to the input video frame",
                "Deka <speedcrash100@ya.ru>",
            )
        });
        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            let caps = gst_video::VideoCapsBuilder::new()
                .format(gst_video::VideoFormat::Rgba)
                .build();
            vec![
                gst::PadTemplate::new(
                    "src",
                    gst::PadDirection::Src,
                    gst::PadPresence::Always,
                    &caps,
                )
                .unwrap(),
                gst::PadTemplate::new(
                    "sink",
                    gst::PadDirection::Sink,
                    gst::PadPresence::Always,
                    &caps,
                )
                .unwrap(),
            ]
        });
        PAD_TEMPLATES.as_ref()
    }
}

impl BaseTransformImpl for CpuSobel {
    const MODE: BaseTransformMode = BaseTransformMode::NeverInPlace;
    const PASSTHROUGH_ON_SAME_CAPS: bool = false;
    const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;
}

impl VideoFilterImpl for CpuSobel {
    fn transform_frame(
        &self,
        inframe: &gst_video::VideoFrameRef<&gst::BufferRef>,
        outframe: &mut gst_video::VideoFrameRef<&mut gst::BufferRef>,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        // TODO: make sobel here
        let start = Instant::now();
        inframe.copy(outframe).unwrap();
        let elapsed = start.elapsed();
        gst::debug!(CAT, imp: self, "processed in {} ms", 1_000.0 * elapsed.as_secs_f64());

        Ok(gst::FlowSuccess::Ok)
    }
}

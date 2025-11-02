use std::sync::LazyLock;
use std::time::Instant;

use crate::glib;
use gst::glib::subclass::prelude::*;
use gst::glib::subclass::{object::ObjectImpl, types::ObjectSubclass};
use gst::subclass::prelude::*;
use gst_base::subclass::prelude::BaseTransformImpl;
use gst_base::subclass::BaseTransformMode;
use gst_video::subclass::prelude::VideoFilterImpl;
use gst_video::VideoFrameExt;

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
                .format(gst_video::VideoFormat::Rgbx)
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

        // We will process line by line starting from the second
        // let in_width = inframe.width() as usize;
        let in_stride = inframe.plane_stride()[0] as usize;
        let in_height = inframe.height() as usize;
        let out_stride = outframe.plane_stride()[0] as usize;

        const MATRIX: [[i16; 3]; 3] = [
            [-1, 0, 1], //
            [-1, 0, 1], //
            [-1, 0, 1], //
        ];

        let plane_data = inframe.plane_data(0).unwrap();
        let out_data = outframe.plane_data_mut(0).unwrap();

        for line in 1..(in_height - 1) {
            let prev_line = line - 1;
            let next_line = line + 1;
            let line_offset = in_stride * line;
            let prev_line_offset = in_stride * prev_line;
            let next_line_offset = in_stride * next_line;

            let channels = 4;
            let window_size = channels * 3; // Sobel uses 3 x 3 window, but we working with color images, we fetch all colors at once

            let line_data = &plane_data[line_offset..line_offset + in_stride];
            let line_windows = line_data.windows(window_size).step_by(channels);
            let prev_line_data = &plane_data[prev_line_offset..prev_line_offset + in_stride];
            let prev_line_windows = prev_line_data.windows(window_size).step_by(channels);
            let next_line_data = &plane_data[next_line_offset..next_line_offset + in_stride];
            let next_line_windows = next_line_data.windows(window_size).step_by(channels);

            let iter = prev_line_windows
                .zip(line_windows)
                .zip(next_line_windows)
                .map(|x| {
                    WindowDataRgbx {
                        kernel: &MATRIX,
                        prev_line_window: x.0 .0,
                        line_window: x.0 .1,
                        next_line_window: x.1,
                    }
                    .convolve()
                });

            let out_line_data = &mut out_data[line_offset..line_offset + out_stride];

            for (col_shifted, color) in iter.enumerate() {
                let col = col_shifted + 1;
                let write_pos = col * channels;
                out_line_data[write_pos] = color.0;
                out_line_data[write_pos + 1] = color.1;
                out_line_data[write_pos + 2] = color.2;
            }
        }

        let elapsed = start.elapsed();
        gst::debug!(CAT, imp: self, "processed in {} ms", 1_000.0 * elapsed.as_secs_f64());

        Ok(gst::FlowSuccess::Ok)
    }
}

struct WindowDataRgbx<'a> {
    kernel: &'a [[i16; 3]; 3],
    prev_line_window: &'a [u8],
    line_window: &'a [u8],
    next_line_window: &'a [u8],
}

impl WindowDataRgbx<'_> {
    #[inline(always)]
    fn element_wise(&self, line: &[u8], kernel_line: &[i16]) -> (i16, i16, i16) {
        const CHANNEL: usize = 4; // RGBx

        let r_base = 0;
        let g_base = 1;
        let b_base = 2;
        // x_base = 3; unused

        let r = line[r_base] as i16 * kernel_line[0]
            + line[r_base + CHANNEL] as i16 * kernel_line[1]
            + line[r_base + CHANNEL * 2] as i16 * kernel_line[2];

        let g = line[g_base] as i16 * kernel_line[0]
            + line[g_base + CHANNEL] as i16 * kernel_line[1]
            + line[g_base + CHANNEL * 2] as i16 * kernel_line[2];

        let b = line[b_base] as i16 * kernel_line[0]
            + line[b_base + CHANNEL] as i16 * kernel_line[1]
            + line[b_base + CHANNEL * 2] as i16 * kernel_line[2];

        (r, g, b)
    }

    fn convolve(&self) -> (u8, u8, u8) {
        let (prev_r, prev_g, prev_b) = self.element_wise(&self.prev_line_window, &self.kernel[0]);
        let (curr_r, curr_g, curr_b) = self.element_wise(&self.line_window, &self.kernel[1]);
        let (next_r, next_g, next_b) = self.element_wise(&self.next_line_window, &self.kernel[2]);

        (
            (prev_r + curr_r + next_r).abs() as u8,
            (prev_g + curr_g + next_g).abs() as u8,
            (prev_b + curr_b + next_b).abs() as u8,
        )
    }
}

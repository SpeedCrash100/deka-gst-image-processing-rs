use std::sync::LazyLock;
use std::time::Instant;

use crate::glib;
use gst::glib::subclass::prelude::*;
use gst::glib::subclass::{object::ObjectImpl, types::ObjectSubclass};
use gst::subclass::prelude::*;
use gst_base::subclass::prelude::BaseTransformImpl;
use gst_base::subclass::BaseTransformMode;
use gst_video::subclass::prelude::VideoFilterImpl;
use parking_lot::Mutex;
use pollster::FutureExt;

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "dekawgpucopy",
        gst::DebugColorFlags::empty(),
        Some("Deka's WebGPU copy"),
    )
});

#[derive(Debug)]
struct WebGPUState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    input_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
}

#[derive(Debug)]
pub struct WgpuCopy {
    pipeline: Mutex<Option<WebGPUState>>,
}

impl WgpuCopy {}

#[glib::object_subclass]
impl ObjectSubclass for WgpuCopy {
    const NAME: &'static str = "GstWgpuCopy";
    type Type = super::WgpuCopy;
    type ParentType = gst_video::VideoFilter;

    fn with_class(_klass: &Self::Class) -> Self {
        Self {
            pipeline: Mutex::new(None),
        }
    }
}

impl ObjectImpl for WgpuCopy {}
impl GstObjectImpl for WgpuCopy {}
impl ElementImpl for WgpuCopy {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
            gst::subclass::ElementMetadata::new(
                "Deka's WebGPU copy and back",
                "Filter/Effect/Video",
                "Copies frame to GPU and back, using WebGPU",
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

impl BaseTransformImpl for WgpuCopy {
    const MODE: BaseTransformMode = BaseTransformMode::NeverInPlace;
    const PASSTHROUGH_ON_SAME_CAPS: bool = false;
    const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;
}

impl VideoFilterImpl for WgpuCopy {
    fn set_info(
        &self,
        _incaps: &gst::Caps,
        in_info: &gst_video::VideoInfo,
        _outcaps: &gst::Caps,
        out_info: &gst_video::VideoInfo,
    ) -> Result<(), gst::LoggableError> {
        let instance_description = wgpu::InstanceDescriptor::from_env_or_default();

        let instance = wgpu::Instance::new(&instance_description);

        let adapter_fut = instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        });

        let adapter = match adapter_fut.block_on() {
            Ok(adapter) => adapter,
            Err(err) => {
                return Err(gst::loggable_error!(
                    CAT,
                    "Could not find a suitable WebGPU adapter: {}",
                    err
                ));
            }
        };

        let channels = 4; // RGBx
        let in_frame_size = in_info.width() as u64 * in_info.height() as u64 * channels;
        let out_frame_size = out_info.width() as u64 * out_info.height() as u64 * channels;
        let min_length = std::cmp::max(in_frame_size, out_frame_size);

        let device_fut = adapter.request_device(&wgpu::DeviceDescriptor {
            memory_hints: wgpu::MemoryHints::Performance,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits {
                max_buffer_size: min_length,
                ..wgpu::Limits::downlevel_defaults()
            },
            ..Default::default()
        });

        let (device, queue) = match device_fut.block_on() {
            Ok(device) => device,
            Err(err) => {
                return Err(gst::loggable_error!(
                    CAT,
                    "Could not create WebGPU device: {}",
                    err
                ));
            }
        };

        // This buffer will be used to copy the input frame into.
        let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("input frame buffer"),
            mapped_at_creation: true,
            size: in_frame_size,
            usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
        });

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output frame buffer"),
            mapped_at_creation: false,
            size: out_frame_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        });

        {
            let mut pipeline = self.pipeline.lock();
            *pipeline = Some(WebGPUState {
                device,
                queue,
                input_buffer,
                output_buffer,
            })
        }

        Ok(())
    }

    fn transform_frame(
        &self,
        inframe: &gst_video::VideoFrameRef<&gst::BufferRef>,
        outframe: &mut gst_video::VideoFrameRef<&mut gst::BufferRef>,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let start = Instant::now();

        let Some(pipeline) = &*self.pipeline.lock() else {
            return Err(gst::FlowError::NotNegotiated);
        };

        let input_slice = pipeline.input_buffer.slice(..);
        {
            let mut input_mapped = input_slice.get_mapped_range_mut();
            input_mapped.copy_from_slice(inframe.plane_data(0).unwrap());
            gst::debug!(CAT, imp: self, "reached copy to mapped GPU in {} ms", 1_000.0 * start.elapsed().as_secs_f64());
        }

        pipeline.input_buffer.unmap();

        let mut encoder = pipeline.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&pipeline.input_buffer, 0, &pipeline.output_buffer, 0, None);
        let command_buffer = encoder.finish();

        let index = pipeline.queue.submit([command_buffer]);

        let output_slice = pipeline.output_buffer.slice(..);
        output_slice.map_async(wgpu::MapMode::Read, |_| {}); // We depend on poll, so we don't need an callback
        input_slice.map_async(wgpu::MapMode::Write, |_| {}); // We also map the input buffer for next iteration

        if let Err(err) = pipeline.device.poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: None,
        }) {
            gst::error!(CAT, imp: self, "Error submitting command buffer: {}", err);
            return Err(gst::FlowError::Error);
        }

        gst::debug!(CAT, imp: self, "reached copy to in GPU copy finish in {} ms", 1_000.0 * start.elapsed().as_secs_f64());

        // Our submission ready, all buffers should be ready
        {
            let output_mapped = output_slice.get_mapped_range();
            outframe
                .plane_data_mut(0)
                .unwrap()
                .copy_from_slice(&output_mapped);
        }

        pipeline.output_buffer.unmap();

        let elapsed = start.elapsed();
        gst::debug!(CAT, imp: self, "processed in {} ms", 1_000.0 * elapsed.as_secs_f64());

        Ok(gst::FlowSuccess::Ok)
    }
}

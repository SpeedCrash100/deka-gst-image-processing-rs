use std::sync::LazyLock;
use std::time::Instant;

use crate::{glib, wgpu_env};
use gst::glib::subclass::prelude::*;
use gst::glib::subclass::{object::ObjectImpl, types::ObjectSubclass};
use gst::subclass::prelude::*;
use gst_base::subclass::prelude::BaseTransformImpl;
use gst_base::subclass::BaseTransformMode;
use gst_video::subclass::prelude::VideoFilterImpl;
use gst_video::VideoFrameExt;
use parking_lot::Mutex;
use pollster::FutureExt;

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "dekawgpusobelsimple",
        gst::DebugColorFlags::empty(),
        Some("Deka's WebGPU simple sobel filter"),
    )
});

#[derive(Debug)]
struct WebGPUState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    input_buffer: wgpu::Buffer,
    input_texture: wgpu::Texture,
    output_texture: wgpu::Texture,
    output_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
}

#[derive(Debug)]
pub struct WgpuSobelSimple {
    pipeline: Mutex<Option<WebGPUState>>,
}

impl WgpuSobelSimple {}

#[glib::object_subclass]
impl ObjectSubclass for WgpuSobelSimple {
    const NAME: &'static str = "GstWgpuSobelSimple";
    type Type = super::WgpuSobelSimple;
    type ParentType = gst_video::VideoFilter;

    fn with_class(_klass: &Self::Class) -> Self {
        Self {
            pipeline: Mutex::new(None),
        }
    }
}

impl ObjectImpl for WgpuSobelSimple {}
impl GstObjectImpl for WgpuSobelSimple {}
impl ElementImpl for WgpuSobelSimple {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
            gst::subclass::ElementMetadata::new(
                "Deka's WebGPU simple sobel filter",
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

impl BaseTransformImpl for WgpuSobelSimple {
    const MODE: BaseTransformMode = BaseTransformMode::NeverInPlace;
    const PASSTHROUGH_ON_SAME_CAPS: bool = false;
    const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;
}

impl VideoFilterImpl for WgpuSobelSimple {
    fn set_info(
        &self,
        _incaps: &gst::Caps,
        in_info: &gst_video::VideoInfo,
        _outcaps: &gst::Caps,
        out_info: &gst_video::VideoInfo,
    ) -> Result<(), gst::LoggableError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu_env::backend(),
            ..Default::default()
        });

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

        let texture_descriptor = wgpu::TextureDescriptor {
            label: Some("input texture"),
            size: wgpu::Extent3d {
                width: in_info.width(),
                height: in_info.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let input_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("input texture"),
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            ..texture_descriptor
        });

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output texture"),
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            ..texture_descriptor
        });

        let module = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let input_texture_view = input_texture.create_view(&wgpu::TextureViewDescriptor {
            ..Default::default()
        });

        let output_texture_view = output_texture.create_view(&wgpu::TextureViewDescriptor {
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&output_texture_view),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("sobel compute"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("computeSobel"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        {
            let mut pipeline = self.pipeline.lock();
            *pipeline = Some(WebGPUState {
                device,
                queue,
                input_buffer,
                input_texture,
                output_texture,
                output_buffer,
                bind_group,
                pipeline: compute_pipeline,
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
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfoBase {
                buffer: &pipeline.input_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * inframe.width()),
                    rows_per_image: Some(4 * inframe.width() * inframe.height()),
                },
            },
            pipeline.input_texture.as_image_copy(),
            wgpu::Extent3d {
                width: inframe.width(),
                height: inframe.height(),
                depth_or_array_layers: 1,
            },
        );

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                ..Default::default()
            });
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, &pipeline.bind_group, &[]);

            let workgroup_x = inframe.width().div_ceil(8);
            let workgroup_y = inframe.height().div_ceil(8);
            pass.dispatch_workgroups(workgroup_x, workgroup_y, 1);
        }

        encoder.copy_texture_to_buffer(
            pipeline.output_texture.as_image_copy(),
            wgpu::TexelCopyBufferInfoBase {
                buffer: &pipeline.output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * outframe.width()),
                    rows_per_image: Some(4 * outframe.width() * outframe.height()),
                },
            },
            wgpu::Extent3d {
                width: outframe.width(),
                height: outframe.height(),
                depth_or_array_layers: 1,
            },
        );

        let command_buffer = encoder.finish();

        let index = pipeline.queue.submit([command_buffer]);

        let output_slice = pipeline.output_buffer.slice(..);
        output_slice.map_async(wgpu::MapMode::Read, |_| {}); // We depend on poll, so we don't need an callback
        input_slice.map_async(wgpu::MapMode::Write, |_| {}); // We also map the input buffer for next iteration

        if let Err(err) = pipeline.device.poll(wgpu::PollType::wait_for(index)) {
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

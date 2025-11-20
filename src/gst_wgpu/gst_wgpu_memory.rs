mod imp;

use gst::{glib::subclass::types::ObjectSubclassIsExt, subclass::prelude::AllocatorImpl};

use crate::glib::translate::{from_glib, from_glib_full};
use crate::gst::glib::translate::IntoGlibPtr;
use crate::gst_wgpu::skip_assert_initialized;
use crate::{glib, gst_wgpu::WgpuContext};

/// Caps with this feature implies that the buffer is a WGPU buffer.
pub const GST_CAPS_FEATURE_MEMORY_WGPU_BUFFER: &str = "memory:wgpu-buffer";

gst::memory_object_wrapper!(
    WgpuMemory,
    WgpuMemoryRef,
    imp::WgpuMemory,
    |mem: &gst::MemoryRef| { unsafe { from_glib(imp::gst_is_wgpu_memory(mem.as_mut_ptr())) } },
    gst::Memory,
    gst::MemoryRef
);

glib::wrapper! {
    pub struct WgpuMemoryAllocator(ObjectSubclass<imp::WgpuMemoryAllocator>) @extends gst::Allocator, gst::Object;
}

impl WgpuMemoryAllocator {
    /// Crates an allocator that uses specified context for allocating buffers
    pub fn new(context: WgpuContext) -> Self {
        let out: Self = glib::Object::new();

        let imp = out.imp();
        // SAFETY: We set context one time, it does not mutate after creation
        // The creation itself cannot be parallel to be a problem
        unsafe {
            *imp.context.get() = Some(context);
        };

        out
    }

    pub fn alloc(
        &self,
        size: usize,
        params: Option<&gst::AllocationParams>,
    ) -> Result<WgpuMemory, glib::BoolError> {
        let imp = self.imp();
        let base_mem = imp.alloc(size, params)?;
        let wgpu_mem = base_mem
            .downcast_memory::<WgpuMemory>()
            .expect("wgpu alloc returned not wgpu mem");

        Ok(wgpu_mem)
    }
}

#[cfg(test)]
mod tests {

    use std::{hint::black_box, time::Duration};

    use crate::gst_wgpu::{gst_wgpu_memory::WgpuMemoryAllocator, WgpuContext};

    #[test]
    fn alloc() {
        gst::init().unwrap();

        let context = WgpuContext::new(
            &wgpu::RequestAdapterOptions {
                compatible_surface: None,
                ..Default::default()
            },
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            },
            true,
        );

        let allocator = WgpuMemoryAllocator::new(context.clone());
        let mut mem = allocator.alloc(64, None).unwrap();

        let map = mem.get_mut().unwrap().map_writable().unwrap();
        black_box(map);
    }

    #[test]
    fn map_is_works() {
        gst::init().unwrap();

        let context = WgpuContext::new(
            &wgpu::RequestAdapterOptions {
                compatible_surface: None,
                ..Default::default()
            },
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            },
            true,
        );

        let allocator = WgpuMemoryAllocator::new(context.clone());
        let mut in_mem = allocator.alloc(64, None).unwrap();

        // data we pass to the GPU
        let data = &[0xAF_u8; 64];

        {
            let mut map = in_mem.get_mut().unwrap().map_writable().unwrap();
            assert_eq!(map.len(), 64);
            map.copy_from_slice(&data[..]);
        }

        // The buffer should be unmapped now

        let alloc_params = gst::AllocationParams::new(gst::MemoryFlags::READONLY, 0, 0, 0);

        let out_mem = allocator.alloc(64, Some(&alloc_params)).unwrap();

        let mut encoder = context.device().create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&in_mem.0.buffer, 0, &out_mem.0.buffer, 0, 64);

        let (tx, rx) = std::sync::mpsc::channel();
        encoder.on_submitted_work_done(move || {
            tx.send(()).ok();
        });
        context.queue().submit([encoder.finish()]);

        rx.recv_timeout(Duration::from_secs(1)).unwrap();

        {
            let map = out_mem.map_readable().unwrap();
            assert_eq!(map.as_slice(), &data[..]);
        }
    }
}

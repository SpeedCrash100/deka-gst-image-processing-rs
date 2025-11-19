mod imp;

use std::mem::MaybeUninit;

use gst::{
    glib::{object::Cast, subclass::types::ObjectSubclassIsExt, translate::ToGlibPtr},
    mini_object_wrapper,
    prelude::GstObjectExt,
};

use crate::{glib, gst_wgpu::WgpuContext};

pub const GST_CAPS_FEATURE_MEMORY_WGPU: &str = "memory:wgpu";

#[repr(transparent)]
pub struct WgpuMemory(imp::WgpuMemory);

impl WgpuMemory {}

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

        let allocator = out.upcast_ref::<gst::Allocator>();
        let allocator_stash: glib::translate::Stash<
            '_,
            *mut gst::ffi::GstAllocator,
            gst::Allocator,
        > = allocator.to_glib_none();

        let raw_alloc_pointer = allocator_stash.0 as *mut gst::ffi::GstAllocator;

        // SAFETY: ptr valid as long as allocator_stash is alive
        let allocator_pointer: &mut gst::ffi::GstAllocator = unsafe { &mut *raw_alloc_pointer };

        // todo!("Set functions");

        out
    }
}

#[cfg(test)]
mod tests {
    use gst::subclass::prelude::AllocatorImpl;

    use super::*;
    use crate::gst_wgpu::{gst_wgpu_memory::WgpuMemoryAllocator, WgpuContext};

    #[test]
    fn alloc() {
        let context = WgpuContext::new(
            &wgpu::RequestAdapterOptions {
                compatible_surface: None,
                ..Default::default()
            },
            &wgpu::DeviceDescriptor {
                ..Default::default()
            },
            true,
        );

        let allocator = WgpuMemoryAllocator::new(context);
        let mem = allocator.imp().alloc(64, None).unwrap();

        let mem_ptr = mem.as_mut_ptr();
        assert!(unsafe { imp::gst_is_wgpu_memory(mem_ptr) });
    }
}

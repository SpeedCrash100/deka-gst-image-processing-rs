mod imp;

use gst::{
    glib::{object::Cast, translate::ToGlibPtr},
    prelude::GstObjectExt,
};

use crate::glib;

pub const GST_CAPS_FEATURE_MEMORY_WGPU: &str = "memory:wgpu";

glib::wrapper! {
    pub struct WgpuMemoryAllocator(ObjectSubclass<imp::WgpuMemoryAllocator>) @extends gst::Allocator, gst::Object;
}

impl WgpuMemoryAllocator {
    pub fn new() -> Self {
        let mut out: WgpuMemoryAllocator = glib::Object::new();

        let allocator = out.upcast_ref::<gst::Allocator>();
        let allocator_stash: glib::translate::Stash<
            '_,
            *mut gst::ffi::GstAllocator,
            gst::Allocator,
        > = allocator.to_glib_none();

        let raw_alloc_pointer = allocator_stash.0 as *mut gst::ffi::GstAllocator;

        // SAFETY: ptr valid as long as allocator_stash is alive
        let allocator_pointer: &mut gst::ffi::GstAllocator = unsafe { &mut *raw_alloc_pointer };

        allocator_pointer.mem_map_full = Some(imp::gst_wgpu_memory_map_full);

        todo!("Set functions");
        out
    }
}

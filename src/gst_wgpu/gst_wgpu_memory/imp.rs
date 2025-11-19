use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::os::raw::c_void;
use std::sync::LazyLock;

use glib::object::ObjectType;
use gst::glib::object::{Cast, ObjectExt};
use gst::glib::subclass::object::ObjectImpl;
use gst::glib::subclass::types::ObjectSubclassExt;
use gst::glib::translate::{from_glib_full, FromGlibPtrBorrow, ToGlibPtr};
use gst::glib::{subclass::types::ObjectSubclass, translate::from_glib};
use gst::subclass::prelude::{AllocatorImpl, GstObjectImpl};
use wgpu::{BufferView, BufferViewMut};

use crate::glib;
use crate::gst_wgpu::{WgpuContext, CAT};

#[repr(C)]
#[derive(Debug)]
pub struct WgpuMemory {
    pub(super) parent: gst::ffi::GstMemory,
    context: WgpuContext,
    buffer: wgpu::Buffer,
}

pub(super) unsafe extern "C" fn gst_is_wgpu_memory(memory: *mut gst::ffi::GstMemory) -> bool {
    let mem = unsafe { &*memory };

    if mem.allocator.is_null() {
        return false;
    }

    let obj = gst::Allocator::from_glib_borrow(mem.allocator);
    if obj.downcast_ref::<super::WgpuMemoryAllocator>().is_none() {
        return false;
    }

    true
}

#[derive(Debug)]
pub struct WgpuMemoryAllocator {
    pub(super) context: UnsafeCell<Option<WgpuContext>>,
}

impl WgpuMemoryAllocator {
    #[inline]
    fn context(&self) -> &WgpuContext {
        let ctx = unsafe { &*self.context.get() };
        ctx.as_ref().unwrap()
    }

    #[inline]
    fn device(&self) -> &wgpu::Device {
        self.context().device()
    }
}

#[glib::object_subclass]
impl ObjectSubclass for WgpuMemoryAllocator {
    const NAME: &'static str = "WgpuMemoryAllocator";
    type Type = super::WgpuMemoryAllocator;
    type ParentType = gst::Allocator;

    fn with_class(_class: &Self::Class) -> Self {
        Self {
            context: Default::default(),
        }
    }
}

impl ObjectImpl for WgpuMemoryAllocator {}
impl GstObjectImpl for WgpuMemoryAllocator {}
impl AllocatorImpl for WgpuMemoryAllocator {
    fn alloc(
        &self,
        size: usize,
        params: Option<&gst::AllocationParams>,
    ) -> Result<gst::Memory, glib::BoolError> {
        let allocator_obj = self.obj().clone().upcast::<gst::Allocator>();

        let mut base_mem = MaybeUninit::<gst::ffi::GstMemory>::zeroed();

        let mut align = wgpu::MAP_ALIGNMENT as usize - 1;
        let mut offset = 0;
        let mut maxsize = size;
        let mut flags = 0;

        if let Some(p) = params {
            flags = p.flags().bits();
            align |= p.align();
            offset = p.prefix();
            maxsize += p.prefix() + p.padding();
            // If we're add align bytes, we can align map as requested
            maxsize += align;
        }

        let gst_mem_ptr = base_mem.as_mut_ptr() as *mut gst::ffi::GstMemory;
        let gst_allocator_ptr =
            allocator_obj.as_object_ref().to_glib_full() as *mut gst::ffi::GstAllocator;

        unsafe {
            gst::ffi::gst_memory_init(
                gst_mem_ptr,
                flags,
                gst_allocator_ptr,
                core::ptr::null_mut(),
                maxsize,
                align,
                offset,
                size,
            )
        };

        let base_mem = unsafe { base_mem.assume_init() };

        let mem_flags = gst::MemoryFlags::from_bits_truncate(flags);
        let usages = if mem_flags.contains(gst::MemoryFlags::READONLY) {
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ
        } else {
            wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE
        };

        let wgpu_buffer = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: None,
            mapped_at_creation: false,
            size: maxsize as u64,
            usage: usages,
        });

        let mem = Box::new(WgpuMemory {
            parent: base_mem,
            buffer: wgpu_buffer,
            context: self.context().clone(),
        });

        let mem_ptr = Box::leak(mem) as *mut WgpuMemory; // This is a memory
        let out_mem = unsafe { gst::Memory::from_glib_full(mem_ptr as *mut gst::ffi::GstMemory) };

        Ok(out_mem)
    }

    fn free(&self, memory: gst::Memory) {
        // Don't forget to unref allocator here, we're not static
        let mem_ptr = memory.as_mut_ptr();
        if !unsafe { gst_is_wgpu_memory(mem_ptr) } {
            return;
        }

        let mem_raw = unsafe { &mut *mem_ptr };
        mem_raw.allocator = core::ptr::null_mut();
        let _alloc: gst::Allocator = unsafe { from_glib_full(mem_raw.allocator) };

        // let wgpu_mem_ptr = mem_ptr as *mut WgpuMemory;
        // let wgpu_mem = unsafe { &mut *wgpu_mem_ptr };
        // wgpu_mem.buffer.destroy();
    }
}

unsafe impl Send for WgpuMemoryAllocator {}
unsafe impl Sync for WgpuMemoryAllocator {}

use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::mem::ManuallyDrop;

use glib::object::ObjectType;
use gst::glib::object::Cast;
use gst::glib::subclass::object::{ObjectImpl, ObjectImplExt};
use gst::glib::subclass::types::ObjectSubclass;
use gst::glib::subclass::types::ObjectSubclassExt;
use gst::glib::translate::{FromGlibPtrBorrow, ToGlibPtr};
use gst::subclass::prelude::{AllocatorImpl, GstObjectImpl};
use parking_lot::Mutex;

use crate::glib;
use crate::gst_wgpu::{WgpuContext, CAT};

pub const GST_WGPU_ALLOCATOR_TYPE: &[u8] = b"RustWgpuMemoryAllocator\0";

trait GetMappedPointer {
    fn get_mapped_pointer(&self) -> *mut c_void;
}

impl GetMappedPointer for wgpu::BufferView {
    fn get_mapped_pointer(&self) -> *mut c_void {
        self.as_ptr() as *mut c_void
    }
}

impl GetMappedPointer for wgpu::BufferViewMut {
    fn get_mapped_pointer(&self) -> *mut c_void {
        self.as_ptr() as *mut c_void
    }
}

#[repr(C)]
pub struct WgpuMemory {
    pub(super) parent: gst::ffi::GstMemory,
    pub(super) context: ManuallyDrop<WgpuContext>,
    pub(super) buffer: ManuallyDrop<wgpu::Buffer>,
    buffer_view: Mutex<Option<Box<dyn GetMappedPointer>>>,
}

impl std::fmt::Debug for WgpuMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WgpuMemory")
            .field("parent", &self.parent)
            .field("context", &self.context)
            .field("buffer", &self.buffer)
            .field("mapped", &self.buffer_view.lock().is_some())
            .finish_non_exhaustive()
    }
}

impl WgpuMemory {
    pub fn map_read(&self, size: u64) -> glib::ffi::gpointer {
        if !self.buffer.usage().contains(wgpu::BufferUsages::MAP_READ) {
            gst::error!(CAT, "trying to map read buffer which is not MAP_READ");
            return core::ptr::null_mut();
        }

        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        self.buffer
            .map_async(wgpu::MapMode::Read, ..size, move |res| {
                tx.send(res).ok();
            });

        match rx.recv() {
            Ok(Ok(())) => {
                // Success mapping
                let view = Box::new(self.buffer.get_mapped_range(..size));
                let p = view.get_mapped_pointer();
                *self.buffer_view.lock() = Some(view);
                p
            }
            Ok(Err(err)) => {
                gst::error!(CAT, "Failed to map buffer: {}", err);
                core::ptr::null_mut()
            }
            Err(_) => {
                gst::error!(CAT, "Failed to map buffer: no response");
                core::ptr::null_mut()
            }
        }
    }

    pub fn map_write(&self, size: u64) -> glib::ffi::gpointer {
        if !self.buffer.usage().contains(wgpu::BufferUsages::MAP_WRITE) {
            gst::error!(CAT, "trying to map write buffer which is not MAP_WRITE");
            return core::ptr::null_mut();
        }

        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        self.buffer
            .map_async(wgpu::MapMode::Write, ..size, move |res| {
                tx.send(res).ok();
            });

        match rx.recv() {
            Ok(Ok(())) => {
                // Success mapping
                let view = Box::new(self.buffer.get_mapped_range_mut(..size));
                let p = view.get_mapped_pointer();
                *self.buffer_view.lock() = Some(view);
                p
            }
            Ok(Err(err)) => {
                gst::error!(CAT, "Failed to map buffer: {}", err);
                core::ptr::null_mut()
            }
            Err(_) => {
                gst::error!(CAT, "Failed to map buffer: no response");
                core::ptr::null_mut()
            }
        }
    }

    /// Safety: after the call all pointers to mapped memory is invalid
    pub unsafe fn unmap(&self) {
        *self.buffer_view.lock() = None;
        self.buffer.unmap();
        self.context.device().poll(wgpu::PollType::Poll).ok();
    }
}

pub(super) unsafe extern "C" fn gst_is_wgpu_memory(
    memory: *mut gst::ffi::GstMemory,
) -> glib::ffi::gboolean {
    let mem = unsafe { &*memory };

    if mem.allocator.is_null() {
        return false.into();
    }

    let obj = gst::Allocator::from_glib_borrow(mem.allocator);
    if obj.downcast_ref::<super::WgpuMemoryAllocator>().is_none() {
        return false.into();
    }

    true.into()
}

unsafe extern "C" fn gst_wgpu_mem_map(
    mem: *mut gst::ffi::GstMemory,
    maxsize: usize,
    flags: gst::ffi::GstMapFlags,
) -> glib::ffi::gpointer {
    let mem = mem as *mut WgpuMemory;
    assert!(!mem.is_null() && mem.is_aligned());

    let mem_ref = &*mem;

    let mode = if flags & gst::ffi::GST_MAP_WRITE != 0 {
        wgpu::MapMode::Write
    } else if flags & gst::ffi::GST_MAP_READ != 0 {
        wgpu::MapMode::Read
    } else {
        gst::error!(CAT, "Invalid map flags {}", flags);
        return core::ptr::null_mut();
    };

    if mem_ref.buffer_view.lock().is_some() {
        gst::error!(CAT, "only one map can be active");
        return core::ptr::null_mut();
    }

    match mode {
        wgpu::MapMode::Read => mem_ref.map_read(maxsize as u64),
        wgpu::MapMode::Write => mem_ref.map_write(maxsize as u64),
    }
}

unsafe extern "C" fn gst_wgpu_mem_unmap(mem: *mut gst::ffi::GstMemory) {
    let mem = mem as *mut WgpuMemory;
    assert!(!mem.is_null() && mem.is_aligned());

    let mem_ref = &*mem;
    mem_ref.unmap();
}

/// Inits the allocators's function table
unsafe extern "C" fn gst_wgpu_mem_allocator_init(allocator: *mut gst::ffi::GstAllocator) {
    debug_assert!(!allocator.is_null());

    (*allocator).mem_type = GST_WGPU_ALLOCATOR_TYPE.as_ptr() as *const core::ffi::c_char;
    (*allocator).mem_map = Some(gst_wgpu_mem_map);
    (*allocator).mem_unmap = Some(gst_wgpu_mem_unmap);
    (*allocator).mem_copy = None; // TODO
    (*allocator).mem_share = None; // TODO
    (*allocator).mem_is_span = None;
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

impl ObjectImpl for WgpuMemoryAllocator {
    fn constructed(&self) {
        let obj = self.obj();
        let allocator_obj = obj.upcast_ref::<gst::Allocator>();
        let allocator_ptr: *mut gst::ffi::GstAllocator = allocator_obj.to_glib_none().0;

        unsafe {
            gst_wgpu_mem_allocator_init(allocator_ptr);
        }

        self.parent_constructed();
    }
}
impl GstObjectImpl for WgpuMemoryAllocator {}
impl AllocatorImpl for WgpuMemoryAllocator {
    fn alloc(
        &self,
        size: usize,
        params: Option<&gst::AllocationParams>,
    ) -> Result<gst::Memory, glib::BoolError> {
        let layout = core::alloc::Layout::new::<WgpuMemory>();
        // SAFETY: layout have non zero size: WgpuMemory sized fields
        let mem = unsafe { std::alloc::alloc_zeroed(layout) } as *mut WgpuMemory;

        let mut align = wgpu::MAP_ALIGNMENT as usize - 1;
        let mut offset = 0;
        let mut maxsize = size;
        let mut flags = 0;

        if let Some(p) = params {
            flags = p.flags().bits();
            align |= p.align();
            offset = p.prefix();
            maxsize += p.prefix() + p.padding();
        }

        let gst_allocator_ptr =
            self.obj().as_object_ref().to_glib_full() as *mut gst::ffi::GstAllocator;

        unsafe {
            gst::ffi::gst_memory_init(
                mem as *mut gst::ffi::GstMemory,
                flags,
                gst_allocator_ptr,
                core::ptr::null_mut(),
                maxsize,
                align,
                offset,
                size,
            )
        };

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

        unsafe {
            core::ptr::write(
                &raw mut (*mem).context,
                ManuallyDrop::new(self.context().clone()),
            );
            core::ptr::write(&raw mut (*mem).buffer, ManuallyDrop::new(wgpu_buffer));
        }

        gst::trace!(CAT, "allocated buffer {:p}, maxsize {}", mem, maxsize);

        let out_mem = unsafe { gst::Memory::from_glib_full(mem as *mut gst::ffi::GstMemory) };
        Ok(out_mem)
    }

    fn free(&self, memory: gst::Memory) {
        let mut wgpu_mem: super::WgpuMemory =
            memory.downcast_memory().expect("non wgpu mem passed");
        let wgpu_mem_obj = unsafe { wgpu_mem.obj.as_mut() };
        unsafe {
            ManuallyDrop::drop(&mut wgpu_mem_obj.context);
        };
        unsafe {
            ManuallyDrop::drop(&mut wgpu_mem_obj.buffer);
        };

        let layout = core::alloc::Layout::new::<WgpuMemory>();
        unsafe { std::alloc::dealloc(wgpu_mem.as_mut_ptr() as *mut u8, layout) };
        gst::trace!(CAT, "free buffer {:p}", wgpu_mem.as_mut_ptr());
        std::mem::forget(wgpu_mem); // We dealloc the memory ourselves
    }
}

unsafe impl Send for WgpuMemoryAllocator {}
unsafe impl Sync for WgpuMemoryAllocator {}

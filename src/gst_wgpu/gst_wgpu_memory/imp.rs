use std::os::raw::c_void;
use std::sync::LazyLock;

use gst::glib::object::Cast;
use gst::glib::subclass::object::ObjectImpl;
use gst::glib::translate::FromGlibPtrBorrow;
use gst::glib::{subclass::types::ObjectSubclass, translate::from_glib};
use gst::subclass::prelude::{AllocatorImpl, GstObjectImpl};
use wgpu::{BufferView, BufferViewMut};

use crate::glib;
use crate::gst_wgpu::{WgpuContext, CAT};

enum MappedData {
    Ref(BufferView),
    Mut(BufferViewMut),
}

impl MappedData {
    pub fn pointer(&self) -> *mut u8 {
        match self {
            MappedData::Ref(view) => view.as_ptr() as *mut u8,
            MappedData::Mut(view) => view.as_ptr() as *mut u8,
        }
    }
}

#[repr(C)]
pub struct WgpuMemory {
    parent: gst::ffi::GstMemory,
    context: WgpuContext,
    buffer: wgpu::Buffer,

    offset: usize,
    size: usize,
}

#[repr(C)]
pub struct WgpuMemoryAllocParams {
    base: gst::ffi::GstAllocationParams,
    context: WgpuContext,
    size: usize,
}

unsafe extern "C" fn wgpu_memory_init(
    memory: *mut WgpuMemory,
    allocator: *mut gst::ffi::GstAllocator,
    alloc_params: WgpuMemoryAllocParams,
    parent: *mut gst::ffi::GstMemory,
    context: WgpuContext,
    usages: wgpu::BufferUsages,
) {
    assert!(!memory.is_null(), "memory is null");
    assert!(memory.is_aligned(), "memory is not aligned");
    assert!(!allocator.is_null(), "allocator is null");
    assert!(allocator.is_aligned(), "allocator is not aligned");

    let wgpu_alignment = wgpu::MAP_ALIGNMENT as usize - 1;

    let flags = alloc_params.base.flags;
    let align = alloc_params.base.align | wgpu_alignment;
    let offset = alloc_params.base.prefix;
    let maxsize = alloc_params.size + alloc_params.base.prefix + alloc_params.base.padding;
    let alloc_size = maxsize + align;

    gst::ffi::gst_memory_init(
        memory as *mut gst::ffi::GstMemory,
        flags,
        allocator,
        parent,
        maxsize,
        align,
        offset,
        alloc_params.size,
    );

    let mem = &mut *memory;
    mem.context = context.clone();

    mem.buffer = mem.context.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        mapped_at_creation: false,
        size: alloc_size as u64,
        usage: usages,
    });
    mem.size = alloc_params.size;
    mem.offset = alloc_params.base.prefix;
}

unsafe extern "C" fn gst_is_wgpu_memory(memory: *mut gst::ffi::GstMemory) -> bool {
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

pub(super) unsafe extern "C" fn gst_wgpu_memory_map_full(
    memory: *mut gst::ffi::GstMemory,
    map_info: *mut gst::ffi::GstMapInfo,
    _size: usize,
) -> *mut c_void {
    assert!(!memory.is_null(), "memory is null");
    assert!(!map_info.is_null(), "map_info is null");

    let downcased_mem: *mut WgpuMemory = unsafe { core::mem::transmute(memory) };
    let mem = unsafe { &mut *downcased_mem };

    let map_info = unsafe { &mut *map_info };

    let (tx, rx) = std::sync::mpsc::sync_channel(0);

    let mode = if map_info.flags & gst::ffi::GST_MAP_READ != 0 {
        wgpu::MapMode::Read
    } else if map_info.flags & gst::ffi::GST_MAP_WRITE != 0 {
        wgpu::MapMode::Write
    } else {
        gst::error!(CAT, "unsupported flags: {}", map_info.flags);
        return core::ptr::null_mut();
    };
    // todo: impl GST_MAP_REF_MEMORY

    if mem.size < map_info.maxsize {
        gst::error!(CAT, "buffer is too small");
        return core::ptr::null_mut();
    }

    let start = mem.offset;
    let end = start + mem.size;
    let bounds = (start as u64)..(end as u64);

    mem.buffer.map_async(mode, bounds.clone(), move |res| {
        tx.send(res).ok();
    });

    match rx.recv() {
        Ok(Ok(_)) => {}
        Ok(Err(err)) => {
            gst::error!(CAT, "map error: {}", err);
            return core::ptr::null_mut();
        }
        Err(_) => {
            gst::error!(CAT, "map closed");
            return core::ptr::null_mut();
        }
    };

    // Now we can get range
    let mapped_data = match mode {
        wgpu::MapMode::Read => MappedData::Ref(mem.buffer.get_mapped_range(bounds)),
        wgpu::MapMode::Write => MappedData::Mut(mem.buffer.get_mapped_range_mut(bounds)),
    };

    let user_data = Box::new(mapped_data);
    let data = user_data.pointer();
    map_info.data = data;
    map_info.user_data = [
        Box::into_raw(user_data) as *mut c_void,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    ];
    map_info.size = mem.size;

    data as *mut c_void
}

pub struct WgpuMemoryAllocator;

#[glib::object_subclass]
impl ObjectSubclass for WgpuMemoryAllocator {
    const NAME: &'static str = "WgpuMemoryAllocator";
    type Type = super::WgpuMemoryAllocator;
    type ParentType = gst::Allocator;

    fn with_class(_class: &Self::Class) -> Self {
        Self
    }
}

impl ObjectImpl for WgpuMemoryAllocator {}
impl GstObjectImpl for WgpuMemoryAllocator {}
impl AllocatorImpl for WgpuMemoryAllocator {}

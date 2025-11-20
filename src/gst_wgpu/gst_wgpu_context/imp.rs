use std::{
    cell::UnsafeCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{glib, gst_wgpu::CAT};
use gst::glib::subclass::prelude::*;

#[derive(Debug)]
pub struct WgpuContext {
    pub(super) device: UnsafeCell<Option<Arc<wgpu::Device>>>,
    pub(super) queue: UnsafeCell<Option<Arc<wgpu::Queue>>>,
    pub(super) running: Arc<AtomicBool>,
}

#[glib::object_subclass]
impl ObjectSubclass for WgpuContext {
    const NAME: &'static str = "GstWgpuContext";
    type Type = super::WgpuContext;
    type ParentType = glib::Object;

    fn with_class(_class: &Self::Class) -> Self {
        Self {
            device: Default::default(),
            queue: Default::default(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ObjectImpl for WgpuContext {
    fn dispose(&self) {
        gst::info!(CAT, imp: self, "stopping ctx");
        self.running.store(false, Ordering::Release);
    }
}

unsafe impl Send for WgpuContext {}
unsafe impl Sync for WgpuContext {}

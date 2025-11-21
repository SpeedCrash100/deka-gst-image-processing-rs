use std::{
    cell::UnsafeCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

use crate::{glib, gst_wgpu::CAT};
use gst::glib::subclass::prelude::*;

pub(super) struct Inner {
    #[allow(dead_code)]
    pub instance: wgpu::Instance,
    #[allow(dead_code)]
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

#[derive(Debug)]
pub struct WgpuContext {
    pub(super) inner: UnsafeCell<Option<Inner>>,
    pub(super) poll_type: UnsafeCell<super::PollType>,
    pub(super) poll_thread: UnsafeCell<Option<JoinHandle<()>>>,
    pub(super) running: Arc<AtomicBool>,
}

#[glib::object_subclass]
impl ObjectSubclass for WgpuContext {
    const NAME: &'static str = "GstWgpuContext";
    type Type = super::WgpuContext;
    type ParentType = glib::Object;

    fn with_class(_class: &Self::Class) -> Self {
        Self {
            inner: Default::default(),
            poll_type: UnsafeCell::new(super::PollType::Manual),
            poll_thread: Default::default(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ObjectImpl for WgpuContext {
    fn dispose(&self) {
        gst::info!(CAT, imp: self, "stopping ctx");
        self.running.store(false, Ordering::Release);
        // SAFETY: assuming dispose never be called in parallel
        let handle = unsafe { &mut *self.poll_thread.get() };

        if let Some(handle) = handle.take() {
            if let Err(err) = handle.join() {
                gst::error!(CAT, imp: self, "failed to join poll thread {:?}", err);
            }
        }
    }
}

unsafe impl Send for WgpuContext {}
unsafe impl Sync for WgpuContext {}

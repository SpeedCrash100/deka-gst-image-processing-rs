mod imp;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use gst::glib;
use gst::glib::subclass::types::ObjectSubclassIsExt;

pub const GST_CONTEXT_WGPU_TYPE: &str = "rust.wgpu.Context";
pub const GST_CONTEXT_WGPU_FIELD: &str = "context";

glib::wrapper! {
    pub struct WgpuContext(ObjectSubclass<imp::WgpuContext>);
}

impl WgpuContext {
    pub fn into_gst_context(&self) -> gst::Context {
        let mut ctx = gst::Context::new(GST_CONTEXT_WGPU_TYPE, false);
        {
            let ctx_mut = ctx.get_mut().expect("failed to get mut ctx");
            let structure_mut = ctx_mut.structure_mut();

            structure_mut.set(GST_CONTEXT_WGPU_FIELD, self);
        }
        ctx
    }

    pub fn new(
        adapter_options: &wgpu::RequestAdapterOptions<'_, '_>,
        desc: &wgpu::DeviceDescriptor<'_>,
        busy_wait: bool,
    ) -> Self {
        let instance_description = wgpu::InstanceDescriptor::from_env_or_default();
        let instance = wgpu::Instance::new(&instance_description);

        let adapter = match pollster::block_on(instance.request_adapter(&adapter_options)) {
            Ok(adapter) => adapter,
            Err(err) => {
                glib::g_error!("wgpu", "Failed to request adapter: {}", err);
                panic!("Failed to request adapter");
            }
        };

        let (device, queue) = match pollster::block_on(adapter.request_device(&desc)) {
            Ok(device) => device,
            Err(err) => {
                glib::g_error!("wgpu", "Failed to request device: {}", err);
                panic!("Failed to request device");
            }
        };

        Self::from_ready(device, queue, busy_wait)
    }

    pub fn from_ready(device: wgpu::Device, queue: wgpu::Queue, busy_wait: bool) -> Self {
        let out: Self = glib::Object::new();

        let poll_device = device.clone();
        let running = Arc::clone(&out.imp().running);
        std::thread::spawn(move || {
            let poll_type = if busy_wait {
                wgpu::PollType::Poll
            } else {
                wgpu::PollType::wait_indefinitely()
            };

            running.store(true, Ordering::Relaxed);

            while running.load(Ordering::Relaxed) {
                if let Err(err) = poll_device.poll(poll_type.clone()) {
                    glib::g_error!("wgpu", "Failed to poll device: {}", err);
                    break;
                }
            }

            running.store(false, Ordering::Relaxed);
        });

        let imp = out.imp();
        // SAFETY: This is the only place where we write - at creation. Should not be any problems with race conditions
        unsafe { *imp.device.get() = Some(device) };
        unsafe { *imp.queue.get() = Some(queue) };

        out
    }

    #[inline]
    pub fn device(&self) -> &wgpu::Device {
        let out = unsafe { &*self.imp().device.get() };
        out.as_ref().unwrap()
    }

    #[inline]
    pub fn queue(&self) -> &wgpu::Queue {
        let out = unsafe { &*self.imp().queue.get() };
        out.as_ref().unwrap()
    }
}

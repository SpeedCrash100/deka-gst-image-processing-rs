mod imp;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use gst::glib;
use gst::glib::clone::Downgrade;
use gst::glib::subclass::types::ObjectSubclassIsExt;
use gst::prelude::{ElementExt, GstObjectExt, PadExt, PadExtManual};

use super::CAT;

pub const GST_CONTEXT_WGPU_TYPE: &str = "rust.wgpu.Context";
const GST_CONTEXT_WGPU_FIELD: &str = "context";

#[derive(Debug, Clone, Copy, Default)]
pub enum PollType {
    /// The background thread will be spawned to poll for the device
    #[default]
    Threaded,
    /// The background thread will be spawned to poll for the device in busy loop
    ThreadedBusy,
    /// The user must call `poll` on device manually
    Manual,
}

glib::wrapper! {
    pub struct WgpuContext(ObjectSubclass<imp::WgpuContext>);
}

impl Default for WgpuContext {
    fn default() -> Self {
        Self::new(
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
            PollType::Manual,
        )
    }
}

impl WgpuContext {
    /// Creates GstContext from self
    pub fn as_gst_context(&self) -> gst::Context {
        let mut ctx = gst::Context::new(GST_CONTEXT_WGPU_TYPE, true);
        {
            let ctx_mut = ctx.get_mut().expect("failed to get mut ctx");
            let structure_mut = ctx_mut.structure_mut();

            structure_mut.set(GST_CONTEXT_WGPU_FIELD, self.clone());
        }

        ctx
    }

    /// Creates WgpuContext using specified options
    ///
    /// # Arguments
    /// * `adapter_options` - Options to get WGPU Adapter
    /// * `desc` - WGPU Device Descriptor
    /// * `poll_type` - sets poll behavior
    pub fn new(
        adapter_options: &wgpu::RequestAdapterOptions<'_, '_>,
        desc: &wgpu::DeviceDescriptor<'_>,
        poll_type: PollType,
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

        let inner = imp::Inner {
            instance,
            adapter,
            device,
            queue,
        };

        Self::from_inner(inner, poll_type)
    }

    fn from_inner(inner: imp::Inner, poll_type: PollType) -> Self {
        let out: Self = glib::Object::new();
        let imp = out.imp();

        let device = inner.device.clone();

        // SAFETY: This is the only place where we write - at creation. Should not be any problems with race conditions
        unsafe { *imp.inner.get() = Some(inner) };
        unsafe { *imp.poll_type.get() = poll_type };

        // Spawn thread for polling
        let join_handle = {
            let running = Arc::clone(&imp.running);
            let obj = out.downgrade();

            std::thread::spawn(move || {
                let poll_type = match poll_type {
                    PollType::Manual => {
                        if let Some(obj) = obj.upgrade() {
                            gst::info!(CAT, obj: obj, "Manual polling");
                        }
                        return;
                    }
                    PollType::Threaded => wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_millis(25000)),
                    },
                    PollType::ThreadedBusy => wgpu::PollType::Poll,
                };

                running.store(true, Ordering::Relaxed);
                if let Some(obj) = obj.upgrade() {
                    gst::info!(CAT, obj: obj, "ctx started");
                }

                while running.load(Ordering::Acquire) {
                    if obj.upgrade().is_none() {
                        gst::info!(CAT, "ctx dropped, exiting");
                        break;
                    }

                    if let Err(err) = device.poll(poll_type.clone()) {
                        match err {
                            wgpu::PollError::Timeout => {
                                // Do nothing on timeout
                            }
                            other => {
                                if let Some(obj) = obj.upgrade() {
                                    gst::error!(CAT, obj: obj, "poll error: {}", other)
                                }
                            }
                        }
                    }
                }
                running.store(false, Ordering::Relaxed);
                gst::info!(CAT, "ctx stopped");
            })
        };

        unsafe { *imp.poll_thread.get() = Some(join_handle) };

        out
    }

    /// Get the wgpu device
    #[inline]
    pub fn device(&self) -> &wgpu::Device {
        let out = unsafe { &*self.imp().inner.get() };
        // SAFETY: the only one _pub_ constructor always init inner
        out.as_ref()
            .map(|x| &x.device)
            .expect("inner is None, you must create WgpuContext using associated WgpuContext::new")
    }

    /// Gets device limits
    #[inline]
    pub fn limits(&self) -> wgpu::Limits {
        self.device().limits()
    }

    /// Get the wgpu queue
    #[inline]
    pub fn queue(&self) -> &wgpu::Queue {
        let out = unsafe { &*self.imp().inner.get() };
        out.as_ref()
            .map(|x| &x.queue)
            .expect("inner is None, you must create WgpuContext using associated WgpuContext::new")
    }

    #[inline]
    pub fn poll_type(&self) -> PollType {
        let out = unsafe { &*self.imp().poll_type.get() };
        *out
    }

    fn query_context_pad(element: &gst::Element, pad: &gst::Pad) -> Option<gst::Context> {
        let mut query = gst::query::Context::new(GST_CONTEXT_WGPU_TYPE);
        let remote_pad = pad.peer();
        let remote_element_name = remote_pad
            .as_ref()
            .and_then(|x| x.parent_element())
            .map(|x| x.name());

        gst::trace!(
            CAT,
            obj: element,
            "Querying context for element {} from pad {} from element {:?}",
            element.name(),
            pad.name(),
            remote_element_name
        );

        let sent_success = pad.peer_query(&mut query);
        if !sent_success {
            return None;
        }

        let Some(pad_ctx) = query.context_owned() else {
            // Try next pad
            return None;
        };

        gst::info!(
            CAT,
            obj: element,
            "got context from pad {} from element {:?}",
            pad.name(),
            remote_element_name
        );

        element.set_context(&pad_ctx);

        Some(pad_ctx)
    }

    fn query_context_pad_fn<'a>() -> impl FnMut(&gst::Element, &gst::Pad) -> bool + 'a {
        move |element, pad| {
            Self::query_context_pad(element, pad);

            true
        }
    }

    fn check_context_exists(element: &gst::Element) -> bool {
        element.context(GST_CONTEXT_WGPU_TYPE).is_some()
    }

    /// Returns true if a wgpu context was found and set on the element
    fn query_context_from_pads(element: &gst::Element) -> bool {
        if Self::check_context_exists(element) {
            return true;
        }

        // Query downstream for the context
        element.foreach_src_pad(Self::query_context_pad_fn());
        if Self::check_context_exists(element) {
            return true;
        }

        // Query upstream for the context
        element.foreach_sink_pad(Self::query_context_pad_fn());
        if Self::check_context_exists(element) {
            return true;
        }

        return false;
    }

    /// Query
    fn query_context_by_message(element: &gst::Element) -> Result<bool, glib::BoolError> {
        let message = gst::message::NeedContext::builder(GST_CONTEXT_WGPU_TYPE)
            .src(&*element)
            .build();

        gst::trace!(CAT, obj: element, "Posting need WGPU context message");
        if let Err(err) = element.post_message(message) {
            glib::g_error!("wgpu", "Failed to post need context message: {}", err);
            return Err(err);
        }

        Ok(element.context(GST_CONTEXT_WGPU_TYPE).is_some())
    }

    pub fn map_gst_context_to_wgpu(context: gst::Context) -> Option<WgpuContext> {
        if context.context_type() != GST_CONTEXT_WGPU_TYPE {
            return None;
        }

        let structure = context.structure();
        let wgpu_ctx = match structure.get::<WgpuContext>(GST_CONTEXT_WGPU_FIELD).ok() {
            Some(ctx) => ctx,
            None => return None,
        };
        Some(wgpu_ctx)
    }

    /// Query the WGPU context from nearby elements.
    /// Returns `None` if the context is not found.
    pub fn query_context_from_nearby_elements(
        element: &gst::Element,
    ) -> Result<bool, glib::BoolError> {
        if Self::query_context_from_pads(element) {
            return Ok(true);
        }

        if Self::query_context_by_message(element)? {
            return Ok(true);
        }

        gst::info!(CAT, obj: element, "No WGPU context found in nearby elements");

        Ok(false)
    }
}

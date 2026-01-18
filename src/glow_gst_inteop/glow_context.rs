use std::{ops::ControlFlow, sync::LazyLock};

use glib::subclass::types::ObjectSubclassIsExt;
use gst::{glib::subclass::types::ObjectSubclassExt, prelude::*, subclass::prelude::*};
use gst_gl::{prelude::*, GLBaseFilter};

use crate::glib;

/// GstContext type string
pub const GST_CONTEXT_GLOW_TYPE: &str = "rust.glow.Context";

const GLOW_CONTEXT_FIELD: &str = "context";

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "gstglowcontext",
        gst::DebugColorFlags::empty(),
        Some("Gstreamer Glow Context"),
    )
});

glib::wrapper! {

    pub struct GlowContext(ObjectSubclass<imp::GlowContext>);
}

impl GlowContext {
    /// Creates a new glow context for the given `parent` GL context from gstreamer-gl
    ///
    /// # Note
    /// The glow does not specify safety for `Context::from_loader_function` so this function is unsafe without
    /// any safety requirements
    pub unsafe fn new(parent: &gst_gl::GLContext) -> Self {
        let wrapper = glow::Context::from_loader_function({
            let ctx = parent.clone();

            move |name| ctx.proc_address(name) as *const std::ffi::c_void
        });

        let out: Self = glib::Object::new();
        let imp = out.imp();

        let inner = imp::Inner {
            context: wrapper,
            parent_context: parent.clone(),
        };

        // Safety: we don't change the pointer to `inner` after creation so all accesses are read only and can be shared
        unsafe { *imp.inner.get() = Some(inner) };

        out
    }

    pub fn as_gst_context(&self) -> gst::Context {
        let mut ctx = gst::Context::new(GST_CONTEXT_GLOW_TYPE, true);
        {
            let ctx_mut = ctx.get_mut().expect("failed to get mut ctx");
            let structure_mut = ctx_mut.structure_mut();

            structure_mut.set(GLOW_CONTEXT_FIELD, self.clone());
        }

        ctx
    }

    #[inline]
    pub fn glow(&self) -> &glow::Context {
        let out = unsafe { &*self.imp().inner.get() };
        out.as_ref()
            .map(|x| &x.context)
            .expect("inner is None, you must create GlowContext using associated GlowContext::new")
    }

    #[inline]
    pub fn gst_gl_context(&self) -> &gst_gl::GLContext {
        let out = unsafe { &*self.imp().inner.get() };
        out.as_ref()
            .map(|x| &x.parent_context)
            .expect("inner is None, you must create GlowContext using associated GlowContext::new")
    }

    fn query_context_pad(element: &gst::Element, pad: &gst::Pad) -> bool {
        let mut query = gst::query::Context::new(GST_CONTEXT_GLOW_TYPE);
        let remote_pad = pad.peer();
        let remote_element_name = remote_pad
            .as_ref()
            .and_then(|x| x.parent_element())
            .map(|x| x.name());

        gst::trace!(
            CAT,
            obj = element,
            "query context for element {} from pad {} from element {:?}",
            element.name(),
            pad.name(),
            remote_element_name
        );

        if !pad.peer_query(&mut query) {
            return false;
        }

        let Some(pad_ctx) = query.context_owned() else {
            return false;
        };

        gst::info!(
            CAT,
            obj = element,
            "got context from pad {} from element {:?}",
            pad.name(),
            remote_element_name
        );

        element.set_context(&pad_ctx);

        true
    }

    fn query_context_pad_fn<'a>(
        found: &'a mut bool,
    ) -> impl FnMut(&gst::Element, &gst::Pad) -> ControlFlow<()> + 'a {
        move |element, pad| {
            if Self::query_context_pad(element, pad) {
                *found = true;
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }
    }

    fn query_context_from_pads(element: &gst::Element) -> bool {
        let mut found = false;

        element.foreach_src_pad(Self::query_context_pad_fn(&mut found));
        if found {
            return found;
        }

        element.foreach_sink_pad(Self::query_context_pad_fn(&mut found));

        found
    }

    fn check_context_exists(element: &gst::Element) -> bool {
        element.context(GST_CONTEXT_GLOW_TYPE).is_some()
    }

    fn query_context_by_message(element: &gst::Element) -> Result<bool, glib::BoolError> {
        let message = gst::message::NeedContext::builder(GST_CONTEXT_GLOW_TYPE)
            .src(&*element)
            .build();

        gst::trace!(CAT, obj = element, "Posting need GLOW context message");
        if let Err(err) = element.post_message(message) {
            gst::error!(
                CAT,
                obj = element,
                "Failed to post need context message: {}",
                err
            );
            return Err(err);
        }

        Ok(element.context(GST_CONTEXT_GLOW_TYPE).is_some())
    }

    pub fn map_gst_context_to_glow(context: gst::Context) -> Option<GlowContext> {
        if context.context_type() != GST_CONTEXT_GLOW_TYPE {
            return None;
        }

        let structure = context.structure();
        let glow_ctx = match structure.get::<GlowContext>(GLOW_CONTEXT_FIELD).ok() {
            Some(ctx) => ctx,
            None => return None,
        };
        Some(glow_ctx)
    }

    pub fn query_context_from_nearby_elements(
        element: &gst::Element,
    ) -> Result<bool, glib::BoolError> {
        if Self::query_context_from_pads(element) {
            return Ok(true);
        }

        if Self::query_context_by_message(element)? {
            return Ok(true);
        }

        gst::info!(
            CAT,
            obj = element,
            "No GLOW context found in nearby elements"
        );

        Ok(false)
    }
}

mod imp {

    use std::cell::UnsafeCell;

    use gst::glib::subclass::{object::ObjectImpl, types::ObjectSubclass};

    use super::*;

    pub(super) struct Inner {
        pub(super) parent_context: gst_gl::GLContext,
        pub(super) context: glow::Context,
    }

    pub struct GlowContext {
        pub(super) inner: UnsafeCell<Option<Inner>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GlowContext {
        const NAME: &'static str = "GstGlowContext";
        type Type = super::GlowContext;
        type ParentType = glib::Object;

        fn with_class(_class: &Self::Class) -> Self {
            Self {
                inner: Default::default(),
            }
        }
    }

    impl ObjectImpl for GlowContext {}

    unsafe impl Send for GlowContext {}
    unsafe impl Sync for GlowContext {}
}

pub trait GstElementGetGlowContextExt {
    fn glow_context(&self) -> Option<GlowContext>;
}

impl<T> GstElementGetGlowContextExt for T
where
    T: ElementExt,
{
    fn glow_context(&self) -> Option<GlowContext> {
        let context = self.context(GST_CONTEXT_GLOW_TYPE)?;
        GlowContext::map_gst_context_to_glow(context)
    }
}

pub trait GstElementFindGlowContextExt {
    fn find_glow_context(&self) -> bool;
}

impl GstElementFindGlowContextExt for GLBaseFilter {
    fn find_glow_context(&self) -> bool {
        if GlowContext::check_context_exists(self.upcast_ref()) {
            return true;
        }

        match GlowContext::query_context_from_nearby_elements(self.upcast_ref()) {
            Ok(true) => {
                gst::info!(CAT, obj = self, "found glow context in nearby element");
                return true;
            }
            Ok(false) => {}
            Err(err) => {
                gst::error!(CAT, obj = self, "failed to query for glow context: {}", err);
            }
        }

        // Creating one, make sure the control flow returns if Glow context available!

        if !GLBaseFilterExt::find_gl_context(self) {
            gst::error!(CAT, obj = self, "can't find gl context");
            return false;
        }

        let Some(parent_gl) = GLBaseFilterExt::context(self) else {
            gst::error!(
                CAT,
                obj = self,
                "can't find gl context, even when find success"
            );
            return false;
        };

        let mut new_context = None;
        parent_gl.thread_add(|ctx| {
            // Should be safe to call this in GL thread
            let glow = unsafe { GlowContext::new(ctx) };
            new_context = Some(glow);
        });

        let Some(glow_context) = new_context else {
            gst::error!(CAT, obj = self, "failed to create GLOW context");
            return false;
        };

        let gst_glow_context = glow_context.as_gst_context();
        self.set_context(&gst_glow_context);

        true
    }
}

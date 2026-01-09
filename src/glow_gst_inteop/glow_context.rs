use std::sync::LazyLock;

use glib::subclass::types::ObjectSubclassIsExt;
use gst_gl::prelude::GLContextExtManual;

use crate::glib;

/// GstContext type string
pub const GST_CONTEXT_GLOW: &str = "rust.glow.Context";

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

    #[inline]
    pub fn glow(&self) -> &glow::Context {
        let out = unsafe { &*self.imp().inner.get() };
        out.as_ref()
            .map(|x| &x.context)
            .expect("inner is None, you must create GlowContext using associated GlowContext::new")
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

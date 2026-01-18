mod glow_context;
pub mod transition;

pub use glow_context::{
    GlowContext, GstElementFindGlowContextExt, GstElementGetGlowContextExt, GST_CONTEXT_GLOW_TYPE,
};

pub mod prelude {
    pub use super::transition::{AsGlowProgram, AsGlowTexture};
}

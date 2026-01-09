mod glow_context;
pub mod transition;

pub use glow_context::{GlowContext, GST_CONTEXT_GLOW};

pub mod prelude {
    pub use super::transition::{AsGlowProgram, AsGlowTexture};
}

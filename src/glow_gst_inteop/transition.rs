//!
//! Specifies transition between glow crate and gstreamer-gl
//!

use std::num::NonZeroU32;

pub trait AsGlowProgram {
    fn as_glow_program(&self) -> Option<glow::NativeProgram>;
}

impl AsGlowProgram for gst_gl::GLShader {
    fn as_glow_program(&self) -> Option<glow::NativeProgram> {
        let handle = self.program_handle();
        (0 < handle)
            // SAFETY: We check bounds above, it will never be < 0 as well so can be cast to u32
            .then_some(unsafe { NonZeroU32::new_unchecked(handle as u32) })
            .map(glow::NativeProgram)
    }
}

pub trait AsGlowTexture {
    fn as_glow_texture(&self) -> Option<glow::NativeTexture>;
}

impl AsGlowTexture for gst_gl::GLMemory {
    fn as_glow_texture(&self) -> Option<glow::NativeTexture> {
        let handle = self.texture_id();
        NonZeroU32::new(handle).map(glow::NativeTexture)
    }
}

impl AsGlowTexture for gst_gl::GLMemoryRef {
    fn as_glow_texture(&self) -> Option<glow::NativeTexture> {
        let handle = self.texture_id();
        NonZeroU32::new(handle).map(glow::NativeTexture)
    }
}

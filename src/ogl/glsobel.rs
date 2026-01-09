use gst::glib;
use gst::prelude::*;

glib::wrapper! {
    pub struct GlSobel(ObjectSubclass<imp::GlSobel>) @extends gst_gl::GLFilter, gst_gl::GLBaseFilter, gst_base::BaseTransform, gst::Element, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "dekaglsobel",
        gst::Rank::NONE,
        GlSobel::static_type(),
    )
}

mod imp {
    use std::sync::{LazyLock, Mutex};

    use gst::{
        glib::subclass::{object::ObjectImpl, types::ObjectSubclass},
        prelude::*,
        subclass::prelude::*,
    };
    use gst_base::subclass::{prelude::BaseTransformImpl, BaseTransformMode};
    use gst_gl::{
        prelude::{GLBaseFilterExt, GLFilterExt},
        subclass::{
            prelude::{GLBaseFilterImpl, GLFilterImpl},
            GLFilterMode,
        },
        GLSLStage, GLShader,
    };
    use gst_video::VideoFormat;

    use crate::glib;

    static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
        gst::DebugCategory::new(
            "dekaglsobel",
            gst::DebugColorFlags::empty(),
            Some("Deka's GL plugin for Sobel edge detection"),
        )
    });

    pub struct GlSobel {
        shader: Mutex<Option<gst_gl::GLShader>>,
    }

    impl GlSobel {}

    #[glib::object_subclass]
    impl ObjectSubclass for GlSobel {
        const NAME: &'static str = "GstDekaGlSobel";
        type Type = super::GlSobel;
        type ParentType = gst_gl::GLFilter;

        fn with_class(_klass: &Self::Class) -> Self {
            Self {
                shader: Mutex::new(None),
            }
        }
    }

    impl ObjectImpl for GlSobel {}
    impl GstObjectImpl for GlSobel {}
    impl ElementImpl for GlSobel {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static ELEMENT_METADATA: LazyLock<gst::subclass::ElementMetadata> =
                LazyLock::new(|| {
                    gst::subclass::ElementMetadata::new(
                        "Deka's GL sobel filter in rust sample",
                        "Filter/Effect/Video",
                        "Applies sobel kernel to image",
                        "Deka <speedcrash100@ya.ru>",
                    )
                });
            Some(&*ELEMENT_METADATA)
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            static PAD_TEMPLATES: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
                let src_caps = gst_video::VideoCapsBuilder::new()
                    .format_list([VideoFormat::Rgba])
                    .field("texture-target", "2D")
                    .features([gst_gl::CAPS_FEATURE_MEMORY_GL_MEMORY])
                    .build();

                let sink_caps = gst_video::VideoCapsBuilder::new()
                    .format_list([
                        VideoFormat::Rgba,
                        VideoFormat::Rgbx,
                        VideoFormat::Rgb,
                        VideoFormat::Nv12,
                    ])
                    .field("texture-target", "external-oes")
                    .features([gst_gl::CAPS_FEATURE_MEMORY_GL_MEMORY])
                    .build();

                vec![
                    gst::PadTemplate::new(
                        "sink",
                        gst::PadDirection::Sink,
                        gst::PadPresence::Always,
                        &sink_caps,
                    )
                    .unwrap(),
                    gst::PadTemplate::new(
                        "src",
                        gst::PadDirection::Src,
                        gst::PadPresence::Always,
                        &src_caps,
                    )
                    .unwrap(),
                ]
            });

            &PAD_TEMPLATES
        }
    }

    impl BaseTransformImpl for GlSobel {
        const MODE: BaseTransformMode = BaseTransformMode::NeverInPlace;
        const PASSTHROUGH_ON_SAME_CAPS: bool = false;
        const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;

        fn transform_caps(
            &self,
            direction: gst::PadDirection,
            caps: &gst::Caps,
            filter: Option<&gst::Caps>,
        ) -> Option<gst::Caps> {
            let other_caps = if direction == gst::PadDirection::Src {
                let mut caps = caps.clone();

                for s in caps.make_mut().iter_mut() {
                    s.set("texture-target", "external-oes");
                }

                caps
            } else {
                let mut caps = caps.clone();

                for s in caps.make_mut().iter_mut() {
                    s.set("texture-target", "2D");
                    s.set("format", VideoFormat::Rgba.to_str());
                }

                caps
            };

            gst::debug!(
                CAT,
                imp = self,
                "Transformed caps from {} to {} in direction {:?}",
                caps,
                other_caps,
                direction
            );

            // In the end we need to filter the caps through an optional filter caps to get rid of any
            // unwanted caps.
            if let Some(filter) = filter {
                Some(filter.intersect_with_mode(&other_caps, gst::CapsIntersectMode::First))
            } else {
                Some(other_caps)
            }
        }
    }

    impl GLBaseFilterImpl for GlSobel {
        fn gl_start(&self) -> Result<(), gst::LoggableError> {
            let obj = self.obj();
            let gl_base_filter = obj.upcast_ref::<gst_gl::GLBaseFilter>();

            let Some(ctx) = GLBaseFilterExt::context(gl_base_filter) else {
                return Err(gst::loggable_error!(CAT, "Cannot find GL context"));
            };

            let vert_stage = GLSLStage::new(&ctx, glow::VERTEX_SHADER);
            vert_stage.set_strings(
                gst_gl::GLSLVersion::None,
                gst_gl::GLSLProfile::ES | gst_gl::GLSLProfile::COMPATIBILITY,
                &[&include_str!("base.vert")],
            )?;
            if let Err(err) = vert_stage.compile() {
                return Err(gst::loggable_error!(CAT, "Vert compile error: {err}"));
            }
            let frag_stage = GLSLStage::new(&ctx, glow::FRAGMENT_SHADER);
            frag_stage.set_strings(
                gst_gl::GLSLVersion::None,
                gst_gl::GLSLProfile::ES | gst_gl::GLSLProfile::COMPATIBILITY,
                &[&include_str!("glsobel.frag")],
            )?;
            if let Err(err) = frag_stage.compile() {
                return Err(gst::loggable_error!(CAT, "Frag compile error: {err}"));
            }

            let shader = GLShader::new(&ctx);
            shader.attach(&vert_stage)?;
            shader.attach(&frag_stage)?;
            if let Err(err) = shader.link() {
                return Err(gst::loggable_error!(CAT, "Link error: {err}"));
            }

            *self.shader.lock().unwrap() = Some(shader);

            Ok(())
        }
    }
    impl GLFilterImpl for GlSobel {
        const ADD_RGBA_PAD_TEMPLATES: bool = false;
        const MODE: GLFilterMode = GLFilterMode::Texture;

        fn filter_texture(
            &self,
            input: &gst_gl::GLMemory,
            output: &gst_gl::GLMemory,
        ) -> Result<(), gst::LoggableError> {
            let shader_lock = self.shader.lock().unwrap();

            let Some(shader) = &*shader_lock else {
                return Err(gst::loggable_error!(CAT, "Shader is not loaded"));
            };

            let obj = self.obj();

            obj.render_to_target_with_shader(input, output, shader);

            Ok(())
        }
    }
}

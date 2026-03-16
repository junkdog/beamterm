//! glutin / winit boilerplate //

use std::num::NonZeroU32;

use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version,
    },
    display::{GetGlDisplay, GlDisplay},
    prelude::GlSurface,
    surface::{Surface, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::{
    dpi::LogicalSize,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

pub struct GlWindow {
    pub window: Window,
    gl_context: PossiblyCurrentContext,
    gl_surface: Surface<WindowSurface>,
    pub gl: glow::Context,
}

impl GlWindow {
    pub fn new(event_loop: &ActiveEventLoop, title: &str, size: (u32, u32)) -> Self {
        let window_attrs = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(LogicalSize::new(size.0, size.1));

        let config_template = ConfigTemplateBuilder::new().with_alpha_size(8);

        let (window, gl_config) =
            DisplayBuilder::new()
                .with_window_attributes(Some(window_attrs))
                .build(event_loop, config_template, |configs| {
                    configs
                        .reduce(|accum, config| {
                            if config.num_samples() > accum.num_samples() { config } else { accum }
                        })
                        .unwrap()
                })
                .expect("failed to build display");

        let window = window.expect("failed to create window");
        let gl_display = gl_config.display();

        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(Some(
                window
                    .window_handle()
                    .expect("failed to get window handle")
                    .into(),
            ));

        let not_current_context = unsafe { gl_display.create_context(&gl_config, &context_attrs) }
            .expect("failed to create GL context");

        let inner = window.inner_size();
        let surface_attrs = glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new()
            .build(
                window
                    .window_handle()
                    .expect("failed to get window handle")
                    .into(),
                NonZeroU32::new(inner.width).unwrap(),
                NonZeroU32::new(inner.height).unwrap(),
            );

        let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &surface_attrs) }
            .expect("failed to create GL surface");

        let gl_context = not_current_context
            .make_current(&gl_surface)
            .expect("failed to make GL context current");

        let _ = gl_surface
            .set_swap_interval(&gl_context, SwapInterval::DontWait);

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| gl_display.get_proc_address(name))
        };

        Self { window, gl_context, gl_surface, gl }
    }

    pub fn physical_size(&self) -> (i32, i32) {
        let s = self.window.inner_size();
        (s.width as i32, s.height as i32)
    }

    pub fn pixel_ratio(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    pub fn resize_surface(&self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(new_size.width).unwrap(),
            NonZeroU32::new(new_size.height).unwrap(),
        );
    }

    pub fn swap_buffers(&self) {
        self.gl_surface
            .swap_buffers(&self.gl_context)
            .expect("failed to swap buffers");
    }
}

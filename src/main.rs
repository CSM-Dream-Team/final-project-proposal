#![feature(conservative_impl_trait, fnbox)]

// Crates
#[macro_use]
extern crate log;
extern crate simplelog;
extern crate clap;
extern crate flight;
extern crate gfx;
extern crate nalgebra;
extern crate ncollide;
extern crate nphysics3d;
extern crate glutin;
extern crate gfx_device_gl;
extern crate gfx_window_glutin;

use std::boxed::FnBox;
use std::time::Instant;
use simplelog::{Config, TermLogger, LogLevelFilter};
use clap::Arg;
use gfx::{handle, Factory, texture, Device};
use gfx::format::*;
use gfx_device_gl::{NewTexture};
use gfx::memory::Typed;
use glutin::GlContext;
use nalgebra::{self as na, Vector3, Point3};
use ncollide::shape::{Plane};
use nphysics3d::object::{RigidBody};

use flight::{draw, Light};
use flight::vr::*;

mod app;
pub mod common;
pub mod geo;

// use app::{App, halo, home, lets_get_physical, snowflakes, workshop};
use app::{App, snowflakes};
use common::{Common, Gurus, Meshes, Painters};
use common::gurus::{interact, physics};

pub const NEAR_PLANE: f64 = 0.1;
pub const FAR_PLANE: f64 = 75.;
pub const BACKGROUND: [f32; 4] = [0.529, 0.808, 0.980, 1.0];
const PI: f32 = ::std::f32::consts::PI;
const PI2: f32 = 2. * PI;

fn main() {
    // Logging setup
    TermLogger::init(LogLevelFilter::Info, Config::default()).unwrap();

    // Command line arguments
    let matches = clap::App::new("VR")
        .arg(Arg::with_name("mock")
             .short("m")
             .long("mock")
             .help("Use mock VR API"))
        .get_matches();
    let mock = matches.is_present("mock");

    // VR init
    let mut vrctx = match if mock { VrContext::mock() } else { VrContext::new() } {
        Some(v) => v,
        None => {
            error!("Could not create VrContext, exiting");
            return
        },
    };

    // Set clipping planes
    vrctx.near = NEAR_PLANE;
    vrctx.far = FAR_PLANE;

    // Get some frame sizeing information
    let (render_width, render_height) = vrctx.retrieve_size();

    // Window manager stuff
    let mut events_loop = glutin::EventsLoop::new();
    let window_builder = glutin::WindowBuilder::new()
        .with_visibility(false)
        .with_dimensions(render_width, render_height)
        .with_title("Mock OpenVR Display");
    let context = glutin::ContextBuilder::new();
    // Fuuny thing I found here: changing `_window` to `_` (ignoring it) makes everything explode
    // because of early drop.
    let (window, mut device, mut factory, wcolor, wdepth) =
        gfx_window_glutin::init::<Rgba8, DepthStencil>(window_builder, context, &events_loop);

    // Create texture to render to
    let (tex, texture_id) = {
        let desc = texture::Info {
            kind: texture::Kind::D2(render_width as u16, render_height as u16, texture::AaMode::Single),
            levels: 1,
            format: R8_G8_B8_A8::get_surface_type(),
            bind: gfx::RENDER_TARGET | gfx::SHADER_RESOURCE,
            usage: gfx::memory::Usage::Data,
        };

        let raw = factory.create_texture_raw(desc, Some(ChannelType::Unorm), None).unwrap();
        let mut manager = handle::Manager::new();
        let texture_id = match *manager.ref_texture(&raw) {
            NewTexture::Texture(t) => t as u32,
            _ => panic!("Something went wrong here"),
        };
        (Typed::new(raw), texture_id)
    };
    vrctx.set_texture(texture_id);

    // Create depth buffer
    let (.., depth) = factory.create_depth_stencil(render_width as u16, render_height as u16).unwrap();

    let surface = factory.view_texture_as_render_target::<(R8_G8_B8_A8, Unorm)>(&tex, 0, None).unwrap();
    let mut applications: Vec<Box<App<_, _>>> = vec![
        // Box::new(halo::Halo::new()),
        // Box::new(home::Home::new()),
        // Box::new(lets_get_physical::LetsGetPhysical::new()),
        Box::new(snowflakes::Snowflakes::new(&mut factory).unwrap()),
        // Box::new(workshop::Workshop::new()),
    ];

    // setup context
    let mut ctx = draw::DrawParams {
        encoder: factory.create_command_buffer().into(),
        color: if mock { wcolor } else { surface },
        depth: if mock { wdepth } else { depth },
        left: Default::default(),
        right: Default::default(),
    };

    if mock { window.show() }

    // Setup Controllers
    let mut primary = MappedController::new(primary());
    let mut secondary = MappedController::new(secondary());

    // Setup Common stuff
    let mut meshes = Meshes::new(&mut factory).unwrap();
    let mut painters = Painters::new(&mut factory).unwrap();

    // Main loop
    vrctx.start();
    let mut running = true;
    let mut last_time: Option<Instant> = None;
    while running {
        // Calculate dt
        let dt = if let Some(last) = last_time {
            let elapsed = last.elapsed();
            elapsed.as_secs() as f64 + (elapsed.subsec_nanos() as f64 * 1e-9)
        } else {
            0.
        };
        last_time = Some(Instant::now());

        let moment = vrctx.sync();

        let hmd = match moment.hmd() {
            Some(h) => h.clone(),
            None => continue,
        };

        // Update controllers
        match (primary.update(&moment), secondary.update(&moment)) {
            (Ok(_), Ok(_)) => (),
            _ => warn!("Error updating controllers"),
        }

        // Update context
        running = !moment.exit;
        ctx.left = hmd.left;
        ctx.right = hmd.right;

        // Create Common
        let mut common = Common {
            draw_params: ctx,
            gurus: Gurus {
                interact: interact::InteractGuru::new(&primary, &secondary),
                physics: physics::PhysicsGuru::new(Vector3::new(0., -9.81, 0.)),
            },
            meshes: meshes,
            painters: painters,
        };

         // Clear targets
        common.draw_params.encoder.clear_depth(&common.draw_params.depth, FAR_PLANE as f32);
        common.draw_params.encoder.clear(&common.draw_params.color, [BACKGROUND[0].powf(1. / 2.2), BACKGROUND[1].powf(1. / 2.2), BACKGROUND[2].powf(1. / 2.2), BACKGROUND[3]]);

        // Configure lights
        common.painters.pbr.cfg(|s| {
            s.ambient(BACKGROUND);
            s.lights(&[
                Light {
                    pos: moment.stage * Point3::new((0. * PI2 / 3.).sin() * 2., 4., (0. * PI2 / 3.).cos() * 2.),
                    color: [1.0, 0.8, 0.8, 15.],
                },
                Light {
                    pos: moment.stage * Point3::new((1. * PI2 / 3.).sin() * 2., 4., (1. * PI2 / 3.).cos() * 2.),
                    color: [0.8, 1.0, 0.8, 15.],
                },
                Light {
                    pos: moment.stage * Point3::new((2. * PI2 / 3.).sin() * 2., 4., (2. * PI2 / 3.).cos() * 2.),
                    color: [0.8, 0.8, 1.0, 15.],
                },
                Light {
                    pos: moment.stage * Point3::new(0., -8., 0.),
                    color: [1.0, 1.0, 1.0, 110.],
                },
            ]);
        });

        // Draw controllers
        for cont in moment.controllers() {
            common.painters.pbr.draw(&mut common.draw_params, na::convert(cont.pose), &common.meshes.controller);
        }

        // Draw floor
        let stage = na::try_convert(moment.stage).unwrap_or(na::one());
        let floor = Plane::new(Vector3::y());
        common.gurus.interact.primary.laser(&stage, &floor);
        let mut floor_rb = RigidBody::new_static(floor, 0.1, 0.6);
        floor_rb.set_margin(0.00001);
        common.gurus.physics.body(floor_rb);
        common.painters.pbr.draw(&mut common.draw_params, na::convert(stage), &common.meshes.floor);

        // Resolve Gurus
        // Draw frame
        let mut common_reply;
        {
            let futures: Vec<_> = applications.iter_mut().map(|app| app.update(&mut common)).collect();
            common_reply = common.resolve(dt.min(0.01) as f32);
            for f in futures {
                FnBox::call_box(f, (&mut common_reply, ));
            }
        }

        ctx = common_reply.draw_params;
        meshes = common_reply.meshes;
        painters = common_reply.painters;

        // Send instructions to OpenGL
        // TODO: Move flush to separate thread
        ctx.encoder.flush(&mut device);

        // Send resulting texture to VR device
        moment.submit(&mut vrctx);
        if mock { window.swap_buffers().unwrap() }

        // Cleanup GFX data
        device.cleanup();

        // Window Events
        events_loop.poll_events(|event| {
            match event {
                // process events here
                glutin::Event::WindowEvent { event: glutin::WindowEvent::Closed, .. } =>
                    running = false,
                _ => ()
            }
        });
    }
    vrctx.stop();
}

#[macro_use]
extern crate ash;
extern crate cgmath;
extern crate image;
extern crate regex;
extern crate winit;

mod config;
mod nurbs;
mod object;
mod renderer;
mod scene;

use cgmath::{Deg, Matrix4, Point3, Rad};
use config::Config;
use nurbs::{NURBSpline, Order};
use object::Camera;
use renderer::{MainPass, PresentPass, RenderState};
use scene::Scene;
use std::time::{Duration, SystemTime};

fn main() {
    // init stuff
    let cfg = Config::read_config("options.cfg");

    let mut renderstate = RenderState::init(&cfg);
    let mut presentpass = PresentPass::init(&renderstate);
    let mut mainpass = MainPass::init(&renderstate, &cfg);
    let mut scene = Scene::new(&renderstate, &mainpass);
    let camera = Camera::new(Point3::new(0.0, 0.0, 0.0));
    let fov_horizontal = 90.0;
    let aspect_ratio = cfg.render_dimensions.0 as f32 / cfg.render_dimensions.1 as f32;
    let fov_vertical = Rad::from(Deg(fov_horizontal / aspect_ratio));
    let near = 1.0;
    let far = 1000.0;
    // Need to flip projection matrix due to the Vulkan NDC coordinates.
    // See https://matthewwellings.com/blog/the-new-vulkan-coordinate-system/ for details.
    let glu_projection_matrix = cgmath::perspective(fov_vertical, aspect_ratio, near, far);
    let vulkan_ndc = Matrix4::new(
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        -1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.5,
        0.5,
        0.0,
        0.0,
        0.0,
        1.0,
    );
    let projection_matrix = vulkan_ndc * glu_projection_matrix;

    let points = vec![
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
        Point3::new(-1.0, 0.0, 0.0),
        Point3::new(0.0, -1.0, 0.0),
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(0.0, 0.0, -1.0),
        Point3::new(0.0, 1.0, -1.0),
        Point3::new(1.0, 0.0, -1.0),
    ];

    let mut u = 0.0;
    let step = 0.1;
    let spline = NURBSpline::new(Order::CUBIC, points);

    while u < spline.eval_limit() {
        let _point = spline.evaluate_at(u);
        u += step;
    }

    // main loop
    let mut running = true;
    let mut framecount: u64 = 0;
    // aim for 60fps = 16.66666... ms
    let delta_time = Duration::from_millis(17);
    let mut elapsed_time = Duration::new(0, 0);
    let mut accumulator = Duration::new(0, 0);
    let mut current_time = SystemTime::now();

    while running {
        let new_time = SystemTime::now();
        let frame_time = new_time
            .duration_since(current_time)
            .expect("duration_since failed :(");
        current_time = new_time;
        accumulator += frame_time;

        while accumulator >= delta_time {
            scene.update();
            //animation, physics engine, scene progression etc. goes here
            accumulator -= delta_time;
            elapsed_time += delta_time;
        }

        // Update the view matrix uniform buffer
        let view_matrix = camera.generate_view_matrix();

        // Do the main rendering
        let main_cmd_buf = mainpass.begin_frame(&renderstate);
        scene.draw(
            main_cmd_buf,
            mainpass.pipeline_layout,
            &view_matrix,
            &projection_matrix,
        );
        mainpass.end_frame(&renderstate);

        // Present the rendered image
        presentpass.present_image(&renderstate, &mut mainpass.render_image);
        framecount += 1;

        if framecount % 100 == 0 {
            let frame_time_ms = frame_time.subsec_nanos() as f64 / 1_000_000.0;
            println!(
                "frametime: {}ms => {} FPS",
                frame_time_ms,
                1_000.0 / frame_time_ms
            );
        }

        renderstate.event_loop.poll_events(|ev| match ev {
            winit::Event::WindowEvent {
                event: winit::WindowEvent::Closed,
                ..
            } => running = false,
            _ => (),
        });
    }

    //cleanup
}

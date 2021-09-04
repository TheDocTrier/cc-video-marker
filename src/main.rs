//! Provides a Slick Video Clip to Identify CC-BY-SA Content

use clap::{App, AppSettings::DeriveDisplayOrder, Arg};
use rayon::prelude::*;
use std::{
    cell::RefCell,
    io::{stdout, Write},
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
};
use tiny_skia::*;
use usvg::*;

// build a standard rectangle version of animation, then fit to specified resolution and fps

/// Loads an svg file and produces an svg tree
fn load_svg(path: &str) -> Tree {
    let data = std::fs::read(&path).unwrap();
    let opt = Options::default();
    Tree::from_data(&data, &opt).unwrap()
}

thread_local! {
    static LAYOUT: RefCell<Tree> = RefCell::new(load_svg("layout.svg"));
}

type LKRC<T> = std::thread::LocalKey<RefCell<T>>;

fn clone_rc<T>(x: &'static LKRC<T>) -> T
where
    T: Clone,
{
    x.with(|f| f.borrow().clone())
}

#[derive(Debug, Clone, Copy)]
struct Time(f64);

impl Time {
    fn wait(&self, t0: f64) -> Self {
        Self(self.0 - t0)
    }

    fn until<F: FnOnce(Time) -> ()>(&self, dt: f64, f: F) -> Self {
        if 0.0 <= self.0 {
            f(Time(self.0.min(dt)));
        }
        *self
    }

    fn during<F: FnOnce(Time) -> ()>(&self, t: f64, f: F) -> Self {
        if 0.0 <= self.0 {
            f(*self);
        }
        self.wait(t)
    }

    fn until_during<F: FnOnce(Time) -> ()>(&self, dt: f64, t: f64, f: F) -> Self {
        self.during(t, |time| {
            time.until(dt, f);
        })
    }
}

fn slide_in(t: f64, n: &mut Node) {
    // quadratic slide [0,1]
    unimplemented!()
}

fn fade_in(t: f64, n: &mut Node) {
    // quadratic fade [0,1]
    unimplemented!()
}

#[derive(Debug, Clone, Copy)]
struct Resolution {
    width: u32,
    height: u32,
}

impl From<(u32, u32)> for Resolution {
    fn from(pair: (u32, u32)) -> Self {
        Self {
            width: pair.0,
            height: pair.1,
        }
    }
}

/// The default resolution is 2160p
// maybe move this info the to clap app

type Scene = dyn Fn(u32) -> Tree + Sync + Send;

#[derive(Clone, Copy)]
struct Renderer<'a> {
    resolution: Resolution,
    /// Desired framerate in units of fps
    framerate: f64,
    /// The total length of the video in frames
    frame_length: u32,
    /// Method for generating each frame
    scene: &'a Scene,
}

enum FrameError {
    NewPixmap,
    RenderSVG,
    SavePng,
}

impl<'a> Renderer<'a> {
    fn new(resolution: Resolution, framerate: f64, frame_length: u32, scene: &'a Scene) -> Self {
        Self {
            resolution,
            framerate,
            frame_length,
            scene,
        }
    }

    fn render_frame(&self, frame_time: u32) -> Result<(), FrameError> {
        let tree = (self.scene)(frame_time);
        let mut pixmap = Pixmap::new(self.resolution.width, self.resolution.height)
            .ok_or(FrameError::NewPixmap)?;
        resvg::render(
            &tree,
            FitTo::Height(self.resolution.height),
            pixmap.as_mut(),
        )
        .ok_or(FrameError::RenderSVG)?;
        pixmap
            .save_png(format!("frames/{:06}.png", frame_time + 1))
            .map_err(|_| FrameError::SavePng)?;
        Ok(())
    }

    fn render(&self) -> Result<(), ()> {
        Command::new("ffmpeg")
            .args(&[
                // specify framerate
                "-framerate",
                &self.framerate.to_string(),
                // specify resolution
                "-s",
                &format!("{}x{}", self.resolution.width, self.resolution.height),
                // give location of rendered frames
                "-i",
                "frames/%06d.png",
                // provide other options
                "-y",
                "-vcodec",
                "libx264",
                "-crf",
                "15",
                "-pix_fmt",
                "yuv420p",
                "video.mp4",
            ])
            .spawn()
            .map_err(|_| ())?
            .wait()
            .map_err(|_| ())?;
        Ok(())
    }
}

fn main() -> Result<(), ()> {
    let matches = App::new("cc-video-marker")
        .version("1.0")
        .author("Michael Bradley <thedoctrier@gmail.com>")
        .about("Produces a video which can be used to mark other videos as cc-by-sa")
        .setting(DeriveDisplayOrder)
        .arg(
            Arg::with_name("resolution")
                .short("r")
                .help("Resolution of video as 'WIDTHxHEIGHT'")
                .default_value("3840x2160"),
        )
        .arg(
            Arg::with_name("framerate")
                .short("f")
                .help("Framerate in units of fps")
                .default_value("60.0"),
        )
        .arg(
            Arg::with_name("delay")
                .short("D")
                .help("Seconds of intro blank")
                .default_value("0.5"),
        )
        .arg(
            Arg::with_name("interval")
                .short("I")
                .help("Seconds between introducing each symbol")
                .default_value("0.2"),
        )
        .arg(
            Arg::with_name("entry")
                .short("E")
                .help("Seconds of animation for each symbol")
                .default_value("0.2"),
        )
        .arg(
            Arg::with_name("sustain")
                .short("S")
                .help("Seconds of leaving symbols on screen")
                .default_value("1.5"),
        )
        .arg(
            Arg::with_name("fade")
                .short("F")
                .help("Seconds of fade to blank")
                .default_value("0.5"),
        )
        .arg(
            Arg::with_name("leave")
                .short("L")
                .help("Seconds of outro blank")
                .default_value("0.5"),
        )
        .get_matches();

    let resolution: Resolution = {
        let v: Vec<u32> = matches
            .value_of("resolution")
            .unwrap()
            .split("x")
            .map(|s| s.parse().expect("non-integer resolution"))
            .collect();
        (v[0], v[1]).into()
    };
    let match_f64 = |name: &str| {
        matches
            .value_of(name)
            .unwrap()
            .parse()
            .expect(&format!("non-float {}", name))
    };
    let framerate = match_f64("framerate");
    let delay = match_f64("delay");
    let interval = match_f64("interval");
    let entry = match_f64("entry");
    let sustain = match_f64("sustain");
    let fade = match_f64("fade");
    let leave = match_f64("leave");
    let length = delay + sustain + fade + leave;

    let scene = move |frame_time: u32| {
        let mut layout = clone_rc(&LAYOUT);

        Time((frame_time as f64) / framerate)
            .wait(delay)
            .during(sustain, |time| {
                time.until_during(entry, interval, |time| {
                    // animate cc
                    let mut cc = layout.node_by_id("cc").unwrap();
                    fade_in(time.0 / entry, &mut cc);
                    slide_in(time.0 / entry, &mut cc);
                })
                .until_during(entry, interval, |time| {
                    // animate by
                })
                .until_during(entry, interval, |time| {
                    // animate sa
                })
                .until(entry, |time| {
                    // animate text
                });
            })
            .until_during(fade, fade, |time| {
                // animate fade
            })
            .wait(leave);

        layout
    };
    let r = Renderer::new(resolution, framerate, (length * framerate) as u32, &scene);

    let finished_frames = AtomicU32::new(0);
    let make_progress = || {
        let i = finished_frames.fetch_add(1, Ordering::Relaxed);
        print!("\rRendering video frames ({}/{})", i + 1, r.frame_length);
        stdout().flush().unwrap();
    };

    std::fs::create_dir_all("frames").expect("could not create frames directory");
    let _: Vec<()> = (0..r.frame_length)
        .into_par_iter()
        .map(|time| {
            let f = r.render_frame(time);
            assert!(f.is_ok(), "failed to render frame {}", time);
            make_progress();
        })
        .collect();
    println!(); // finish progress

    println!("Running ffmpeg to convert frames into video");
    r.render()
}

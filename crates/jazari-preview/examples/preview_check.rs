//! Renders SWF frames to PNG files on the software adapter and prints what
//! the movies logged. Never touches a hardware GPU.
//!
//! ```text
//! cargo run -p jazari-preview --example preview_check -- <out_dir> <file.swf>[:1,5,10] ...
//! ```
//!
//! Without arguments it renders the two reference movies of the milestone-1
//! check into `target/preview-check`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, bail};
use jazari_preview::{OffscreenRenderer, run_headless};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\dev\jazari\target\preview-check"));
    let mut jobs: Vec<(PathBuf, Vec<u32>)> =
        args.map(|a| parse_job(&a)).collect::<Result<_, _>>()?;
    if jobs.is_empty() {
        jobs = vec![
            (
                PathBuf::from(r"C:\dev\samples\fixtures\shapes-basic\shapes-basic.swf"),
                vec![1],
            ),
            (
                PathBuf::from(r"C:\dev\samples\f8\smoke.swf"),
                vec![1, 10, 20],
            ),
        ];
    }
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let started = Instant::now();
    let renderer = OffscreenRenderer::new()?;
    let info = renderer.adapter_info();
    println!(
        "adapter: {} ({:?}, {:?}) in {:?}",
        info.name,
        info.backend,
        info.device_type,
        started.elapsed()
    );
    if info.device_type != wgpu::DeviceType::Cpu {
        bail!("refusing to render on a non-software adapter");
    }

    for (swf_path, frames) in jobs {
        let swf =
            std::fs::read(&swf_path).with_context(|| format!("reading {}", swf_path.display()))?;
        let stem = swf_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("movie")
            .to_owned();
        let started = Instant::now();
        let output = renderer.render(&swf, &frames, 1.0)?;
        println!(
            "{}: {} frame(s) in {:?}, halted: {}",
            swf_path.display(),
            frames.len(),
            started.elapsed(),
            output.halted
        );
        for (frame, image) in frames.iter().zip(&output.images) {
            let path = out_dir.join(format!("{stem}-f{frame:03}.png"));
            image
                .save(&path)
                .with_context(|| format!("writing {}", path.display()))?;
            println!(
                "  wrote {} ({}x{})",
                path.display(),
                image.width(),
                image.height()
            );
        }
        for line in &output.log {
            println!("  log {:?}: {}", line.kind, line.text);
        }
        let last = frames.iter().copied().max().unwrap_or(1);
        let headless = run_headless(&swf, last)?;
        println!("  headless run of {last} frame(s), no renderer:");
        for line in &headless {
            println!("    {:?}: {}", line.kind, line.text);
        }
    }
    Ok(())
}

/// `path.swf` or `path.swf:1,5,10`.
fn parse_job(arg: &str) -> anyhow::Result<(PathBuf, Vec<u32>)> {
    // A Windows drive letter also contains ':', so split at the last one
    // only when what follows looks like a frame list.
    if let Some((path, list)) = arg.rsplit_once(':')
        && !list.is_empty()
        && list.chars().all(|c| c.is_ascii_digit() || c == ',')
    {
        let frames = list
            .split(',')
            .map(|f| f.parse::<u32>().with_context(|| format!("bad frame {f:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok((Path::new(path).to_path_buf(), frames));
    }
    Ok((PathBuf::from(arg), vec![1]))
}

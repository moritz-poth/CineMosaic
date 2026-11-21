//! Command line for testing the core: cinemosaic <master> <cut list> <out.png|tif|jpg|pdf> [options]

use cinemosaic::{Gate, Job, avb, bin, export, frames, grab_stills, layout, master_jobs, picks, read_cut_list};
use std::path::PathBuf;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let usage = "usage: cinemosaic <master> <cut list> <out.png|tif|jpg|pdf> [options]\n       cinemosaic --bin <bin.avb> <volume[:volume…]> <out|--list> [--seq N[,N…] (default: all sequences)] [options]\noptions: [--paper A4|<w>x<h>mm] [--landscape] [--dpi 300] [--margin 10] [--gap 3] [--cols N] [--picture 2.39]";
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let bin_mode = args.first().is_some_and(|a| a == "--bin");
    if bin_mode {
        args.remove(0);
    }
    if args.len() < 3 {
        return Err(usage.into());
    }
    let (master, list, out) = (PathBuf::from(&args[0]), PathBuf::from(&args[1]), PathBuf::from(&args[2]));
    let list_only = out.as_os_str() == "--list";
    let format = if list_only { export::Format::Png } else { export::Format::from_path(&out).ok_or(usage)? };
    let (mut seq, mut picture) = (None::<Vec<usize>>, None);
    let mut sheet = layout::Sheet { width_mm: 210.0, height_mm: 297.0, dpi: 300.0, margin_mm: 10.0, gap_mm: 3.0, columns: None, ..Default::default() };
    let mut landscape = false;
    let mut it = args[3..].iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().ok_or(format!("{a} needs a value"));
        let num = |v: &str| v.parse::<f64>().map_err(|_| format!("not a number: {v}"));
        match a.as_str() {
            "--paper" => {
                let v = val()?;
                if let Some(d) = layout::DIN.iter().find(|d| d.0.eq_ignore_ascii_case(v)) {
                    (sheet.width_mm, sheet.height_mm) = (d.1, d.2);
                } else {
                    let (w, h) = v.trim_end_matches("mm").split_once('x').ok_or(usage)?;
                    (sheet.width_mm, sheet.height_mm) = (num(w)?, num(h)?);
                }
            }
            "--landscape" => landscape = true,
            "--dpi" => sheet.dpi = num(val()?)?,
            "--margin" => sheet.margin_mm = num(val()?)?,
            "--gap" => sheet.gap_mm = num(val()?)?,
            "--cols" => sheet.columns = Some(num(val()?)? as u32),
            "--seq" => seq = Some(val()?.split(',').map(|v| num(v).map(|n| n as usize)).collect::<Result<_, _>>()?),
            "--picture" => picture = Some(layout::parse_aspect(val()?).ok_or(usage)?),
            _ => return Err(usage.into()),
        }
    }
    if landscape {
        (sheet.width_mm, sheet.height_mm) = (sheet.height_mm.max(sheet.width_mm), sheet.height_mm.min(sheet.width_mm));
    }
    let tc = |f: i64, r| cinemosaic::timeline::format_tc(f, r);
    let (shots, picks, jobs, rate) = if bin_mode {
        let b = avb::Bin::read(&master)?;
        let names = bin::sequences(&b);
        for (i, n) in names.iter().enumerate() {
            eprintln!("sequence {i}: {n}");
        }
        let cut = bin::BinCut::open(&master, b, seq.as_deref(), &|| true)?;
        let shots = cut.shots();
        let volumes: Vec<PathBuf> = list.to_string_lossy().split(':').map(PathBuf::from).collect();
        // shortcut: the bin gives no frame size before grabbing, so the grid is planned for 16:9 (or --picture)
        let picks = picks(&shots, &sheet, picture.unwrap_or(16.0 / 9.0));
        let jobs = cut.jobs(&shots, &picks, &volumes, &|| true, "en");
        (shots, picks, jobs, cut.rate())
    } else {
        let info = frames::probe(&master)?;
        let shots = read_cut_list(&list, info.rate)?.shots();
        eprintln!("master starts at {}", tc(info.start_tc, info.rate));
        let picks = picks(&shots, &sheet, picture.unwrap_or(info.aspect));
        let jobs = master_jobs(&shots, &picks, &master, &info, "en");
        (shots, picks, jobs, info.rate)
    };
    eprintln!("{} shots, {} stills", shots.len(), picks.len());
    if list_only {
        for (&(i, part), j) in picks.iter().zip(&jobs) {
            let s = &shots[i];
            let what = match j {
                Ok(Job { file, frame, .. }) => format!("{} frame {frame}", file.file_name().unwrap_or_default().to_string_lossy()),
                Err(e) => format!("ERROR {e}"),
            };
            println!("V{} {} - {} {part:?} {}  {}  -> {what}", s.track + 1, tc(s.start, rate), tc(s.end, rate), tc(s.at(part), rate), s.name);
        }
        return Ok(());
    }
    let dir = std::env::temp_dir().join(format!("cinemosaic-{}", std::process::id()));
    let gate = Gate::default();
    let stills = grab_stills(&jobs, &dir, &gate, &|d, t| eprint!("\r{d}/{t}")).ok_or("cancelled")?;
    eprintln!();
    for (&(i, part), r) in picks.iter().zip(&stills) {
        if let Err(e) = r {
            eprintln!("{} {}: {e}", tc(shots[i].at(part), rate), shots[i].name);
        }
    }
    let first = stills.iter().find_map(|r| r.as_ref().ok().and_then(|p| image::image_dimensions(p).ok()));
    let aspect = picture.unwrap_or(first.map_or(16.0 / 9.0, |(w, h)| w as f64 / h as f64));
    let page = layout::render(&sheet, stills.len(), aspect, |i| {
        let img = stills[i].as_ref().ok().and_then(|p| image::open(p).ok())?.to_rgb8();
        Some(match picture {
            Some(a) => layout::crop(&img, a),
            None => img,
        })
    });
    export::save(&page, &out, sheet.dpi, format)?;
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!("saved {} ({}×{} px)", out.display(), page.width(), page.height());
    Ok(())
}

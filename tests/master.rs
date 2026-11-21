//! End to end on a generated master: every frame's red/green value encodes its frame number, so the
//! grabbed still proves the exact frame. Needs ffmpeg.

use cinemosaic::{Gate, frames, grab_stills, master_jobs, read_cut_list};
use std::process::Command;

#[test]
fn grabs_the_exact_middle_frame_of_every_shot() {
    let dir = std::env::temp_dir().join(format!("cinemosaic-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let master = dir.join("master.mov");
    // 300 frames at 25 fps, timecode 10:00:00:00, lossless RGB
    let ok = Command::new(frames::tool("ffmpeg"))
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=black:s=64x36:r=25:d=12,format=rgb24,geq=r='mod(N,256)':g='floor(N/256)*40':b=128"])
        .args(["-c:v", "png", "-timecode", "10:00:00:00"])
        .arg(&master)
        .status()
        .expect("ffmpeg must be installed for this test");
    assert!(ok.success());
    let info = frames::probe(&master).unwrap();
    assert_eq!((info.rate.num, info.rate.den, info.start_tc, info.width), (25, 1, 900_000, 64));
    let edl = dir.join("cut.edl");
    std::fs::write(
        &edl,
        "TITLE: T\nFCM: NON-DROP FRAME\n\
001  AX V C 00:00:00:00 00:00:02:00 10:00:00:00 10:00:02:00\n\
002  AX V C 00:00:00:00 00:00:00:01 10:00:02:00 10:00:02:01\n\
003  AX V C 00:00:00:00 00:00:07:00 10:00:04:00 10:00:11:00\n",
    )
    .unwrap();
    let shots = read_cut_list(&edl, info.rate).unwrap().shots();
    let jobs = master_jobs(&shots, &cinemosaic::timeline::fill(&shots, 0), &master, &info, "en");
    let stills = grab_stills(&jobs, &dir.join("s"), &Gate::default(), &|_, _| {}).unwrap();
    // middles: 0 + 50/2 = 25, 50, 100 + 175/2 = 187
    let got: Vec<u32> = stills
        .iter()
        .map(|p| {
            let px = *image::open(p.as_ref().unwrap()).unwrap().to_rgb8().get_pixel(10, 10);
            px[0] as u32 + px[1] as u32 / 40 * 256
        })
        .collect();
    assert_eq!(got, [25, 50, 187]);
    std::fs::remove_dir_all(&dir).unwrap();
}

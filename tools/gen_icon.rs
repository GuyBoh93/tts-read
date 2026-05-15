// Generates assets/icon.png — the 1024×1024 application icon used by
// cargo-packager to produce .ico (Windows) and .icns (macOS) on installer
// build. Run with: cargo run --example gen_icon
//
// "T" silhouette built from two pill bars in the Whisper FreeFlow logo
// language. Geometry duplicated from src/tray.rs so the two stay in sync.

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const SIZE: u32 = 1024;
    let pixmap = render_t_logo(SIZE);
    std::fs::create_dir_all("assets")?;
    pixmap.save_png("assets/icon.png")?;
    println!("Wrote assets/icon.png ({}×{})", SIZE, SIZE);
    Ok(())
}

fn render_t_logo(size: u32) -> Pixmap {
    let mut pixmap = Pixmap::new(size, size).expect("pixmap");
    pixmap.fill(Color::TRANSPARENT);

    let s = size as f32 / 64.0;

    let mut paint = Paint::default();
    paint.set_color_rgba8(250, 249, 245, 255);
    paint.anti_alias = true;

    let bars: [(f32, f32, f32, f32); 2] = [
        (4.0, 6.0, 56.0, 12.0),
        (26.0, 18.0, 12.0, 44.0),
    ];

    for (bx, by, bw, bh) in bars {
        let x = bx * s;
        let y = by * s;
        let w = bw * s;
        let h = bh * s;
        let r = w.min(h) / 2.0;

        let mut pb = PathBuilder::new();
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
        if let Some(p) = pb.finish() {
            pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    pixmap
}

mod app;
mod camera;
mod filters;
mod puzzle;

fn main() -> Result<(), eframe::Error> {
    // let puzzle = puzzle::MixupCube::new();
    // for piece in &puzzle.pieces {
    //     let non_none_count = piece.stickers.iter().filter(|s| s.side.is_some()).count();
    //     println!(
    //         "piece with {} stickers, with {} non-None sides",
    //         piece.stickers.len(),
    //         non_none_count
    //     );
    //     // for sticker in &piece.stickers {
    //     //     println!(
    //     //         "sticker with {} verts on side {:?}",
    //     //         sticker.verts.len(),
    //     //         sticker.side
    //     //     );
    //     // }
    //     println!();
    // }
    // panic!();

    eframe::run_native(
        "speedsun",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

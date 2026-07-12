mod app;
mod commands;
mod filters;
mod keybinds;
mod puzzle_state;
mod puzzle_view;
mod render;
mod simulation;
mod styles;

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
        eframe::NativeOptions {
            // the puzzle is rendered with wgpu directly (see render.rs).
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

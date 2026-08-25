// The floor under the floor: a pure agb ROM, no tish anywhere.
//
// bench-boot's `floor.tish` is an import-free tish program, which measures the runtime's own startup
// but cannot separate it from the hardware's. This does the SAME two things floor.tish does — set
// backdrop colour 0x101018, then present frames forever — with nothing between it and agb. The gap
// between the two first-paints is what the tish runtime costs to start.
//
// Keep this in step with floor.tish. If they stop doing the same work the subtraction means nothing.
#![no_std]
#![no_main]

extern crate alloc;

use agb::display::Rgb;

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let mut gfx = gba.graphics.get();

    // Same colour and same call tish-agb's `backdrop()` makes, so neither side is paying for a
    // different kind of write.
    let c = Rgb::new(0x10, 0x10, 0x18).to_rgb15();
    gfx.set_background_palette_colour(0, 0, c);

    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}

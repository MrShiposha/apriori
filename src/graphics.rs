use super::r#type::{Color, ColorChannel, PackedColor};
use rand::{thread_rng, Rng};

pub fn random_color() -> Color {
    let mut rng = thread_rng();
    Color::new(
        rng.gen_range(0.0, 1.0),
        rng.gen_range(0.0, 1.0),
        rng.gen_range(0.0, 1.0),
    )
}

// pub fn opposite_color(color: &Color) -> Color {
//     Color::new(1.0 - color[0], 1.0 - color[1], 1.0 - color[2])
// }

pub fn pack_color(color: &Color) -> PackedColor {
    let r = (color[0] * std::u8::MAX as ColorChannel) as u8;
    let g = (color[1] * std::u8::MAX as ColorChannel) as u8;
    let b = (color[2] * std::u8::MAX as ColorChannel) as u8;

    let channel_bits = 8 * std::mem::size_of::<u8>();

    r as PackedColor
        | ((g as PackedColor) << channel_bits)
        | ((b as PackedColor) << 2 * channel_bits)
}

pub fn unpack_color(color: &PackedColor) -> Color {
    let channel_bits = 8 * std::mem::size_of::<u8>();

    let r = (color & 0xFF) as ColorChannel;
    let g = ((color >> channel_bits) & 0xFF) as ColorChannel;
    let b = ((color >> 2*channel_bits) & 0xFF) as ColorChannel;

    Color::new(r, g, b) / std::u8::MAX as f32
}

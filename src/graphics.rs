use rand::{thread_rng, Rng};
use super::r#type::Color;

pub fn random_color() -> Color {
    let mut rng = thread_rng();
    Color::new(
        rng.gen_range(0.0, 1.0),
        rng.gen_range(0.0, 1.0),
        rng.gen_range(0.0, 1.0)
    )
}
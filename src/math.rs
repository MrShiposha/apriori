use super::r#type::{
    Vector,
    RelativeTime
};

pub fn hermite_interpolation(
    location_0: &Vector,
    velocity_0: &Vector,
    time_0: RelativeTime,
    location_1: &Vector,
    velocity_1: &Vector,
    time_1: RelativeTime,
    interest_time: RelativeTime
) -> Vector {
    let step = time_1 - time_0;
    let t = (interest_time - time_0) / step;

    let t2 = t * t;
    let t3 = t2 * t;

    let p1_coeff = -2.0 * t3 + 3.0 * t2;
    let p0_coeff = -p1_coeff + 1.0;

    let p0 = location_0.scale(p0_coeff);
    let m0 = velocity_0.scale((t3 - 2.0 * t2 + t) * step);

    let p1 = location_1.scale(p1_coeff);
    let m1 = velocity_1.scale((t3 - t2) * step);

    p0 + m0 + p1 + m1
}
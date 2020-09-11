use {
    crate::{
        object::GenCoord,
        r#type::{Coord, Distance},
    },
    lr_tree::*
};

pub fn radius_mbr(coord: &GenCoord, radius: Distance) -> MBR<Coord> {
    macro_rules! min_max {
        ($expr:expr) => {
            ($expr - radius, $expr + radius)
        };
    }

    let t = coord.time();

    let location = coord.location();
    let (x_min, x_max) = min_max![location[0]];
    let (y_min, y_max) = min_max![location[1]];
    let (z_min, z_max) = min_max![location[2]];

    mbr! {
        t = [t; t],
        x = [x_min; x_max],
        y = [y_min; y_max],
        z = [z_min; z_max]
    }
}

pub fn coords_mbr(lhs: &GenCoord, rhs: &GenCoord) -> MBR<Coord> {
    let lhs_t = lhs.time();

    let location = lhs.location();
    let lhs_x = location[0];
    let lhs_y = location[0];
    let lhs_z = location[0];

    let rhs_t = rhs.time();

    let location = rhs.location();
    let rhs_x = location[0];
    let rhs_y = location[0];
    let rhs_z = location[0];

    mbr! {
        t = [lhs_t; rhs_t],
        x = [lhs_x; rhs_x],
        y = [lhs_y; rhs_y],
        z = [lhs_z; rhs_z]
    }
}
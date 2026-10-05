//! Small linear-algebra helpers for mapping between coordinate spaces.

/// Applies column-major modelview + projection and the viewport transform to a 2D point.
pub fn project(mv: &[f32; 16], p: &[f32; 16], viewport: &[i32; 4], x: f32, y: f32) -> (f32, f32) {
    let eye = mul(mv, [x, y, 0.0, 1.0]);
    let clip = mul(p, eye);
    let w = if clip[3].abs() > f32::EPSILON { clip[3] } else { 1.0 };
    let (nx, ny) = (clip[0] / w, clip[1] / w);
    let px = viewport[0] as f32 + (nx + 1.0) * 0.5 * viewport[2] as f32;
    let py = viewport[1] as f32 + (ny + 1.0) * 0.5 * viewport[3] as f32;
    (px, py)
}

fn mul(m: &[f32; 16], v: [f32; 4]) -> [f32; 4] {
    let mut out = [0f32; 4];
    for (row, o) in out.iter_mut().enumerate() {
        *o = m[row] * v[0] + m[4 + row] * v[1] + m[8 + row] * v[2] + m[12 + row] * v[3];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orthographic_projection_maps_to_viewport() {
        let identity = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        // glOrtho(0, 100, 0, 50, -1, 1)
        let ortho = [2.0 / 100.0, 0.0, 0.0, 0.0, 0.0, 2.0 / 50.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, -1.0, -1.0, 0.0, 1.0];
        let viewport = [0, 0, 200, 100];
        assert_eq!(project(&identity, &ortho, &viewport, 50.0, 25.0), (100.0, 50.0));
        assert_eq!(project(&identity, &ortho, &viewport, 100.0, 50.0), (200.0, 100.0));
    }
}

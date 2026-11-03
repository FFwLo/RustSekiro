//! Shared animation interpolation and source-space motion maths.

/// Havok curve 0 (SMOOTH), traced at sekiro.exe RVA 0x1408650.
pub fn smoothstep(fraction: f32) -> f32 {
    let t = fraction.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rotate source X/Z by yaw. Source X is mirrored when placed in the world.
pub fn rotate_xz([x, z]: [f32; 2], yaw: f32) -> [f32; 2] {
    let (sin, cos) = yaw.sin_cos();
    [cos * x + sin * z, -sin * x + cos * z]
}

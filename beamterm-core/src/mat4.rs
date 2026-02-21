#[derive(Debug, Clone)]
pub(crate) struct Mat4 {
    pub data: [f32; 16],
}

impl Mat4 {
    pub fn new_identity() -> Self {
        let mut data = [0.0; 16];
        data[0] = 1.0;
        data[5] = 1.0;
        data[10] = 1.0;
        data[15] = 1.0;
        Self { data }
    }

    pub fn orthographic_from_size(width: f32, height: f32) -> Self {
        Self::new_orthographic(0.0, width, height, 0.0, -1.0, 1.0)
    }

    pub fn new_orthographic(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let mut result = Self::new_identity();
        let data = &mut result.data;

        data[0] = 2.0 / (right - left);
        data[5] = 2.0 / (top - bottom);
        data[10] = -2.0 / (far - near);

        data[12] = -(right + left) / (right - left);
        data[13] = -(top + bottom) / (top - bottom);
        data[14] = -(far + near) / (far - near);

        result
    }
}

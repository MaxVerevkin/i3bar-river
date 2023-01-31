#[derive(Debug, Default)]
pub struct ButtonManager<T = usize>(Vec<(f64, f64, T)>);

impl<T> ButtonManager<T> {
    pub fn push(&mut self, x_offset: f64, width: f64, elem: T) {
        self.0.push((x_offset, width, elem));
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn click(&self, x: f64) -> Option<&T> {
        self.0
            .iter()
            .find(|(x_off, w, _)| x >= *x_off && x <= *x_off + *w)
            .map(|(_, _, e)| e)
    }
}

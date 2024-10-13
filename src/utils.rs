pub struct Defered<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Defered<F> {
    pub fn abort(mut self) {
        self.0.take();
    }
}

impl<F: FnOnce()> Drop for Defered<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f()
        }
    }
}

pub fn defer<F: FnOnce()>(f: F) -> Defered<F> {
    Defered(Some(f))
}

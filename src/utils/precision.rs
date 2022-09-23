pub trait Precision {
    fn max_precision(self, max_precision: u32) -> Self;
}

impl Precision for f64 {
    fn max_precision(self, max_precision: u32) -> Self {
        let p = f64::from(10i32.pow(max_precision));
        (self * p).round() / p
    }
}

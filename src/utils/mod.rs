pub mod interval;
pub mod shutdown;

pub fn m_to_nm(m: f64) -> f64 {
    m / 1852.0
}

pub fn nm_to_m(nm: f64) -> f64 {
    nm * 1852.0
}

pub fn m_to_ft(m: f64) -> f64 {
    m * 3.28084
}

pub fn ft_to_m(ft: f64) -> f64 {
    ft / 3.28084
}

pub fn ft_to_nm(ft: f64) -> f64 {
    ft / 6076.118
}

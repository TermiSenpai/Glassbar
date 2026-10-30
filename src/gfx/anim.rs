//! Animaciones de DirectComposition con curvas polinómicas. El compositor de Windows las ejecuta.

use windows::Win32::Graphics::DirectComposition::*;
use windows::core::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Ease {
    OutCubic,
    InCubic,
    Linear,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AnimSpec {
    pub from: f32,
    pub to: f32,
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub ease: Ease,
}

impl AnimSpec {
    pub const fn new(from: f32, to: f32, duration_ms: u32, delay_ms: u32, ease: Ease) -> AnimSpec {
        AnimSpec { from, to, duration_ms, delay_ms, ease }
    }
}

/// Coeficientes (constante, lineal, cuadrático, cúbico) del polinomio en t∈[0,d] que lleva de `a` a `b`.
pub fn coeffs(ease: Ease, a: f32, b: f32, d: f32) -> (f32, f32, f32, f32) {
    let r = b - a;
    match ease {
        Ease::Linear => (a, r / d, 0.0, 0.0),
        Ease::OutCubic => (a, 3.0 * r / d, -3.0 * r / (d * d), r / (d * d * d)),
        Ease::InCubic => (a, 0.0, 0.0, r / (d * d * d)),
    }
}

/// Evalúa el polinomio (solo aritmética, sin ejecución de código).
pub fn eval(c: (f32, f32, f32, f32), t: f32) -> f32 {
    c.0 + c.1 * t + c.2 * t * t + c.3 * t * t * t
}

pub fn build(dcomp: &IDCompositionDesktopDevice, s: &AnimSpec) -> Result<IDCompositionAnimation> {
    unsafe {
        let a = dcomp.CreateAnimation()?;
        let d = (s.duration_ms.max(1)) as f32 / 1000.0;
        let delay = s.delay_ms as f64 / 1000.0;
        if s.delay_ms > 0 {
            a.AddCubic(0.0, s.from, 0.0, 0.0, 0.0)?;
        }
        let (c, l, q, cu) = coeffs(s.ease, s.from, s.to, d);
        a.AddCubic(delay, c, l, q, cu)?;
        a.End(delay + d as f64, s.to)?;
        Ok(a)
    }
}

pub fn set_opacity(visual: &IDCompositionVisual2, v: f32) -> Result<()> {
    let v3: IDCompositionVisual3 = visual.cast()?;
    unsafe { v3.SetOpacity2(v) }
}

pub fn set_offset(visual: &IDCompositionVisual2, x: f32, y: f32) -> Result<()> {
    unsafe {
        visual.SetOffsetX2(x)?;
        visual.SetOffsetY2(y)
    }
}

pub fn apply_opacity(dcomp: &IDCompositionDesktopDevice, visual: &IDCompositionVisual2, s: &AnimSpec, animate: bool) -> Result<()> {
    if animate {
        let v3: IDCompositionVisual3 = visual.cast()?;
        unsafe { v3.SetOpacity(&build(dcomp, s)?) }
    } else {
        set_opacity(visual, s.to)
    }
}

pub fn apply_offset_x(dcomp: &IDCompositionDesktopDevice, visual: &IDCompositionVisual2, s: &AnimSpec, animate: bool) -> Result<()> {
    unsafe {
        if animate { visual.SetOffsetX(&build(dcomp, s)?) } else { visual.SetOffsetX2(s.to) }
    }
}

pub fn apply_offset_y(dcomp: &IDCompositionDesktopDevice, visual: &IDCompositionVisual2, s: &AnimSpec, animate: bool) -> Result<()> {
    unsafe {
        if animate { visual.SetOffsetY(&build(dcomp, s)?) } else { visual.SetOffsetY2(s.to) }
    }
}

/// Escala uniforme alrededor de `center` (coordenadas del visual).
pub fn apply_scale(dcomp: &IDCompositionDesktopDevice, visual: &IDCompositionVisual2, s: &AnimSpec, center: (f32, f32), animate: bool) -> Result<()> {
    unsafe {
        let t = dcomp.CreateScaleTransform()?;
        t.SetCenterX2(center.0)?;
        t.SetCenterY2(center.1)?;
        if animate {
            let a = build(dcomp, s)?;
            t.SetScaleX(&a)?;
            t.SetScaleY(&a)?;
        } else {
            t.SetScaleX2(s.to)?;
            t.SetScaleY2(s.to)?;
        }
        visual.SetTransform(&t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_cubic_hits_endpoints_and_is_monotonic() {
        let c = coeffs(Ease::OutCubic, 0.0, 1.0, 0.26);
        assert!((eval(c, 0.0)).abs() < 1e-5 && (eval(c, 0.26) - 1.0).abs() < 1e-4);
        let mut prev = 0.0;
        for i in 1..=26 {
            let v = eval(c, 0.01 * i as f32);
            assert!(v >= prev - 1e-5);
            prev = v;
        }
        assert!(eval(c, 0.13) > 0.8);
    }

    #[test]
    fn in_cubic_starts_slow() {
        let c = coeffs(Ease::InCubic, 1.0, 0.0, 0.11);
        assert!(eval(c, 0.055) > 0.8);
        assert!(eval(c, 0.11).abs() < 1e-4);
    }

    #[test]
    fn linear_is_linear() {
        let c = coeffs(Ease::Linear, 2.0, 4.0, 1.0);
        assert!((eval(c, 0.5) - 3.0).abs() < 1e-6);
    }
}

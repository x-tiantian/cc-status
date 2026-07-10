//! 灯的软件光栅化:把圆形灯(含抗锯齿与外发光)画进 ARGB 缓冲。
//!
//! 输出为 BGRA、预乘 alpha 的像素,直接供 `UpdateLayeredWindow` 使用(需求 §7)。

use crate::status::Rgb;

/// 一块 32bpp 像素缓冲(自上而下,BGRA,预乘 alpha)。
pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u32>,
}

impl Canvas {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            pixels: vec![0u32; (width.max(1) * height.max(1)) as usize],
        }
    }

    /// 全部清空为透明。
    pub fn clear(&mut self) {
        self.pixels.iter_mut().for_each(|p| *p = 0);
    }

    /// 混合一个像素(source-over)。`a`/`r`/`g`/`b` 为非预乘的 0..=255,`cov` 0.0..=1.0。
    #[inline]
    fn blend(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8, a: f32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let a = a.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let idx = (y * self.width + x) as usize;
        let dst = self.pixels[idx];
        let (dr, dg, db, da) = unpack(dst);
        // source-over(非预乘域计算后再预乘存储)。
        let sa = a;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0 {
            self.pixels[idx] = 0;
            return;
        }
        let out_r = (r as f32 * sa + dr * da * (1.0 - sa)) / out_a;
        let out_g = (g as f32 * sa + dg * da * (1.0 - sa)) / out_a;
        let out_b = (b as f32 * sa + db * da * (1.0 - sa)) / out_a;
        self.pixels[idx] = pack_premul(out_r, out_g, out_b, out_a);
    }

    /// 在 (cx,cy) 画一个半径 radius 的抗锯齿实心圆,外加柔和光晕。
    /// `intensity` 0.0..=1.0 控制整体亮度(用于呼吸/闪烁动画)。
    pub fn draw_dot(&mut self, cx: f32, cy: f32, radius: f32, color: Rgb, intensity: f32) {
        let glow = radius * 0.9; // 光晕向外延伸的范围
        let outer = radius + glow;
        let x0 = (cx - outer).floor() as i32;
        let x1 = (cx + outer).ceil() as i32;
        let y0 = (cy - outer).floor() as i32;
        let y1 = (cy + outer).ceil() as i32;
        let intensity = intensity.clamp(0.0, 1.0);

        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let dist = (dx * dx + dy * dy).sqrt();

                // 实心圆:边缘 1px 抗锯齿过渡。
                let core = if dist <= radius - 0.5 {
                    1.0
                } else if dist < radius + 0.5 {
                    radius + 0.5 - dist
                } else {
                    0.0
                };

                // 外发光:从半径处向外线性衰减。
                let halo = if dist > radius && dist < outer {
                    (1.0 - (dist - radius) / glow) * 0.35
                } else {
                    0.0
                };

                let alpha = (core.max(halo)) * intensity;
                if alpha > 0.0 {
                    self.blend(x, y, color.r, color.g, color.b, alpha);
                }
            }
        }

        // 高光点:让灯更有"玻璃感"。
        let hl_r = radius * 0.4;
        let hx = cx - radius * 0.3;
        let hy = cy - radius * 0.3;
        for y in (hy - hl_r).floor() as i32..=(hy + hl_r).ceil() as i32 {
            for x in (hx - hl_r).floor() as i32..=(hx + hl_r).ceil() as i32 {
                let dx = x as f32 + 0.5 - hx;
                let dy = y as f32 + 0.5 - hy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < hl_r {
                    let a = (1.0 - dist / hl_r) * 0.5 * intensity;
                    self.blend(x, y, 255, 255, 255, a);
                }
            }
        }
    }
}

#[inline]
fn unpack(p: u32) -> (f32, f32, f32, f32) {
    // 存储为预乘 BGRA(内存字节序 B,G,R,A);取回时反预乘到非预乘域。
    let a = ((p >> 24) & 0xFF) as f32 / 255.0;
    let r = ((p >> 16) & 0xFF) as f32 / 255.0;
    let g = ((p >> 8) & 0xFF) as f32 / 255.0;
    let b = (p & 0xFF) as f32 / 255.0;
    if a > 0.0 {
        (r * 255.0 / a, g * 255.0 / a, b * 255.0 / a, a)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    }
}

#[inline]
fn pack_premul(r: f32, g: f32, b: f32, a: f32) -> u32 {
    // UpdateLayeredWindow 要求预乘 alpha。DIB 内存字节序为 B,G,R,A,
    // 即 u32(小端)= (A<<24)|(R<<16)|(G<<8)|B。
    let a = a.clamp(0.0, 1.0);
    let rr = (r.clamp(0.0, 255.0) * a) as u32;
    let gg = (g.clamp(0.0, 255.0) * a) as u32;
    let bb = (b.clamp(0.0, 255.0) * a) as u32;
    let aa = (a * 255.0) as u32;
    (aa << 24) | (rr << 16) | (gg << 8) | bb
}

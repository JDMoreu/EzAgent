//! Bit-exact port of Pillow's composition helpers.
//!
//! [`alpha_composite`] reproduces `ImagingAlphaComposite` (7 bits of extra
//! precision plus the `src.a == 0` copy). The [`Image::alpha_composite`],
//! [`Image::paste`] and [`Image::paste_masked`] methods mirror the in-place
//! Pillow methods, including clipping at the canvas edges. `paste_masked` uses
//! Pillow's `BLEND`/`DIV255` integer matte. [`linear_gradient`] is Pillow's
//! 256×256 `Image.linear_gradient("L")`.

use std::sync::OnceLock;

use crate::raster::{div255, Image, Mask, Rgba};
use crate::GfxError;

const PRECISION_BITS: u32 = 7;
const COEF_ONE: u32 = 255 * (1 << PRECISION_BITS);
const ROUND: u32 = 0x80 << PRECISION_BITS;

/// Pillow `Image.alpha_composite(background, overlay)`.
///
/// Both images must share dimensions; the overlay's straight alpha is
/// composited over the background and a new image is returned.
pub fn alpha_composite(background: &Image, overlay: &Image) -> Result<Image, GfxError> {
    if background.width() != overlay.width() || background.height() != overlay.height() {
        return Err(GfxError::SizeMismatch);
    }
    let mut out = background.clone();
    out.alpha_composite(overlay, (0, 0));
    Ok(out)
}

/// Pillow `Image.linear_gradient("L")`: a 256×256 top-to-bottom ramp.
pub fn linear_gradient() -> Mask {
    let mut data = vec![0; 256 * 256];
    for (y, row) in data.as_chunks_mut::<256>().0.iter_mut().enumerate() {
        row.fill(y as u8);
    }
    Mask::from_raw(256, 256, data).expect("a 256x256 mask has a valid buffer")
}

impl Image {
    /// In-place `Image.alpha_composite(overlay, dest)`: composites `overlay`
    /// with its top-left corner at `dest`, clipping what falls outside.
    pub fn alpha_composite(&mut self, overlay: &Image, dest: (i32, i32)) {
        let Some((sx0, sy0, sx1, sy1)) = clipped_source(
            self.width(),
            self.height(),
            (overlay.width(), overlay.height()),
            dest,
        ) else {
            return;
        };
        for sy in sy0..sy1 {
            let dy = (i64::from(dest.1) + i64::from(sy)) as u32;
            let source = overlay.row(sy);
            let target = self.row_mut(dy);
            for sx in sx0..sx1 {
                let source_index = sx as usize * 4;
                let target_index = (i64::from(dest.0) + i64::from(sx)) as usize * 4;
                let destination = Rgba::from_array([
                    target[target_index],
                    target[target_index + 1],
                    target[target_index + 2],
                    target[target_index + 3],
                ]);
                let pixel = Rgba::from_array([
                    source[source_index],
                    source[source_index + 1],
                    source[source_index + 2],
                    source[source_index + 3],
                ]);
                let blended = composite_pixel(destination, pixel);
                target[target_index..target_index + 4].copy_from_slice(&blended.to_array());
            }
        }
    }

    /// In-place `Image.paste(source, dest)` without a mask: an opaque copy,
    /// clipped to the canvas.
    pub fn paste(&mut self, source: &Image, dest: (i32, i32)) {
        let Some((sx0, sy0, sx1, sy1)) = clipped_source(
            self.width(),
            self.height(),
            (source.width(), source.height()),
            dest,
        ) else {
            return;
        };
        let bytes = (sx1 - sx0) as usize * 4;
        for sy in sy0..sy1 {
            let dy = (i64::from(dest.1) + i64::from(sy)) as u32;
            let dx = (i64::from(dest.0) + i64::from(sx0)) as usize * 4;
            let sx = sx0 as usize * 4;
            self.row_mut(dy)[dx..dx + bytes].copy_from_slice(&source.row(sy)[sx..sx + bytes]);
        }
    }

    /// In-place `Image.paste(source, dest, mask)` with an `L` matte.
    ///
    /// Every channel, alpha included, is blended with Pillow's integer
    /// `BLEND(mask, dst, src)`. The mask must have the source's dimensions.
    pub fn paste_masked(
        &mut self,
        source: &Image,
        dest: (i32, i32),
        mask: &Mask,
    ) -> Result<(), GfxError> {
        if mask.width() != source.width() || mask.height() != source.height() {
            return Err(GfxError::SizeMismatch);
        }
        let Some((sx0, sy0, sx1, sy1)) = clipped_source(
            self.width(),
            self.height(),
            (source.width(), source.height()),
            dest,
        ) else {
            return Ok(());
        };
        for sy in sy0..sy1 {
            let dy = (i64::from(dest.1) + i64::from(sy)) as u32;
            let source_row = source.row(sy);
            let mask_row = mask.row(sy);
            let target = self.row_mut(dy);
            for sx in sx0..sx1 {
                let amount = u32::from(mask_row[sx as usize]);
                let inverse = 255 - amount;
                let index = (i64::from(dest.0) + i64::from(sx)) as usize * 4;
                for channel in 0..4 {
                    let dst = u32::from(target[index + channel]);
                    let src = u32::from(source_row[sx as usize * 4 + channel]);
                    target[index + channel] = div255(dst * inverse + src * amount);
                }
            }
        }
        Ok(())
    }
}

/// Alpha-composites one straight-alpha pixel, following `AlphaComposite.c`.
fn composite_pixel(dst: Rgba, src: Rgba) -> Rgba {
    if src.a == 0 {
        return dst;
    }
    if src.a == 255 || dst.a == 0 {
        return src;
    }
    let coefficients = coefficients().lookup(src.a, dst.a);
    let coef1 = u32::from(coefficients.coef1);
    let coef2 = COEF_ONE - coef1;
    let blend = |source: u8, destination: u8| -> u8 {
        let value = u32::from(source) * coef1 + u32::from(destination) * coef2;
        (shift_for_div255(value + ROUND) >> PRECISION_BITS) as u8
    };
    Rgba::new(
        blend(src.r, dst.r),
        blend(src.g, dst.g),
        blend(src.b, dst.b),
        coefficients.out_alpha,
    )
}

const fn shift_for_div255(value: u32) -> u32 {
    ((value >> 8) + value) >> 8
}

/// Precomputed `(src.a, dst.a)` coefficients, as `AlphaComposite.c` derives
/// them per pixel. The table keeps the inner loop free of divisions.
#[derive(Clone, Copy)]
struct Coefficients {
    coef1: u16,
    out_alpha: u8,
}

struct CoefficientTable(Vec<Coefficients>);

impl CoefficientTable {
    fn build() -> Self {
        let mut table = vec![
            Coefficients {
                coef1: 0,
                out_alpha: 0,
            };
            256 * 256
        ];
        for src_alpha in 1..256u32 {
            for dst_alpha in 0..256u32 {
                let out_alpha_255 = src_alpha * 255 + dst_alpha * (255 - src_alpha);
                let coef1 = src_alpha * 255 * 255 * (1 << PRECISION_BITS) / out_alpha_255;
                table[(src_alpha * 256 + dst_alpha) as usize] = Coefficients {
                    coef1: coef1 as u16,
                    out_alpha: shift_for_div255(out_alpha_255 + 0x80) as u8,
                };
            }
        }
        Self(table)
    }

    fn lookup(&self, src_alpha: u8, dst_alpha: u8) -> Coefficients {
        self.0[src_alpha as usize * 256 + dst_alpha as usize]
    }
}

static COEFFICIENTS: OnceLock<CoefficientTable> = OnceLock::new();

fn coefficients() -> &'static CoefficientTable {
    COEFFICIENTS.get_or_init(CoefficientTable::build)
}

/// Clipped source rectangle `(x0, y0, x1, y1)` for `dest` on a canvas.
fn clipped_source(
    canvas_width: u32,
    canvas_height: u32,
    source: (u32, u32),
    dest: (i32, i32),
) -> Option<(u32, u32, u32, u32)> {
    let dx = i64::from(dest.0);
    let dy = i64::from(dest.1);
    let x0 = (-dx).max(0);
    let y0 = (-dy).max(0);
    let x1 = (i64::from(canvas_width) - dx).min(i64::from(source.0));
    let y1 = (i64::from(canvas_height) - dy).min(i64::from(source.1));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0 as u32, y0 as u32, x1 as u32, y1 as u32))
}

#[cfg(test)]
mod tests {
    use super::{alpha_composite, linear_gradient};
    use crate::raster::{Image, Mask, Rgba};
    use crate::test_support::{fnv1a, unhex};

    const ALPHA_BG: &str = "6c4912f0c48229f39d70d3bf5a42912a86b94303e943c064828ce544ad263232cc29f78561fc87bc931991a048dc1ff074456019cd666eeb258c6e8103e6c902953133c0061370e2279f13b168d5354f57b174ea51be272639f5f6f468b345e0c8726f481d2dac75f5e5b28472642d55873544b3df697b000b9ab36247afb6b2fb1f3242c9a9601d3362538a18fec0bd8f79880fac0b0f33ec442ed5824f6fd6f4d7ef9d97490c6bef3f95a5ed4763686df42e5a48c9a4ccde33ab1e9e50c7b926322e5bd4f5027a3ae40fe24fe324ec65173bd2408e7f623d7667eefcbbf4a9188a93e4f8307c980f260c435ca9d4f463e60be289d54582d8c64ca2b79f11073ad84ccfd32c6f448fc57b725b07632722b950a062d89430de38538830e2102c08468d57b91b25722d13567ef2f908f7f803bf9fa1fa8d3ce588aab52cfd476c0db303a959c81a4404dc980f1a0a5135239c82e3fac17c0200e68df2da29597bdf44e40dbb451a95ddc3a7640a9b2a0a267b08ae7817d5486d704c4105ce757823ffde403978999b168cfb89035a9eaa9f7597bcfeeba540cf1c4f552967b47cd4f7e312c3c3e61b1196e2e489e1baea91430afc3bef5eb918e3973c56813a046432aabc365db4726eed1660257d9d3f2ad9075f16597ecc0d4d23b790610939ca1156de6c8110ecfba8424512fe97c8deb424a206b23d2a9c68ef2b449d99a3e38c6256b858068bbe993cde073ae79b8c0751dec1c7ee1ae3021a36ab4f02907b42f8eb2fd77ac87ab3d70409ef5eb50e52263ed541ec24f25b284d34298ecd85929afbf67abbae2f0f5740255f6a293053b68d45e0cfdf20f46ab97503c912827576a70508158df9c35dd2c83513486d6065f3d265a040116263c7368ff46fad12e0355abd92c8686d2df38ce4c08143b7d27f2ba31840430b0eca8671cc81e9e383de61f8ea078a1e760cf0857893eaaff326f3613ff4e7873e0c83570062490ecb207e175bd3221d4964e512251c7249dea23fe47e71e6fba24917f1d9152637001bcc505dd5558de58a5857f04dbb4d64a4ddbafed126b4934a2bba38f3879b7368c71ce501adea925aa47f7b53a8e8f687044853fb8c3712140caa1aab6dfbb102260ca560eb43c3a0431f2bd789e0eefc85b97a0e8de16231851243a186f9a0247e3a8224bfd3689bedb999aa8e1c5528ce0d814230b5b362a540ce6754160ea5be63c789e404871295d786d1eb6cf2dc09cb23a1f56977265f2b338ab6220bec12789bf7fd1b9bd6b76aea55c89f13c7884736180576dcea8cdb40c32a3939dea3ee54043480889b6f67e3fa9abc2b386ad2882207a5e8330dbbbe0d4a9a0c10b470729ae68f5a316ef6f22486460b7f70a770a11ffab8abe45db12962f07992773a1332400ae607721870c29fc2f8f56482fcc6954b0c47c66df4c9";
    const ALPHA_FG: &str = "3c6f6000eba2c4a8ac5c647b561d2be4b90c8a12dd094dea82aa35a865d2a01b568de705bd302a6b3fffbddf15f0ffbbe67436c3e7b83f401d6a3d7b3a5762e58555a460c35c42ac62d9a8550e4939973366836851b43127dde5c2bab3de65fc89aa22b3da3f2f45fc893c09cbbe12ff676e3392b0d544f250b26782a1413f328476a4430ba648ce6e7b0d3eb2bb170d483d3e97f83d7c36c1cccb445373b541c3ceedf2facf2d1d7b09235449b02ffa90fcdaa084b2b69a29700634ed8b9b239e33493bee7d1dbf2bcda9d752a37cefd29c73b71486878edc441adf4a2313ad96d62d8814087992c7f910c97e848094b9d4b6727daaeb851056f887491caebf20847d12976a80170f0e645c361a01d0a63775b0c07fe0942dac73e2c72ab3b385bc4299c64dbcd24af53b7d6ee4c8b68ba50e161cedc50681f214cc1a0af8af38d8963683a01a46a154d6da19453019dfb611d9847833ced23f6786f0a52016f95932ca45abeff1ef9d4e6590766730a5ce68b85a33ca490cae6c1cdeacc068fdaee13977dacbccd7534136737f38b76bb6bcfe79a87c375b5639089c27f619ccd293d94e9075bb6133d0f570354e377cdfe9705705ecf0299759e282c74d67a9a6b8b09ddd06d17e21900b24fea959261dec70266ff4c787c16a757b365f79bd673a46f02d8950b1c70b70376de2af0e776625cf8e6568aed4734734914f22a0923e2b96229f0360a0d097d6296978e3a62ad635f24508748e63a222e05743af62663204aedb764ad8906e81b1f8441877faad76e24b457b08fc55217ed22edc95e0e49050a141c65b367624012e3ce9613d5e27388c16ff0c14c1d561615570b3bdb801a6d4a2756f4141cfe8f85c6eec9b27b1942c5d7f4425bb631a5bb231d2d8d4ce0b5bfdb1c430a2326d6b20e71d0abc001720749a1b704f456ef6f655747ae30ae41aec9d999be0a977d38599805d395a657f3dfe0e0943a792152976ff0ec2f016a18e43856b0dd0b7b9d0b336de2120549dc5731e0f9b3cd69588eb01ab7baee70f49751a57ce0363509c19decd0f1e2c0b14bc752abdb89ede383d5caca45ea75285f483962b23737a63b2e3267e02b8262ab6eec90543125abc3e07a9e17d89c6bb059677459245704020d2611a378a8b51c8d45a1b653e820a1959ec85c801c25ff222bf08b7721d36a65c35f6d5d933333e6ee496f3a5bafcd94c02b3e4046857ca0716b17fe421def283f2686fbdb4034b41f2cde49640b8f51b4cd21d0f7698f45d7d9663b9e982fa30d9af989933b8a204a142e3e71c4eaf16785aca38110ad0456a0079db3560db1e5556aee24738e06f370ef13c4788241e25a7fc33289b261e966b1fb9b45cb37145501aee5bf16e1385e641e5f205bd312bd9d61bd63864e76c181eb6a9d3d1e1a154a9dfce4032166ee14a4cfb6a";
    const CANVAS: &str = "bfddad9504144464c4df707bdf88f527ee91fa81f3ddf7f3ed1c75af14dd264e2bab1b7c115c87034d7f57ec719194f74794203d6bff40439311c6bfd17229e7ef56e0d0eac9084cef9289595d01e7ccb6bc37253b9263a16cb355bd02e55afb04e76c4be5e62384a2a2e44144cededc4020d741ef16065edb7072b0a13461e5f7bf5482bf514f69257f6206fe951bf39cf26fe9d977fb7e0cdfe8409c2bf92fd95d55b9aaeeaf6de1c844ff00806d6125cfd50019dc14602f12e26ab2c5ff4b";
    const TILE: &str = "9622f37148fb6ec8c7f16a5fac1ea372a70857a78aa1de6b0e1f851229ae03170e18e99b0fc6293922e2fe8da785b880";
    const TILE2: &str = "677fcdd44eaac4c9135019c55da2fc17c892a5eabfeeaffdd65c50e285738e2b5806776ed69df978c9b35ae50c7b87fa";
    const PASTE_CLIP_DST: &str = "25f728598a0f754600814ed8f21997f6272c5acdba01a2d9b883ec409887a1fcab910e1833e8c0fb5d1030d77f1b8fcb6fb881623ac634a7fecf1546e19e52028b7c466511c99d58c348b47d8b2d1d9f56872bccd1a7ef1e5c7f4aa68021b10862136c9c570ff0bd6e3f61ea186d4367e173e439147290627f4773754f5caaec11d6b154efa4fac418d15639967af2a0078cba65a6c3417475a6ed2df464988f96ee403cc13f378d3873aca3e6903956ad1b54a1d5c950f0cfa33dd2f70a3c62b722441a7967ea1516585e2f7203fb27c795aae650f26fb2487525dd5ec15d49a213db893328f3dd52d58187a65bc670476e7200a1d880a38a862a3c";
    const PASTE_CLIP_SRC: &str = "0cbdf18198fa96181151705a8e29a273698719f314efb3260f2538e6f672ac451ee315c15d9ed971f3f1db8d0b2bb30e2fc8dc0ad9212dd8a2f0bd37ade49017fdbf19aa8b97bc470fa715a16eafa62b06715fa621ec2d5737398769e6d7efc7e2a1b6fc";
    const PASTE_CLIP_MASK: &str = "5b2a73be187f7ba8364fef204d2845d91a658705734035d6a4";
    const PASTE_OPAQUE_DST: &str = "51e0fe4efdcd4724055fa1f263a68315b2feab3fb1a4936fb08817bddcb58291aef1b456e6c968113190fdc8b842e04a31dfeed98358f794e4a223c30d2ebfd7";

    fn image(width: u32, height: u32, hex: &str) -> Image {
        Image::from_rgba(width, height, unhex(hex)).expect("the oracle emitted a valid image")
    }

    #[test]
    fn alpha_composite_matches_pillow_exhaustive_tables() {
        let colors = [
            ((10, 20, 30), (200, 100, 50), 0xac64_d136_1193_37f1),
            ((0, 0, 0), (255, 255, 255), 0xd37e_644c_af2c_c317),
            ((255, 7, 128), (3, 250, 99), 0xd460_0f50_18b5_0983),
        ];
        for (background_rgb, overlay_rgb, expected) in colors {
            let mut background = Vec::with_capacity(256 * 256 * 4);
            let mut overlay = Vec::with_capacity(256 * 256 * 4);
            for dst_alpha in 0..=255u8 {
                for src_alpha in 0..=255u8 {
                    background.extend_from_slice(&[
                        background_rgb.0,
                        background_rgb.1,
                        background_rgb.2,
                        dst_alpha,
                    ]);
                    overlay.extend_from_slice(&[
                        overlay_rgb.0,
                        overlay_rgb.1,
                        overlay_rgb.2,
                        src_alpha,
                    ]);
                }
            }
            let background = Image::from_rgba(256, 256, background).expect("full table");
            let overlay = Image::from_rgba(256, 256, overlay).expect("full table");
            let result = alpha_composite(&background, &overlay).expect("same size");
            assert_eq!(fnv1a(result.as_raw()), expected);
        }
    }

    #[test]
    fn alpha_composite_matches_explicit_vectors() {
        let composite = |dst: Rgba, src: Rgba| {
            alpha_composite(&Image::new(1, 1, dst), &Image::new(1, 1, src))
                .expect("1x1 images match")
                .pixel(0, 0)
        };
        assert_eq!(
            composite(Rgba::new(10, 20, 30, 128), Rgba::new(200, 100, 50, 128)),
            Rgba::new(137, 73, 43, 192)
        );
        assert_eq!(
            composite(Rgba::new(10, 20, 30, 200), Rgba::new(200, 100, 50, 1)),
            Rgba::new(11, 20, 30, 200)
        );
        assert_eq!(
            composite(Rgba::new(10, 20, 30, 0), Rgba::new(200, 100, 50, 7)),
            Rgba::new(200, 100, 50, 7)
        );
        assert_eq!(
            composite(Rgba::new(10, 20, 30, 0), Rgba::new(200, 100, 50, 0)),
            Rgba::new(10, 20, 30, 0)
        );
    }

    #[test]
    fn alpha_composite_matches_pillow_random_and_in_place() {
        let background = image(16, 16, ALPHA_BG);
        let overlay = image(16, 16, ALPHA_FG);
        let result = alpha_composite(&background, &overlay).expect("same size");
        assert_eq!(fnv1a(result.as_raw()), 0xd3b6_6b5e_6d93_6983);

        let mut canvas = image(6, 8, CANVAS);
        canvas.alpha_composite(&image(3, 4, TILE), (-2, 1));
        assert_eq!(fnv1a(canvas.as_raw()), 0x99bf_04e9_532d_5327);

        let mut corner = Image::new(8, 6, Rgba::TRANSPARENT);
        corner.alpha_composite(&image(3, 4, TILE2), (4, 5));
        assert_eq!(fnv1a(corner.as_raw()), 0x5e6d_5f57_cd70_bee4);
    }

    #[test]
    fn paste_mask_matches_pillow_tables() {
        let (mut dst_data, mut src_data) = (Vec::new(), Vec::new());
        let mut mask_data = Vec::new();
        for y in 0..256u32 {
            for x in 0..256u32 {
                dst_data.extend_from_slice(&[y as u8, 255 - y as u8, (y / 2) as u8, y as u8]);
                src_data.extend_from_slice(&[255, 128, 1, 0]);
                mask_data.push(x as u8);
            }
        }
        let mut image_ = Image::from_rgba(256, 256, dst_data).expect("full table");
        let source = Image::from_rgba(256, 256, src_data).expect("full table");
        let mask = Mask::from_raw(256, 256, mask_data).expect("full table");
        image_
            .paste_masked(&source, (0, 0), &mask)
            .expect("mask matches");
        assert_eq!(fnv1a(image_.as_raw()), 0xe40c_7b7e_9744_5735);

        let (mut dst_data, mut src_data) = (Vec::new(), Vec::new());
        let mut mask_data = Vec::new();
        for y in 0..256u32 {
            for x in 0..256u32 {
                dst_data.extend_from_slice(&[0, 127, 255, 64]);
                src_data.extend_from_slice(&[y as u8, 255 - y as u8, (y / 3) as u8, y as u8]);
                mask_data.push(x as u8);
            }
        }
        let mut image_ = Image::from_rgba(256, 256, dst_data).expect("full table");
        let source = Image::from_rgba(256, 256, src_data).expect("full table");
        let mask = Mask::from_raw(256, 256, mask_data).expect("full table");
        image_
            .paste_masked(&source, (0, 0), &mask)
            .expect("mask matches");
        assert_eq!(fnv1a(image_.as_raw()), 0xf7c9_6b3b_2864_b3b1);
    }

    #[test]
    fn paste_mask_clips_at_the_edges() {
        let mut canvas = image(7, 9, PASTE_CLIP_DST);
        let source = image(5, 5, PASTE_CLIP_SRC);
        let mask = Mask::from_raw(5, 5, unhex(PASTE_CLIP_MASK)).expect("5x5 mask");
        canvas
            .paste_masked(&source, (-2, 3), &mask)
            .expect("mask matches");
        assert_eq!(fnv1a(canvas.as_raw()), 0xe454_4f16_7fab_1aaf);

        let before = canvas.clone();
        canvas
            .paste_masked(&source, (40, 40), &mask)
            .expect("mask matches");
        assert_eq!(canvas, before);
    }

    #[test]
    fn opaque_paste_copies_the_visible_region() {
        let mut canvas = image(4, 4, PASTE_OPAQUE_DST);
        canvas.paste(&Image::new(2, 2, Rgba::new(9, 8, 7, 6)), (3, 3));
        assert_eq!(fnv1a(canvas.as_raw()), 0x88b6_161d_a8f4_b5f8);
    }

    #[test]
    fn gradients_and_errors_match_pillow() {
        let gradient = linear_gradient();
        assert_eq!((gradient.width(), gradient.height()), (256, 256));
        for y in 0..256 {
            assert!(gradient.row(y).iter().all(|&value| u32::from(value) == y));
        }
        assert_eq!(fnv1a(gradient.as_raw()), 0x9e73_1e58_b77c_a325);

        let small = Image::new(1, 1, Rgba::TRANSPARENT);
        let large = Image::new(2, 1, Rgba::TRANSPARENT);
        assert!(alpha_composite(&small, &large).is_err());
        let mut canvas = Image::new(2, 2, Rgba::TRANSPARENT);
        assert!(canvas
            .paste_masked(&small, (0, 0), &Mask::new(2, 2, 0))
            .is_err());
        assert_eq!(
            crate::GfxError::SizeMismatch.to_string(),
            "images do not match"
        );
    }
}

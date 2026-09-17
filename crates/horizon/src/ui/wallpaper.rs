//! The wallpaper: the picture the config names, drawn under the windows of every workspace.
//!
//! The picture is read and scaled on a thread, once for each size an output has. It fills the
//! output, and whatever is left over is cut evenly from both sides. Until it is ready, and when it
//! cannot be read, the workspace's background color shows. When the config names another file,
//! the old picture stays up until the new one is ready, so a change never shows the color in
//! between, and a picture that arrives for a file the config no longer names is dropped.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::path::{Path, PathBuf};
use std::thread;

use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageDecoder as _, ImageReader, Limits, RgbImage};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::Renderer as _;
use smithay::output::Output;
use smithay::reexports::calloop::{channel, LoopHandle};
use smithay::utils::{Rectangle, Size, Transform};

use crate::niri::State;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::utils::expand_home;

/// The widest or tallest picture that is read.
const MAX_SIDE: u32 = 16384;
/// The most memory the decoder may take for one picture.
const MAX_ALLOC: u64 = 1024 * 1024 * 1024;

/// A width and a height in physical pixels, turned the way the picture is drawn.
pub type Pixels = (i32, i32);

pub struct Wallpaper {
    /// The file the config names, with ~ expanded.
    path: Option<PathBuf>,
    /// Counts the files named so far, so pictures read for an earlier one can be told apart.
    generation: u64,
    /// The picture of the current file at each size, or none when it could not be read.
    pictures: HashMap<Pixels, Option<Picture>>,
    /// The sizes of the current file that are still being read.
    reading: HashSet<Pixels>,
    /// The pictures of the file before, drawn at a size whose new picture is still being read.
    previous: HashMap<Pixels, Picture>,
    event_loop: LoopHandle<'static, State>,
}

/// What a thread sends back: the file it read and the pixels for each size, or why it could not.
pub struct Read {
    generation: u64,
    path: PathBuf,
    result: Result<Vec<(Pixels, Vec<u8>)>, String>,
}

struct Picture {
    /// Xbgr8888 pixels at the size they are drawn, kept to make the texture again if the renderer
    /// changes.
    pixels: Vec<u8>,
    size: Pixels,
    /// Made on the primary GPU the first time the picture is drawn.
    texture: RefCell<Option<TextureBuffer<GlesTexture>>>,
}

impl Wallpaper {
    pub fn new(config: &niri_config::Wallpaper, event_loop: LoopHandle<'static, State>) -> Self {
        Self {
            path: resolve(config),
            generation: 0,
            pictures: HashMap::new(),
            reading: HashSet::new(),
            previous: HashMap::new(),
            event_loop,
        }
    }

    /// Takes the file the config names now. Returns whether it changed, in which case the caller
    /// asks for the sizes again with [`Self::ensure`].
    pub fn update_config(&mut self, config: &niri_config::Wallpaper) -> bool {
        let path = resolve(config);
        if path == self.path {
            return false;
        }
        self.path = path;
        self.generation += 1;
        self.reading.clear();
        let pictures = mem::take(&mut self.pictures);
        if self.path.is_some() {
            for (size, picture) in pictures {
                if let Some(picture) = picture {
                    self.previous.insert(size, picture);
                }
            }
        } else {
            self.previous.clear();
        }
        true
    }

    /// Makes sure there is a picture, or one on its way, for each of these sizes, and forgets the
    /// sizes no output has anymore.
    pub fn ensure(&mut self, sizes: &[Pixels]) {
        self.pictures.retain(|size, _| sizes.contains(size));
        self.reading.retain(|size| sizes.contains(size));
        self.previous.retain(|size, _| sizes.contains(size));

        let Some(path) = self.path.clone() else {
            return;
        };
        let mut wanted: Vec<_> = sizes
            .iter()
            .copied()
            .filter(|size| !self.pictures.contains_key(size) && !self.reading.contains(size))
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        if wanted.is_empty() {
            return;
        }
        self.reading.extend(wanted.iter().copied());

        let (sender, receiver) = channel::channel::<Read>();
        let source = self.event_loop.insert_source(receiver, |event, _, state| {
            if let channel::Event::Msg(read) = event {
                state.niri.wallpaper_read(read);
            }
        });
        if let Err(err) = source {
            warn!("error listening for the wallpaper: {err}");
            return;
        }

        let generation = self.generation;
        let spawned = thread::Builder::new()
            .name("Wallpaper Reader".to_owned())
            .spawn(move || {
                let _span = tracy_client::span!("Wallpaper::read");
                let result = read(&path, &wanted);
                let _ = sender.send(Read {
                    generation,
                    path,
                    result,
                });
            });
        if let Err(err) = spawned {
            warn!("error starting the wallpaper thread: {err}");
        }
    }

    /// Takes the pictures a thread read. Returns whether they are the current file's, so the
    /// outputs need drawing again.
    pub fn finish(&mut self, read: Read) -> bool {
        if read.generation != self.generation {
            return false;
        }
        match read.result {
            Ok(sized) => {
                for (size, pixels) in sized {
                    self.reading.remove(&size);
                    self.previous.remove(&size);
                    let picture = Picture {
                        pixels,
                        size,
                        texture: RefCell::new(None),
                    };
                    self.pictures.insert(size, Some(picture));
                }
            }
            Err(err) => {
                warn!("error reading the wallpaper {}: {err}", read.path.display());
                for size in mem::take(&mut self.reading) {
                    self.previous.remove(&size);
                    self.pictures.insert(size, None);
                }
            }
        }
        true
    }

    /// The picture for this output, or nothing when the color should show.
    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
    ) -> Option<PrimaryGpuTextureRenderElement> {
        let size = physical_size(output)?;
        let picture = match self.pictures.get(&size) {
            Some(picture) => picture.as_ref()?,
            None if self.reading.contains(&size) => self.previous.get(&size)?,
            None => return None,
        };

        let renderer = renderer.as_gles_renderer();
        let mut texture = picture.texture.borrow_mut();
        let stale = texture
            .as_ref()
            .is_none_or(|made| *made.renderer_context_id() != renderer.context_id());
        if stale {
            let whole = Rectangle::from_size(Size::from(picture.size));
            match TextureBuffer::from_memory(
                renderer,
                &picture.pixels,
                Fourcc::Xbgr8888,
                picture.size,
                false,
                1.,
                Transform::Normal,
                vec![whole],
            ) {
                Ok(made) => *texture = Some(made),
                Err(err) => {
                    warn!("error importing the wallpaper: {err:?}");
                    *texture = None;
                    return None;
                }
            }
        }

        let mut buffer = texture.clone()?;
        buffer.set_texture_scale(output.current_scale().fractional_scale());
        Some(PrimaryGpuTextureRenderElement(
            TextureRenderElement::from_texture_buffer(
                buffer,
                (0., 0.),
                1.,
                None,
                None,
                Kind::Unspecified,
            ),
        ))
    }
}

/// The size of an output in pixels the way the picture is drawn on it, turned with the output.
pub fn physical_size(output: &Output) -> Option<Pixels> {
    let mode = output.current_mode()?;
    let size = output.current_transform().transform_size(mode.size);
    Some((size.w, size.h))
}

fn resolve(config: &niri_config::Wallpaper) -> Option<PathBuf> {
    let path = PathBuf::from(config.0.as_ref()?);
    match expand_home(&path) {
        Ok(Some(expanded)) => Some(expanded),
        Ok(None) => Some(path),
        Err(err) => {
            warn!("error expanding ~ in the wallpaper path: {err:?}");
            None
        }
    }
}

fn read(path: &Path, sizes: &[Pixels]) -> Result<Vec<(Pixels, Vec<u8>)>, String> {
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|err| err.to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|err| err.to_string())?;
    let orientation = decoder.orientation().map_err(|err| err.to_string())?;
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|err| err.to_string())?;
    picture.apply_orientation(orientation);
    // the desktop under it is opaque, so any alpha goes
    let picture = picture.into_rgb8();

    Ok(sizes
        .iter()
        .filter(|(w, h)| *w > 0 && *h > 0)
        .map(|&(w, h)| ((w, h), fill(&picture, w.unsigned_abs(), h.unsigned_abs())))
        .collect())
}

/// The part of the picture with the output's shape, as large as fits and in the middle, scaled to
/// the output's size, as Xbgr8888 pixels.
fn fill(picture: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    let (x, y, crop_w, crop_h) = fill_crop(picture.dimensions(), (width, height));
    let view = imageops::crop_imm(picture, x, y, crop_w, crop_h);
    let scaled = imageops::resize(&*view, width, height, FilterType::CatmullRom);

    let mut pixels = Vec::with_capacity(scaled.len() / 3 * 4);
    for px in scaled.pixels() {
        pixels.extend_from_slice(&[px[0], px[1], px[2], 0xff]);
    }
    pixels
}

/// Where the part of a picture of `size` that fills `wanted` starts, and how large it is.
fn fill_crop((pic_w, pic_h): (u32, u32), (want_w, want_h): (u32, u32)) -> (u32, u32, u32, u32) {
    let (pic_w64, pic_h64) = (u64::from(pic_w), u64::from(pic_h));
    let (want_w64, want_h64) = (u64::from(want_w.max(1)), u64::from(want_h.max(1)));
    let (crop_w, crop_h) = if pic_w64 * want_h64 > pic_h64 * want_w64 {
        // wider than the output: the sides go
        let w = (pic_h64 * want_w64 + want_h64 / 2) / want_h64;
        (
            u32::try_from(w).unwrap_or(pic_w).clamp(1, pic_w.max(1)),
            pic_h,
        )
    } else {
        // taller: the top and the bottom go
        let h = (pic_w64 * want_h64 + want_w64 / 2) / want_w64;
        (
            pic_w,
            u32::try_from(h).unwrap_or(pic_h).clamp(1, pic_h.max(1)),
        )
    };
    (
        (pic_w - crop_w.min(pic_w)) / 2,
        (pic_h - crop_h.min(pic_h)) / 2,
        crop_w,
        crop_h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wider_picture_loses_its_sides() {
        // 3:2 on 16:9
        assert_eq!(fill_crop((3840, 2560), (1920, 1080)), (0, 200, 3840, 2160));
        // 3:2 on 16:10
        assert_eq!(fill_crop((3840, 2560), (1280, 800)), (0, 80, 3840, 2400));
        // 16:9 on 16:10 cuts the sides
        assert_eq!(fill_crop((1920, 1080), (1280, 800)), (96, 0, 1728, 1080));
    }

    #[test]
    fn a_picture_with_the_same_shape_is_whole() {
        assert_eq!(fill_crop((2560, 1600), (1280, 800)), (0, 0, 2560, 1600));
        assert_eq!(fill_crop((100, 100), (100, 100)), (0, 0, 100, 100));
    }

    #[test]
    fn a_turned_output_takes_a_tall_part() {
        assert_eq!(fill_crop((3840, 2160), (1080, 1920)), (1312, 0, 1215, 2160));
    }

    #[test]
    fn filling_gives_four_bytes_a_pixel_in_rgbx_order() {
        let mut picture = RgbImage::new(4, 2);
        for px in picture.pixels_mut() {
            *px = image::Rgb([10, 20, 30]);
        }
        let pixels = fill(&picture, 2, 1);
        assert_eq!(pixels.len(), 8);
        assert_eq!(&pixels[..4], &[10, 20, 30, 0xff]);
    }
}

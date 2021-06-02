use embedded_graphics::{DrawTarget, primitives};
use embedded_graphics::geometry::{Size, Point};
use embedded_graphics::drawable::Pixel;
use embedded_graphics::style::{Styled, PrimitiveStyle};
use embedded_graphics::image::{ImageDimensions, IntoPixelIter, Image};
use embedded_graphics::pixelcolor::PixelColor;
use embedded_graphics::primitives::{Rectangle, Triangle, Circle, Line};
use core::marker::PhantomData;
use embedded_graphics::prelude::RgbColor;
use core::convert::Infallible;

#[derive(Clone, Copy, Debug)]
pub enum Rotation {
    /// No rotation
    Rotate0,
    /// Rotate by 90 degrees clockwise
    Rotate90,
    /// Rotate by 180 degrees clockwise
    Rotate180,
    /// Rotate 270 degrees clockwise
    Rotate270,
}

impl Default for Rotation {
    fn default() -> Self {
        Rotation::Rotate0
    }
}

pub trait RotateFrame {
    fn rotate(self, r: Rotation) -> Self;
}

impl RotateFrame for Size {
    fn rotate(self, r: Rotation) -> Self {
        match r {
            Rotation::Rotate90 | Rotation::Rotate270 => Size::new(self.height, self.width),
            _ => self
        }
    }
}

pub trait RotateItem {
    fn rotate(self, r: Rotation, frame: Size) -> Self;
}

impl RotateItem for Point {
    fn rotate(self, r: Rotation, frame: Size) -> Self {
        match r {
            Rotation::Rotate0 => self,
            Rotation::Rotate90 => Point::new((frame.width - 1) as i32 - self.y, self.x),
            Rotation::Rotate180 => Point::new(1 - self.x, 1 - self.y),
            Rotation::Rotate270 => Point::new(self.y, (frame.height - 1) as i32 - self.x),
        }
    }
}

impl<C: PixelColor> RotateItem for Pixel<C> {
    fn rotate(self, r: Rotation, frame: Size) -> Self {
        Pixel(self.0.rotate(r, frame), self.1)
    }
}

impl RotateItem for Rectangle {
    fn rotate(self, r: Rotation, frame: Size) -> Self {
        Rectangle::new(self.top_left.rotate(r, frame), self.bottom_right.rotate(r, frame))
    }
}

impl RotateItem for Triangle {
    fn rotate(self, r: Rotation, frame: Size) -> Self {
        Triangle::new(self.p1.rotate(r, frame), self.p2.rotate(r, frame), self.p2.rotate(r, frame))
    }
}

impl RotateItem for Circle {
    fn rotate(self, r: Rotation, frame: Size) -> Self {
        Circle::new(self.center.rotate(r, frame), self.radius)
    }
}

impl RotateItem for Line {
    fn rotate(self, r: Rotation, frame: Size) -> Self {
        Line::new(self.start.rotate(r, frame), self.end.rotate(r, frame))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RotatedImage<'a, I, C> {
    image_data: &'a I,
    offset: Point,
    rotation: Rotation,
    c: PhantomData<C>,
}


pub struct Rotating<DT> {
    rotation: Rotation,
    inner: DT,
}

impl <DT> Rotating<DT> {
    pub fn new(rotation: Rotation, target: DT) -> Self {
        Rotating { rotation, inner: target }
    }
}

impl<C: PixelColor, DT: DrawTarget<C>> DrawTarget<C> for Rotating<DT> {
    type Error = DT::Error;

    fn draw_pixel(&mut self, item: Pixel<C>) -> Result<(), Self::Error> {
        self.inner.draw_pixel(item.rotate(self.rotation, self.inner.size()))
    }

    fn size(&self) -> Size {
        self.inner.size().rotate(self.rotation)
    }

    fn draw_line(&mut self, item: &Styled<primitives::Line, PrimitiveStyle<C>>) -> Result<(), Self::Error> {
        self.draw_line(&Styled::new(item.primitive.rotate(self.rotation, self.inner.size()), item.style))
    }

    fn draw_triangle(&mut self, item: &Styled<primitives::Triangle, PrimitiveStyle<C>>) -> Result<(), Self::Error> {
        self.draw_triangle(&Styled::new(item.primitive.rotate(self.rotation, self.inner.size()), item.style))
    }

    fn draw_rectangle(&mut self, item: &Styled<primitives::Rectangle, PrimitiveStyle<C>>) -> Result<(), Self::Error> {
        self.draw_rectangle(&Styled::new(item.primitive.rotate(self.rotation, self.inner.size()), item.style))
    }

    fn draw_circle(&mut self, item: &Styled<primitives::Circle, PrimitiveStyle<C>>) -> Result<(), Self::Error> {
        self.draw_circle(&Styled::new(item.primitive.rotate(self.rotation, self.inner.size()), item.style))
    }
}

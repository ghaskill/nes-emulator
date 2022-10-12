bitflags! {


    // 7  bit  0
    // ---- ----
    // BGRs bMmG
    // |||| ||||
    // |||| |||+- Greyscale (0: normal color, 1: produce a greyscale display)
    // |||| ||+-- 1: Show background in leftmost 8 pixels of screen, 0: Hide
    // |||| |+--- 1: Show sprites in leftmost 8 pixels of screen, 0: Hide
    // |||| +---- 1: Show background
    // |||+------ 1: Show sprites
    // ||+------- Emphasize red (green on PAL/Dendy)
    // |+-------- Emphasize green (red on PAL/Dendy)
    // +--------- Emphasize blue

    pub struct MaskRegister: u8 {
        const GREYSCALE             = 0b0000_0001;
        const LEFTMOST_8PXL_BKG    = 0b0000_0010;
        const LEFTMOST_8PXL_SPRITE = 0b0000_0100;
        const SHOW_BKG              = 0b0000_1000;
        const SHOW_SPRITES          = 0b0001_0000;
        const EMPHASIZE_RED         = 0b0010_0000;
        const EMPHASIZE_GREEN       = 0b0100_0000;
        const EMPASIZE_BLUE         = 0b1000_0000;
    }
}   

pub enum Color {
    Red,
    Green,
    Blue,
}

impl MaskRegister {
    pub fn new() -> Self {
        MaskRegister::from_bits_truncate(0b0000_0000)
    }

    pub fn is_greyscale(&self) -> bool {
        self.contains(MaskRegister::GREYSCALE)
    }

    pub fn leftmost_8pxl_bkg(&self) -> bool {
        self.contains(MaskRegister::LEFTMOST_8PXL_BKG)
    }

    pub fn leftmost_8pxl_sprite(&self) -> bool {
        self.contains(MaskRegister::LEFTMOST_8PXL_SPRITE)
    }

    pub fn show_bkg(&self) -> bool {
        self.contains(MaskRegister::SHOW_BKG)
    }

    pub fn show_sprites(&self) -> bool {
        self.contains(MaskRegister::SHOW_SPRITES)
    }

    pub fn emphasize(&self) -> Vec<Color> {
        let mut result = Vec::<Color>::new();
        if self.contains(MaskRegister::EMPHASIZE_RED) {
            result.push(Color::Red);
        }
        if self.contains(MaskRegister::EMPASIZE_BLUE) {
            result.push(Color::Blue);
        }
        if self.contains(MaskRegister::EMPHASIZE_GREEN) {
            result.push(Color::Green)
        }

        result
    }

    pub fn update(&mut self, data: u8) {
        self.bits = data;
    }
}
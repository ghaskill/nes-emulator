// Supports .NES files

/// # Control Byte 1 https://www.nesdev.org/wiki/INES
/// 76543210
/// ||||||||
/// |||||||+- Mirroring: 0: horizontal (vertical arrangement) (CIRAM A10 = PPU A11)
/// |||||||              1: vertical (horizontal arrangement) (CIRAM A10 = PPU A10)
/// ||||||+-- 1: Cartridge contains battery-backed PRG RAM ($6000-7FFF) or other persistent memory
/// |||||+--- 1: 512-byte trainer at $7000-$71FF (stored before PRG data)
/// ||||+---- 1: Ignore mirroring control or above mirroring bit; instead provide four-screen VRAM
/// ++++----- Four lower bits of mapper number

const NES_TAG: [u8; 4] = [0x4E, 0x45, 0x53, 0x1A];
const PRG_ROM: usize = 0x4000;
const CHR_ROM: usize = 0x2000;

#[derive(Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum Mirroring {
    VERTICAL,
    HORIZONTAL,
    FOUR_SCREEN,
}

pub struct Rom {
    pub prg_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub mapper: u8,
    pub screen_mirroring: Mirroring
}

impl Rom {
    pub fn new(rom_data: &Vec<u8>) -> Result<Rom, String> {
        if &rom_data[0..4] != NES_TAG {
            return Err("Unsupported format".to_string());
        }
        // Gets Mapping Type packed in Control Byte 1 and 2
        let mapper = (rom_data[7] & 0b1111_0000) | (rom_data[6] >> 4);

        // Checks .NES version
        let ines_ver = (rom_data[7] >> 2) & 0b11;
        if ines_ver != 0 {
            return Err("NES2.0 is not supported".to_string());
        }

        let four_screen = rom_data[6] & 0b1000 != 0;
        let vertical_mirroring = rom_data[6] & 0b1 != 0;

        let screen_mirroring = match (four_screen, vertical_mirroring) {
            (true, _) => Mirroring::FOUR_SCREEN,
            (false, true) => Mirroring::VERTICAL,
            (false, false) => Mirroring::HORIZONTAL,
        };

        // Gets PRG and CHR ROM size
        let prg_rom_size = rom_data[4] as usize * PRG_ROM;
        let chr_rom_size = rom_data[5] as usize * CHR_ROM;

        // Checks for trainer
        let skip_trainer = rom_data[6] &0b100 != 0;

        let prg_rom_start = 16 + if skip_trainer { 512 } else { 0 };
        let chr_rom_start = prg_rom_start + prg_rom_size;

        Ok(Rom {
            prg_rom: rom_data[prg_rom_start..(prg_rom_start + prg_rom_size)].to_vec(),
            chr_rom: rom_data[chr_rom_start..(chr_rom_start + chr_rom_size)].to_vec(),
            mapper: mapper,
            screen_mirroring: screen_mirroring,
        })
    }
}
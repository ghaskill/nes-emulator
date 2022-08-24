// The NES has a CPU with addressing space reserved at 
// [0x0000 - 0x2000] but only 2KB of RAM. To circumvent 
// this problem, the two highest bits are cut off address requests.

use crate::cpu::Mem;
use crate::cart::Rom;

const RAM_START: u16 = 0x0000;
const RAM_MIRROR_END: u16 = 0x1FFF;
const PPU_REGISTERS: u16 = 0x2000;
const PPU_REGISTERS_MIRROR_END: u16 = 0x3FFF;
const CARTRIDGE_ROM_START: u16 = 0x8000;
const CARTRIDGE_ROM_END: u16 = 0xFFFF;

pub struct Bus {
    cpu_vram: [u8; 2048],
    rom: Rom,
}

impl Mem for Bus {
    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            RAM_START ..= RAM_MIRROR_END => {
                // cut down 13 bit address request to 11 bits
                let mirrored_addr = addr & 0b0000_0111_1111_1111;
                self.cpu_vram[mirrored_addr as usize]
            }
            PPU_REGISTERS ..= PPU_REGISTERS_MIRROR_END => {
                // PPU registers at 0x2006 and 0x2007
                let _mirrored_addr = addr & 0b0010_0000_0000_0111;
                //todo!("PPU not implemented yet")
                0
            }
            CARTRIDGE_ROM_START ..= CARTRIDGE_ROM_END => {
                self.read_prg_rom(addr)
            }

            _ => {
                println!("Ignoring mem access at {}", addr);
                0
            }
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        match addr {
            RAM_START ..= RAM_MIRROR_END => {
                let mirrored_addr = addr & 0b11111111111;
                self.cpu_vram[mirrored_addr as usize] = data;
            }
            PPU_REGISTERS ..= PPU_REGISTERS_MIRROR_END => {
                let _mirrored_addr = addr & 0b0010_0000_0000_0111;
                todo!("PPU not implemented yet");
            }
            CARTRIDGE_ROM_START ..= CARTRIDGE_ROM_END => {
                panic!("Address reserved for Cartridge ROM")
            }
            _ => {
                println!("Ignoring mem write-access at {}", addr);
            }
        }
    }
}

impl Bus {
    pub fn new(rom: Rom) -> Self {
        Bus {
            cpu_vram: [0; 2048],
            rom: rom,
        }
    }

    fn read_prg_rom(&self, mut addr: u16) -> u8 {
        addr -= CARTRIDGE_ROM_START;
        //mirrors ROM for games with only 16KB PRG ROM
        if self.rom.prg_rom.len() == 0x4000 && addr >= 0x4000 {
            addr = addr % 0x4000;
        }
        self.rom.prg_rom[addr as usize]
    }
}
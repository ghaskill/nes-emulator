use std::collections::HashMap;
use crate::opcodes;
use crate::bus::Bus;
use crate::trace;

bitflags! {
    /// # Status Register (P) http://wiki.nesdev.com/w/index.php/Status_flags
    ///
    ///  7 6 5 4 3 2 1 0
    ///  N V _ B D I Z C
    ///  | |   | | | | +--- Carry Flag
    ///  | |   | | | +----- Zero Flag
    ///  | |   | | +------- Interrupt Disable
    ///  | |   | +--------- Decimal Mode (not used on NES)
    ///  | |   +----------- Break Command
    ///  | +--------------- Overflow Flag
    ///  +----------------- Negative Flag
    ///
    pub struct CpuFlags: u8 {
        const CARRY             = 0b00000001;
        const ZERO              = 0b00000010;
        const INTERRUPT_DISABLE = 0b00000100;
        const DECIMAL_MODE      = 0b00001000;
        const BREAK             = 0b00010000;
        const BREAK2            = 0b00100000;
        const OVERFLOW          = 0b01000000;
        const NEGATIVE          = 0b10000000;
    }
}

const STACK: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;

pub struct CPU<'a> {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: CpuFlags,
    pub program_counter: u16,
    pub stack_pointer: u8,
    pub bus: Bus<'a>,
    // memory: [u8; 0xFFFF]
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPage_X,
    ZeroPage_Y,
    Absolute,
    Absolute_X,
    Absolute_Y,
    Indirect_X,
    Indirect_Y,
    NoneAddressing,
}

mod interrupt {
    #[derive(PartialEq, Eq)]
    pub enum InterruptType {
        NMI,
    }

    #[derive(PartialEq, Eq)]
    pub(super) struct Interrupt {
        pub(super) itype: InterruptType,
        pub(super) vector_addr: u16,
        pub(super) b_flag_mask: u8,
        pub(super) cpu_cycles: u8,
    }
    pub(super) const NMI: Interrupt = Interrupt {
        itype: InterruptType::NMI,
        vector_addr: 0xfffA,
        b_flag_mask: 0b00100000,
        cpu_cycles: 2,
    };
}


pub trait Mem {
    fn mem_read(&mut self, addr: u16) -> u8; 

    fn mem_write(&mut self, addr: u16, data: u8);
    
    fn mem_read_u16(&mut self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos + 1) as u16;
        (hi << 8) | (lo as u16)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo);
        self.mem_write(pos + 1, hi);
    }
}

impl Mem for CPU<'_> {
    
    fn mem_read(&mut self, addr: u16) -> u8 { 
        self.bus.mem_read(addr)
    }

    fn mem_write(&mut self, addr: u16, data: u8) { 
        self.bus.mem_write(addr, data)
    }

    fn mem_read_u16(&mut self, pos: u16) -> u16 {
        self.bus.mem_read_u16(pos)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        self.bus.mem_write_u16(pos, data);
    }
    
}

fn page_cross(addr1: u16, addr2: u16) -> bool {
    addr1 & 0xFF00 != addr2 & 0xFF00
}

impl<'a> CPU<'a> {
    pub fn new<'b>(bus: Bus<'b>) -> CPU<'b> {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            status: CpuFlags::from_bits_truncate(0b100100),
            program_counter: 0,
            stack_pointer: STACK_RESET,
            bus: bus,
            // memory: [0; 0xFFFF]
        }
    }

    fn get_operand_address(&mut self, mode: &AddressingMode) -> (u16, bool) {
        match mode {
            AddressingMode::Immediate => (self.program_counter, false),
            _ => self.get_stored_value_address(mode, self.program_counter),
        }
    }

    pub fn get_stored_value_address(&mut self, mode: &AddressingMode, addr: u16) -> (u16, bool) {
        match mode {
            AddressingMode::ZeroPage => (self.mem_read(addr) as u16, false),

            AddressingMode::Absolute => (self.mem_read_u16(addr), false),

            AddressingMode::ZeroPage_X => {
                let pos = self.mem_read(addr);
                let addr = pos.wrapping_add(self.register_x) as u16;
                (addr, false)
            }
            AddressingMode::ZeroPage_Y => {
                let pos = self.mem_read(addr);
                let addr = pos.wrapping_add(self.register_y) as u16;
                (addr, false)
            }

            AddressingMode::Absolute_X => {
                let base = self.mem_read_u16(addr);
                let addr = base.wrapping_add(self.register_x as u16);
                (addr, page_cross(base, addr))
            }
            AddressingMode::Absolute_Y => {
                let base = self.mem_read_u16(addr);
                let addr = base.wrapping_add(self.register_y as u16);
                (addr, page_cross(base, addr))
            }

            AddressingMode::Indirect_X => {
                let base = self.mem_read(addr);

                let ptr: u8 = (base as u8).wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                ((hi as u16) << 8 | (lo as u16), false)
            }
            AddressingMode::Indirect_Y => {
                let base = self.mem_read(addr);

                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);
                let deref = deref_base.wrapping_add(self.register_y as u16);
                (deref, page_cross(deref, deref_base))
            }

            _ => {
                panic!("mode {:?} is not supported", mode);
            }
        }
    }

    fn interrupt_nmi(&mut self, interrupt: interrupt::Interrupt) {
        self.stack_push_u16(self.program_counter);
        let mut flag = self.status.clone();
        flag.set(CpuFlags::BREAK, interrupt.b_flag_mask & 0b010000 == 1);
        flag.set(CpuFlags::BREAK2, interrupt.b_flag_mask & 0b100000 == 1);

        self.stack_push(flag.bits);
        self.status.insert(CpuFlags::INTERRUPT_DISABLE);

        self.bus.tick(2);
        self.program_counter = self.mem_read_u16(0xFFFA);
    }

    fn interrupt_brk(&mut self) {
        self.stack_push_u16(self.program_counter + 2);
        let mut flag = self.status.clone();
        flag.insert(CpuFlags::BREAK);

        self.stack_push(flag.bits);
        self.status.insert(CpuFlags::INTERRUPT_DISABLE);

        self.bus.tick(7);
        self.program_counter = self.mem_read_u16(0xFFFE);
    }

    fn stack_push(&mut self, data: u8) {
        self.mem_write((STACK as u16) + self.stack_pointer as u16, data);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1)

    }

    fn stack_pop(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.mem_read((STACK as u16) + self.stack_pointer as u16)
    }

    fn stack_push_u16(&mut self, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.stack_push(hi);
        self.stack_push(lo);
    }

    fn stack_pop_u16(&mut self) -> u16 {
        let lo = self.stack_pop() as u16;
        let hi = self.stack_pop() as u16;

        hi << 8 | lo
    }

    fn set_carry_flag(&mut self) {
        self.status.insert(CpuFlags::CARRY);
    }

    fn clear_carry_flag(&mut self) {
        self.status.remove(CpuFlags::CARRY);
    }

    fn clear_negative_flag(&mut self) {
        self.status.remove(CpuFlags::NEGATIVE);
    }

    fn set_register_a(&mut self, data: u8) {
        self.register_a = data;
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        if result >> 7 == 1 {
            self.status.insert(CpuFlags::NEGATIVE);
        } else {
            self.status.remove(CpuFlags::NEGATIVE);
        }
    }

    fn update_negative_flag(&mut self, result: u8) {
        if result >> 7 == 1 {
            self.status.insert(CpuFlags::NEGATIVE);
        } else {
            self.status.remove(CpuFlags::NEGATIVE);
        }
    }

    fn update_zero_flag(&mut self, result: u8) {
        if result == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }
    }

    fn add_with_carry(&mut self, data: u8) {
        let sum = self.register_a as u16
            + data as u16
            + (if self.status.contains(CpuFlags::CARRY) {
                1
            } else {
                0
            }) as u16;

        let carry = sum > 0xff;

        if carry {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        let result = sum as u8;

        if (data ^ result) & (result ^ self.register_a) & 0x80 != 0 {
            self.status.insert(CpuFlags::OVERFLOW);
        } else {
            self.status.remove(CpuFlags::OVERFLOW)
        }

        self.set_register_a(result);
    }

    // OPCODES (alphabetical order)
    // https://www.nesdev.org/obelisk-6502-guide/reference.html

    fn adc(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let result = self.mem_read(addr);

        self.add_with_carry(result);
        self.update_zero_and_negative_flags(self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn and(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let value = self.mem_read(addr);

        self.register_a = self.register_a & value;
        self.update_zero_and_negative_flags(self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn asl_accumulator(&mut self) {
        let mut data = self.register_a;
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else { 
            self.clear_carry_flag();
        }
        data = data << 1;
        self.register_a = data;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn asl(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(&mode);
        let mut data = self.mem_read(addr);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
    }

    fn branch(&mut self, condition: bool) {
        if condition {
            self.bus.tick(1);

            let jump: i8 = self.mem_read(self.program_counter) as i8;
            let jump_addr = self
                .program_counter
                .wrapping_add(1)
                .wrapping_add(jump as u16);

            if self.program_counter.wrapping_add(1) & 0xFF00 != jump_addr & 0xFF00 {
                self.bus.tick(1);
            }

            self.program_counter = jump_addr;
        }
    }

    fn bit(&mut self, mode: &AddressingMode){
        let (addr, _) = self.get_operand_address(mode);
        let data = self.mem_read(addr);

        let and = self.register_a & data;
        if and == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        self.update_negative_flag(data);
        self.status.set(CpuFlags::OVERFLOW, data & 0b0100_0000 > 0);
    }

    fn compare(&mut self, mode: &AddressingMode, comparison: u8) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let data = self.mem_read(addr);

        if comparison >= data {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        self.update_zero_and_negative_flags(comparison.wrapping_sub(data));

        if page_cross {
            self.bus.tick(1);
        }
    }

    fn dec(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(&mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_sub(1);

        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
    } 

    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn eor(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let data = self.mem_read(addr);

        self.set_register_a(data ^ self.register_a);
        self.update_zero_and_negative_flags(self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn inc(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(&mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_add(1);

        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
    }

    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn jmp_absolute(&mut self) {
        let addr = self.mem_read_u16(self.program_counter);
        self.program_counter = addr;
    }

    fn jump_indirect(&mut self) {
        let mem_address = self.mem_read_u16(self.program_counter);
        let indirect_ref = self.mem_read_u16(mem_address);
        //6502 bug mode with with page boundary:
        //  if address $3000 contains $40, $30FF contains $80, and $3100 contains $50,
        // the result of JMP ($30FF) will be a transfer of control to $4080 rather than $5080 as you intended
        // i.e. the 6502 took the low byte of the address from $30FF and the high byte from $3000

        let indirect_ref = if mem_address & 0x00FF == 0x00FF {
            let lo = self.mem_read(mem_address);
            let hi = self.mem_read(mem_address & 0xFF00);
            (hi as u16) << 8 | (lo as u16)
        } else {
            self.mem_read_u16(mem_address)
        };

        self.program_counter = indirect_ref;
    }

    fn jsr(&mut self) {
        self.stack_push_u16(self.program_counter + 2 - 1);
        let target_address = self.mem_read_u16(self.program_counter);
        self.program_counter = target_address
    }

    fn lda(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let value = self.mem_read(addr);

        self.register_a = value;
        self.update_zero_and_negative_flags(self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn ldx(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let value = self.mem_read(addr);

        self.register_x = value;
        //self.update_zero_and_negative_flags(self.register_x);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn ldy(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let value = self.mem_read(addr);

        self.register_y = value;
        self.update_zero_and_negative_flags(self.register_y);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn lsr_accumulator(&mut self) {
        let mut data = self.register_a;
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        self.update_zero_flag(data); //
        self.clear_negative_flag(); //
        self.set_register_a(data)
    }

    fn lsr(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn ora(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let data = self.mem_read(addr);

        self.set_register_a(data | self.register_a);
        self.update_zero_and_negative_flags(self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn php(&mut self) {
        //http://wiki.nesdev.com/w/index.php/CPU_status_flag_behavior
        let mut flags = self.status.clone();
        flags.insert(CpuFlags::BREAK);
        flags.insert(CpuFlags::BREAK2);
        self.stack_push(flags.bits());
    }

    fn pla(&mut self) {
        let data = self.stack_pop();
        self.set_register_a(data);
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn plp(&mut self) {
        self.status.bits = self.stack_pop();
        self.status.remove(CpuFlags::BREAK);
        self.status.insert(CpuFlags::BREAK2);
    }

    fn rol_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        if old_carry {
            data = data | 1;
        }
        self.set_register_a(data);
        self.update_negative_flag(self.register_a);
    }

    fn rol(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        if old_carry {
            data = data | 1;
        }
        self.mem_write(addr, data);
        self.update_negative_flag(data);
        data
    }

    fn ror_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        if old_carry {
            data = data | 0b10000000;
            self.status.insert(CpuFlags::NEGATIVE);
        } else {
            self.status.remove(CpuFlags::NEGATIVE);
        }
        self.set_register_a(data);
        self.update_zero_flag(self.register_a);

    }

    fn ror(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        if old_carry {
            data = data | 0b10000000;
        }
        self.mem_write(addr, data);
        self.update_zero_flag(data);

        if old_carry {
            self.status.insert(CpuFlags::NEGATIVE);
        } else {
            self.status.remove(CpuFlags::NEGATIVE)
        }
        data
    }

    fn rti(&mut self) {
        self.status.bits = self.stack_pop();
        self.status.remove(CpuFlags::BREAK);
        self.status.insert(CpuFlags::BREAK2);
        self.program_counter = self.stack_pop_u16();

    }

    fn rts(&mut self) {
        self.program_counter = self.stack_pop_u16() + 1;
    }

    fn sbc(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let data = self.mem_read(addr);

        self.add_with_carry(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
        self.update_zero_and_negative_flags(self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn stx(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        self.mem_write(addr, self.register_x);
    }

    fn sty(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        self.mem_write(addr, self.register_y);
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn tay(&mut self) {
        self.register_y = self.register_a;
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn tsx(&mut self) {
        self.register_x = self.stack_pointer;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn txa(&mut self) {
        self.register_a = self.register_x;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn txs(&mut self) {
        self.stack_pointer = self.register_x;
    }

    fn tya(&mut self) {
        self.register_a = self.register_y;
        self.update_zero_and_negative_flags(self.register_a);
    }

    // ILLEGAL OPCODES

    /* ANC */
    fn anc(&mut self, mode: &AddressingMode) {
        // AND {imm} then set carry flag as if ASL performed
        self.and(&mode);

        if self.register_a >> 7 == 1 {
            self.set_carry_flag();
        } else { 
            self.clear_carry_flag();
        }
        self.update_zero_and_negative_flags(self.register_a);
    }

    /* ALR */
    fn asr(&mut self, mode: &AddressingMode) {
        self.and(&mode);
        self.lsr_accumulator();
    }

    /* ARR */
    fn arr(&mut self, mode: &AddressingMode) {
        self.and(&mode);
        let (addr, _) =  self.get_operand_address(&mode);
        let data: u8 = self.mem_read(addr) & self.register_a;
        let sum = self.register_a + data;

        if (data ^ sum) & (self.register_a ^ sum) & 0x80 != 0 {
            self.status.insert(CpuFlags::OVERFLOW);
        } else {
            self.status.remove(CpuFlags::OVERFLOW);
        }

        let carry = sum >> 7 > 0;
        let vflag: bool = self.status.contains(CpuFlags::CARRY);

        if carry && !vflag {
            self.mem_write(addr, sum ^ 0b1000_0000);
            self.status.insert(CpuFlags::CARRY);
        } else if !carry && vflag {
            self.mem_write(addr, sum | 0b1000_0000);
            self.status.remove(CpuFlags::CARRY);
        }

        self.ror_accumulator();
    } 

    /* AXS */
    fn axs(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(&mode);
        let data = self.mem_read(addr);
        let and = self.register_a & self.register_x;

        // Get 2's complement of data
        let complement_data = (data ^ 0b1111_1111) + 1;
        let difference = and + complement_data;

        if and >= complement_data {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        self.register_x = difference;
        self.update_zero_and_negative_flags(self.register_x);
    }

    /* DCP */
    fn dcp(&mut self, mode: &AddressingMode) {
        self.dec(&mode);
        self.compare(&mode, self.register_a);
    }

    /* ISB */
    fn isb(&mut self, mode: &AddressingMode) {
        self.inc(&mode);
        self.sbc(&mode);
    }
    /* LAX */
    fn lax(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(&mode);
        let value = self.mem_read(addr);

        self.update_zero_and_negative_flags(value);
        self.register_a = value;
        self.register_x = value;
    }

    /* RLA */
    fn rla(&mut self, mode: &AddressingMode) {
        self.rol(&mode);
        self.and(&mode);
    }

    /* RRA */
    fn rra(&mut self, mode: &AddressingMode) {
        self.ror(&mode);
        self.adc(&mode);
    }

    /* SAX */
    fn sax(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(&mode);
        let data:u8 = self.register_a & self.register_x;
        self.mem_write(addr, data);
    }

    /* SLO */
    fn slo(&mut self, mode: &AddressingMode) {
        self.asl(&mode);
        self.ora(&mode);
    }

    /* SRE */
    fn sre(&mut self, mode: &AddressingMode) {
        self.lsr(&mode);
        self.eor(&mode);
    }

    pub fn load(&mut self, program: Vec<u8>) {
        for i in 0..(program.len() as u16) {
            self.mem_write(0x0600 + i, program[i as usize]);
        }
        //self.mem_write_u16(0xFFFC, 0x0600);
    }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.register_y = 0;
        self.status = CpuFlags::from_bits_truncate(0b100100);

        self.program_counter = self.mem_read_u16(0xFFFC);
    }

    pub fn run(&mut self) {
        self.run_with_callback(|_| {});
    }

    pub fn run_with_callback<F>(&mut self, mut callback: F)
    where 
        F: FnMut(&mut CPU),
    {
       //let mut i = 1;
        let ref opcodes: HashMap<u8, &'static opcodes::OpCode> = *opcodes::OPCODES_MAP;

        loop {
            if let Some(_nmi) = self.bus.poll_nmi_status() {
                self.interrupt_nmi(interrupt::NMI);
            }

            callback(self);
            let code = self.mem_read(self.program_counter);
            self.program_counter += 1;
            let program_counter_state = self.program_counter;

            let opcode = opcodes.get(&code).expect(&format!("OpCode {:x} is not recognized", code));
            match code {
                /* CLC */
                0x18 => {
                    self.status.remove(CpuFlags::CARRY);
                },

                /* CLD */
                0xd8 => {
                    self.status.remove(CpuFlags::DECIMAL_MODE);
                },

                /* CLI */ 
                0x58 => {
                    self.status.remove(CpuFlags::INTERRUPT_DISABLE);
                },

                /* CLV */ 
                0xb8 => {
                    self.status.remove(CpuFlags::OVERFLOW);
                },

                /* PHA */
                0x48 => {
                    self.stack_push(self.register_a);
                },

                /* ADC */
                0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => {
                    self.adc(&opcode.mode);
                },

                /* AND */
                0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
                    self.and(&opcode.mode);
                },

                /* ASL */
                0x0a => {
                    self.asl_accumulator();
                },
                
                0x06 | 0x16 | 0x0e | 0x1e => {
                    self.asl(&opcode.mode);
                },

                /* BCC */
                0x90 => {
                    self.branch(!self.status.contains(CpuFlags::CARRY));
                },

                /* BCS */
                0xb0 => {
                    self.branch(self.status.contains(CpuFlags::CARRY));
                },

                /* BCE */
                0xf0 => {
                    self.branch(self.status.contains(CpuFlags::ZERO));
                },

                /* BIT */
                0x24 | 0x2c => {
                    self.bit(&opcode.mode);
                },

                /* BMI */
                0x30 => {
                    self.branch(self.status.contains(CpuFlags::NEGATIVE));
                },

                /* BNE */
                0xd0 => {
                    self.branch(!self.status.contains(CpuFlags::ZERO));
                },

                /* BPL */
                0x10 => {
                    self.branch(!self.status.contains(CpuFlags::NEGATIVE));
                },

                /* BVC */
                0x50 => {
                    self.branch(!self.status.contains(CpuFlags::OVERFLOW));
                },

                /* BVS */
                0x70 => {
                    self.branch(self.status.contains(CpuFlags::OVERFLOW));
                },
                /* CMP */
                0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                    self.compare(&opcode.mode, self.register_a);
                },

                /* CPX */
                0xe0 | 0xe4 | 0xec => {
                    self.compare(&opcode.mode, self.register_x);
                },

                /* CPY */
                0xc0 | 0xc4 | 0xcc => {
                    self.compare(&opcode.mode, self.register_y);
                },

                /* DEC */
                0xc6 | 0xd6 | 0xce | 0xde => {
                    self.dec(&opcode.mode);
                },

                /* DEX */ 
                0xca => {
                    self.dex();
                },

                /* DEY */
                0x88 => {
                    self.dey();
                },

                /* EOR */
                0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => {
                    self.eor(&opcode.mode);
                },

                /* INC */
                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                },

                /* INX */
                0xe8 => {
                    self.inx();
                },

                /* INY */
                0xc8 => {
                    self.iny();
                },

                /* JMP */
                0x4c => {
                    self.jmp_absolute();
                },
                0x6c => {self.jump_indirect();
                },
                
                /* JSR */
                0x20 => {
                    self.jsr();
                },

                /* LDA */
                0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    self.lda(&opcode.mode);
                },

                /* LDX */
                0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => {
                    self.ldx(&opcode.mode);
                },

                /* LDY */
                0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => {
                    self.ldy(&opcode.mode);
                },

                /* LSR */
                0x4a => {
                    self.lsr_accumulator();
                },
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                },
                
                /* NOP */
                0xea | 0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xb2 | 0xd2
                | 0xf2 => { /* do nothing */ }

                0x1a | 0x3a | 0x5a | 0x7a | 0xda | 0xfa => { /* do nothing */ }

                /* NOP read*/
                0x04 | 0x44 | 0x64 | 0x14 | 0x34 | 0x54 | 0x74 | 0xd4 | 0xf4 | 0x0c | 0x1c
                | 0x3c | 0x5c | 0x7c | 0xdc | 0xfc => {
                    let (addr, page_cross) = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    if page_cross {
                        self.bus.tick(1);
                    }
                    // do nothing
                },

                /* ORA */
                0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => {
                    self.ora(&opcode.mode);
                },

                /* PHP */
                0x08 => {
                    self.php();
                },

                /* PLA */
                0x68 => {
                    self.pla();
                },

                /* PLP */
                0x28 => {
                    self.plp();
                },

                /* ROL */
                0x2a => {
                    self.rol_accumulator();
                },
                0x26 | 0x36 | 0x2e | 0x3e => {
                    self.rol(&opcode.mode);
                },

                /* ROR */
                0x6a => {
                    self.ror_accumulator();
                },
                0x66 | 0x76 | 0x6e | 0x7e => {
                    self.ror(&opcode.mode);
                },

                /* RTI */
                0x40 => {
                    self.rti();
                },

                /* RTS */
                0x60 => {
                    self.rts();
                },

                /* SBC */
                0xe9 | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 | 0xeb => {
                    self.sbc(&opcode.mode);
                },

                /* SEC */
                0x38 => {
                    self.status.insert(CpuFlags::CARRY);
                },

                /* SED */
                0xf8 => {
                    self.status.insert(CpuFlags::DECIMAL_MODE);
                },

                /* SEI */
                0x78 => {
                    self.status.insert(CpuFlags::INTERRUPT_DISABLE);
                },

                /* STA */
                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                },

                /* STX */
                0x86 | 0x96 | 0x8e => {
                    self.stx(&opcode.mode);
                },

                /* STY */
                0x84 | 0x94 | 0x8c => {
                    self.sty(&opcode.mode);
                },

                /* TXA */
                0x8a => {
                    self.txa();
                },

                /* TAX */
                0xAA => {
                    self.tax();
                },

                /* TAY */
                0xa8 => {
                    self.tay();
                },

                /* TSX */
                0xba => {
                    self.tsx();
                },

                /* TXS */
                0x9a => {
                    self.txs();
                },

                /* TYA */
                0x98 => {
                    self.tya();
                },

                0x00 => self.interrupt_brk(),

                /* ILLEGAL OPCODES */

                /* ANC */
                0x0b => self.anc(&opcode.mode),

                /* ASR */
                0x4b => self.asr(&opcode.mode),

                /* AXS */
                0xcb => self.axs(&opcode.mode),
                
                /* DCP */
                0xd3 | 0xdb | 0xcf | 0xdf | 0xc7 | 0xd7 | 0xc3 => {
                    self.dcp(&opcode.mode);
                }

                /* ISB */
                0xef | 0xff | 0xfb | 0xe7 | 0xf7 | 0xe3 | 0xf3 => {
                    self.isb(&opcode.mode);
                }

                /* LAX */
                0xb3 | 0xa7 | 0xa3 | 0xaf | 0xb7 | 0xbf => {
                    self.lax(&opcode.mode);
                }
                
                /* RLA */
                0x2f | 0x3f | 0x3b | 0x27 | 0x37 | 0x23 | 0x33 => {
                    self.rla(&opcode.mode);
                }

                /* RRA */
                0x6f | 0x7f | 0x7b | 0x67 | 0x77 | 0x63 | 0x73 => {
                    self.rra(&opcode.mode);
                }

                /* SAX */
                0x8f | 0x83 | 0x97 | 0x87 => self.sax(&opcode.mode),

                /* SLO */
                0x07 | 0x0f | 0x1f | 0x1b | 0x17 | 0x03 | 0x13 => {
                    self.slo(&opcode.mode);
                }

                /* SRE */
                0x4f | 0x5f | 0x5b | 0x47 | 0x57 | 0x43 | 0x53 => {
                    self.sre(&opcode.mode);
                }

                _ => todo!(),
            }

            self.bus.tick(opcode.cycles);

            if program_counter_state == self.program_counter {
                self.program_counter += (opcode.len - 1) as u16;
            }
        }
    }
}
/*
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_0xa9_lda_immediate_load_data() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x05, 0x00]);
        assert_eq!(cpu.register_a, 5);
        assert!(!cpu.status.contains(CpuFlags::ZERO));
        assert!(!cpu.status.contains(CpuFlags::NEGATIVE));
    }

    #[test]
    fn test_0xa9_lda_zero_flag() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x00, 0x00]);
        assert!(cpu.status.contains(CpuFlags::ZERO));
    }

    #[test]
    fn test_0xaa_tax_move_a_to_x() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x0A,0xaa, 0x00]);

        assert_eq!(cpu.register_x, 10)
    }

    #[test]
    fn test_5_ops_working_together() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);

        assert_eq!(cpu.register_x, 0xc1)
    }

    #[test]
    fn test_inx_overflow() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0xff, 0xaa,0xe8, 0xe8, 0x00]);

        assert_eq!(cpu.register_x, 1)
    }

    #[test]
    fn test_lda_from_memory() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55);

        cpu.load_and_run(vec![0xa5, 0x10, 0x00]);

        assert_eq!(cpu.register_a, 0x55);
    }

    #[test]
    fn test_and() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0b1010_1010, 0x29, 0b0101_0101]);
        assert_eq!(cpu.register_a, 0)
    }
}*/
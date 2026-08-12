use crate::mmu as mmu;
use crate::cpu_tables as cpu_tables;
use crate::disassembler::Disassembler;
use crate::ppu::PPU;

use std::fs::OpenOptions;
use std::io::Write;

fn log_line(s: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("log.txt")
        .expect("Failed to open log file");

    writeln!(file, "{}", s).unwrap();
    file.flush().unwrap();
    //println!("{}", s);
}

macro_rules! ternary {
    ($c:expr, $v:expr, $v1:expr) => {
        if $c {$v} else {$v1}
    };
}

//TODO::
// mark all these functions with attribute #[inline], or else
// they won't be inlined

// ENUM (sort of) for the flags
const ZERO : u8 = 7;
const SUB : u8 = 6;
const HC : u8 = 5;
const CARRY : u8 = 4;


// SOO much flag modification, so this function is my favorite EVER.
#[inline]
fn set_flag(flags: &mut u8, bit: u8, val: bool) {
    if val {
        *flags |= 1 << bit; 
    }
    else {
        *flags &= !(1 << bit);
    }
}

#[inline]
fn get_flag(flags: &mut u8, bit: u8) -> bool {
    (*flags & (1 << bit)) != 0
}



fn rra(a: &mut u8, flags: &mut u8) {
    let carry_bit = *a & 0x01; // Get the rightmost bit (bit 0)
    let old_carry = get_flag(flags, CARRY) as u8; // Get the current carry flag

    *a = (*a >> 1) | (old_carry << 7); // Rotate right through carry

    set_flag(flags, ZERO, false);  // Z flag is not affected
    set_flag(flags, SUB, false);   // N flag is always cleared
    set_flag(flags, HC, false);    // H flag is always cleared
    set_flag(flags, CARRY, carry_bit != 0); // C flag is set to the old bit 0
}

fn rrca(a: &mut u8, flags: &mut u8) {
    let carry_bit = *a & 0x01; // Get the rightmost bit (bit 0)
    *a = (*a >> 1) | (carry_bit << 7); // Rotate right and wrap bit 0 to bit 7

    set_flag(flags, ZERO, false);  // Z flag is not affected
    set_flag(flags, SUB, false);   // N flag is always cleared
    set_flag(flags, HC, false);    // H flag is always cleared
    set_flag(flags, CARRY, carry_bit != 0); // C flag is set to the value of the old bit 0
}

fn rlca(a: &mut u8, flags: &mut u8) {
    let carry_bit = (*a & 0x80) >> 7; // Get the leftmost bit (bit 7)
    *a = (*a << 1) | carry_bit;       // Rotate left and wrap bit 7 to bit 0

    set_flag(flags, ZERO, false);     // Z flag is not affected
    set_flag(flags, SUB, false);      // N flag is always cleared
    set_flag(flags, HC, false);       // H flag is always cleared
    set_flag(flags, CARRY, carry_bit != 0); // C flag is set to the value of the old bit 7
}

fn ld_dec(mmu_ref : &mut mmu::MMU, r1 : &mut u16, r2 : u8) {
	mmu_ref.set_byte(*r1 as usize, r2);
	(*r1).wrapping_sub(1);
}


fn twos_complement(r1: u8) -> i16 {

    let neg : bool = (r1 & 0b10000000) > 0;

    let mut new_rep = (!r1 + 1) as i16;

    if neg {
        new_rep *= -1;
    }

    new_rep
}



fn ld_word(r1: &mut u8, r2: &mut u8, nn : u16) {
	//*reg = nn;
	//*r1 = ((nn & 0b11110000) >> 4) as u8;

    //println!("nn val: {:#04x}", nn);
	*r1 = ((nn & 0xFF00) >> 8) as u8;
    //println!("h val: {:#02x}", *r1);
	*r2 = (nn & 0x00FF) as u8;
    //println!("l val: {:#02x}", *r2);
}

fn xor(reg: &mut u8, n: u8, flags: &mut u8) {
    *reg = *reg ^ n;

    // Set flags
    set_flag(flags, ZERO, *reg == 0);  // Z flag is set if the result is zero
    set_flag(flags, SUB, false);       // N flag is always cleared (XOR is not a subtraction)
    set_flag(flags, HC, false);        // H flag is always cleared (no half-carry)
    set_flag(flags, CARRY, false);     // C flag is always cleared (no carry)
}

fn and(reg : &mut u8, n : u8, flags: &mut u8) {
	*reg = n & (*reg);
	*flags = ternary!(*reg == 0, 0b10100000, 0b00100000);
}


fn or(r1 : &mut u8, r2 : u8, flags: &mut u8) {

	*r1 = *r1 | r2;
	*flags = ternary!(*r1 == 0, 0b10000000, 0b00000000);
}

// increment 8 bit register by 1, returns it, sets flags
fn inc_reg(r1: u8, flags: &mut u8) -> u8 {
    let res = r1.wrapping_add(1);

    // Zero flag: set if result is 0
    let z_flag = res == 0;
    
    // Half Carry flag: set if there's a carry from bit 3 to bit 4
    let h_flag = (r1 & 0xF) == 0xF;

    // Set flags
    set_flag(flags, ZERO, z_flag);
    set_flag(flags, SUB, false); // Always cleared for INC
    set_flag(flags, HC, h_flag);

    res
}


// decrements 8 bit register by 1, returns it, sets flags
fn dec_reg(r1 : u8, flags: &mut u8) -> u8 {
    //println!("REG IN DEC REG: {:#02x}", r1);
    let mut z_flag = false;
    let mut h_flag = false;

    let res = r1.wrapping_sub(1);

    if res == 0 {
        z_flag = true;
    }
    h_flag = ((r1 & 0xF) < (1 & 0xF));

    set_flag(flags, ZERO, z_flag);
    set_flag(flags, SUB, true);
    set_flag(flags, HC, h_flag);

    //println!("REG AFTER DEC REG: {:#02x}", res);
    //println!("FLAGS AFTER DEC REG: {:b}", *flags);
    res
}

// subtracts the value of r2 from r1, returns this value (in order to set r1 in the CPU), sets flags
// TODO should we just make this mutate r1 (aka 'a')? Cleaner design?
fn sub_reg(r1: u8, r2: u8, flags: &mut u8) -> u8 {
    let res = r1.wrapping_sub(r2);

    let h_flag = (r1 & 0x0F) < (r2 & 0x0F);

    set_flag(flags, ZERO, res==0);
    set_flag(flags, SUB, true);
    set_flag(flags, HC, h_flag);
    set_flag(flags, CARRY, r2 > r1);
    res
}
// TODO same concern here
fn add_reg(r1: u8, r2: u8, flags: &mut u8) -> u8 {
    let res = r1.wrapping_add(r2);

    // Check if there is a half-carry (carry from bit 3 to bit 4)
    let h_flag = ((r1 & 0x0F) + (r2 & 0x0F)) > 0x0F;

    // Check if there is a carry (sum > 255)
    let carry_flag = (r1 as u16 + r2 as u16) > 0xFF;

    set_flag(flags, ZERO, res==0);
    set_flag(flags, SUB, false);
    set_flag(flags, HC, h_flag);
    set_flag(flags, CARRY, carry_flag);

    res
}

fn adc_reg(r1: u8, r2: u8, flags: &mut u8) -> u8 {
    let carry = get_flag(flags, CARRY) as u8;  // Get the carry flag (1 if set, 0 if not)
    let res = r1.wrapping_add(r2).wrapping_add(carry);

    // Check if there is a half-carry (carry from bit 3 to bit 4, with carry included)
    let h_flag = ((r1 & 0x0F) + (r2 & 0x0F) + carry) > 0x0F;

    // Check if there is a carry (sum > 255, with carry included)
    let carry_flag = (r1 as u16 + r2 as u16 + carry as u16) > 0xFF;

    set_flag(flags, ZERO, res == 0);
    set_flag(flags, SUB, false);  // ADC doesn't set the subtraction flag
    set_flag(flags, HC, h_flag);
    set_flag(flags, CARRY, carry_flag);

    res
}

fn sbc_reg(r1: u8, r2: u8, flags: &mut u8) -> u8 {
    let carry = get_flag(flags, CARRY) as u8;  // Get the carry flag (1 if set, 0 if not)
    let res = r1.wrapping_sub(r2).wrapping_sub(carry);

    // Check if there is a half-carry (borrow from bit 4, with carry included)
    let h_flag = (r1 & 0x0F) < (r2 & 0x0F) + carry;

    // Check if there is a carry (borrow, with carry included)
    let carry_flag = (r1 as u16) < (r2 as u16 + carry as u16);

    set_flag(flags, ZERO, res == 0);
    set_flag(flags, SUB, true);  // SBC sets the subtraction flag
    set_flag(flags, HC, h_flag);
    set_flag(flags, CARRY, carry_flag);

    res
}

fn daa(a: &mut u8, flags: &mut u8) {
    let mut correction = 0;
    let mut carry_flag = false;

    // Check if we need to adjust the lower nibble
    if !get_flag(flags, SUB) {  // If last operation was addition
        if get_flag(flags, HC) || (*a & 0x0F) > 9 {
            correction |= 0x06;
        }
        if get_flag(flags, CARRY) || *a > 0x99 {
            correction |= 0x60;
            carry_flag = true;
        }
    } else {  // If last operation was subtraction
        if get_flag(flags, HC) {
            correction |= 0x06;
        }
        if get_flag(flags, CARRY) {
            correction |= 0x60;
            carry_flag = true;
        }
    }

    // Apply the correction
    if get_flag(flags, SUB) {
        *a = a.wrapping_sub(correction);
    } else {
        *a = a.wrapping_add(correction);
    }

    // Set the flags
    set_flag(flags, ZERO, *a == 0);
    set_flag(flags, HC, false); // Half-carry is always cleared after DAA
    set_flag(flags, CARRY, carry_flag);
}

// set carry flag instruction
fn scf(flags: &mut u8) {
    set_flag(flags, CARRY, true);   // Set the carry flag
    set_flag(flags, SUB, false);    // Reset the subtract flag (N)
    set_flag(flags, HC, false);     // Reset the half-carry flag (H)
}

// complement carry flag
fn ccf(flags: &mut u8) {
    let carry = get_flag(flags, CARRY);
    set_flag(flags, CARRY, !carry); // Complement the carry flag
    set_flag(flags, SUB, false);    // Reset the subtract flag (N)
    set_flag(flags, HC, false);     // Reset the half-carry flag (H)
}

fn add_double_regs(r1: u16, r2: u16, flags: &mut u8) -> u16 {
    let result = r1.wrapping_add(r2);

    // Check for Half Carry: If there's a carry from bit 11 to bit 12
    let half_carry = ((r1 & 0x0FFF) + (r2 & 0x0FFF)) > 0x0FFF;

    // Check for Carry: If the result is larger than 16 bits
    let carry = (r1 as u32 + r2 as u32) > 0xFFFF;

    // Set flags
    set_flag(flags, SUB, false);       // Clear Subtract flag
    set_flag(flags, HC, half_carry);   // Set Half Carry flag if there's a carry from bit 11
    set_flag(flags, CARRY, carry);     // Set Carry flag if the result overflows 16 bits

    result
}



fn compare(r1 : u8, r2 : u8, flags: &mut u8) {
    //println!("COMPARE a val to other val {:#02x}, {:#02x} ", r1, r2);

    let mut z_flag = false;
    let mut h_flag = false;
    let mut c_flag = false;

    match r1.checked_sub(r2) {
        Some(result) => {
            z_flag = result == 0; 
            // c flag remains false, since no full borrow 
        }
        None => {
            c_flag = true;
        }
    }
    h_flag = ((r1 & 0xF) < (r2 & 0xF));

    set_flag(flags, ZERO, z_flag);
    set_flag(flags, SUB, true);
    set_flag(flags, HC, h_flag);
    set_flag(flags, CARRY, c_flag);
}




fn test_bit(reg: u8, n : u8, flags : &mut u8) {
    //println!("H REGISTER!!! {:#02x}", reg);
	let res : u8 = ternary!(((reg) & (1u8 << n)) == 0, 0b10100000, 0b00100000);
	*flags = res | (*flags & 0b00011111);
}

fn swap(reg : u8, flags : &mut u8) -> u8 {
	let res = ((reg & 0x0F) << 4 ) | ((reg & 0xF0) >> 4);
	*flags = ternary!(res == 0, 0b10000000, 0b00000000) | (*flags & 0x0F);
	return res;
}

/*
// this instruction is weird
fn sra(reg : &mut u8, flags : &mut u8) {
}
*/

// rotate register left by one bit
fn rlc(reg : u8, flags : &mut u8) -> u8 {
	let ret = (reg << 1) | ((reg & 0b10000000) >> 7);
	*flags = ternary!(ret == 0, 0b10000000, 0b00000000) | ((ret & 0b00000001) << 4);
	return ret;
}






// rotate register right by one bit
fn rrc(reg : u8, flags : &mut u8) -> u8 {
	let ret = (reg >> 1) | ((reg & 0b00000001) << 7);
	*flags = ternary!(ret == 0, 0b10000000, 0b00000000) | ((ret & 0b10000000) >> 3);
	ret
}


// TODO ugly solution for now but we obviously want to combine rla and rl (and etc.) in the future
fn rla(reg : u8, flags : &mut u8) -> u8 {
    let ret = rl(reg, flags);
    set_flag(flags, ZERO, false);
    ret
}


// rotate register left through carry
fn rl(reg : u8, flags : &mut u8) -> u8 {
    let old_carry = get_flag(flags, CARRY) as u8;
    let new_carry = reg & 0b10000000;
    let ret = (reg << 1) | old_carry;

    set_flag(flags, ZERO, ret == 0);
    set_flag(flags, SUB, false);
    set_flag(flags, HC, false);

    set_flag(flags, CARRY, new_carry != 0); 
    ret
}

fn rr(reg : u8, flags : &mut u8) -> u8 {
        let old_carry = get_flag(flags, CARRY);
        let new_carry = reg & 0b00000001;
	let ret = ((old_carry as u8) << 7) | (reg >> 1);

        set_flag(flags, CARRY, new_carry > 0);
        set_flag(flags, ZERO, ret == 0);
        set_flag(flags, SUB, false);
        set_flag(flags, HC, false);

	ret
}


// shift register left by one bit
fn sla(reg : u8, flags : &mut u8) -> u8 {
	let saved_bit = (reg & 0b10000000) >> 3;	
	let ret = reg << 1;
	*flags = ternary!(ret == 0, 0b10000000, 0b00000000) | saved_bit;
	return ret;
}



// shift register right by one bit
fn sra(reg : u8, flags : &mut u8) -> u8 {
	let saved_bit = (reg & 0b00000001) << 4;
	let ret = (reg >> 1) | (reg & 0b10000000);
	*flags = ternary!(ret == 0, 0b10000000, 0b00000000) | saved_bit;
	return ret;
}

fn srl(reg: u8, flags: &mut u8) -> u8 {
    let carry_bit = reg & 0b00000001;  // Save the least significant bit
    let result = reg >> 1;             // Shift the register right by 1 bit

    // Set the flags
    set_flag(flags, ZERO, result == 0);      // Set Z if result is zero
    set_flag(flags, SUB, false);             // N flag is always cleared
    set_flag(flags, HC, false);              // H flag is always cleared
    set_flag(flags, CARRY, carry_bit != 0);  // Set C flag based on the old bit 0

    result
}


fn complement_reg(reg : &mut u8, flags : &mut u8) {
	*reg = !*reg;
        set_flag(flags, SUB, true);
        set_flag(flags, HC, true);
}









#[derive(Clone)]
pub struct CPU {
	// NOTE on the Flags register 'f':
	/*
	Bit 	Name 	Explanation
	7 	z 	Zero flag
	6 	n 	Subtraction flag (BCD)
	5 	h 	Half Carry flag (BCD)
	4 	c 	Carry flag
	... the other bits are ALWAYS 0
	*/
	a : u8,
	f : u8,

	b : u8,
	c : u8,

	d : u8,
	e : u8,
	
	h : u8,
	l : u8,

	// yes, we split the stack pointer in half.
	// it makes defining general cpu functions easier
	//sp : u16,
	s : u8,
	p : u8,
	pc : u16,



	// flags


	//Halt CPU & LCD display until button pressed.
	stop_flag : bool,
	cgb_handoff_done: bool,
	
	 //Power down CPU until an interrupt occurs. Use this whenever possible to reduce energy consumption
	halt_flag : bool,

        ime : bool, // interrupt master enable flag

	// set whenever instruction EI encountered: enables interrupts on mmu after the next
        // instruction. 
	ei_pending : bool,
	di_pending : bool,

        // debugging
        trace_enabled: bool,

        halt_bug_enabled: bool,
        timer_cycles_advanced: u8,

        dis : Disassembler,

	}


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PostBootRegisters {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}


	impl CPU  {

    pub fn new() -> CPU {
        CPU {
                a : 0, f : 0,
                b : 0, c : 0,
                d : 0, e : 0,
                h : 0, l : 0,
                s : 0, p : 0,
                pc : 0,
                stop_flag : false,
                cgb_handoff_done: false,
                halt_flag : false,
                ime: false,
                ei_pending : false,
                di_pending : false,
                trace_enabled: false,
                halt_bug_enabled: false,
                timer_cycles_advanced: 0,
                dis : Disassembler::from_csv(),
        }
    }

	    pub fn set_trace_enabled(&mut self, enabled: bool) {
	        self.trace_enabled = enabled;
	    }

    pub fn apply_post_boot_registers(&mut self, registers: PostBootRegisters) {
        self.a = registers.a;
        self.f = registers.f & 0xF0;
        self.b = registers.b;
        self.c = registers.c;
        self.d = registers.d;
        self.e = registers.e;
        self.h = registers.h;
        self.l = registers.l;
        self.set_sp(registers.sp);
        self.pc = registers.pc;
        self.stop_flag = false;
        self.halt_flag = false;
        self.ime = false;
        self.ei_pending = false;
        self.di_pending = false;
        self.cgb_handoff_done = true;
    }

    pub fn program_counter(&self) -> u16 {
        self.pc
    }

    pub fn accumulator(&self) -> u8 {
        self.a
    }

    pub fn stack_pointer(&self) -> u16 {
        self.get_sp()
    }

    pub fn bc(&self) -> u16 {
        self.get_bc()
    }

    pub fn hl(&self) -> u16 {
        self.get_hl()
    }

    pub fn de(&self) -> u16 {
        self.get_de()
    }

    pub fn is_halted(&self) -> bool {
        self.halt_flag
    }

    pub fn interrupts_enabled(&self) -> bool {
        self.ime
    }

    fn reg_val(self, reg : &u8) -> u8 {
            return *reg;
    }


    fn get_a(&self) -> u8 {
        return self.a;
    }

    fn get_af(&self) -> u16 {
        return ((self.a as u16) << 8) | (self.f as u16);
    }

    fn get_bc(&self) -> u16 {
            return ((self.b as u16) << 8) | (self.c as u16);
    }


    fn get_de(&self) -> u16 {
            return ((self.d as u16) << 8) | (self.e as u16);
    }

    fn get_hl(&self) -> u16 {
            return ((self.h as u16) << 8) | (self.l as u16);
    }


    fn get_sp(&self) -> u16 {
            return ((self.s as u16) << 8) | (self.p as u16);
    }

    fn set_af(&mut self, val : u16) {
            self.a = ((val & 0xFF00) >> 8) as u8;
            // TODO CRUCIAL DETAIL -- you must zero out the lower nibble of F. Always.
            // We might need to do this elsewhere, unsure. Making a note.
            self.f = (val & 0x00F0) as u8;
    }

    fn set_bc(&mut self, val : u16) {
            self.b = ((val & 0xFF00) >> 8) as u8;
            self.c = (val & 0x00FF) as u8;
    }

    fn set_de(&mut self, val : u16) {
            self.d = ((val & 0xFF00) >> 8) as u8;
            self.e = (val & 0x00FF) as u8;
    }

    // TODO shift first register to the right
    fn set_hl(&mut self, val : u16) {
            self.h = ((val & 0xFF00) >> 8) as u8;
            self.l = (val & 0x00FF) as u8;
    }

    fn set_sp(&mut self, val : u16) {
            self.s = ((val & 0xFF00) >> 8) as u8;
            self.p = (val & 0x00FF) as u8;
    }

    fn jump_relative(&mut self, a8 : u8) {
        //println!("jr a8: {:#02x}", a8);
        let offset = ternary!(((a8 & 0b10000000) > 0), twos_complement(a8), a8 as i16);
        if offset < 0 {
            self.pc = self.pc.wrapping_sub(offset.abs() as u16);
            //println!("jr offset negative! {:#04x}", offset.abs());
        }
        else {
            self.pc = self.pc.wrapping_add(offset as u16);
            //println!("jr offset positive! {:#04x}", offset.abs());
        }
    }

    fn stack_push_at(
        &mut self,
        mmu_ref: &mut mmu::MMU,
        ppu_ref: &mut Option<&mut PPU>,
        val: u16,
        high_cycle: u8,
        low_cycle: u8,
    ) {
        let old_sp = self.get_sp();
        mmu_ref.trigger_oam_idu_corruption_at(old_sp, 0);
        let mut sp = old_sp.wrapping_sub(1);
        mmu_ref.trigger_oam_write_corruption_at(sp, u16::from(high_cycle));
        self.timed_write(mmu_ref, ppu_ref, sp as usize, (val >> 8) as u8, high_cycle);

        sp = sp.wrapping_sub(1);
        mmu_ref.trigger_oam_write_corruption_at(sp, u16::from(low_cycle));
        self.timed_write(mmu_ref, ppu_ref, sp as usize, (val & 0xFF) as u8, low_cycle);

        self.set_sp(sp);
    }

    // pops a word off the stack, increments sp
    fn stack_pop(&mut self, mmu_ref : &mut mmu::MMU) -> u16 {
        self.stack_pop_at(mmu_ref, &mut None, 0, 4)
    }

    fn stack_pop_at(
        &mut self,
        mmu_ref: &mut mmu::MMU,
        ppu_ref: &mut Option<&mut PPU>,
        low_cycle: u8,
        high_cycle: u8,
    ) -> u16 {
        let mut sp = self.get_sp() as usize;
        //println!("sp_val: {:#04x}", self.get_sp());

        mmu_ref.trigger_oam_read_idu_corruption_at(sp as u16, u16::from(low_cycle));
        let low_byte = self.timed_read(mmu_ref, ppu_ref, sp, low_cycle);
        sp = sp.wrapping_add(1);

        mmu_ref.trigger_oam_read_corruption_at(sp as u16, u16::from(high_cycle));
        let high_byte = self.timed_read(mmu_ref, ppu_ref, sp, high_cycle);
        sp = sp.wrapping_add(1);

        self.set_sp(sp as u16);

        (high_byte as u16) << 8 | (low_byte as u16)
    }

    pub fn update_timers(&mut self, t_cycles : u8, mmu_ref : &mut mmu::MMU) {
        self.update_system_clocks(t_cycles, mmu_ref, None);
    }

    fn update_system_clocks(
        &mut self,
        t_cycles: u8,
        mmu_ref: &mut mmu::MMU,
        ppu_ref: Option<&mut PPU>,
    ) {
        let tac_reg = mmu_ref.get_byte(0xFF07 as usize);
        let enabled_bit = tac_reg & 0b100;
        let clock_select = tac_reg & 0b11;
        let mut watched_bit : u8 = 0;
        match clock_select {
            0 => watched_bit = 9,
            1 => watched_bit = 3,
            2 => watched_bit = 5,
            3 => watched_bit = 7,
            _ => (),
        }

        // TODO too many function calls but whatever for now
        for _ in 0..t_cycles {
            mmu_ref.tick_oam_dma();
            mmu_ref.tick_tima_reload();
            mmu_ref.tick_serial();
            let old_div : u16 = mmu_ref.fetch_div();
            mmu_ref.increment_div(1);
            let new_div : u16 = mmu_ref.fetch_div();

            let old_bit = (old_div >> watched_bit) & 1;
            let new_bit = (new_div >> watched_bit) & 1;

            // increment TIMA only on falling edges
            if old_bit == 1 && new_bit == 0 && (enabled_bit > 0) {
                mmu_ref.increment_tima();
            }
        }
        if let Some(ppu) = ppu_ref {
            let peripheral_cycles = mmu_ref.peripheral_cycles(t_cycles);
            ppu.cycle(peripheral_cycles, mmu_ref);
        }


        
        //println!("DIV {:#04x} CLOCK SELECT {} OLD BIT {} NEW BIT {}", new_div, clock_select, old_bit, new_bit);
        
        // TODO go and account for that pain in the ass edge case when you
        // write to TMA on the same cycle that you overflow TIMA
    }

    fn advance_timers_to(
        &mut self,
        target: u8,
        mmu_ref: &mut mmu::MMU,
        ppu_ref: &mut Option<&mut PPU>,
    ) {
        if target > self.timer_cycles_advanced {
            self.update_system_clocks(
                target - self.timer_cycles_advanced,
                mmu_ref,
                ppu_ref.as_deref_mut(),
            );
            self.timer_cycles_advanced = target;
        }
    }

    fn timed_read(
        &mut self,
        mmu_ref: &mut mmu::MMU,
        ppu_ref: &mut Option<&mut PPU>,
        addr: usize,
        at_cycle: u8,
    ) -> u8 {
        self.advance_timers_to(at_cycle, mmu_ref, ppu_ref);
        mmu_ref.get_byte(addr)
    }

    fn timed_write(
        &mut self,
        mmu_ref: &mut mmu::MMU,
        ppu_ref: &mut Option<&mut PPU>,
        addr: usize,
        data: u8,
        at_cycle: u8,
    ) {
        self.advance_timers_to(at_cycle, mmu_ref, ppu_ref);
        mmu_ref.set_byte(addr, data);
    }

    fn timed_immediate_word(&mut self, mmu_ref: &mut mmu::MMU) -> u16 {
        let low_addr = self.pc.wrapping_add(1) as usize;
        let high_addr = self.pc.wrapping_add(2) as usize;
        let low = if (0xFE00..=0xFE9F).contains(&low_addr) {
            self.timed_read(mmu_ref, &mut None, low_addr, 0)
        } else {
            self.fetch(low_addr as u16, mmu_ref)
        };
        let high = if (0xFE00..=0xFE9F).contains(&high_addr) {
            self.timed_read(mmu_ref, &mut None, high_addr, 4)
        } else {
            self.fetch(high_addr as u16, mmu_ref)
        };
        u16::from(low) | (u16::from(high) << 8)
    }

    fn service_interrupt(&mut self, mmu_ref: &mut mmu::MMU, ppu_ref: &mut Option<&mut PPU>) -> u8 {
        let interrupt_vectors = [0x40, 0x48, 0x50, 0x58, 0x60];
        self.ime = false;
        self.set_sp(self.get_sp().wrapping_sub(1));
        self.timed_write(
            mmu_ref,
            ppu_ref,
            self.get_sp() as usize,
            (self.pc >> 8) as u8,
            8,
        );

        let pending_after_high_push = mmu_ref.get_ie() & mmu_ref.get_if() & 0x1F;
        let Some(interrupt) = (0..5).find(|bit| pending_after_high_push & (1 << bit) != 0) else {
            self.pc = 0x0000;
            self.halt_bug_enabled = false;
            return 20;
        };
        let mask = 1 << interrupt;
        let interrupt_flags = mmu_ref.get_if();
        mmu_ref.set_if(interrupt_flags & !mask);

        self.set_sp(self.get_sp().wrapping_sub(1));
        self.timed_write(
            mmu_ref,
            ppu_ref,
            self.get_sp() as usize,
            (self.pc & 0xFF) as u8,
            12,
        );
        self.pc = interrupt_vectors[interrupt];
        self.halt_bug_enabled = false;

        20
    }


    // performs a F/D/E/WB cycle
    pub fn cycle(&mut self, mmu_ref : &mut mmu::MMU) -> u8 {
        self.cycle_inner(mmu_ref, &mut None)
    }

    pub fn cycle_with_ppu(&mut self, mmu_ref: &mut mmu::MMU, ppu_ref: &mut PPU) -> u8 {
        self.cycle_inner(mmu_ref, &mut Some(ppu_ref))
    }

    fn cycle_inner(&mut self, mmu_ref : &mut mmu::MMU, ppu_ref: &mut Option<&mut PPU>) -> u8 {
        self.timer_cycles_advanced = 0;
        if self.stop_flag {
            return 0;
        }

        let pending_interrupts = mmu_ref.get_ie() & mmu_ref.get_if() & 0x1F;
        if self.halt_flag {
            if pending_interrupts == 0 {
                let cycles = 4;
                self.update_system_clocks(cycles, mmu_ref, ppu_ref.as_deref_mut());
                return cycles;
            }
            self.halt_flag = false;
        }

        if self.ime && pending_interrupts != 0 {
            let cycles = self.service_interrupt(mmu_ref, ppu_ref);
            self.update_system_clocks(
                cycles - self.timer_cycles_advanced,
                mmu_ref,
                ppu_ref.as_deref_mut(),
            );
            return cycles;
        }

        let next_opcode : u8;
        let apply_halt_bug = self.halt_bug_enabled;
        self.halt_bug_enabled = false;

        // check for 0xCB prefix
        let first_opcode = mmu_ref.get_opcode_byte(self.pc as usize);
        let cb_prefix : bool = first_opcode == 0xCB;
        if cb_prefix {
            if !apply_halt_bug {
                self.pc += 1;
            }
        } else if apply_halt_bug {
            // HALT with IME clear and a pending interrupt suppresses the PC
            // increment for the next opcode fetch. Model that by shifting the
            // decoder's instruction origin back one byte after fetching it.
            self.pc = self.pc.wrapping_sub(1);
        }
        next_opcode = if cb_prefix {
            mmu_ref.get_opcode_byte(self.pc as usize)
        } else {
            first_opcode
        };

        // disassemble and print the instruction we're looking at, for debugging purposes
        if self.trace_enabled {
            let nn = self.next_word(self.pc+1, mmu_ref);
            let n = self.fetch(self.pc+1, mmu_ref);

            let disasm = self.dis.lookup(next_opcode, cb_prefix, n, nn);
            println!("[{:#04X}] {}", self.pc, disasm.unwrap_or("???".to_string()));
        }

        // returns the number of cycles for the current instruction
        let cycles = self.decode_execute(next_opcode, mmu_ref, ppu_ref, cb_prefix);

        // The bundled DMG boot ROM cannot provide the CGB register state itself.
        // Preserve its visual boot sequence, then expose CGB hardware to dual-mode
        // cartridges through the documented A=0x11 handoff value.
        if !self.cgb_handoff_done && mmu_ref.get_boot() != 0 {
            if mmu_ref.cgb_mode() {
                self.a = 0x11;
            }
            self.cgb_handoff_done = true;
        }

        self.update_system_clocks(
            cycles - self.timer_cycles_advanced,
            mmu_ref,
            ppu_ref.as_deref_mut(),
        );

        return cycles;
    }


    // fetches the next byte in memory
    fn fetch(&self, addr : u16, mmu_ref : &mut mmu::MMU) -> u8 {
            let next_byte : u8 = mmu_ref.get_byte(addr as usize);
            return next_byte;
    }

    fn next_word(&self, addr : u16, mmu_ref : &mut mmu::MMU) -> u16 {
            let b1 = self.fetch(addr, mmu_ref) as u16;
            let b2 = (self.fetch(addr+1, mmu_ref) as u16) << 8;
            return b1 | b2; // we have to mind the little-endianness of the GB EMU.
    }

    fn decode_execute(
        &mut self,
        mut opcode : u8,
        mmu_ref : &mut mmu::MMU,
        ppu_ref: &mut Option<&mut PPU>,
        cb_prefix : bool,
    ) -> u8 {
            let i1 = ((opcode & 0xF0) >> 4) as usize;
            let i2 = (opcode & 0x0F) as usize;
            let mut cycles : u8 = ternary!(cb_prefix, cpu_tables::cb_prefixed_cycle_times[i1][i2], cpu_tables::cycle_times[i1][i2]);
            mmu_ref.apu_bus_cycles = cycles;
            let	instruction_size : u8 = ternary!(cb_prefix, 2, cpu_tables::instruction_sizes[i1][i2]);
            let mut skip_increment = false;
            let ei_was_pending = self.ei_pending;

            let nn = self.next_word(self.pc+1, mmu_ref);
            let n = self.fetch(self.pc+1, mmu_ref);

            // PRINT FLAGS FOR DEBUG
            /*
            println!(
                "FLAGS before: Z={} N={} H={} C={}",
                get_flag(&mut self.f, ZERO),
                get_flag(&mut self.f, SUB),
                get_flag(&mut self.f, HC),
                get_flag(&mut self.f, CARRY)
            );
            */


            if cb_prefix {
                    // decrement to 'back up' once
                    self.pc = self.pc - 1;
                    if opcode & 0x07 == 0x06 {
                        let addr = self.get_hl() as usize;
                        let bit = (opcode >> 3) & 0x07;
                        let is_bit_test = (0x40..=0x7F).contains(&opcode);
                        let read_cycle = if is_bit_test { cycles - 8 } else { cycles - 12 };
                        let value = self.timed_read(mmu_ref, ppu_ref, addr, read_cycle);
                        let result = match opcode {
                            0x00..=0x3F => Some(match bit {
                                0 => rlc(value, &mut self.f),
                                1 => rrc(value, &mut self.f),
                                2 => rl(value, &mut self.f),
                                3 => rr(value, &mut self.f),
                                4 => sla(value, &mut self.f),
                                5 => sra(value, &mut self.f),
                                6 => swap(value, &mut self.f),
                                7 => srl(value, &mut self.f),
                                _ => unreachable!(),
                            }),
                            0x40..=0x7F => {
                                test_bit(value, bit, &mut self.f);
                                None
                            },
                            0x80..=0xBF => Some(value & !(1 << bit)),
                            0xC0..=0xFF => Some(value | (1 << bit)),
                        };
                        if let Some(result) = result {
                            self.timed_write(mmu_ref, ppu_ref, addr, result, cycles - 8);
                        }
                    } else {
                    match opcode {
                            0x00 => self.b = rlc(self.b, &mut self.f),
                            0x01 => self.c = rlc(self.c, &mut self.f),
                            0x02 => self.d = rlc(self.d, &mut self.f),
                            0x03 => self.e = rlc(self.e, &mut self.f),
                            0x04 => self.h = rlc(self.h, &mut self.f),
                            0x05 => self.l = rlc(self.l, &mut self.f),
                            0x06 => mmu_ref.set_byte(self.get_hl() as usize, rlc(mmu_ref.get_byte(self.get_hl() as usize), &mut self.f)),
                            0x07 => self.a = rlc(self.a, &mut self.f),
                            0x08 => self.b = rrc(self.b, &mut self.f),
                            0x09 => self.c = rrc(self.c, &mut self.f),
                            0x0A => self.d = rrc(self.d, &mut self.f),
                            0x0B => self.e = rrc(self.e, &mut self.f),
                            0x0C => self.h = rrc(self.h, &mut self.f),
                            0x0D => self.l = rrc(self.l, &mut self.f),
                            0x0E => mmu_ref.set_byte(self.get_hl() as usize, rrc(mmu_ref.get_byte(self.get_hl() as usize), &mut self.f)),
                            0x0F => self.a = rrc(self.a, &mut self.f),
                            0x10 => self.b = rl(self.b, &mut self.f),
                            0x11 => self.c = rl(self.c, &mut self.f),
                            0x12 => self.d = rl(self.d, &mut self.f),
                            0x13 => self.e = rl(self.e, &mut self.f),
                            0x14 => self.h = rl(self.h, &mut self.f),
                            0x15 => self.l = rl(self.l, &mut self.f),
                            0x16 => mmu_ref.set_byte(self.get_hl() as usize, rl(mmu_ref.get_byte(self.get_hl() as usize), &mut self.f)),
                            0x17 => self.a = rl(self.a, &mut self.f),
                            0x18 => self.b = rr(self.b, &mut self.f),
                            0x19 => self.c = rr(self.c, &mut self.f),
                            0x1A => self.d = rr(self.d, &mut self.f),
                            0x1B => self.e = rr(self.e, &mut self.f),
                            0x1C => self.h = rr(self.h, &mut self.f),
                            0x1D => self.l = rr(self.l, &mut self.f),
                            0x1E => mmu_ref.set_byte(self.get_hl() as usize, rr(mmu_ref.get_byte(self.get_hl() as usize), &mut self.f)),
                            0x1F => self.a = rr(self.a, &mut self.f),
                            0x20 => self.b = sla(self.b, &mut self.f),
                            0x21 => self.c = sla(self.c, &mut self.f),
                            0x22 => self.d = sla(self.d, &mut self.f),
                            0x23 => self.e = sla(self.e, &mut self.f),
                            0x24 => self.h = sla(self.h, &mut self.f),
                            0x25 => self.l = sla(self.l, &mut self.f),
                            0x26 => mmu_ref.set_byte(self.get_hl() as usize, sla(mmu_ref.get_byte(self.get_hl() as usize), &mut self.f)),
                            0x27 => self.a = sla(self.a, &mut self.f),
                            0x28 => self.b = sra(self.b, &mut self.f),
                            0x29 => self.c = sra(self.c, &mut self.f),
                            0x2A => self.d = sra(self.d, &mut self.f),
                            0x2B => self.e = sra(self.e, &mut self.f),
                            0x2C => self.h = sra(self.h, &mut self.f),
                            0x2D => self.l = sra(self.l, &mut self.f),
                            0x2E => mmu_ref.set_byte(self.get_hl() as usize, sra(mmu_ref.get_byte(self.get_hl() as usize), &mut self.f)),
                            0x2F => self.a = sra(self.a, &mut self.f),
                            0x30 => self.b = swap(self.b, &mut self.f),
                            0x31 => self.c = swap(self.c, &mut self.f),
                            0x32 => self.d = swap(self.d, &mut self.f),
                            0x33 => self.e = swap(self.e, &mut self.f),
                            0x34 => self.h = swap(self.h, &mut self.f),
                            0x35 => self.l = swap(self.l, &mut self.f),
                            0x36 => mmu_ref.set_byte(self.get_hl() as usize, swap(mmu_ref.get_byte(self.get_hl() as usize), &mut self.f)),
                            0x37 => self.a = swap(self.a, &mut self.f),
                            0x38 => self.b = srl(self.b, &mut self.f),
                            0x39 => self.c = srl(self.c, &mut self.f),
                            0x3A => self.d = srl(self.d, &mut self.f),
                            0x3B => self.e = srl(self.e, &mut self.f),
                            0x3C => self.h = srl(self.h, &mut self.f),
                            0x3D => self.l = srl(self.l, &mut self.f),
                            0x3E => {
                                let at_hl = mmu_ref.get_byte(self.get_hl() as usize);
                                let at_hl_shifted = srl(at_hl, &mut self.f);
                                mmu_ref.set_byte(self.get_hl() as usize, at_hl_shifted);
                            },
                            0x3F => self.a = srl(self.a, &mut self.f),
                            0x40 => test_bit(self.b, 0, &mut self.f),
                            0x41 => test_bit(self.c, 0, &mut self.f),
                            0x42 => test_bit(self.d, 0, &mut self.f),
                            0x43 => test_bit(self.e, 0, &mut self.f),
                            0x44 => test_bit(self.h, 0, &mut self.f),
                            0x45 => test_bit(self.l, 0, &mut self.f),
                            0x46 => test_bit(mmu_ref.get_byte(self.get_hl() as usize), 0, &mut self.f),
                            0x47 => test_bit(self.a, 0, &mut self.f),
                            0x48 => test_bit(self.b, 1, &mut self.f),
                            0x49 => test_bit(self.c, 1, &mut self.f),
                            0x4A => test_bit(self.d, 1, &mut self.f),
                            0x4B => test_bit(self.e, 1, &mut self.f),
                            0x4C => test_bit(self.h, 1, &mut self.f),
                            0x4D => test_bit(self.l, 1, &mut self.f),
                            0x4E => test_bit(mmu_ref.get_byte(self.get_hl() as usize), 1, &mut self.f),
                            0x4F => test_bit(self.a, 1, &mut self.f),
                            0x50 => test_bit(self.b, 2, &mut self.f),
                            0x51 => test_bit(self.c, 2, &mut self.f),
                            0x52 => test_bit(self.d, 2, &mut self.f),
                            0x53 => test_bit(self.e, 2, &mut self.f),
                            0x54 => test_bit(self.h, 2, &mut self.f),
                            0x55 => test_bit(self.l, 2, &mut self.f),
                            0x56 => test_bit(mmu_ref.get_byte(self.get_hl() as usize), 2, &mut self.f),
                            0x57 => test_bit(self.a, 2, &mut self.f),
                            0x58 => test_bit(self.b, 3, &mut self.f),
                            0x59 => test_bit(self.c, 3, &mut self.f),
                            0x5A => test_bit(self.d, 3, &mut self.f),
                            0x5B => test_bit(self.e, 3, &mut self.f),
                            0x5C => test_bit(self.h, 3, &mut self.f),
                            0x5D => test_bit(self.l, 3, &mut self.f),
                            0x5E => test_bit(mmu_ref.get_byte(self.get_hl() as usize), 3, &mut self.f),
                            0x5F => test_bit(self.a, 3, &mut self.f),
                            0x60 => test_bit(self.b, 4, &mut self.f),
                            0x61 => test_bit(self.c, 4, &mut self.f),
                            0x62 => test_bit(self.d, 4, &mut self.f),
                            0x63 => test_bit(self.e, 4, &mut self.f),
                            0x64 => test_bit(self.h, 4, &mut self.f),
                            0x65 => test_bit(self.l, 4, &mut self.f),
                            0x66 => test_bit(mmu_ref.get_byte(self.get_hl() as usize), 4, &mut self.f),
                            0x67 => test_bit(self.a, 4, &mut self.f),
                            0x68 => test_bit(self.b, 5, &mut self.f),
                            0x69 => test_bit(self.c, 5, &mut self.f),
                            0x6A => test_bit(self.d, 5, &mut self.f),
                            0x6B => test_bit(self.e, 5, &mut self.f),
                            0x6C => test_bit(self.h, 5, &mut self.f),
                            0x6D => test_bit(self.l, 5, &mut self.f),
                            0x6E => test_bit(mmu_ref.get_byte(self.get_hl() as usize), 5, &mut self.f),
                            0x6F => test_bit(self.a, 5, &mut self.f),
                            0x70 => test_bit(self.b, 6, &mut self.f),
                            0x71 => test_bit(self.c, 6, &mut self.f),
                            0x72 => test_bit(self.d, 6, &mut self.f),
                            0x73 => test_bit(self.e, 6, &mut self.f),
                            0x74 => test_bit(self.h, 6, &mut self.f),
                            0x75 => test_bit(self.l, 6, &mut self.f),
                            0x76 => test_bit(mmu_ref.get_byte(self.get_hl() as usize), 6, &mut self.f),
                            0x77 => test_bit(self.a, 6, &mut self.f),
                            0x78 => test_bit(self.b, 7, &mut self.f),
                            0x79 => test_bit(self.c, 7, &mut self.f),
                            0x7A => test_bit(self.d, 7, &mut self.f),
                            0x7B => test_bit(self.e, 7, &mut self.f),
                            0x7C => test_bit(self.h, 7, &mut self.f),
                            0x7D => test_bit(self.l, 7, &mut self.f),
                            0x7E => test_bit(mmu_ref.get_byte(self.get_hl() as usize), 7, &mut self.f),
                            0x7F => test_bit(self.a, 7, &mut self.f),
                            0x80 => self.b = self.b & !(1 << 0),
                            0x81 => self.c = self.c & !(1 << 0),
                            0x82 => self.d = self.d & !(1 << 0),
                            0x83 => self.e = self.e & !(1 << 0),
                            0x84 => self.h = self.h & !(1 << 0),
                            0x85 => self.l = self.l & !(1 << 0),
                            0x86 => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) & !(1 << 0)),
                            0x87 => self.a = self.a & !(1 << 0),
                            0x88 => self.b = self.b & !(1 << 1),
                            0x89 => self.c = self.c & !(1 << 1),
                            0x8A => self.d = self.d & !(1 << 1),
                            0x8B => self.e = self.e & !(1 << 1),
                            0x8C => self.h = self.h & !(1 << 1),
                            0x8D => self.l = self.l & !(1 << 1),
                            0x8E => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) & !(1 << 1)),
                            0x8F => self.a = self.a & !(1 << 1),
                            0x90 => self.b = self.b & !(1 << 2),
                            0x91 => self.c = self.c & !(1 << 2),
                            0x92 => self.d = self.d & !(1 << 2),
                            0x93 => self.e = self.e & !(1 << 2),
                            0x94 => self.h = self.h & !(1 << 2),
                            0x95 => self.l = self.l & !(1 << 2),
                            0x96 => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) & !(1 << 2)),
                            0x97 => self.a = self.a & !(1 << 2),
                            0x98 => self.b = self.b & !(1 << 3),
                            0x99 => self.c = self.c & !(1 << 3),
                            0x9A => self.d = self.d & !(1 << 3),
                            0x9B => self.e = self.e & !(1 << 3),
                            0x9C => self.h = self.h & !(1 << 3),
                            0x9D => self.l = self.l & !(1 << 3),
                            0x9E => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) & !(1 << 3)),
                            0x9F => self.a = self.a & !(1 << 3),
                            0xA0 => self.b = self.b & !(1 << 4),
                            0xA1 => self.c = self.c & !(1 << 4),
                            0xA2 => self.d = self.d & !(1 << 4),
                            0xA3 => self.e = self.e & !(1 << 4),
                            0xA4 => self.h = self.h & !(1 << 4),
                            0xA5 => self.l = self.l & !(1 << 4),
                            0xA6 => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) & !(1 << 4)),
                            0xA7 => self.a = self.a & !(1 << 4),
                            0xA8 => self.b = self.b & !(1 << 5),
                            0xA9 => self.c = self.c & !(1 << 5),
                            0xAA => self.d = self.d & !(1 << 5),
                            0xAB => self.e = self.e & !(1 << 5),
                            0xAC => self.h = self.h & !(1 << 5),
                            0xAD => self.l = self.l & !(1 << 5),
                            0xAE => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) & !(1 << 5)),
                            0xAF => self.a = self.a & !(1 << 5),
                            0xB0 => self.b = self.b & !(1 << 6),
                            0xB1 => self.c = self.c & !(1 << 6),
                            0xB2 => self.d = self.d & !(1 << 6),
                            0xB3 => self.e = self.e & !(1 << 6),
                            0xB4 => self.h = self.h & !(1 << 6),
                            0xB5 => self.l = self.l & !(1 << 6),
                            0xB6 => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) & !(1 << 6)),
                            0xB7 => self.a = self.a & !(1 << 6),
                            0xB8 => self.b = self.b & !(1 << 7),
                            0xB9 => self.c = self.c & !(1 << 7),
                            0xBA => self.d = self.d & !(1 << 7),
                            0xBB => self.e = self.e & !(1 << 7),
                            0xBC => self.h = self.h & !(1 << 7),
                            0xBD => self.l = self.l & !(1 << 7),
                            0xBE => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) & !(1 << 7)),
                            0xBF => self.a = self.a & !(1 << 7),
                            0xC0 => self.b = self.b | (1 << 0),
                            0xC1 => self.c = self.c | (1 << 0),
                            0xC2 => self.d = self.d | (1 << 0),
                            0xC3 => self.e = self.e | (1 << 0),
                            0xC4 => self.h = self.h | (1 << 0),
                            0xC5 => self.l = self.l | (1 << 0),
                            0xC6 => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) | (1 << 0)),
                            0xC7 => self.a = self.a | (1 << 0),
                            0xC8 => self.b = self.b | (1 << 1),
                            0xC9 => self.c = self.c | (1 << 1),
                            0xCA => self.d = self.d | (1 << 1),
                            0xCB => self.e = self.e | (1 << 1),
                            0xCC => self.h = self.h | (1 << 1),
                            0xCD => self.l = self.l | (1 << 1),
                            0xCE => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) | (1 << 1)),
                            0xCF => self.a = self.a | (1 << 1),
                            0xD0 => self.b = self.b | (1 << 2),
                            0xD1 => self.c = self.c | (1 << 2),
                            0xD2 => self.d = self.d | (1 << 2),
                            0xD3 => self.e = self.e | (1 << 2),
                            0xD4 => self.h = self.h | (1 << 2),
                            0xD5 => self.l = self.l | (1 << 2),
                            0xD6 => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) | (1 << 2)),
                            0xD7 => self.a = self.a | (1 << 2),
                            0xD8 => self.b = self.b | (1 << 3),
                            0xD9 => self.c = self.c | (1 << 3),
                            0xDA => self.d = self.d | (1 << 3),
                            0xDB => self.e = self.e | (1 << 3),
                            0xDC => self.h = self.h | (1 << 3),
                            0xDD => self.l = self.l | (1 << 3),
                            0xDE => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) | (1 << 3)),
                            0xDF => self.a = self.a | (1 << 3),
                            0xE0 => self.b = self.b | (1 << 4),
                            0xE1 => self.c = self.c | (1 << 4),
                            0xE2 => self.d = self.d | (1 << 4),
                            0xE3 => self.e = self.e | (1 << 4),
                            0xE4 => self.h = self.h | (1 << 4),
                            0xE5 => self.l = self.l | (1 << 4),
                            0xE6 => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) | (1 << 4)),
                            0xE7 => self.a = self.a | (1 << 4),
                            0xE8 => self.b = self.b | (1 << 5),
                            0xE9 => self.c = self.c | (1 << 5),
                            0xEA => self.d = self.d | (1 << 5),
                            0xEB => self.e = self.e | (1 << 5),
                            0xEC => self.h = self.h | (1 << 5),
                            0xED => self.l = self.l | (1 << 5),
                            0xEE => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) | (1 << 5)),
                            0xEF => self.a = self.a | (1 << 5),
                            0xF0 => self.b = self.b | (1 << 6),
                            0xF1 => self.c = self.c | (1 << 6),
                            0xF2 => self.d = self.d | (1 << 6),
                            0xF3 => self.e = self.e | (1 << 6),
                            0xF4 => self.h = self.h | (1 << 6),
                            0xF5 => self.l = self.l | (1 << 6),
                            0xF6 => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) | (1 << 6)),
                            0xF7 => self.a = self.a | (1 << 6),
                            0xF8 => self.b = self.b | (1 << 7),
                            0xF9 => self.c = self.c | (1 << 7),
                            0xFA => self.d = self.d | (1 << 7),
                            0xFB => self.e = self.e | (1 << 7),
                            0xFC => self.h = self.h | (1 << 7),
                            0xFD => self.l = self.l | (1 << 7),
                            0xFE => mmu_ref.set_byte(self.get_hl() as usize, mmu_ref.get_byte(self.get_hl() as usize) | (1 << 7)),
                            0xFF => self.a = self.a | (1 << 7),
                            _ => {
                                    panic!("Error: CB Invalid opcode!");
                            }
                    }
                    }
            }
            else {
                    match opcode {
                            0x00 => (),
                            0x01 => ld_word(&mut self.b, &mut self.c, nn),
                            0x02 => mmu_ref.set_byte(self.get_bc() as usize, self.a),
                            0x03 => {
                                let value = self.get_bc();
                                mmu_ref.trigger_oam_idu_corruption(value);
                                self.set_bc(value.wrapping_add(1));
                            },
                            0x04 => self.b = inc_reg(self.b, &mut self.f),
                            0x05 => self.b = dec_reg(self.b, &mut self.f),
                            0x06 => self.b = n,
                            0x07 => rlca(&mut self.a, &mut self.f),
                            0x08 => { 
                                let sp : u16 = self.get_sp();
                                mmu_ref.set_byte(nn as usize, (sp & 0xFF) as u8);
                                mmu_ref.set_byte((nn+1) as usize, ((sp & 0xFF00) >> 8) as u8);
                            },
                            0x09 => {
                                let hl = self.get_hl();
                                let bc = self.get_bc();
                                let new_hl = add_double_regs(hl, bc, &mut self.f);
                                self.set_hl(new_hl);
                            },
                            0x0A => self.a = mmu_ref.get_byte(self.get_bc() as usize),
                            0x0B => {
                                let bc = self.get_bc();
                                mmu_ref.trigger_oam_idu_corruption(bc);
                                let new_bc = bc.wrapping_sub(1);
                                self.set_bc(new_bc);
                            },
                            0x0C => self.c = inc_reg(self.c, &mut self.f),
                            0x0D => self.c = dec_reg(self.c, &mut self.f),
                            0x0E => self.c = n,
                            0x0F => rrca(&mut self.a, &mut self.f),
                            0x10 => {
                                if !mmu_ref.perform_speed_switch() {
                                    self.stop_flag = true;
                                }
                                mmu_ref.set_byte(0xFF04 as usize, 0);
                            },
                            0x11 => self.set_de(nn),
                            0x12 => mmu_ref.set_byte(self.get_de() as usize, self.a),
                            0x13 => {
                                let value = self.get_de();
                                mmu_ref.trigger_oam_idu_corruption(value);
                                self.set_de(value.wrapping_add(1));
                            },
                            0x14 => self.d = inc_reg(self.d, &mut self.f),
                            0x15 => self.d = dec_reg(self.d, &mut self.f),
                            0x16 => self.d = n,
                            0x17 => self.a = rla(self.a, &mut self.f),
                            0x18 => self.jump_relative(n),
                            0x19 => {
                                let hl = self.get_hl();
                                let de = self.get_de();
                                let new_hl = add_double_regs(hl, de, &mut self.f);
                                self.set_hl(new_hl);
                            },
                            0x1A => self.a = mmu_ref.get_byte(self.get_de() as usize),
                            0x1B => {
                                let de = self.get_de();
                                mmu_ref.trigger_oam_idu_corruption(de);
                                let new_de = de.wrapping_sub(1);
                                self.set_de(new_de);
                            },
                            0x1C => self.e = inc_reg(self.e, &mut self.f),
                            0x1D => self.e = dec_reg(self.e, &mut self.f),
                            0x1E => self.e = n,
                            0x1F => rra(&mut self.a, &mut self.f),
                            0x20 => {
                                if (self.f & 0b10000000) == 0 {
                                    self.jump_relative(n);
                                    cycles = 12;
                                }
                            },
                            0x21 => ld_word(&mut self.h, &mut self.l, nn),
                            0x22 => { 
                                let hl_val = self.get_hl(); 
                                mmu_ref.set_byte(hl_val as usize, self.a);
                                mmu_ref.trigger_oam_idu_corruption(hl_val);
                                self.set_hl(hl_val.wrapping_add(1)); 
                            },
                            0x23 => {
                                let value = self.get_hl();
                                mmu_ref.trigger_oam_idu_corruption(value);
                                self.set_hl(value.wrapping_add(1));
                            },
                            0x24 => self.h = inc_reg(self.h, &mut self.f),
                            0x25 => self.h = dec_reg(self.h, &mut self.f),
                            0x26 => self.h = n,
                            0x27 => daa(&mut self.a, &mut self.f),
                            0x28 => {
                                if (self.f & 0b10000000) != 0 {
                                    self.jump_relative(n);
                                    cycles = 12;
                                }
                            }
                            0x29 => {
                                let hl = self.get_hl();
                                let new_hl = add_double_regs(hl, hl, &mut self.f);
                                self.set_hl(new_hl);
                            },
                            0x2A => {
                                let hl = self.get_hl();
                                mmu_ref.trigger_oam_read_idu_corruption(hl);
                                self.a = mmu_ref.get_byte(hl as usize);
                                let new_hl = hl.wrapping_add(1);
                                self.set_hl(new_hl);
                            },
                            0x2B => {
                                let hl = self.get_hl();
                                mmu_ref.trigger_oam_idu_corruption(hl);
                                let new_hl = hl.wrapping_sub(1);
                                self.set_hl(new_hl);
                            },
                            0x2C => self.l = inc_reg(self.l, &mut self.f),
                            0x2D => self.l = dec_reg(self.l, &mut self.f),
                            0x2E => self.l = n,
                            0x2F => complement_reg(&mut self.a, &mut self.f),
                            0x30 => {
                                if !get_flag(&mut self.f, CARRY) {
                                    self.jump_relative(n);
                                    cycles = 12;
                                }
                            },
                            0x31 => ld_word(&mut self.s, &mut self.p, nn),
                            0x32 => { 
                                let hl_val = self.get_hl(); 
                                mmu_ref.set_byte(hl_val as usize, self.a);
                                mmu_ref.trigger_oam_idu_corruption(hl_val);
                                self.set_hl(hl_val.wrapping_sub(1));
                            },
                            0x33 => {
                                let value = self.get_sp();
                                mmu_ref.trigger_oam_idu_corruption(value);
                                self.set_sp(value.wrapping_add(1));
                            },
                            0x34 => {
                                let addr = self.get_hl() as usize;
                                let at_hl = self.timed_read(mmu_ref, ppu_ref, addr, cycles - 12);
                                let new_at_hl = inc_reg(at_hl, &mut self.f);
                                self.timed_write(mmu_ref, ppu_ref, addr, new_at_hl, cycles - 8);
                            },
                            0x35 => {
                                let addr = self.get_hl() as usize;
                                let at_hl = self.timed_read(mmu_ref, ppu_ref, addr, cycles - 12);
                                let new_at_hl = dec_reg(at_hl, &mut self.f);
                                self.timed_write(mmu_ref, ppu_ref, addr, new_at_hl, cycles - 8);
                            },
                            0x36 => {
                                self.timed_write(mmu_ref, ppu_ref, self.get_hl() as usize, n, cycles - 8);
                            },
                            0x37 => scf(&mut self.f),
                            0x38 => {
                                if get_flag(&mut self.f, CARRY) {
                                    self.jump_relative(n);
                                    cycles = 12;
                                }
                            }
                            0x39 => {
                                let hl = self.get_hl();
                                let sp = self.get_sp();
                                let new_hl = add_double_regs(hl, sp, &mut self.f);
                                self.set_hl(new_hl);
                            },
                            0x3A => {
                                let hl = self.get_hl();
                                mmu_ref.trigger_oam_read_idu_corruption(hl);
                                self.a = mmu_ref.get_byte(hl as usize);
                                let new_hl = hl.wrapping_sub(1);
                                self.set_hl(new_hl);
                            },
                            0x3B => {
                                let sp = self.get_sp();
                                mmu_ref.trigger_oam_idu_corruption(sp);
                                let new_sp = sp.wrapping_sub(1);
                                self.set_sp(new_sp);
                            },
                            0x3C => self.a = inc_reg(self.a, &mut self.f),
                            0x3D => self.a = dec_reg(self.a, &mut self.f),
                            0x3E => self.a = n,
                            0x3F => ccf(&mut self.f),
                            0x40 => self.b = self.b,
                            0x41 => self.b = self.c,
                            0x42 => self.b = self.d,
                            0x43 => self.b = self.e,
                            0x44 => self.b = self.h,
                            0x45 => self.b = self.l,
                            0x46 => self.b = mmu_ref.get_byte(self.get_hl() as usize),
                            0x47 => self.b = self.a,
                            0x48 => self.c = self.b,
                            0x49 => self.c = self.c,
                            0x4A => self.c = self.d,
                            0x4B => self.c = self.e,
                            0x4C => self.c = self.h,
                            0x4D => self.c = self.l,
                            0x4E => self.c = mmu_ref.get_byte(self.get_hl() as usize),
                            0x4F => self.c = self.a,
                            0x50 => self.d = self.b,
                            0x51 => self.d = self.c,
                            0x52 => self.d = self.d,
                            0x53 => self.d = self.e,
                            0x54 => self.d = self.h,
                            0x55 => self.d = self.l,
                            0x56 => self.d = mmu_ref.get_byte(self.get_hl() as usize),
                            0x57 => self.d = self.a,
                            0x58 => self.e = self.b,
                            0x59 => self.e = self.c,
                            0x5A => self.e = self.d,
                            0x5B => self.e = self.e,
                            0x5C => self.e = self.h,
                            0x5D => self.e = self.l,
                            0x5E => self.e = mmu_ref.get_byte(self.get_hl() as usize),
                            0x5F => self.e = self.a,
                            0x60 => self.h = self.b,
                            0x61 => self.h = self.c,
                            0x62 => self.h = self.d,
                            0x63 => self.h = self.e,
                            0x64 => self.h = self.h,
                            0x65 => self.h = self.l,
                            0x66 => self.h = mmu_ref.get_byte(self.get_hl() as usize),
                            0x67 => self.h = self.a,
                            0x68 => self.l = self.b,
                            0x69 => self.l = self.c,
                            0x6A => self.l = self.d,
                            0x6B => self.l = self.e,
                            0x6C => self.l = self.h,
                            0x6D => self.l = self.l,
                            0x6E => self.l = mmu_ref.get_byte(self.get_hl() as usize),
                            0x6F => self.l = self.a,
                            0x70 => mmu_ref.set_byte(self.get_hl() as usize, self.b),
                            0x71 => mmu_ref.set_byte(self.get_hl() as usize, self.c),
                            0x72 => mmu_ref.set_byte(self.get_hl() as usize, self.d),
                            0x73 => mmu_ref.set_byte(self.get_hl() as usize, self.e),
                            0x74 => mmu_ref.set_byte(self.get_hl() as usize, self.h),
                            0x75 => mmu_ref.set_byte(self.get_hl() as usize, self.l),
                            0x76 => {
                                let pending = mmu_ref.get_ie() & mmu_ref.get_if() & 0x1F;
                                if !self.ime && pending != 0 {
                                    self.halt_bug_enabled = true;
                                } else {
                                    self.halt_flag = true;
                                }
                            },
                            0x77 => mmu_ref.set_byte(self.get_hl() as usize, self.a),
                            0x78 => self.a = self.b,
                            0x79 => self.a = self.c,
                            0x7A => self.a = self.d,
                            0x7B => self.a = self.e,
                            0x7C => self.a = self.h,
                            0x7D => self.a = self.l,
                            0x7E => self.a = mmu_ref.get_byte(self.get_hl() as usize),
                            0x7F => self.a = self.a,
                            0x80 => self.a = add_reg(self.a, self.b, &mut self.f),
                            0x81 => self.a = add_reg(self.a, self.c, &mut self.f),
                            0x82 => self.a = add_reg(self.a, self.d, &mut self.f),
                            0x83 => self.a = add_reg(self.a, self.e, &mut self.f),
                            0x84 => self.a = add_reg(self.a, self.h, &mut self.f),
                            0x85 => self.a = add_reg(self.a, self.l, &mut self.f),
                            0x86 => self.a = add_reg(self.a, mmu_ref.get_byte(self.get_hl() as usize), &mut self.f),
                            0x87 => self.a = add_reg(self.a, self.a, &mut self.f),
                            0x88 => self.a = adc_reg(self.a, self.b, &mut self.f),
                            0x89 => self.a = adc_reg(self.a, self.c, &mut self.f),
                            0x8A => self.a = adc_reg(self.a, self.d, &mut self.f),
                            0x8B => self.a = adc_reg(self.a, self.e, &mut self.f),
                            0x8C => self.a = adc_reg(self.a, self.h, &mut self.f),
                            0x8D => self.a = adc_reg(self.a, self.l, &mut self.f),
                            0x8E => self.a = adc_reg(self.a, mmu_ref.get_byte(self.get_hl() as usize), &mut self.f),
                            0x8F => self.a = adc_reg(self.a, self.a, &mut self.f),
                            0x90 => self.a = sub_reg(self.a, self.b, &mut self.f),
                            0x91 => self.a = sub_reg(self.a, self.c, &mut self.f),
                            0x92 => self.a = sub_reg(self.a, self.d, &mut self.f),
                            0x93 => self.a = sub_reg(self.a, self.e, &mut self.f),
                            0x94 => self.a = sub_reg(self.a, self.h, &mut self.f),
                            0x95 => self.a = sub_reg(self.a, self.l, &mut self.f),
                            0x96 => self.a = sub_reg(self.a, mmu_ref.get_byte(self.get_hl() as usize), &mut self.f),
                            0x97 => self.a = sub_reg(self.a, self.a, &mut self.f),
                            0x98 => self.a = sbc_reg(self.a, self.b, &mut self.f),
                            0x99 => self.a = sbc_reg(self.a, self.c, &mut self.f),
                            0x9A => self.a = sbc_reg(self.a, self.d, &mut self.f),
                            0x9B => self.a = sbc_reg(self.a, self.e, &mut self.f),
                            0x9C => self.a = sbc_reg(self.a, self.h, &mut self.f),
                            0x9D => self.a = sbc_reg(self.a, self.l, &mut self.f),
                            0x9E => self.a = sbc_reg(self.a, mmu_ref.get_byte(self.get_hl() as usize), &mut self.f),
                            0x9F => self.a = sbc_reg(self.a, self.a, &mut self.f),
                            0xA0 => and(&mut self.a, self.b, &mut self.f),
                            0xA1 => and(&mut self.a, self.c, &mut self.f),
                            0xA2 => and(&mut self.a, self.d, &mut self.f),
                            0xA3 => and(&mut self.a, self.e, &mut self.f),
                            0xA4 => and(&mut self.a, self.h, &mut self.f),
                            0xA5 => and(&mut self.a, self.l, &mut self.f),
                            0xA6 => {
                                let at_hl = mmu_ref.get_byte(self.get_hl() as usize);
                                and(&mut self.a, at_hl, &mut self.f);
                            },
                            0xA7 => self.f = ternary!(self.a == 0, 0b10100000, 0b00100000), 
                            0xA8 => xor(&mut self.a, self.b, &mut self.f),
                            0xA9 => xor(&mut self.a, self.c, &mut self.f),
                            0xAA => xor(&mut self.a, self.d, &mut self.f),
                            0xAB => xor(&mut self.a, self.e, &mut self.f),
                            0xAC => xor(&mut self.a, self.h, &mut self.f),
                            0xAD => xor(&mut self.a, self.l, &mut self.f),
                            0xAE => {
                                let at_hl = mmu_ref.get_byte(self.get_hl() as usize);
                                xor(&mut self.a, at_hl, &mut self.f);
                            },
                            0xAF => {
                                let a_val = self.a;
                                xor(&mut self.a, a_val, &mut self.f);
                            },
                            0xB0 => or(&mut self.a, self.b, &mut self.f),
                            0xB1 => or(&mut self.a, self.c, &mut self.f),
                            0xB2 => or(&mut self.a, self.d, &mut self.f),
                            0xB3 => or(&mut self.a, self.e, &mut self.f),
                            0xB4 => or(&mut self.a, self.h, &mut self.f),
                            0xB5 => or(&mut self.a, self.l, &mut self.f),
                            0xB6 => {
                                let at_hl = mmu_ref.get_byte(self.get_hl() as usize);
                                or(&mut self.a, at_hl, &mut self.f);
                            },
                            0xB7 => self.f = ternary!(self.a == 0, 0b10000000, 0b00000000), 
                            0xB8 => compare(self.a, self.b, &mut self.f),
                            0xB9 => compare(self.a, self.c, &mut self.f),
                            0xBA => compare(self.a, self.d, &mut self.f),
                            0xBB => compare(self.a, self.e, &mut self.f),
                            0xBC => compare(self.a, self.h, &mut self.f),
                            0xBD => compare(self.a, self.l, &mut self.f),
                            0xBE => compare(self.a, mmu_ref.get_byte(self.get_hl() as usize), &mut self.f),
                            0xBF => compare(self.a, self.a, &mut self.f),
                            0xC0 => {
                                if !get_flag(&mut self.f, ZERO) {
                                    self.pc = self.stack_pop_at(mmu_ref, ppu_ref, 4, 8);
                                    skip_increment = true;
                                    cycles = 20;
                                }
                            },
                            0xC1 => {let bc_val = self.stack_pop(mmu_ref); self.set_bc(bc_val);},
                            0xC2 => {
                                let target = self.timed_immediate_word(mmu_ref);
                                if !get_flag(&mut self.f, ZERO) {
                                    self.pc = target;
                                    skip_increment = true;
                                    cycles = 16;
                                }
                            },
                            0xC3 => self.pc = self.timed_immediate_word(mmu_ref) - (instruction_size as u16),
                            0xC4 => {
                                let target = self.timed_immediate_word(mmu_ref);
                                if(!get_flag(&mut self.f, ZERO)) {
                                    self.pc += instruction_size as u16;
                                    self.stack_push_at(mmu_ref, ppu_ref, self.pc, 12, 16);
                                    self.pc = target;
                                    skip_increment = true;
                                    cycles = 24;
                                }
                            },
                            0xC5 => self.stack_push_at(mmu_ref, ppu_ref, self.get_bc(), 4, 8),
                            0xC6 => self.a = add_reg(self.a, n, &mut self.f),
                            0xC7 => {
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc+1, 4, 8);
                                self.pc = 0x00;
                                skip_increment = true;
                            },
                            0xC8 => {
                                if(get_flag(&mut self.f, ZERO)) {
                                    self.pc = self.stack_pop_at(mmu_ref, ppu_ref, 4, 8);
                                    skip_increment = true;
                                    cycles = 20;
                                }
                            },
                            0xC9 => { 
                                //println!("RETURN!!\n"); 
                                self.pc = self.stack_pop(mmu_ref);
                                //println!("pc val: {:#04x}", self.pc);
                                skip_increment = true;
                            },
                            0xCA => {
                                let target = self.timed_immediate_word(mmu_ref);
                                if get_flag(&mut self.f, ZERO) {
                                    self.pc = target;
                                    skip_increment = true;
                                    cycles = 16;
                                }
                            },
                            0xCB => (), // this is just the CB prefix
                            0xCC => {
                                let target = self.timed_immediate_word(mmu_ref);
                                if(get_flag(&mut self.f, ZERO)) {
                                    self.pc += instruction_size as u16;
                                    self.stack_push_at(mmu_ref, ppu_ref, self.pc, 12, 16);
                                    self.pc = target;
                                    skip_increment = true;
                                    cycles = 24;
                                }
                            },
                            0xCD => {
                                let target = self.timed_immediate_word(mmu_ref);
                                self.pc += instruction_size as u16;
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc, 12, 16);
                                self.pc = target;
                                skip_increment = true;
                            },
                            0xCE => self.a = adc_reg(self.a, n, &mut self.f),
                            0xCF => {
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc+1, 4, 8);
                                self.pc = 0x08;
                                skip_increment = true;
                            },
                            0xD0 => {
                                if !get_flag(&mut self.f, CARRY) {
                                    self.pc = self.stack_pop_at(mmu_ref, ppu_ref, 4, 8);
                                    skip_increment = true;
                                    cycles = 20;
                                }
                            },
                            0xD1 => {let de_val = self.stack_pop(mmu_ref); self.set_de(de_val);},
                            0xD2 => {
                                let target = self.timed_immediate_word(mmu_ref);
                                if !get_flag(&mut self.f, CARRY) {
                                    self.pc = target;
                                    skip_increment = true;
                                    cycles = 16;
                                }
                            }
                            0xD3 => (),
                            0xD4 => {
                                let target = self.timed_immediate_word(mmu_ref);
                                if(!get_flag(&mut self.f, CARRY)) {
                                    self.pc += instruction_size as u16;
                                    self.stack_push_at(mmu_ref, ppu_ref, self.pc, 12, 16);
                                    self.pc = target;
                                    skip_increment = true;
                                    cycles = 24;
                                }
                            },
                            0xD5 => self.stack_push_at(mmu_ref, ppu_ref, self.get_de(), 4, 8),
                            0xD6 => self.a = sub_reg(self.a, n, &mut self.f),
                            0xD7 => {
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc+1, 4, 8);
                                self.pc = 0x10;
                                skip_increment = true;
                            },
                            0xD8 => {
                                if(get_flag(&mut self.f, CARRY)) {
                                    self.pc = self.stack_pop_at(mmu_ref, ppu_ref, 4, 8);
                                    skip_increment = true;
                                    cycles = 20;
                                }
                            },
                            0xD9 => {
                                self.pc = self.stack_pop(mmu_ref);
                                skip_increment = true;
                                self.ime = true;
                                //println!("EXECUTING RETI, IME NOW: {}", self.ime);
                            },
                            0xDA => {
                                let target = self.timed_immediate_word(mmu_ref);
                                if get_flag(&mut self.f, CARRY) {
                                    self.pc = target;
                                    skip_increment = true;
                                    cycles = 16;
                                }
                            },
                            0xDB => (),
                            0xDC => {
                                let target = self.timed_immediate_word(mmu_ref);
                                if(get_flag(&mut self.f, CARRY)) {
                                    self.pc += instruction_size as u16;
                                    self.stack_push_at(mmu_ref, ppu_ref, self.pc, 12, 16);
                                    self.pc = target;
                                    skip_increment = true;
                                    cycles = 24;
                                }
                            },
                            0xDD => (),
                            0xDE => self.a = sbc_reg(self.a, n, &mut self.f),
                            0xDF => {
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc+1, 4, 8);
                                self.pc = 0x18;
                                skip_increment = true;
                            },
                            0xE0 => self.timed_write(
                                mmu_ref,
                                ppu_ref,
                                (0xFF00 + (n as u16)) as usize,
                                self.a,
                                cycles - 8,
                            ),
                            0xE1 => {let hl_val = self.stack_pop(mmu_ref); self.set_hl(hl_val);},
                            0xE2 => mmu_ref.set_byte((0xFF00 + (self.c as u16)) as usize, self.a),
                            0xE3 => (),
                            0xE4 => (),
                            0xE5 => self.stack_push_at(mmu_ref, ppu_ref, self.get_hl(), 4, 8),
                            0xE6 => and(&mut self.a, n, &mut self.f),
                            0xE7 => {
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc+1, 4, 8);
                                self.pc = 0x20;
                                skip_increment = true;
                            },
                            0xE8 => {
                                // TODO frankly just refactor this. This never should've used
                                // add_double_regs since it really is an 8 bit instruction.. stupid
                                // one-off.
                                let sp = self.get_sp();
                                let sign_extended = n as i8 as i16; // TODO try to understand how
                                                                    // this sign extending twos
                                                                    // complement works
                                let new_sp = add_double_regs(sp, sign_extended as u16, &mut self.f);
                                // we also need to do the 8 bit half carry...
                                let half_carry = ((sp & 0x000F) + ((sign_extended as u16) & 0x000F)) > 0x000F;
                                // AND carry. 
                                let carry = ((sp & 0x00FF) + ((sign_extended as u16) & 0x00FF)) > 0x00FF;
                                set_flag(&mut self.f, HC, half_carry);
                                set_flag(&mut self.f, CARRY, carry);
                                // we have to explicitly clear the ZERO flag
                                set_flag(&mut self.f, ZERO, false);

                                self.set_sp(new_sp);
                            },
                            0xE9 => self.pc = self.get_hl() - (instruction_size as u16),
                            0xEA => self.timed_write(
                                mmu_ref,
                                ppu_ref,
                                nn as usize,
                                self.a,
                                cycles - 8,
                            ),
                            0xEB => (),
                            0xEC => (),
                            0xED => (),
                            0xEE => xor(&mut self.a, n, &mut self.f),
                            0xEF => {
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc+1, 4, 8);
                                self.pc = 0x28;
                                skip_increment = true;
                            },
                            0xF0 => {
                                self.a = self.timed_read(
                                    mmu_ref,
                                    ppu_ref,
                                    0xFF00 + n as usize,
                                    cycles - 8,
                                )
                            }
                            0xF1 => {let af_val = self.stack_pop(mmu_ref); self.set_af(af_val);},
                            0xF2 => self.a = mmu_ref.get_byte((0xFF00 + (self.c as u16) as usize)),
                            0xF3 => {
                                self.ime = false;
                                self.ei_pending = false;
                                self.di_pending = false;
                            },
                            0xF4 => (),
                            0xF5 => self.stack_push_at(mmu_ref, ppu_ref, self.get_af(), 4, 8),
                            0xF6 => or(&mut self.a, n, &mut self.f),
                            0xF7 => {
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc+1, 4, 8);
                                self.pc = 0x30;
                                skip_increment = true;
                            },
                            0xF8 => {
                                let offset = n as i8 as i16 as u16;
                                let sp = self.get_sp();
                                let result = sp.wrapping_add(offset);

                                self.set_hl(result);

                                set_flag(&mut self.f, ZERO, false);
                                set_flag(&mut self.f, SUB, false);
                                
                                // TODO understand this shit later
                                let carry_bits = (sp ^ offset ^ result) as u16;
                                set_flag(&mut self.f, HC, (carry_bits & 0x10) != 0);
                                set_flag(&mut self.f, CARRY, (carry_bits & 0x100) != 0);
                            },
                            0xF9 => {
                                let hl = self.get_hl(); 
                                self.set_sp(hl);
                            },
                            0xFA => {
                                self.a =
                                    self.timed_read(mmu_ref, ppu_ref, nn as usize, cycles - 8)
                            }
                            0xFB => self.ei_pending = true, 
                            0xFC => (),
                            0xFD => (),
                            0xFE => compare(self.a, n, &mut self.f),
                            0xFF => {
                                self.stack_push_at(mmu_ref, ppu_ref, self.pc+1, 4, 8);
                                self.pc = 0x38;
                                skip_increment = true;
                            },


                            _ => {
                                panic!(
                                    "Error: Invalid opcode: 0x{:02X} (CB Prefix: {})", 
                                    opcode, 
                                    if cb_prefix { "on" } else { "off" }
                                );
                            },
                    }
 
            }


    if ei_was_pending && !((opcode == 0xF3) && (!cb_prefix)) {
        self.ime = true;
        self.ei_pending = false;
    }


    if !skip_increment {
        self.pc += instruction_size as u16;
    }

    if mmu_ref.get_boot() != 0 && self.trace_enabled {
        log_line(&format!(
            "A:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}",
            self.a, self.f, self.b, self.c, self.d, self.e, self.h, self.l, self.get_sp(), self.pc,
            mmu_ref.get_byte(self.pc as usize), mmu_ref.get_byte((self.pc+1) as usize), mmu_ref.get_byte((self.pc+2) as usize), mmu_ref.get_byte((self.pc+3) as usize)
        ));
        /*
        println!(
            "FLAGS after: Z={} N={} H={} C={}",
            get_flag(&mut self.f, ZERO),
            get_flag(&mut self.f, SUB),
            get_flag(&mut self.f, HC),
            get_flag(&mut self.f, CARRY)
        );
        */
    }


    cycles
    }
}
